import {
  IconArchive,
  IconAlertTriangle,
  IconBell,
  IconCalendarTime,
  IconCopy,
  IconExternalLink,
  IconFolderOpen,
  IconLoader2,
  IconLink,
  IconMessageCircle,
  IconPlayerPlay,
  IconPlus,
  IconRefresh,
  IconSearch,
  IconTrash,
  IconUnlink,
  IconX,
} from "@tabler/icons-react";
import { DragDropProvider, useDraggable, useDroppable } from "@dnd-kit/react";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ComponentProps,
  type KeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { createRoot } from "react-dom/client";
import { Button } from "@/components/ui/button";
import {
  DEFAULT_ghostex_SETTINGS,
  isDiagnosticLoggingScenarioEnabled,
  normalizeghostexSettings,
} from "../../shared/ghostex-settings";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import {
  TOOLTIP_DELAY_MS,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  BOARD_COLUMNS,
  PRIORITY_OPTIONS,
  TSHIRT_OPTIONS,
  appendImageMarkdownToDescription,
  beadsErrorMessage,
  beadsStatusToBoardStatus,
  boardStatusBeadsValue,
  boardStatusLabel,
  buildAgentWorkPrompt,
  ensureIssuePrefix,
  ensureWorkflowStatuses,
  extractDescriptionImageReferences,
  extractPreviewableDescriptionImageReferences,
  filterBoardTickets,
  formatProjectBoardCommentText,
  formatShortDate,
  getBlockedByIds,
  getBlockingIds,
  normalizeBeadsPayload,
  normalizeDisplayIssueKey,
  normalizeIssuePrefix,
  parseProjectBoardCommentText,
  parseBeadsJson,
  priorityLabel,
  prioritySelectValue,
  projectBoardRawProjectIdFromUrlParam,
  removeDescriptionImageReference,
  isDescriptionImageSource,
  tshirtToEstimate,
  toBoardTickets,
  estimateToTshirt,
  type BeadsBridgeRequest,
  type BeadsBridgeResponse,
  type BoardEstimateFilter,
  type BoardPriorityFilter,
  type ProjectBoardCommentMetadata,
  type BeadsIssue,
  type BoardStatusKey,
  type BoardTicket,
  type DescriptionImageReference,
  type TshirtSize,
} from "./project-board-shared";
import {
  type ProjectBoardAgentOption,
  type ProjectBoardBridgeRequest,
  type ProjectBoardBridgeResponse,
  type ProjectBoardConversationLinkView,
  type ProjectBoardConversationState,
  type ProjectBoardStartLocation,
} from "../../shared/bead-conversation-links";
import type { AppToastLevel } from "../../shared/app-toast-contract";
import {
  compareAutomationRunsNewestFirst,
  computeNextRunAt,
  normalizeAutomationSchedule,
  type AutomationDefinition,
  type AutomationExecutionMode,
  type AutomationRun,
  type AutomationSchedule,
  type ProjectAutomationAgentOption,
  type ProjectAutomationTargetOption,
  type ProjectAutomationsBridgeState,
} from "../../shared/automations";
import { AGENT_LOGO_COLORS, AGENT_LOGOS } from "../../sidebar/agent-logos";
import {
  createSidebarAgentSelectItems,
  getSidebarAgentIconById,
  type SidebarAgentIcon,
} from "../../shared/sidebar-agents";
import "../../sidebar/styles/shadcn.generated.css";

type LoadState = "idle" | "loading" | "ready" | "error";

type DetailDraft = {
  blockedByIds: string[];
  blockingIds: string[];
  comment: string;
  description: string;
  isDeleting: boolean;
  isSaving: boolean;
  labels: string[];
  priority: string;
  status: BoardStatusKey;
  title: string;
  tshirt?: TshirtSize;
  ticket?: BoardTicket;
};

type TicketDetailSaveDraft = Omit<DetailDraft, "isDeleting" | "isSaving" | "ticket"> & {
  commentMetadata: ProjectBoardCommentMetadata;
  ticket: BoardTicket;
};

type TicketFormDraft = {
  blockedByIds: string[];
  blockingIds: string[];
  description: string;
  labels: string[];
  priority: string;
  status: BoardStatusKey;
  title: string;
  tshirt?: TshirtSize;
};

type PendingBoardStatusMove = {
  beadsStatus: string;
  statusKey: BoardStatusKey;
  token: number;
};

type ConversationActionState =
  | { kind: "associate"; beadId: string }
  | { kind: "jump"; linkId: string }
  | { kind: "start"; beadId: string }
  | { kind: "unlink"; linkId: string }
  | undefined;

type ProjectBeadsWebKitWindow = Window & {
  webkit?: {
    messageHandlers?: {
      ghostexProjectBeads?: {
        postMessage: (message: BeadsBridgeRequest) => void;
      };
      ghostexProjectBoard?: {
        postMessage: (message: ProjectBoardBridgeRequest) => void;
      };
      ghostexProjectBoardImages?: {
        postMessage: (message: ProjectBoardImageBridgeRequest) => void;
      };
    };
  };
};

type ProjectBoardIdleWindow = Window & {
  requestIdleCallback?: (
    callback: () => void,
    options?: { timeout?: number },
  ) => number;
};

const BRIDGE_REQUEST_PREFIX = "__GHOSTEX_PROJECT_BEADS_REQUEST__";
const BRIDGE_RESPONSE_EVENT = "ghostex-project-beads-response";
const PROJECT_BOARD_RESPONSE_EVENT = "ghostex-project-board-response";
const PROJECT_BOARD_IMAGE_RESPONSE_EVENT = "ghostex-project-board-image-response";
const PROJECT_BOARD_AUTO_REFRESH_INTERVAL_MS = 8_000;
const PROJECT_BOARD_GENERATED_TITLE_DELAY_MS = 2_000;
const PROJECT_BOARD_GENERATED_TITLE_IDLE_TIMEOUT_MS = 10_000;
const PROJECT_BOARD_DRAFT_TITLE_MAX_LENGTH = 39;
const PROJECT_BOARD_MAX_DEPENDENCY_OPTIONS = 600;
const PROJECT_BOARD_MAX_VISIBLE_TICKETS_PER_COLUMN = 120;
const NATIVE_SETTINGS_STORAGE_KEY = "ghostex-native-settings";

/*
 * CDXC:Automations 2026-07-01-03:24:
 * Experimental automation surfaces use the existing Enable Experimental
 * Features setting as their content gate. Read the native settings snapshot
 * here so disabled pages render only the coming-soon overlay and do not fetch
 * automation state.
 *
 * CDXC:GPUIAutomateStable 2026-07-26:
 * GPUI's project-scoped Automate workarea is a released surface. Its
 * first-party URL explicitly opts out of the experimental gate, while macOS
 * Automate and the Quick Automations Overview keep their existing policy.
 */
function readExperimentalFeaturesEnabled(searchParams: URLSearchParams): boolean {
  if (searchParams.get("automationExperimental") === "false") {
    return true;
  }
  const storedSettingsJson = window.localStorage.getItem(NATIVE_SETTINGS_STORAGE_KEY);
  if (storedSettingsJson) {
    try {
      return normalizeghostexSettings(JSON.parse(storedSettingsJson)).showBetaFeatures;
    } catch {
      return DEFAULT_ghostex_SETTINGS.showBetaFeatures;
    }
  }
  const urlValue = searchParams.get("showBetaFeatures");
  if (urlValue === "true") {
    return true;
  }
  if (urlValue === "false") {
    return false;
  }
  return DEFAULT_ghostex_SETTINGS.showBetaFeatures;
}

const PROJECT_BOARD_START_LOCATION_SELECT_ITEMS: ReadonlyArray<{
  label: string;
  value: ProjectBoardStartLocation;
}> = [
  { label: "Current project", value: "currentProject" },
  { label: "New worktree", value: "newWorktree" },
];
const PROJECT_BOARD_STATUS_SELECT_ITEMS = BOARD_COLUMNS.map((column) => ({
  label: column.label,
  value: column.key,
}));
const PROJECT_BOARD_PRIORITY_SELECT_ITEMS = PRIORITY_OPTIONS.map((option) => ({
  label: option.label,
  value: option.value,
}));
const PROJECT_BOARD_PRIORITY_FILTER_SELECT_ITEMS: Array<{ label: string; value: BoardPriorityFilter }> = [
  { label: "All priorities", value: "all" },
  ...PROJECT_BOARD_PRIORITY_SELECT_ITEMS,
];
const PROJECT_BOARD_TSHIRT_SELECT_ITEMS: Array<{ label: string; value: TshirtSize | "none" }> = [
  { label: "None", value: "none" },
  ...TSHIRT_OPTIONS.map((option) => ({ label: option.label, value: option.label })),
];
const PROJECT_BOARD_ESTIMATE_FILTER_SELECT_ITEMS: Array<{ label: string; value: BoardEstimateFilter }> = [
  { label: "All estimates", value: "all" },
  ...PROJECT_BOARD_TSHIRT_SELECT_ITEMS,
];
const PROJECT_AUTOMATION_TRIAGE_RECENT_COMPLETED_LIMIT = 5;

type BoardRefreshMode = "background" | "initial" | "manual" | "mutation";

type BoardRefreshOptions = {
  mode?: BoardRefreshMode;
};

type ProjectSurfaceTab = "triage" | "automations" | "runs" | "board";

type TicketContextMenuState = {
  confirmingDelete: boolean;
  ticketId: string;
  x: number;
  y: number;
};

const PROJECT_BOARD_CONTEXT_MENU_VIEWPORT_MARGIN_PX = 12;

const AUTOMATION_SCHEDULE_PRESETS = [
  { label: "Every 5 minutes", value: "5m" },
  { label: "Every 15 minutes", value: "15m" },
  { label: "Every 30 minutes", value: "30m" },
  { label: "Hourly", value: "hourly" },
  { label: "Every 6 hours", value: "6h" },
  { label: "Every 12 hours", value: "12h" },
  { label: "Daily", value: "daily" },
  { label: "Weekdays", value: "weekdays" },
  { label: "Weekly", value: "weekly" },
  { label: "Custom cron", value: "cron" },
] as const;

type AutomationSchedulePreset = (typeof AUTOMATION_SCHEDULE_PRESETS)[number]["value"];

const AUTOMATION_INTERVAL_MS_BY_PRESET: Partial<Record<AutomationSchedulePreset, number>> = {
  "5m": 5 * 60 * 1000,
  "15m": 15 * 60 * 1000,
  "30m": 30 * 60 * 1000,
  hourly: 60 * 60 * 1000,
  "6h": 6 * 60 * 60 * 1000,
  "12h": 12 * 60 * 60 * 1000,
};

const AUTOMATION_WEEKDAY_OPTIONS = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
] as const;

type AutomationDraft = {
  agentId: string;
  cronExpression: string;
  enabled: boolean;
  executionKind: AutomationExecutionMode["kind"];
  expiresAt: string;
  id?: string;
  name: string;
  prompt: string;
  projectId: string;
  schedulePreset: AutomationSchedulePreset;
  scheduleTime: string;
  setupCommand: string;
  threadSessionId: string;
  weeklyDay: string;
};

type ProjectBoardImageBridgeRequest = {
  action: "loadPreview" | "pasteImage";
  path?: string;
  requestId: string;
};

type ProjectBoardImageBridgeResponse = {
  dataUrl?: string;
  error?: string;
  imagePath?: string;
  path?: string;
  requestId: string;
};

type ProjectBoardFocusOwnerEvent = "focusin" | "keydown" | "pointerdown";

const PROJECT_BOARD_FOCUS_OWNER_MIN_INTERVAL_MS = 250;

function isProjectBoardEditableFocusTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) {
    return false;
  }
  return (
    target.closest(
      [
        "input",
        "textarea",
        "select",
        '[contenteditable="true"]',
        "[role='textbox']",
      ].join(","),
    ) !== null
  );
}

function postProjectBoardFocusOwnerChanged({
  event,
  projectEditorId,
  projectId,
  remoteMachineId,
}: {
  event: ProjectBoardFocusOwnerEvent;
  projectEditorId: string;
  projectId: string;
  remoteMachineId: string;
}): void {
  const projectBoardBridge = (window as ProjectBeadsWebKitWindow).webkit?.messageHandlers
    ?.ghostexProjectBoard;
  if (!projectBoardBridge) {
    return;
  }
  projectBoardBridge.postMessage({
    action: "projectEditorFocusOwnerChanged",
    event,
    projectEditorId,
    projectId,
    ...(remoteMachineId ? { remoteMachineId } : {}),
    requestId: crypto.randomUUID(),
  });
}

function createEmptyDetailDraft(): DetailDraft {
  return {
    blockedByIds: [],
    blockingIds: [],
    comment: "",
    description: "",
    isDeleting: false,
    isSaving: false,
    labels: [],
    priority: "2",
    status: "todo",
    title: "",
  };
}

function createEmptyTicketFormDraft(): TicketFormDraft {
  return {
    blockedByIds: [],
    blockingIds: [],
    description: "",
    labels: [],
    priority: "2",
    status: "todo",
    title: "",
  };
}

/*
 * CDXC:ProjectBoardTitleGeneration 2026-06-21-16:56:
 * Empty-title Kanban tickets must appear with a useful deterministic draft title immediately, while prompt-agent title generation runs later as background polish.
 * Keep the draft short enough for the board's title column so replacing it with the generated title does not require a full board reload.
 */
function createProjectBoardDraftTitle(prompt: string): string {
  const normalizedPrompt = prompt
    .replace(/```[\s\S]*?```/g, " ")
    .split(/\n+/u)
    .map((line) =>
      line
        .replace(/^\s*(?:#{1,6}\s+|[-*+]\s+|\d+[.)]\s+)/u, "")
        .replace(/[`*_~>#]+/g, " ")
        .replace(/\s+/g, " ")
        .trim(),
    )
    .find(Boolean) ?? "";
  const firstSentence = normalizedPrompt.match(/^[^.!?]{8,}[.!?](?=\s|$)/u)?.[0] ?? normalizedPrompt;
  const title = firstSentence
    .replace(/[.!?]+$/u, "")
    .replace(/\s+/g, " ")
    .trim();
  if (!title) {
    return "New ticket";
  }
  if (title.length <= PROJECT_BOARD_DRAFT_TITLE_MAX_LENGTH) {
    return title;
  }
  const clipped = title.slice(0, PROJECT_BOARD_DRAFT_TITLE_MAX_LENGTH).replace(/\s+\S*$/u, "").trim();
  return (clipped.length >= 12 ? clipped : title.slice(0, PROJECT_BOARD_DRAFT_TITLE_MAX_LENGTH))
    .replace(/[.,;:!?-]+$/u, "")
    .trim() || "New ticket";
}

function applyPendingBoardStatusMoves(
  issues: BeadsIssue[],
  pendingMoves: Map<string, PendingBoardStatusMove>,
): BeadsIssue[] {
  if (pendingMoves.size === 0) {
    return issues;
  }
  let changed = false;
  const nextIssues = issues.map((issue) => {
    const pendingMove = pendingMoves.get(issue.id);
    if (!pendingMove || issue.status === pendingMove.beadsStatus) {
      return issue;
    }
    changed = true;
    return { ...issue, status: pendingMove.beadsStatus };
  });
  return changed ? nextIssues : issues;
}

function upsertProjectBoardIssue(issues: BeadsIssue[], issue: BeadsIssue): BeadsIssue[] {
  const index = issues.findIndex((candidate) => candidate.id === issue.id);
  if (index === -1) {
    return [...issues, issue];
  }
  const nextIssues = [...issues];
  nextIssues[index] = { ...nextIssues[index], ...issue };
  return nextIssues;
}

function upsertProjectBoardTicket(tickets: BoardTicket[], ticket: BoardTicket): BoardTicket[] {
  const index = tickets.findIndex((candidate) => candidate.id === ticket.id);
  if (index === -1) {
    return [...tickets, ticket];
  }
  const nextTickets = [...tickets];
  nextTickets[index] = { ...nextTickets[index], ...ticket };
  return nextTickets;
}

function scheduleProjectBoardGeneratedTitle(work: () => void): void {
  window.setTimeout(() => {
    const requestIdleCallback = (window as ProjectBoardIdleWindow).requestIdleCallback;
    if (requestIdleCallback) {
      requestIdleCallback(work, { timeout: PROJECT_BOARD_GENERATED_TITLE_IDLE_TIMEOUT_MS });
      return;
    }
    work();
  }, PROJECT_BOARD_GENERATED_TITLE_DELAY_MS);
}

function ProjectBoardApp() {
  const urlSearchParams = new URLSearchParams(window.location.search);
  const projectName = urlSearchParams.get("projectName") || "Project";
  const projectPath = urlSearchParams.get("projectPath") || "";
  const projectIdParam = urlSearchParams.get("projectId") || "";
  const projectId = projectBoardRawProjectIdFromUrlParam(projectIdParam);
  const projectEditorId = urlSearchParams.get("projectEditorId") || projectIdParam;
  const remoteMachineId = urlSearchParams.get("remoteMachineId") || "";
  const automationScope = urlSearchParams.get("scope") === "all" ? "all" : "project";
  const isAutomationGlobalScope = automationScope === "all";
  const initialSurfaceTab: ProjectSurfaceTab =
    urlSearchParams.get("surface") === "automations" ? "automations" : "board";
  const automationIsExperimental =
    urlSearchParams.get("automationExperimental") !== "false";
  const [experimentalFeaturesEnabled, setExperimentalFeaturesEnabled] = useState(() =>
    readExperimentalFeaturesEnabled(urlSearchParams),
  );
  const automationSurfaceName = isAutomationGlobalScope ? "Automations Overview" : "Automate";
  const displayKey = normalizeDisplayIssueKey(
    urlSearchParams.get("beadsDisplayKey") ?? projectName,
  );
  const issuePrefix = normalizeIssuePrefix(
    projectName || projectPath.split("/").filter(Boolean).at(-1) || displayKey,
  );
  const [tickets, setTickets] = useState<BoardTicket[]>([]);
  const [allIssues, setAllIssues] = useState<BeadsIssue[]>([]);
  const [knownLabels, setKnownLabels] = useState<string[]>([]);
  const [conversationState, setConversationState] = useState<ProjectBoardConversationState>({
    agents: [],
    debuggingMode: false,
    links: [],
    sessions: [],
  });
  const [selectedAgentId, setSelectedAgentId] = useState("");
  const [loadState, setLoadState] = useState<LoadState>("idle");
  const [hasCompletedInitialBoardLoad, setHasCompletedInitialBoardLoad] = useState(false);
  const [errorMessage, setErrorMessage] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
  const searchInputRef = useRef<HTMLInputElement>(null);
  const [priorityFilter, setPriorityFilter] = useState<BoardPriorityFilter>("all");
  const [estimateFilter, setEstimateFilter] = useState<BoardEstimateFilter>("all");
  const [detail, setDetail] = useState<DetailDraft>(createEmptyDetailDraft);
  const [newTicketOpen, setNewTicketOpen] = useState(false);
  const [newTicket, setNewTicket] = useState<TicketFormDraft>(createEmptyTicketFormDraft);
  const [newTicketStartLocation, setNewTicketStartLocation] =
    useState<ProjectBoardStartLocation>("currentProject");
  const createInFlightRef = useRef(false);
  const pendingStatusMovesRef = useRef(new Map<string, PendingBoardStatusMove>());
  const pendingStatusMoveSerialRef = useRef(0);
  const [deleteConfirmingTicketId, setDeleteConfirmingTicketId] = useState("");
  const [ticketContextMenu, setTicketContextMenu] = useState<TicketContextMenuState>();
  const [contextMenuDeletingTicketId, setContextMenuDeletingTicketId] = useState("");
  const [imagePreviewDataUrls, setImagePreviewDataUrls] = useState<Record<string, string>>({});
  const pendingImagePreviewPathsRef = useRef(new Set<string>());
  const failedImagePreviewPathsRef = useRef(new Set<string>());
  const agentSelectItems = useMemo(
    () =>
      conversationState.agents.map((agent) => ({
        label: agent.label,
        value: agent.agentId,
      })),
    [conversationState.agents],
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
  const issuesSignatureRef = useRef("");
  const labelsSignatureRef = useRef("");
  const newPromptRef = useRef<HTMLTextAreaElement>(null);
  const detailSaveSerialRef = useRef(0);
  const automationProjectsRef = useRef<ProjectAutomationsBridgeState["projects"]>([]);
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
    activeSurfaceTab !== "board" && automationIsExperimental && !experimentalFeaturesEnabled;
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
  const [automationConversationState, setAutomationConversationState] =
    useState<ProjectBoardConversationState>({
      agents: [],
      debuggingMode: false,
      links: [],
      sessions: [],
    });
  const [automationDialogOpen, setAutomationDialogOpen] = useState(false);
  const [automationDraft, setAutomationDraft] = useState<AutomationDraft>(() =>
    createAutomationDraft(),
  );
  const [automationActionId, setAutomationActionId] = useState("");
  const [automationTargetProjectId, setAutomationTargetProjectId] = useState(projectId);
  const [selectedAutomationId, setSelectedAutomationId] = useState("");
  const [selectedAutomationRunId, setSelectedAutomationRunId] = useState("");

  useEffect(() => {
    let lastPostedAt = 0;
    const postFocusOwnerChanged = (
      event: ProjectBoardFocusOwnerEvent,
      target: EventTarget | null,
    ) => {
      if (event !== "pointerdown" && !isProjectBoardEditableFocusTarget(target)) {
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
      postFocusOwnerChanged("pointerdown", event.target);
    };
    const handleFocusIn = (event: globalThis.FocusEvent) => {
      postFocusOwnerChanged("focusin", event.target);
    };
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      postFocusOwnerChanged("keydown", event.target);
    };
    window.addEventListener("pointerdown", handlePointerDown, true);
    window.addEventListener("focusin", handleFocusIn, true);
    window.addEventListener("keydown", handleKeyDown, true);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown, true);
      window.removeEventListener("focusin", handleFocusIn, true);
      window.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [projectEditorId, projectId, remoteMachineId]);

  useEffect(() => {
    const syncExperimentalFeaturesEnabled = () => {
      setExperimentalFeaturesEnabled(
        readExperimentalFeaturesEnabled(new URLSearchParams(window.location.search)),
      );
    };
    const handleStorage = (event: StorageEvent) => {
      if (event.key === null || event.key === NATIVE_SETTINGS_STORAGE_KEY) {
        syncExperimentalFeaturesEnabled();
      }
    };
    const handleVisibilityChange = () => {
      if (document.visibilityState === "visible") {
        syncExperimentalFeaturesEnabled();
      }
    };
    window.addEventListener("storage", handleStorage);
    window.addEventListener("focus", syncExperimentalFeaturesEnabled);
    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => {
      window.removeEventListener("storage", handleStorage);
      window.removeEventListener("focus", syncExperimentalFeaturesEnabled);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, []);

  const openNewTicket = useCallback((status: BoardStatusKey = "todo") => {
    setNewTicket((current) => ({ ...current, status }));
    setNewTicketOpen(true);
  }, []);

  const runBeads = useCallback(
    async (request: Omit<BeadsBridgeRequest, "cwd" | "requestId">) => {
      if (!projectPath) {
        throw new Error("No active project path is available.");
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
    [projectId, projectPath, remoteMachineId],
  );

  const loadConversationState = useCallback(async () => {
    try {
      const response = await sendProjectBoardRequest({
        action: "getState",
        projectId,
        projectEditorId,
        projectPath,
        ...(remoteMachineId ? { remoteMachineId } : {}),
      });
      if (!response.ok) {
        throw new Error(response.error || "Could not load linked conversations.");
      }
      const payload = response.payload ?? { agents: [], links: [], sessions: [] };
      setConversationState(payload);
      setAutomationConversationState((current) =>
        automationTargetProjectId === projectId ? payload : current,
      );
      setSelectedAgentId((current) => current || payload.defaultAgentId || payload.agents[0]?.agentId || "");
    } catch (error) {
      console.warn("Project board conversation state unavailable.", error);
    }
  }, [automationTargetProjectId, projectEditorId, projectId, projectPath, remoteMachineId]);

  const applyAutomationState = useCallback((payload: ProjectAutomationsBridgeState) => {
    automationProjectsRef.current = payload.projects;
    setAutomationState(payload);
    setAutomationTargetProjectId(
      isAutomationGlobalScope ? payload.projects[0]?.projectId ?? payload.projectId : payload.projectId,
    );
  }, [isAutomationGlobalScope]);

  const loadAutomationState = useCallback(async (targetProjectId?: string) => {
    if (!experimentalFeaturesEnabled) {
      return;
    }
    const requestedProjectId = targetProjectId?.trim() || automationTargetProjectId || projectId;
    const targetProject = automationProjectsRef.current.find(
      (candidate) => candidate.projectId === requestedProjectId,
    );
    try {
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: isAutomationGlobalScope ? "automationGetAllState" : "automationGetState",
        projectEditorId,
        projectId: isAutomationGlobalScope ? projectId : requestedProjectId,
        projectPath: isAutomationGlobalScope
          ? undefined
          : targetProject?.path ?? (requestedProjectId === projectId ? projectPath : undefined),
        ...(remoteMachineId ? { remoteMachineId } : {}),
      });
      if (!response.ok) {
        throw new Error(response.error || "Could not load automations.");
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
                  response.payload?.defaultAgentId,
                ),
                projectId:
                  isAutomationGlobalScope
                    ? resolveAutomationDraftProjectId(
                        response.payload?.projects ?? [],
                        current.projectId,
                        response.payload?.projectId || projectId,
                      )
                    : current.projectId || response.payload?.projectId || projectId,
                executionKind: response.payload?.projectCanUseWorktrees === false ? "local" : current.executionKind,
              },
        );
      }
    } catch (error) {
      console.warn("Project automations state unavailable.", error);
    }
  }, [applyAutomationState, automationTargetProjectId, experimentalFeaturesEnabled, isAutomationGlobalScope, projectEditorId, projectId, projectPath, remoteMachineId]);

  const loadAutomationConversationState = useCallback(async (targetProjectId?: string) => {
    if (!experimentalFeaturesEnabled) {
      setAutomationConversationState({ agents: [], debuggingMode: false, links: [], sessions: [] });
      return;
    }
    const requestedProjectId = targetProjectId?.trim() || automationTargetProjectId || projectId;
    if (isAutomationGlobalScope && !automationProjectsRef.current.some((candidate) => candidate.projectId === requestedProjectId)) {
      setAutomationConversationState({ agents: [], debuggingMode: false, links: [], sessions: [] });
      return;
    }
    const targetProject = automationProjectsRef.current.find(
      (candidate) => candidate.projectId === requestedProjectId,
    );
    try {
      const response = await sendProjectBoardRequest({
        action: "getState",
        projectEditorId,
        projectId: requestedProjectId,
        projectPath: targetProject?.path ?? (requestedProjectId === projectId ? projectPath : undefined),
        ...(remoteMachineId ? { remoteMachineId } : {}),
      });
      if (!response.ok) {
        throw new Error(response.error || "Could not load automation sessions.");
      }
      setAutomationConversationState(response.payload ?? { agents: [], links: [], sessions: [] });
    } catch (error) {
      console.warn("Project automation sessions unavailable.", error);
      setAutomationConversationState({ agents: [], debuggingMode: false, links: [], sessions: [] });
    }
  }, [automationTargetProjectId, experimentalFeaturesEnabled, isAutomationGlobalScope, projectEditorId, projectId, projectPath, remoteMachineId]);

  const logProjectBoardDebug = useCallback(
    (event: string, details?: Record<string, unknown>) => {
      if (
        !isDiagnosticLoggingScenarioEnabled(
          conversationState.diagnosticLogging,
          "native.project.board",
        )
      ) {
        return;
      }
      void sendProjectBoardRequest({
        action: "appendDebugLog",
        details: stringifyProjectBoardDebugDetails(details),
        event,
        projectId,
        projectEditorId,
        projectPath,
        ...(remoteMachineId ? { remoteMachineId } : {}),
      }).catch((error) => {
        console.warn("Project board debug log unavailable.", error);
      });
    },
    [
      conversationState.diagnosticLogging,
      projectEditorId,
      projectId,
      projectPath,
      remoteMachineId,
    ],
  );

  const showProjectBoardToast = useCallback(
    (level: AppToastLevel, title: string, description?: string) => {
      void sendProjectBoardRequest({
        action: "showToast",
        projectEditorId,
        projectId,
        projectPath,
        ...(remoteMachineId ? { remoteMachineId } : {}),
        toastDescription: description,
        toastLevel: level,
        toastTitle: title,
      }).catch((error) => {
        console.warn("Project board toast unavailable.", error);
      });
    },
    [projectEditorId, projectId, projectPath, remoteMachineId],
  );

  const setLocalTicketStatus = useCallback(
    (ticketId: string, statusKey: BoardStatusKey, beadsStatus: string) => {
      setAllIssues((current) =>
        current.map((candidate) =>
          candidate.id === ticketId ? { ...candidate, status: beadsStatus } : candidate,
        ),
      );
      setTickets((current) =>
        current.map((candidate) =>
          candidate.id === ticketId
            ? { ...candidate, boardStatus: statusKey, status: beadsStatus }
            : candidate,
        ),
      );
      setDetail((current) =>
        current.ticket?.id === ticketId
          ? {
              ...current,
              status: statusKey,
              ticket: { ...current.ticket, boardStatus: statusKey, status: beadsStatus },
            }
          : current,
      );
    },
    [],
  );

  const upsertLocalIssue = useCallback(
    (issue: BeadsIssue) => {
      setAllIssues((current) => upsertProjectBoardIssue(current, issue));
      setTickets((current) => {
        const localTicket = toCreatedBoardTicket(issue, current, displayKey);
        return localTicket ? upsertProjectBoardTicket(current, localTicket) : current;
      });
      setDetail((current) =>
        current.ticket?.id === issue.id
          ? {
              ...current,
              description: issue.description ?? current.description,
              labels: issue.labels ?? current.labels,
              priority:
                issue.priority === undefined ? current.priority : prioritySelectValue(issue.priority),
              status: beadsStatusToBoardStatus(issue.status),
              title: issue.title,
              tshirt: estimateToTshirt(issue.estimate),
              ticket: {
                ...current.ticket,
                ...issue,
                boardStatus: beadsStatusToBoardStatus(issue.status),
                displayId: current.ticket.displayId,
              },
            }
          : current,
      );
    },
    [displayKey],
  );

  const setLocalTicketTitle = useCallback((ticketId: string, title: string) => {
    setAllIssues((current) =>
      current.map((candidate) => (candidate.id === ticketId ? { ...candidate, title } : candidate)),
    );
    setTickets((current) =>
      current.map((candidate) => (candidate.id === ticketId ? { ...candidate, title } : candidate)),
    );
    setDetail((current) =>
      current.ticket?.id === ticketId
        ? { ...current, title, ticket: { ...current.ticket, title } }
        : current,
    );
  }, []);

  const loadTickets = useCallback(async (options: BoardRefreshOptions = {}) => {
    const mode = options.mode ?? "manual";
    if (isRefreshingRef.current) {
      if (mode === "background") {
        return;
      }
      await waitForProjectBoardRefreshIdle(() => isRefreshingRef.current);
    }
    isRefreshingRef.current = true;
    if (mode !== "background") {
      setLoadState("loading");
      setErrorMessage("");
    }
    try {
      if (mode === "initial" || mode === "manual") {
        await ensureIssuePrefix(runBeads, issuePrefix);
        await ensureWorkflowStatuses(runBeads);
      }
      const payload = await runBeads({ action: "listIssues" });
      const rawIssues = normalizeBeadsPayload<BeadsIssue[]>(payload, Array.isArray(payload) ? payload : []);
      const issues = applyPendingBoardStatusMoves(rawIssues, pendingStatusMovesRef.current);
      const issuesSignature = `${displayKey}:${createIssuesSignature(issues)}`;
      if (issuesSignature !== issuesSignatureRef.current) {
        issuesSignatureRef.current = issuesSignature;
        setAllIssues(issues);
        setTickets(toBoardTickets(issues, displayKey));
      }
      const labels = deriveKnownLabelsFromIssues(issues);
      const labelsSignature = labels.join("\u001f");
      if (labelsSignature !== labelsSignatureRef.current) {
        labelsSignatureRef.current = labelsSignature;
        setKnownLabels(labels);
      }
      if (mode !== "background") {
        setLoadState("ready");
      } else {
        setErrorMessage("");
        setLoadState((current) => (current === "loading" ? current : "ready"));
      }
    } catch (error) {
      if (mode !== "background") {
        setLoadState("error");
        setErrorMessage(error instanceof Error ? error.message : "Could not load Beads issues.");
      } else {
        console.warn("Project board auto refresh failed.", error);
      }
    } finally {
      isRefreshingRef.current = false;
      if (mode === "initial") {
        setHasCompletedInitialBoardLoad(true);
      }
    }
  }, [displayKey, issuePrefix, runBeads]);

  useEffect(() => {
    if (activeSurfaceTab === "board" || (experimentalFeaturesEnabled && !isAutomationGlobalScope)) {
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

  useEffect(() => {
    if (!ticketContextMenu) {
      return;
    }
    if (!tickets.some((ticket) => ticket.id === ticketContextMenu.ticketId)) {
      setTicketContextMenu(undefined);
    }
  }, [ticketContextMenu, tickets]);

  useEffect(() => {
    if (activeSurfaceTab !== "board") {
      return;
    }
    void loadTickets({ mode: "initial" });
  }, [activeSurfaceTab, loadTickets]);

  useEffect(() => {
    const imageSources = [
      ...extractPreviewableDescriptionImageReferences(detail.description),
      ...extractPreviewableDescriptionImageReferences(newTicket.description),
    ].map((image) => image.src);
    for (const imageSource of imageSources) {
      if (imageSource.startsWith("data:image/")) {
        setImagePreviewDataUrls((current) =>
          current[imageSource] ? current : { ...current, [imageSource]: imageSource },
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
      void sendProjectBoardImageRequest({ action: "loadPreview", path: imageSource })
        .then((response) => {
          if (response.dataUrl?.startsWith("data:image/")) {
            setImagePreviewDataUrls((current) => ({
              ...current,
              [imageSource]: response.dataUrl ?? "",
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
      if (document.visibilityState !== "visible") {
        return;
      }
      if (activeSurfaceTab === "board") {
        void loadTickets({ mode: "background" });
        void loadConversationState();
      }
      if (experimentalFeaturesEnabled) {
        void loadAutomationState();
      }
    };
    const intervalId = window.setInterval(
      () => refreshIfVisible(),
      PROJECT_BOARD_AUTO_REFRESH_INTERVAL_MS,
    );
    const handleVisible = () => refreshIfVisible();
    document.addEventListener("visibilitychange", handleVisible);
    window.addEventListener("focus", handleVisible);
    return () => {
      window.clearInterval(intervalId);
      document.removeEventListener("visibilitychange", handleVisible);
      window.removeEventListener("focus", handleVisible);
    };
  }, [activeSurfaceTab, experimentalFeaturesEnabled, loadAutomationState, loadConversationState, loadTickets]);

  const filteredTickets = useMemo(
    () => filterBoardTickets(tickets, searchQuery, priorityFilter, estimateFilter),
    [estimateFilter, priorityFilter, searchQuery, tickets],
  );

  const ticketsByColumn = useMemo(() => {
    return BOARD_COLUMNS.reduce<Record<BoardStatusKey, BoardTicket[]>>(
      (result, column) => {
        result[column.key] = filteredTickets.filter((ticket) => ticket.boardStatus === column.key);
        return result;
      },
      { backlog: [], done: [], in_progress: [], review: [], test: [], todo: [] },
    );
  }, [filteredTickets]);
  const showInitialBoardLoadingOverlay =
    activeSurfaceTab === "board" && loadState === "loading" && !hasCompletedInitialBoardLoad;

  const linksByBeadId = useMemo(() => {
    const result = new Map<string, ProjectBoardConversationLinkView[]>();
    const newestFirstLinks = [...conversationState.links].sort(compareConversationLinksNewestFirst);
    for (const link of newestFirstLinks) {
      const current = result.get(link.beadId) ?? [];
      current.push(link);
      result.set(link.beadId, current);
    }
    return result;
  }, [conversationState.links]);

  const ticketOptions = useMemo(
    () =>
      prioritizeDependencyTickets(tickets)
        .slice(0, PROJECT_BOARD_MAX_DEPENDENCY_OPTIONS)
        .map((ticket) => ({
          id: ticket.id,
          label: `${ticket.displayId} · ${ticket.title}`,
        })),
    [tickets],
  );

  const openTicket = async (ticket: BoardTicket) => {
    setDeleteConfirmingTicketId("");
    setDetail({
      blockedByIds: getBlockedByIds(ticket),
      blockingIds: getBlockingIds(ticket.id, allIssues),
      comment: "",
      description: ticket.description ?? "",
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
      const payload = await runBeads({ action: "show", issueId: ticket.id });
      const issue = normalizeBeadsPayload<BeadsIssue>(payload, ticket);
      const mergedIssue = allIssues.find((candidate) => candidate.id === ticket.id) ?? issue;
      const nextTicket: BoardTicket = {
        ...ticket,
        ...issue,
        ...mergedIssue,
        boardStatus: beadsStatusToBoardStatus(issue.status ?? ticket.status),
        displayId: ticket.displayId,
      };
      setDetail({
        blockedByIds: getBlockedByIds(mergedIssue),
        blockingIds: getBlockingIds(ticket.id, allIssues),
        comment: "",
        description: nextTicket.description ?? "",
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
      setErrorMessage(error instanceof Error ? error.message : "Could not load the ticket.");
    }
  };

  const moveTicket = async (ticketId: string, statusKey: BoardStatusKey) => {
    const column = BOARD_COLUMNS.find((candidate) => candidate.key === statusKey);
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
        action: "updateStatus",
        issueId: ticketId,
        status: column.beadsStatus,
      });
      if (pendingStatusMovesRef.current.get(ticketId)?.token !== token) {
        return;
      }
      pendingStatusMovesRef.current.delete(ticketId);
      void loadTickets({ mode: "background" });
    } catch (error) {
      if (pendingStatusMovesRef.current.get(ticketId)?.token !== token) {
        return;
      }
      pendingStatusMovesRef.current.delete(ticketId);
      setLocalTicketStatus(ticketId, ticket.boardStatus, ticket.status);
      setErrorMessage(error instanceof Error ? error.message : "Could not move the ticket.");
    }
  };

  const handleDragEnd: ComponentProps<typeof DragDropProvider>["onDragEnd"] = (event) => {
    if (event.canceled) {
      return;
    }
    const ticketId = String(event.operation.source?.id ?? "");
    const statusKey = event.operation.target?.id as BoardStatusKey | undefined;
    if (ticketId && statusKey) {
      void moveTicket(ticketId, statusKey);
    }
  };

  const syncDependencies = async (issueId: string, blockedByIds: string[], blockingIds: string[]) => {
    const issue = allIssues.find((candidate) => candidate.id === issueId);
    const currentBlockedBy = issue ? getBlockedByIds(issue) : [];
    const currentBlocking = issue ? getBlockingIds(issueId, allIssues) : [];
    for (const dependencyId of currentBlockedBy.filter((id) => !blockedByIds.includes(id))) {
      await runBeads({ action: "depRemove", dependsOnId: dependencyId, issueId });
    }
    for (const dependencyId of blockedByIds.filter((id) => !currentBlockedBy.includes(id))) {
      await runBeads({ action: "depAdd", dependsOnId: dependencyId, issueId, depType: "blocks" });
    }
    for (const dependentId of currentBlocking.filter((id) => !blockingIds.includes(id))) {
      await runBeads({ action: "depRemove", dependsOnId: issueId, issueId: dependentId });
    }
    for (const dependentId of blockingIds.filter((id) => !currentBlocking.includes(id))) {
      await runBeads({ action: "depAdd", dependsOnId: issueId, issueId: dependentId, depType: "blocks" });
    }
  };

  const persistTicketDetail = async (draft: TicketDetailSaveDraft) => {
    const trimmedComment = draft.comment.trim();
    await runBeads({
      action: "updateTitle",
      issueId: draft.ticket.id,
      title: draft.title.trim(),
    });
    await runBeads({
      action: "updateDescription",
      description: draft.description,
      issueId: draft.ticket.id,
    });
    await runBeads({
      action: "updatePriority",
      issueId: draft.ticket.id,
      priority: draft.priority,
    });
    const estimate = tshirtToEstimate(draft.tshirt);
    if (estimate !== undefined) {
      await runBeads({
        action: "updateEstimate",
        estimate,
        issueId: draft.ticket.id,
      });
    }
    if (draft.labels.length > 0) {
      await runBeads({
        action: "setLabels",
        issueId: draft.ticket.id,
        labels: draft.labels,
      });
    }
    await syncDependencies(draft.ticket.id, draft.blockedByIds, draft.blockingIds);
    if (draft.status !== draft.ticket.boardStatus) {
      await runBeads({
        action: "updateStatus",
        issueId: draft.ticket.id,
        status: boardStatusBeadsValue(draft.status),
      });
    }
    if (trimmedComment) {
      await runBeads({
        action: "addComment",
        comment: formatProjectBoardCommentText(trimmedComment, draft.commentMetadata),
        issueId: draft.ticket.id,
      });
    }
    await loadTickets({ mode: "background" });
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
      status: boardStatusBeadsValue(draft.status),
      title: draft.title.trim(),
    };
    setDeleteConfirmingTicketId("");
    setDetail(createEmptyDetailDraft());
    setErrorMessage("");
    upsertLocalIssue(optimisticIssue);
    setKnownLabels((current) => mergeKnownLabels(current, draft.labels));
    void persistTicketDetail(draft).catch((error) => {
      const message = error instanceof Error ? error.message : "Could not save the ticket.";
      setErrorMessage(message);
      showProjectBoardToast("error", "Ticket save failed", message);
      if (detailSaveSerialRef.current === saveToken) {
        setDetail({
          ...draft,
          isDeleting: false,
          isSaving: false,
        });
      }
      void loadTickets({ mode: "background" });
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
    setNewTicketStartLocation("currentProject");
    setNewTicketOpen(false);
    logProjectBoardDebug("projectBoard.createTicket.started", {
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
        action: "create",
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
      logProjectBoardDebug("projectBoard.createTicket.beadCreated", {
        beadId: createdIssue?.id ?? "",
        shouldGenerateTitle,
        startAfterCreate,
        targetStatus: draft.status,
      });
      if (!createdIssue?.id) {
        const createdIssueLookupPayload = await runBeads({ action: "listIssues" });
        const createdIssueLookupIssues = normalizeBeadsPayload<BeadsIssue[]>(
          createdIssueLookupPayload,
          Array.isArray(createdIssueLookupPayload) ? createdIssueLookupPayload : [],
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
        const promptAgentId = selectedAgentId || conversationState.defaultAgentId || "";
        const promptAgent = conversationState.agents.find((agent) => agent.agentId === promptAgentId);
        const titleGenerationDebugDetails = {
          agentCount: conversationState.agents.length,
          beadId: issueId,
          defaultAgentKind: projectBoardPromptAgentKind(conversationState.defaultAgentId || ""),
          hasDefaultAgentId: Boolean(conversationState.defaultAgentId),
          hasPromptAgent: Boolean(promptAgent),
          hasPromptAgentCommand: Boolean(promptAgent?.command?.trim()),
          hasSelectedAgentId: Boolean(selectedAgentId),
          promptAgentCommandLength: promptAgent?.command?.length ?? 0,
          promptLength: prompt.length,
          resolvedAgentKind: projectBoardPromptAgentKind(promptAgentId),
          selectedAgentKind: projectBoardPromptAgentKind(selectedAgentId || ""),
          startAfterCreate,
        };
        try {
          logProjectBoardDebug("projectBoard.createTicket.titleGeneration.started", {
            ...titleGenerationDebugDetails,
          });
          const generated = normalizeBeadsPayload<{ title?: string }>(
            await runBeads({
              action: "generateTitle",
              agentCommand: promptAgent?.command,
              agentId: promptAgentId,
              issueId,
              prompt,
            }),
            {},
          );
          const generatedTitle = generated.title?.trim();
          logProjectBoardDebug("projectBoard.createTicket.titleGeneration.bridgeResponse", {
            ...titleGenerationDebugDetails,
            generatedTitleLength: generatedTitle?.length ?? 0,
            hasGeneratedTitle: Boolean(generatedTitle),
          });
          if (!generatedTitle) {
            throw new Error("Prompt-agent title generation returned an empty title.");
          }
          await runBeads({
            action: "updateTitle",
            issueId,
            title: generatedTitle,
          });
          /*
           * CDXC:ProjectBoardTitleGeneration 2026-06-21-16:56:
           * Generated title completion is background polish for one card.
           * Patch the local ticket title after the durable Beads update and do not reload the full board, so a slow prompt-agent title cannot hitch Kanban scrolling, drag/drop, or follow-up ticket creation.
           */
          setLocalTicketTitle(issueId, generatedTitle);
          logProjectBoardDebug("projectBoard.createTicket.titleGeneration.completed", {
            beadId: issueId,
            generatedTitleLength: generatedTitle.length,
            startAfterCreate,
          });
        } catch (error) {
          logProjectBoardDebug("projectBoard.createTicket.titleGeneration.failed", {
            ...titleGenerationDebugDetails,
            ...projectBoardTitleGenerationFailureDetails(error),
          });
        }
      };

      if (!createdIssue?.id) {
        throw new Error("Created ticket was not found after create.");
      }

      const targetBeadsStatus = boardStatusBeadsValue(draft.status);
      const parsedPriority = Number.parseInt(draft.priority, 10);
      let pendingCreateStatusToken: number | undefined;
      if (!startAfterCreate && draft.status !== "todo") {
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
        const createdTicket = toCreatedBoardTicket(createdIssue, allIssues, displayKey);
        if (!createdTicket) {
          throw new Error("Created ticket was not available for start.");
        }
        logProjectBoardDebug("projectBoard.createTicket.startAfterCreate.requested", {
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
          if (draft.status !== "todo" && !didStartCreatedTicket) {
            await runBeads({
              action: "updateStatus",
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
              action: "setLabels",
              issueId: createdIssueId,
              labels: draft.labels,
            });
            setKnownLabels((current) => mergeKnownLabels(current, draft.labels));
          }
          await loadTickets({ mode: "background" });
        } catch (error) {
          if (
            pendingCreateStatusToken !== undefined &&
            pendingStatusMovesRef.current.get(createdIssueId)?.token === pendingCreateStatusToken
          ) {
            pendingStatusMovesRef.current.delete(createdIssueId);
          }
          const message = error instanceof Error ? error.message : "Could not finish creating the ticket.";
          setErrorMessage(message);
          showProjectBoardToast("error", "Ticket update failed", message);
          void loadTickets({ mode: "background" });
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
      logProjectBoardDebug("projectBoard.createTicket.completed", {
        beadId: createdIssueId,
        startAfterCreate,
        startLocation,
      });
    } catch (error) {
      logProjectBoardDebug("projectBoard.createTicket.failed", {
        error: error instanceof Error ? error.message : String(error),
        startAfterCreate,
        startLocation,
      });
      const message = error instanceof Error ? error.message : "Could not create the ticket.";
      setErrorMessage(message);
      showProjectBoardToast("error", "Ticket creation failed", message);
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
      await runBeads({ action: "delete", issueId: ticket.id });
      setDeleteConfirmingTicketId("");
      setTicketContextMenu(undefined);
      if (deletingFromDialog) {
        setDetail(createEmptyDetailDraft());
      }
      await loadTickets({ mode: "mutation" });
    } catch (error) {
      setTickets((current) =>
        current.some((candidate) => candidate.id === ticket.id) ? current : [...current, ticket],
      );
      setErrorMessage(error instanceof Error ? error.message : "Could not delete the ticket.");
      if (deletingFromDialog) {
        setDetail((current) => ({ ...current, isDeleting: false }));
      }
    } finally {
      if (!deletingFromDialog) {
        setContextMenuDeletingTicketId((current) => (current === ticket.id ? "" : current));
      }
    }
  };

  const startTicketWork = async (
    ticket: BoardTicket | undefined = detail.ticket,
    options: { startLocation?: ProjectBoardStartLocation } = {},
  ) => {
    if (!ticket) {
      return false;
    }
    const startLocation = options.startLocation ?? "currentProject";
    setConversationAction({ beadId: ticket.id, kind: "start" });
    logProjectBoardDebug("projectBoard.createStart.startWork.requested", {
      agentId: selectedAgentId || conversationState.defaultAgentId || "",
      beadId: ticket.id,
      displayId: ticket.displayId,
      startLocation,
    });
    try {
      const prompt = buildAgentWorkPrompt(ticket);
      const response = await sendProjectBoardRequest({
        action: "startWork",
        agentId: selectedAgentId || conversationState.defaultAgentId,
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
        throw new Error(response.error || "Could not start ticket work.");
      }
      if (response.payload) {
        setConversationState(response.payload);
      }
      const token = pendingStatusMoveSerialRef.current + 1;
      pendingStatusMoveSerialRef.current = token;
      pendingStatusMovesRef.current.set(ticket.id, {
        beadsStatus: "in_progress",
        statusKey: "in_progress",
        token,
      });
      setLocalTicketStatus(ticket.id, "in_progress", "in_progress");
      void runBeads({
        action: "updateStatus",
        issueId: ticket.id,
        status: "in_progress",
      })
        .then(() => {
          if (pendingStatusMovesRef.current.get(ticket.id)?.token !== token) {
            return;
          }
          pendingStatusMovesRef.current.delete(ticket.id);
          void loadTickets({ mode: "background" });
        })
        .catch((error) => {
          if (pendingStatusMovesRef.current.get(ticket.id)?.token !== token) {
            return;
          }
          pendingStatusMovesRef.current.delete(ticket.id);
          setErrorMessage(error instanceof Error ? error.message : "Could not move the ticket.");
          void loadTickets({ mode: "background" });
        });
      setErrorMessage("");
      logProjectBoardDebug("projectBoard.createStart.startWork.completed", {
        beadId: ticket.id,
        startLocation,
      });
      return true;
    } catch (error) {
      logProjectBoardDebug("projectBoard.createStart.startWork.failed", {
        beadId: ticket.id,
        error: error instanceof Error ? error.message : String(error),
        startLocation,
      });
      setErrorMessage(error instanceof Error ? error.message : "Could not start ticket work.");
      return false;
    } finally {
      setConversationAction((current) =>
        current?.kind === "start" && current.beadId === ticket.id ? undefined : current,
      );
    }
  };

  const associateFocusedSession = async () => {
    if (!detail.ticket) {
      return;
    }
    const ticket = detail.ticket;
    setConversationAction({ beadId: ticket.id, kind: "associate" });
    try {
      const response = await sendProjectBoardRequest({
        action: "associateFocusedSession",
        beadDisplayId: ticket.displayId,
        beadId: ticket.id,
        projectId,
        projectPath,
        ...(remoteMachineId ? { remoteMachineId } : {}),
        ticketTitle: ticket.title,
      });
      if (!response.ok) {
        throw new Error(response.error || "Could not associate the focused session.");
      }
      if (response.payload) {
        setConversationState(response.payload);
      }
      setErrorMessage("");
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : "Could not associate the focused session.");
    } finally {
      setConversationAction((current) =>
        current?.kind === "associate" && current.beadId === ticket.id ? undefined : current,
      );
    }
  };

  const jumpToConversation = async (link: ProjectBoardConversationLinkView) => {
    setConversationAction({ kind: "jump", linkId: link.id });
    try {
      const response = await sendProjectBoardRequest({
        action: "jumpToConversation",
        beadId: link.beadId,
        projectId,
        projectPath,
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: link.ghostexSessionId,
      });
      if (!response.ok) {
        throw new Error(response.error || "Could not jump to the linked conversation.");
      }
      if (response.payload) {
        setConversationState(response.payload);
      }
      setErrorMessage("");
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : "Could not jump to the linked conversation.");
    } finally {
      setConversationAction((current) =>
        current?.kind === "jump" && current.linkId === link.id ? undefined : current,
      );
    }
  };

  const unlinkConversation = async (link: ProjectBoardConversationLinkView) => {
    setConversationAction({ kind: "unlink", linkId: link.id });
    try {
      const response = await sendProjectBoardRequest({
        action: "unlinkConversation",
        beadId: link.beadId,
        projectId,
        projectPath,
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: link.ghostexSessionId,
      });
      if (!response.ok) {
        throw new Error(response.error || "Could not unlink the conversation.");
      }
      if (response.payload) {
        setConversationState(response.payload);
      }
      setErrorMessage("");
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : "Could not unlink the conversation.");
    } finally {
      setConversationAction((current) =>
        current?.kind === "unlink" && current.linkId === link.id ? undefined : current,
      );
    }
  };

  const openNewAutomationDialog = () => {
    const targetProjectId = isAutomationGlobalScope
      ? resolveAutomationDraftProjectId(
          automationState.projects,
          automationTargetProjectId,
          automationState.projectId || projectId,
      )
      : automationState.projectId;
    const targetProject = automationProjectsById.get(targetProjectId);
    void loadAutomationConversationState(targetProjectId);
    setAutomationDraft(
      createAutomationDraft({
        agentId: resolveAutomationDraftAgentId(automationState.agents, automationState.defaultAgentId),
        executionKind: (isAutomationGlobalScope ? targetProject?.canUseWorktrees : automationState.projectCanUseWorktrees)
          ? "worktree"
          : "local",
        projectId: targetProjectId,
      }),
    );
    setAutomationDialogOpen(true);
  };

  const openEditAutomationDialog = (automation: AutomationDefinition) => {
    const targetProjectId = automation.projectIds[0] || automationState.projectId || projectId;
    void loadAutomationConversationState(targetProjectId);
    setAutomationDraft(
      createAutomationDraftFromDefinition(automation, targetProjectId),
    );
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
      ? resolveAutomationDraftProjectId(automationState.projects, automationDraft.projectId, "")
      : automationDraft.projectId || automationState.projectId || projectId;
    if (!targetProjectId) {
      setErrorMessage("Choose a project before saving automation.");
      return;
    }
    const definition = createAutomationDefinitionFromDraft(automationDraft, {
      fallbackAgentId: resolveAutomationDraftAgentId(automationState.agents, automationState.defaultAgentId),
      projectId: targetProjectId,
    });
    if (!definition) {
      setErrorMessage("Name, agent, prompt, and schedule are required.");
      return;
    }
    if (definition.executionMode.kind === "worktree" && !automationDraftCanUseWorktrees) {
      setErrorMessage(automationDraftWorktreeUnavailableReason || "Worktree mode is unavailable for this project.");
      return;
    }
    setAutomationActionId(definition.id);
    try {
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: "automationSave",
        payloadJson: JSON.stringify(definition),
        projectEditorId,
        projectId: definition.projectIds[0] ?? projectId,
        projectPath: automationProjectPathForId(definition.projectIds[0] ?? projectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
      });
      if (!response.ok) {
        throw new Error(response.error || "Could not save automation.");
      }
      if (response.payload) {
        await applyAutomationMutationState(response.payload);
      }
      setAutomationDialogOpen(false);
      setErrorMessage("");
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : "Could not save automation.");
    } finally {
      setAutomationActionId("");
    }
  };

  const deleteAutomation = async (automation: AutomationDefinition) => {
    const targetProjectId = automation.projectIds[0] || automationState.projectId || projectId;
    setAutomationActionId(automation.id);
    try {
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: "automationDelete",
        projectEditorId,
        projectId: targetProjectId,
        projectPath: automationProjectPathForId(targetProjectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: automation.id,
      });
      if (!response.ok) {
        throw new Error(response.error || "Could not delete automation.");
      }
      if (response.payload) {
        await applyAutomationMutationState(response.payload);
      }
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : "Could not delete automation.");
    } finally {
      setAutomationActionId("");
    }
  };

  const setAutomationEnabled = async (automation: AutomationDefinition, enabled: boolean) => {
    const targetProjectId = automation.projectIds[0] || automationState.projectId || projectId;
    setAutomationActionId(automation.id);
    try {
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: "automationSetEnabled",
        payloadJson: JSON.stringify({ enabled }),
        projectEditorId,
        projectId: targetProjectId,
        projectPath: automationProjectPathForId(targetProjectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: automation.id,
      });
      if (!response.ok) {
        throw new Error(response.error || "Could not update automation.");
      }
      if (response.payload) {
      await applyAutomationMutationState(response.payload);
      }
      setErrorMessage("");
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : "Could not update automation.");
    } finally {
      setAutomationActionId("");
    }
  };

  const runAutomationNow = async (automation: AutomationDefinition) => {
    const targetProjectId = automation.projectIds[0] || automationState.projectId || projectId;
    setAutomationActionId(automation.id);
    try {
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: "automationRunNow",
        projectEditorId,
        projectId: targetProjectId,
        projectPath: automationProjectPathForId(targetProjectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: automation.id,
      });
      if (!response.ok) {
        throw new Error(response.error || "Could not run automation.");
      }
      if (response.payload) {
      await applyAutomationMutationState(response.payload);
      }
      setActiveSurfaceTab("runs");
      setErrorMessage("");
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : "Could not run automation.");
    } finally {
      setAutomationActionId("");
    }
  };

  const archiveAutomationRun = async (run: AutomationRun) => {
    const targetProjectId = run.projectId || automationState.projectId || projectId;
    setAutomationActionId(run.id);
    try {
      const removeWorktree =
        Boolean(run.worktree) &&
        window.confirm(
          `Archive this run and remove its worktree?\n\nPath: ${run.worktree?.path ?? ""}\nBranch: ${run.worktree?.branch ?? ""}`,
        );
      if (removeWorktree) {
        const confirmation = window.prompt(
          `Type the exact worktree path to remove it:\n\n${run.worktree?.path ?? ""}`,
        );
        if (confirmation !== run.worktree?.path) {
          setErrorMessage("Worktree removal was not confirmed. The run was not archived.");
          return;
        }
      }
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: "automationArchiveRun",
        payloadJson: JSON.stringify({ removeWorktree }),
        projectEditorId,
        projectId: targetProjectId,
        projectPath: automationProjectPathForId(targetProjectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: run.id,
      });
      if (!response.ok) {
        throw new Error(response.error || "Could not archive automation run.");
      }
      if (response.payload) {
      await applyAutomationMutationState(response.payload);
      }
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : "Could not archive automation run.");
    } finally {
      setAutomationActionId("");
    }
  };

  const markAutomationRunRead = async (run: AutomationRun) => {
    const targetProjectId = run.projectId || automationState.projectId || projectId;
    setAutomationActionId(run.id);
    try {
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: "automationMarkRunRead",
        projectEditorId,
        projectId: targetProjectId,
        projectPath: automationProjectPathForId(targetProjectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: run.id,
      });
      if (!response.ok) {
        throw new Error(response.error || "Could not mark automation run read.");
      }
      if (response.payload) {
      await applyAutomationMutationState(response.payload);
      }
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : "Could not mark automation run read.");
    } finally {
      setAutomationActionId("");
    }
  };

  const openAutomationRunSession = async (run: AutomationRun) => {
    const targetProjectId = run.projectId || automationState.projectId || projectId;
    setAutomationActionId(run.id);
    try {
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: "automationOpenRunSession",
        projectEditorId,
        projectId: targetProjectId,
        projectPath: automationProjectPathForId(targetProjectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: run.id,
      });
      if (!response.ok) {
        throw new Error(response.error || "Could not open automation session.");
      }
      if (response.payload) {
      await applyAutomationMutationState(response.payload);
      }
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : "Could not open automation session.");
    } finally {
      setAutomationActionId("");
    }
  };

  const openAutomationRunWorktree = async (run: AutomationRun) => {
    const targetProjectId = run.projectId || automationState.projectId || projectId;
    setAutomationActionId(run.id);
    try {
      const response = await sendProjectBoardRequest<ProjectAutomationsBridgeState>({
        action: "automationOpenWorktree",
        projectEditorId,
        projectId: targetProjectId,
        projectPath: automationProjectPathForId(targetProjectId),
        ...(remoteMachineId ? { remoteMachineId } : {}),
        sessionId: run.id,
      });
      if (!response.ok) {
        throw new Error(response.error || "Could not open automation worktree.");
      }
      if (response.payload) {
      await applyAutomationMutationState(response.payload);
      }
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : "Could not open automation worktree.");
    } finally {
      setAutomationActionId("");
    }
  };

  const detailConversationLinks = detail.ticket ? (linksByBeadId.get(detail.ticket.id) ?? []) : [];
  const detailPrimaryConversationLink = getPrimaryUsableConversationLink(detailConversationLinks);
  const detailCommentMetadataLink = detailPrimaryConversationLink ?? detailConversationLinks[0];
  const detailPrimaryActionLabel =
    conversationAction?.kind === "jump" && conversationAction.linkId === detailPrimaryConversationLink?.id
      ? "Opening"
      : detailPrimaryConversationLink
        ? "Go to Session"
        : conversationAction?.kind === "start" && conversationAction.beadId === detail.ticket?.id
          ? "Starting"
          : "Start work";
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
    [automationState.projects],
  );
  const automationProjectSelectItems = useMemo(
    () => automationState.projects.map((project) => ({ label: project.label, value: project.projectId })),
    [automationState.projects],
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
    [automationState.projects],
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
        })),
      ),
    [automationState.agents],
  );
  const automationScheduleSelectItems = useMemo(
    () => AUTOMATION_SCHEDULE_PRESETS.map((option) => ({ label: option.label, value: option.value })),
    [],
  );
  const automationWeekdaySelectItems = useMemo(
    () =>
      AUTOMATION_WEEKDAY_OPTIONS.map((day, index) => ({
        label: day,
        value: String(index),
      })),
    [],
  );
  const automationSessionSelectItems = useMemo(
    () =>
      automationConversationState.sessions.map((session) => ({
        label: session.label,
        value: session.sessionId,
      })),
    [automationConversationState.sessions],
  );
  const contextMenuTicket = ticketContextMenu
    ? tickets.find((ticket) => ticket.id === ticketContextMenu.ticketId)
    : undefined;
  const contextMenuPrimaryLink = contextMenuTicket
    ? getPrimaryUsableConversationLink(linksByBeadId.get(contextMenuTicket.id) ?? [])
    : undefined;
  const contextMenuPrimaryActionLabel = contextMenuPrimaryLink ? "Go to Session" : "Start work";
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
    <main className="project-board-shell">
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
      <section
        className="project-board-toolbar"
        data-surface={activeSurfaceTab === "board" ? "board" : "automations"}
      >
        <div className="project-board-toolbar-heading">
          {activeSurfaceTab !== "board" ? (
            <span className="project-automation-eyebrow">
              {automationIsExperimental
                ? isAutomationGlobalScope
                  ? "Experimental"
                  : "Automations (Experimental)"
                : "Automations"}
            </span>
          ) : null}
          <h1 className="project-board-toolbar-title">{projectName}</h1>
        </div>
        {activeSurfaceTab !== "board" && !showAutomationComingSoonOverlay ? (
          <>
            {/*
             * CDXC:Automations 2026-06-30-09:36:
             * The dedicated Automation page header needs an experimental eyebrow, project title, centered section tabs, and right-aligned refresh/create actions in one row so the page reads as one gxserver-backed control surface instead of a separate tab strip above the content.
             */}
            <nav className="project-automation-tabs" aria-label="Automation sections">
              {(["automations", "runs", "triage"] as const).map((tab) => (
                <button
                  aria-current={activeSurfaceTab === tab ? "page" : undefined}
                  className="project-automation-tab"
                  data-active={activeSurfaceTab === tab ? "true" : "false"}
                  key={tab}
                  onClick={() => setActiveSurfaceTab(tab)}
                  type="button"
                >
                  {tab === "automations" ? "Automations" : tab === "runs" ? "Runs" : "Triage"}
                </button>
              ))}
            </nav>
          </>
        ) : null}
        {!showAutomationComingSoonOverlay ? (
          <div className="project-board-toolbar-actions">
            <Button
              aria-label="Refresh project"
              disabled={loadState === "loading"}
              onClick={() => {
                if (activeSurfaceTab === "board") {
                  void loadTickets({ mode: "manual" });
                  void loadConversationState();
                }
                void loadAutomationState();
              }}
              size="icon-sm"
              variant="ghost"
            >
              <IconRefresh />
            </Button>
            {activeSurfaceTab === "board" ? (
              <Button onClick={() => openNewTicket()} size="sm" variant="secondary">
                <IconPlus data-icon="inline-start" />
                Ticket
              </Button>
            ) : (
              <Button
                className="project-automation-toolbar-button"
                onClick={openNewAutomationDialog}
                size="sm"
                variant="secondary"
              >
                <IconPlus data-icon="inline-start" />
                Automation
              </Button>
            )}
          </div>
        ) : null}
      </section>

      {showAutomationComingSoonOverlay ? (
        <AutomationComingSoonOverlay surfaceName={automationSurfaceName} />
      ) : (
        <>
          {activeSurfaceTab === "board" ? (
        <section className="project-board-filters" aria-label="Ticket filters">
          <div className="project-board-search">
            {/*
             * CDXC:SearchInputs 2026-06-04-03:11:
             * Project Board ticket search is hosted by the native tasks bundle,
             * so mirror the sidebar search affordance locally: keep the search
             * icon on the right while empty, replace it with an X button after
             * typing, and let Escape clear the focused non-empty field.
             */}
            <Input
              aria-label="Search tickets"
              onChange={(event) => setSearchQuery(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key !== "Escape" || searchQuery.length === 0) {
                  return;
                }
                event.preventDefault();
                event.stopPropagation();
                setSearchQuery("");
                searchInputRef.current?.focus();
              }}
              placeholder="Search tickets"
              ref={searchInputRef}
              value={searchQuery}
            />
            {searchQuery.length > 0 ? (
              <button
                aria-label="Clear ticket search"
                className="project-board-search-clear-button"
                onClick={() => {
                  setSearchQuery("");
                  searchInputRef.current?.focus();
                }}
                type="button"
              >
                <IconX aria-hidden="true" />
              </button>
            ) : (
              <IconSearch aria-hidden="true" className="project-board-search-icon" />
            )}
          </div>
          <Select
            items={PROJECT_BOARD_PRIORITY_FILTER_SELECT_ITEMS}
            onValueChange={(value) => setPriorityFilter(value as BoardPriorityFilter)}
            value={priorityFilter}
          >
            <SelectTrigger aria-label="Filter by priority" className="project-board-filter-select" size="sm">
              <SelectValue placeholder="All priorities" />
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
            <SelectTrigger aria-label="Filter by estimate" className="project-board-filter-select" size="sm">
              <SelectValue placeholder="All estimates" />
            </SelectTrigger>
            <SelectContent>
              {PROJECT_BOARD_ESTIMATE_FILTER_SELECT_ITEMS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </section>
      ) : null}

      {activeSurfaceTab === "triage" ? (
        triageAutomationRuns.length === 0 ? (
          /*
           * CDXC:Automations 2026-06-30-15:35:
           * Empty Runs and Triage tabs should show one centered empty state in a single panel, not the split view with a second "No run selected" placeholder on the right. Match the Automations tab pattern.
           */
          <section className="project-automation-panel">
            <AutomationRunList
              actionId={automationActionId}
              agents={automationState.agents}
              automations={automationState.automations}
              emptyTitle="No automation results need triage"
              onArchive={archiveAutomationRun}
              onMarkRead={markAutomationRunRead}
              onOpenSession={openAutomationRunSession}
              onOpenWorktree={openAutomationRunWorktree}
              onSelect={setSelectedAutomationRunId}
              projectName={automationState.projectName}
              runs={triageAutomationRuns}
              selectedRunId={selectedTriageRun?.id ?? ""}
            />
          </section>
        ) : (
        <section className="project-automation-split">
          <AutomationRunList
            actionId={automationActionId}
            agents={automationState.agents}
            automations={automationState.automations}
            emptyTitle="No automation results need triage"
            onArchive={archiveAutomationRun}
            onMarkRead={markAutomationRunRead}
            onOpenSession={openAutomationRunSession}
            onOpenWorktree={openAutomationRunWorktree}
            onSelect={setSelectedAutomationRunId}
            projectName={automationState.projectName}
            runs={triageAutomationRuns}
            selectedRunId={selectedTriageRun?.id ?? ""}
          />
          <AutomationRunDetail
            actionId={automationActionId}
            agents={automationState.agents}
            automation={selectedTriageRun ? automationState.automations.find((candidate) => candidate.id === selectedTriageRun.automationId) : undefined}
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

      {activeSurfaceTab === "automations" ? (
        automationState.automations.length === 0 ? (
          <section className="project-automation-panel">
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
              selectedAutomationId={selectedAutomation?.id ?? ""}
              showProjectLabels={isAutomationGlobalScope}
            />
          </section>
        ) : (
          <section className="project-automation-split">
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
            selectedAutomationId={selectedAutomation?.id ?? ""}
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

      {activeSurfaceTab === "runs" ? (
        visibleAutomationRuns.length === 0 ? (
          <section className="project-automation-panel">
            <AutomationRunList
              actionId={automationActionId}
              agents={automationState.agents}
              automations={automationState.automations}
              emptyTitle="No automation runs yet"
              onArchive={archiveAutomationRun}
              onMarkRead={markAutomationRunRead}
              onOpenSession={openAutomationRunSession}
              onOpenWorktree={openAutomationRunWorktree}
              onSelect={setSelectedAutomationRunId}
              projectName={automationState.projectName}
              runs={visibleAutomationRuns}
              selectedRunId={selectedVisibleRun?.id ?? ""}
            />
          </section>
        ) : (
        <section className="project-automation-split">
          <AutomationRunList
            actionId={automationActionId}
            agents={automationState.agents}
            automations={automationState.automations}
            emptyTitle="No automation runs yet"
            onArchive={archiveAutomationRun}
            onMarkRead={markAutomationRunRead}
            onOpenSession={openAutomationRunSession}
            onOpenWorktree={openAutomationRunWorktree}
            onSelect={setSelectedAutomationRunId}
            projectName={automationState.projectName}
            runs={visibleAutomationRuns}
            selectedRunId={selectedVisibleRun?.id ?? ""}
          />
          <AutomationRunDetail
            actionId={automationActionId}
            agents={automationState.agents}
            automation={selectedVisibleRun ? automationState.automations.find((candidate) => candidate.id === selectedVisibleRun.automationId) : undefined}
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

      {activeSurfaceTab === "board" ? (
        <>
          {errorMessage ? <ProjectBoardNotice message={errorMessage} /> : null}
          <div className="project-board-board-region">
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
              <section className="project-board-lanes horizontal-scroll-fade-mask" aria-label="Project issue board">
                {BOARD_COLUMNS.map((column) => (
                  <BoardLane
                    column={column}
                    conversationAction={conversationAction}
                    key={column.key}
                    linksByBeadId={linksByBeadId}
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
                aria-label="Loading board"
                aria-live="polite"
                className="project-board-loading-overlay"
                role="status"
              >
                <IconLoader2
                  aria-hidden="true"
                  className="project-board-loading-spinner"
                  size={32}
                  stroke={1.8}
                />
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
                    current?.ticketId === contextMenuTicket.id
                      ? { ...current, confirmingDelete: true }
                      : current,
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
        <ProjectBoardNotice message={errorMessage} />
      ) : null}
        </>
      )}

      {experimentalFeaturesEnabled ? (
      <Dialog open={automationDialogOpen} onOpenChange={setAutomationDialogOpen}>
        <DialogContent className="project-ticket-dialog project-automation-dialog">
          <DialogHeader>
            <DialogTitle>{automationDraft.id ? "Edit automation" : "Create automation"}</DialogTitle>
            <DialogDescription>
              {isAutomationGlobalScope ? "Schedule recurring agent work for a selected project." : `Schedule recurring agent work for ${projectName}.`}
            </DialogDescription>
          </DialogHeader>
          <div className="project-ticket-dialog-body project-automation-form vertical-scroll-fade-mask">
            {/*
             * CDXC:ProjectAutomations 2026-06-09-10:30:
             * Automation setup is scoped to the Project board's current project, so the create/edit dialog drops project switching and keeps dropdown widths aligned at 250px for agent, schedule, weekday, and thread-session fields.
             *
             * CDXC:Automations 2026-06-30-11:05:
             * The Quick-level global Automations page shows all projects, so its create/edit dialog restores a Project selector. Project-scoped Automate pages keep the original no-project-switch form.
             */}
            <label className="project-automation-field-full">
              <span>Name</span>
              <Input
                onChange={(event) => {
                  const name = event.currentTarget.value;
                  setAutomationDraft((current) => ({ ...current, name }));
                }}
                value={automationDraft.name}
              />
            </label>
            <div className="project-automation-form-grid">
              {isAutomationGlobalScope ? (
                <label>
                  <span>Project</span>
                  <Select
                    items={automationProjectSelectItems}
                    onValueChange={(value) => {
                      const selectedProject = automationProjectsById.get(value);
                      setAutomationDraft((current) => ({
                        ...current,
                        executionKind:
                          current.executionKind === "worktree" && selectedProject?.canUseWorktrees !== true
                            ? "local"
                            : current.executionKind,
                        projectId: value,
                        threadSessionId: "",
                      }));
                      void loadAutomationConversationState(value);
                    }}
                    value={automationDraft.projectId}
                  >
                    <SelectTrigger className="project-automation-select">
                      <SelectValue placeholder="Choose project" />
                    </SelectTrigger>
                    <SelectContent>
                      {automationState.projects.map((project) => (
                        <SelectItem key={project.projectId} value={project.projectId}>
                          {project.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </label>
              ) : null}
              <label>
                <span>Agent</span>
                <Select
                  disabled={automationState.agents.length === 0}
                  items={automationAgentSelectItems}
                  onValueChange={(value) =>
                    setAutomationDraft((current) => ({ ...current, agentId: value }))
                  }
                  value={automationDraft.agentId}
                >
                  <SelectTrigger className="project-automation-select">
                    <SelectValue placeholder={automationState.agents.length === 0 ? "No agents configured" : "Choose agent"} />
                  </SelectTrigger>
                  <SelectContent>
                    {automationState.agents.map((agent) => (
                      <SelectItem key={agent.agentId} value={agent.agentId}>
                        <AutomationAgentOptionLabel agent={agent} />
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </label>
              <label>
                <span>Schedule</span>
                <Select
                  items={automationScheduleSelectItems}
                  onValueChange={(value) =>
                    setAutomationDraft((current) => ({
                      ...current,
                      schedulePreset: value as AutomationDraft["schedulePreset"],
                    }))
                  }
                  value={automationDraft.schedulePreset}
                >
                  <SelectTrigger className="project-automation-select">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {AUTOMATION_SCHEDULE_PRESETS.map((option) => (
                      <SelectItem key={option.value} value={option.value}>
                        {option.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </label>
              {automationDraft.schedulePreset === "weekly" ? (
                <label>
                  <span>Day</span>
                  <Select
                    items={automationWeekdaySelectItems}
                    onValueChange={(value) =>
                      setAutomationDraft((current) => ({ ...current, weeklyDay: value }))
                    }
                    value={automationDraft.weeklyDay}
                  >
                    <SelectTrigger className="project-automation-select">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {AUTOMATION_WEEKDAY_OPTIONS.map((day, index) => (
                        <SelectItem key={day} value={String(index)}>
                          {day}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </label>
              ) : null}
              {automationDraft.schedulePreset === "daily" ||
              automationDraft.schedulePreset === "weekly" ||
              automationDraft.schedulePreset === "weekdays" ? (
                <label>
                  <span>Time</span>
                  <Input
                    className="project-automation-select"
                    onChange={(event) => {
                      const scheduleTime = event.currentTarget.value;
                      setAutomationDraft((current) => ({
                        ...current,
                        scheduleTime,
                      }));
                    }}
                    type="time"
                    value={automationDraft.scheduleTime}
                  />
                </label>
              ) : null}
            </div>
            {automationDraft.schedulePreset === "cron" ? (
              <label className="project-automation-field-full">
                <span>Cron</span>
                <Input
                  onChange={(event) => {
                    const cronExpression = event.currentTarget.value;
                    setAutomationDraft((current) => ({
                      ...current,
                      cronExpression,
                    }));
                  }}
                  placeholder="*/15 * * * *"
                  value={automationDraft.cronExpression}
                />
              </label>
            ) : null}
            <div className="project-automation-form-section">
              <div className="project-automation-form-section-title">Execution</div>
            <div className="project-automation-segmented" role="group" aria-label="Execution mode">
              {[
                ["worktree", "Worktree"],
                ["local", "Local"],
                ["thread", "Thread"],
              ].map(([value, label]) => {
                const disabled = value === "worktree" && !automationDraftCanUseWorktrees;
                return (
                  <button
                    data-active={automationDraft.executionKind === value}
                    disabled={disabled}
                    key={value}
                    onClick={() =>
                      setAutomationDraft((current) => ({
                        ...current,
                        executionKind: value as AutomationExecutionMode["kind"],
                      }))
                    }
                    type="button"
                  >
                    {label}
                  </button>
                );
              })}
            </div>
            {!automationDraftCanUseWorktrees && automationDraftWorktreeUnavailableReason ? (
              <p className="project-automation-inline-note">{automationDraftWorktreeUnavailableReason}</p>
            ) : null}
            {automationDraft.executionKind === "worktree" ? (
              <label>
                <span>Setup command</span>
                <Input
                  onChange={(event) => {
                    const setupCommand = event.currentTarget.value;
                    setAutomationDraft((current) => ({
                      ...current,
                      setupCommand,
                    }));
                  }}
                  placeholder="Use project worktree command"
                  value={automationDraft.setupCommand}
                />
              </label>
            ) : null}
            {automationDraft.executionKind === "thread" ? (
              <div className="project-automation-form-grid">
                <label>
                  <span>Session</span>
                  <Select
                    items={automationSessionSelectItems}
                    onValueChange={(value) =>
                      setAutomationDraft((current) => ({ ...current, threadSessionId: value }))
                    }
                    value={automationDraft.threadSessionId}
                  >
                    <SelectTrigger className="project-automation-select">
                      <SelectValue placeholder="Choose session" />
                    </SelectTrigger>
                    <SelectContent>
                      {automationConversationState.sessions.map((session) => (
                        <SelectItem key={session.sessionId} value={session.sessionId}>
                          {session.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </label>
                <label>
                  <span>Expires</span>
                  <Input
                    className="project-automation-select"
                    onChange={(event) => {
                      const expiresAt = event.currentTarget.value;
                      setAutomationDraft((current) => ({
                        ...current,
                        expiresAt,
                      }));
                    }}
                    type="datetime-local"
                    value={automationDraft.expiresAt}
                  />
                </label>
              </div>
            ) : null}
            </div>
            <label className="project-automation-prompt-field">
              <span>Prompt</span>
              <Textarea
                onChange={(event) => {
                  const prompt = event.currentTarget.value;
                  setAutomationDraft((current) => ({ ...current, prompt }));
                }}
                value={automationDraft.prompt}
              />
            </label>
            <div className="project-automation-enabled">
              <Switch
                checked={automationDraft.enabled}
                onCheckedChange={(enabled: boolean) => {
                  setAutomationDraft((current) => ({ ...current, enabled }));
                }}
                size="sm"
              />
              <span>Enabled</span>
            </div>
          </div>
          <DialogFooter className="project-ticket-dialog-footer">
            <Button onClick={() => setAutomationDialogOpen(false)} type="button" variant="ghost">
              Cancel
            </Button>
            <Button disabled={Boolean(automationActionId)} onClick={saveAutomation} type="button">
              Save
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      ) : null}

      <Dialog
        open={Boolean(detail.ticket)}
        onOpenChange={(open) => {
          if (!open) {
            setDeleteConfirmingTicketId("");
            setDetail(createEmptyDetailDraft());
          }
        }}
      >
        <DialogContent className="project-ticket-dialog">
          <DialogHeader>
            <DialogTitle>Edit ticket</DialogTitle>
            <DialogDescription>
              {detail.ticket?.displayId} · {detail.ticket?.id}
            </DialogDescription>
          </DialogHeader>
          <div
            className="project-ticket-dialog-body vertical-scroll-fade-mask"
            onKeyDown={(event) => handleCmdEnter(event, () => void saveTicketDetail())}
          >
            <TicketMetaFields
              blockedByIds={detail.blockedByIds}
              blockingIds={detail.blockingIds}
              knownLabels={knownLabels}
              labels={detail.labels}
              onBlockedByChange={(blockedByIds) =>
                setDetail((current) => ({ ...current, blockedByIds }))
              }
              onBlockingChange={(blockingIds) =>
                setDetail((current) => ({ ...current, blockingIds }))
              }
              onLabelsChange={(labels) => setDetail((current) => ({ ...current, labels }))}
              onPriorityChange={(priority) => setDetail((current) => ({ ...current, priority }))}
              onStatusChange={(status) => setDetail((current) => ({ ...current, status }))}
              onTshirtChange={(tshirt) => setDetail((current) => ({ ...current, tshirt }))}
              priority={detail.priority}
              status={detail.status}
              ticketOptions={ticketOptions.filter((option) => option.id !== detail.ticket?.id)}
              tshirt={detail.tshirt}
            />
            <label className="project-ticket-field">
              <span>Title</span>
              <Input
                className="project-ticket-title-input"
                onChange={(event) => {
                  const title = event.currentTarget.value;
                  setDetail((current) => ({ ...current, title }));
                }}
                value={detail.title}
              />
            </label>
            <label className="project-ticket-field">
              <span>Prompt</span>
              <Textarea
                className="project-ticket-prompt-input"
                onChange={(event) => {
                  const description = event.currentTarget.value;
                  setDetail((current) => ({
                    ...current,
                    description,
                  }));
                }}
                onPaste={(event) => {
                  if (!hasProjectBoardImagePastePayload(event.clipboardData)) {
                    return;
                  }
                  event.preventDefault();
                  const selectionStart = event.currentTarget.selectionStart;
                  const selectionEnd = event.currentTarget.selectionEnd;
                  void sendProjectBoardImageRequest({ action: "pasteImage" }).then((response) => {
                    if (!response.imagePath) {
                      setErrorMessage(response.error || "Clipboard image could not be converted to a path.");
                      return;
                    }
                    setDetail((current) => ({
                      ...current,
                      description: appendImageMarkdownToDescription(
                        current.description,
                        response.imagePath ?? "",
                        selectionStart,
                        selectionEnd,
                      ),
                    }));
                  }).catch((error) => {
                    setErrorMessage(error instanceof Error ? error.message : "Clipboard image paste failed.");
                  });
                }}
                placeholder="Write the full prompt for this ticket."
                value={detail.description}
              />
            </label>
            <ImagePreviewStrip
              description={detail.description}
              imagePreviewDataUrls={imagePreviewDataUrls}
              onRemove={(image) =>
                setDetail((current) => ({
                  ...current,
                  description: removeDescriptionImageReference(current.description, image.id),
                }))
              }
            />
            <DependencySummary
              blockedByIds={detail.blockedByIds}
              blockingIds={detail.blockingIds}
              tickets={tickets}
            />
            {detail.ticket ? (
              <ConversationSection
                agents={conversationState.agents}
                action={conversationAction}
                focusedSessionId={conversationState.focusedTerminalSessionId}
                links={detailConversationLinks}
                onAssociateFocusedSession={() => void associateFocusedSession()}
                onJumpToConversation={(link) => void jumpToConversation(link)}
                onSelectedAgentChange={setSelectedAgentId}
                onUnlinkConversation={(link) => void unlinkConversation(link)}
                selectedAgentId={selectedAgentId}
              />
            ) : null}
            <section className="project-ticket-comments" aria-label="Comments">
              <div className="project-ticket-section-title">Comments</div>
              <ScrollArea className="project-ticket-comment-list">
                {detail.ticket?.comments?.length ? (
                  detail.ticket.comments.map((comment, index) => {
                    const parsedComment = parseProjectBoardCommentText(comment.text);
                    const fallbackMetadata = projectBoardCommentMetadataFromLink(detailCommentMetadataLink);
                    const agentName = parsedComment.agentName ?? fallbackMetadata.agentName;
                    const sessionId = parsedComment.sessionId ?? fallbackMetadata.sessionId;
                    const createdAtLabel = formatShortDate(comment.created_at);
                    return (
                      <article className="project-ticket-comment" key={`${comment.created_at}-${index}`}>
                        <div className="project-ticket-comment-header">
                          <div className="project-ticket-comment-author-row">
                            <strong className="project-ticket-comment-author">
                              {comment.author || "Comment"}
                            </strong>
                            {agentName ? (
                              <span className="project-ticket-comment-agent">({agentName})</span>
                            ) : null}
                          </div>
                          {createdAtLabel ? (
                            <time dateTime={comment.created_at} className="project-ticket-comment-date">
                              {createdAtLabel}
                            </time>
                          ) : null}
                        </div>
                        <p>{parsedComment.body || comment.text}</p>
                        {sessionId ? (
                          <footer className="project-ticket-comment-session">
                            <span>Session</span>
                            <code>{sessionId}</code>
                          </footer>
                        ) : null}
                      </article>
                    );
                  })
                ) : (
                  <p className="project-ticket-empty">No comments yet.</p>
                )}
              </ScrollArea>
            </section>
            <label className="project-ticket-field">
              <span>Add comment</span>
              <Textarea
                onChange={(event) => {
                  const comment = event.currentTarget.value;
                  setDetail((current) => ({ ...current, comment }));
                }}
                placeholder="Add a note for the team."
                value={detail.comment}
              />
            </label>
          </div>
          <DialogFooter className="project-ticket-dialog-footer">
            <Button
              disabled={detail.isDeleting || detail.isSaving}
              onClick={() => {
                if (deleteConfirmingTicketId === detail.ticket?.id) {
                  void deleteTicket();
                  return;
                }
                setDeleteConfirmingTicketId(detail.ticket?.id ?? "");
              }}
              type="button"
              variant="destructive"
            >
              <IconTrash data-icon="inline-start" />
              {deleteConfirmingTicketId === detail.ticket?.id
                ? detail.isDeleting
                  ? "Deleting"
                  : "Confirm delete"
                : "Delete"}
            </Button>
            <div className="project-ticket-dialog-primary-actions">
              <Button
                disabled={detailPrimaryActionDisabled}
                onClick={() => {
                  if (detailPrimaryConversationLink) {
                    void jumpToConversation(detailPrimaryConversationLink);
                    return;
                  }
                  setDeleteConfirmingTicketId("");
                  setDetail(createEmptyDetailDraft());
                  void startTicketWork();
                }}
                type="button"
                variant="outline"
              >
                {detailPrimaryConversationLink ? (
                  <IconExternalLink data-icon="inline-start" />
                ) : (
                  <IconLink data-icon="inline-start" />
                )}
                {detailPrimaryActionLabel}
              </Button>
              <Button disabled={detail.isDeleting || detail.isSaving} onClick={() => void saveTicketDetail()}>
                {detail.isSaving ? "Saving" : "Save"}
              </Button>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={newTicketOpen}
        onOpenChange={(open) => {
          setNewTicketOpen(open);
          if (!open) {
            setNewTicketStartLocation("currentProject");
          }
        }}
      >
        <DialogContent className="project-ticket-dialog">
          <DialogHeader>
            <DialogTitle>New Ticket</DialogTitle>
            <DialogDescription>
              Leave the title empty to auto-generate it from the prompt. Creates in{" "}
              {boardStatusLabel(newTicket.status)}.
            </DialogDescription>
          </DialogHeader>
          <div
            className="project-ticket-dialog-body vertical-scroll-fade-mask"
            onKeyDown={(event) => handleCmdEnter(event, () => void createTicket())}
          >
            <TicketMetaFields
              blockedByIds={newTicket.blockedByIds}
              blockingIds={newTicket.blockingIds}
              knownLabels={knownLabels}
              labels={newTicket.labels}
              onBlockedByChange={(blockedByIds) =>
                setNewTicket((current) => ({ ...current, blockedByIds }))
              }
              onBlockingChange={(blockingIds) =>
                setNewTicket((current) => ({ ...current, blockingIds }))
              }
              onLabelsChange={(labels) => setNewTicket((current) => ({ ...current, labels }))}
              onPriorityChange={(priority) => setNewTicket((current) => ({ ...current, priority }))}
              onStatusChange={() => undefined}
              onTshirtChange={(tshirt) => setNewTicket((current) => ({ ...current, tshirt }))}
              priority={newTicket.priority}
              status="todo"
              showStatus={false}
              ticketOptions={ticketOptions}
              tshirt={newTicket.tshirt}
            />
            <label className="project-ticket-field">
              <span>Title</span>
              <Input
                className="project-ticket-title-input"
                onChange={(event) => {
                  const title = event.currentTarget.value;
                  setNewTicket((current) => ({ ...current, title }));
                }}
                placeholder="Auto-generated from prompt when left empty"
                value={newTicket.title}
              />
            </label>
            <label className="project-ticket-field">
              <span>Prompt</span>
              <Textarea
                className="project-ticket-prompt-input"
                onChange={(event) => {
                  const description = event.currentTarget.value;
                  setNewTicket((current) => ({
                    ...current,
                    description,
                  }));
                }}
                onPaste={(event) => {
                  if (!hasProjectBoardImagePastePayload(event.clipboardData)) {
                    return;
                  }
                  event.preventDefault();
                  const selectionStart = event.currentTarget.selectionStart;
                  const selectionEnd = event.currentTarget.selectionEnd;
                  void sendProjectBoardImageRequest({ action: "pasteImage" }).then((response) => {
                    if (!response.imagePath) {
                      setErrorMessage(response.error || "Clipboard image could not be converted to a path.");
                      return;
                    }
                    setNewTicket((current) => ({
                      ...current,
                      description: appendImageMarkdownToDescription(
                        current.description,
                        response.imagePath ?? "",
                        selectionStart,
                        selectionEnd,
                      ),
                    }));
                  }).catch((error) => {
                    setErrorMessage(error instanceof Error ? error.message : "Clipboard image paste failed.");
                  });
                }}
                placeholder="Write the full prompt for this ticket."
                ref={newPromptRef}
                value={newTicket.description}
              />
            </label>
            <ImagePreviewStrip
              description={newTicket.description}
              imagePreviewDataUrls={imagePreviewDataUrls}
              onRemove={(image) =>
                setNewTicket((current) => ({
                  ...current,
                  description: removeDescriptionImageReference(current.description, image.id),
                }))
              }
            />
          </div>
          <DialogFooter className="project-ticket-create-footer">
            <section className="project-ticket-create-start" aria-label="Create and start options">
              <div className="project-ticket-section-title">Start work</div>
              <div className="project-ticket-create-start-controls">
                <Select
                  disabled={conversationState.agents.length === 0}
                  items={agentSelectItems}
                  onValueChange={setSelectedAgentId}
                  value={selectedAgentId}
                >
                  <SelectTrigger
                    aria-label="Agent for Create and Start"
                    className="project-ticket-footer-select"
                    size="sm"
                  >
                    <SelectValue placeholder="Choose agent" />
                  </SelectTrigger>
                  <SelectContent>
                    {conversationState.agents.map((agent) => (
                      <SelectItem key={agent.agentId} value={agent.agentId}>
                        {agent.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <Select
                  items={PROJECT_BOARD_START_LOCATION_SELECT_ITEMS}
                  onValueChange={(value) =>
                    setNewTicketStartLocation(value as ProjectBoardStartLocation)
                  }
                  value={newTicketStartLocation}
                >
                  <SelectTrigger
                    aria-label="Start location"
                    className="project-ticket-footer-select"
                    size="sm"
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {PROJECT_BOARD_START_LOCATION_SELECT_ITEMS.map((option) => (
                      <SelectItem key={option.value} value={option.value}>
                        {option.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </section>
            <div className="project-ticket-create-actions">
              <Button
                disabled={!newTicket.description.trim()}
                onClick={() => void createTicket()}
                type="button"
                variant="outline"
              >
                Create
              </Button>
              <Button
                disabled={
                  !newTicket.description.trim() ||
                  conversationState.agents.length === 0 ||
                  Boolean(conversationAction)
                }
                onClick={() => void createTicket({ startAfterCreate: true })}
                type="button"
              >
                <IconLink data-icon="inline-start" />
                Create & Start
              </Button>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </main>
  );
}

function TicketMetaFields({
  blockedByIds,
  blockingIds,
  knownLabels,
  labels,
  onBlockedByChange,
  onBlockingChange,
  onLabelsChange,
  onPriorityChange,
  onStatusChange,
  onTshirtChange,
  priority,
  showStatus = true,
  status,
  ticketOptions,
  tshirt,
}: {
  blockedByIds: string[];
  blockingIds: string[];
  knownLabels: string[];
  labels: string[];
  onBlockedByChange: (ids: string[]) => void;
  onBlockingChange: (ids: string[]) => void;
  onLabelsChange: (labels: string[]) => void;
  onPriorityChange: (priority: string) => void;
  onStatusChange: (status: BoardStatusKey) => void;
  onTshirtChange: (size: TshirtSize | undefined) => void;
  priority: string;
  showStatus?: boolean;
  status: BoardStatusKey;
  ticketOptions: Array<{ id: string; label: string }>;
  tshirt?: TshirtSize;
}) {
  const [labelDraft, setLabelDraft] = useState("");
  const labelSuggestions = knownLabels.filter((label) => !labels.includes(label));

  return (
    <div className="project-ticket-meta-grid">
      {showStatus ? (
        <label className="project-ticket-field project-ticket-field-inline">
          <span>Status</span>
          <Select
            items={PROJECT_BOARD_STATUS_SELECT_ITEMS}
            onValueChange={(value) => onStatusChange(value as BoardStatusKey)}
            value={status}
          >
            <SelectTrigger size="sm">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {BOARD_COLUMNS.map((column) => (
                <SelectItem key={column.key} value={column.key}>
                  {column.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </label>
      ) : null}
      <label className="project-ticket-field project-ticket-field-inline">
        <span>Priority</span>
        <Select
          items={PROJECT_BOARD_PRIORITY_SELECT_ITEMS}
          onValueChange={onPriorityChange}
          value={priority}
        >
          <SelectTrigger size="sm">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {PRIORITY_OPTIONS.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </label>
      <label className="project-ticket-field project-ticket-field-inline">
        <span>T-shirt</span>
        <Select
          items={PROJECT_BOARD_TSHIRT_SELECT_ITEMS}
          onValueChange={(value) => onTshirtChange(value === "none" ? undefined : (value as TshirtSize))}
          value={tshirt ?? "none"}
        >
          <SelectTrigger size="sm">
            <SelectValue placeholder="None" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="none">None</SelectItem>
            {TSHIRT_OPTIONS.map((option) => (
              <SelectItem key={option.label} value={option.label}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </label>
      {/*
        CDXC:ProjectBoardTicketMetadata 2026-05-30-08:31:
        Ticket metadata should put Labels where Blocked by was, keep every metadata control's label-to-element spacing consistent, and show T-shirt select values as friendly labels.
      */}
      <div className="project-ticket-field project-ticket-field-inline project-ticket-labels-field">
        <span>Labels</span>
        {labels.length > 0 ? (
          <div className="project-ticket-label-list">
            {labels.map((label) => (
              <button
                className="project-ticket-label-chip"
                key={label}
                onClick={() => onLabelsChange(labels.filter((candidate) => candidate !== label))}
                type="button"
              >
                {label}
                <IconX aria-hidden="true" />
              </button>
            ))}
          </div>
        ) : null}
        <div className="project-ticket-label-editor">
          <Input
            aria-label="Add label"
            list="project-board-label-suggestions"
            onChange={(event) => setLabelDraft(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                const next = labelDraft.trim();
                if (next && !labels.includes(next)) {
                  onLabelsChange([...labels, next]);
                }
                setLabelDraft("");
              }
            }}
            placeholder="Add label"
            value={labelDraft}
          />
          <datalist id="project-board-label-suggestions">
            {labelSuggestions.map((label) => (
              <option key={label} value={label} />
            ))}
          </datalist>
          <Button
            onClick={() => {
              const next = labelDraft.trim();
              if (next && !labels.includes(next)) {
                onLabelsChange([...labels, next]);
              }
              setLabelDraft("");
            }}
            size="sm"
            type="button"
            variant="outline"
          >
            Add
          </Button>
        </div>
      </div>
      <DependencyPicker
        label="Blocking"
        onChange={onBlockingChange}
        selectedIds={blockingIds}
        ticketOptions={ticketOptions}
      />
      <DependencyPicker
        label="Blocked by"
        onChange={onBlockedByChange}
        selectedIds={blockedByIds}
        ticketOptions={ticketOptions}
      />
    </div>
  );
}

function DependencyPicker({
  label,
  onChange,
  selectedIds,
  ticketOptions,
}: {
  label: string;
  onChange: (ids: string[]) => void;
  selectedIds: string[];
  ticketOptions: Array<{ id: string; label: string }>;
}) {
  const [draft, setDraft] = useState("");
  const available = ticketOptions.filter((option) => !selectedIds.includes(option.id));
  return (
    <div className="project-ticket-field project-ticket-field-inline">
      <span>{label}</span>
      {selectedIds.length > 0 ? (
        <div className="project-ticket-label-list">
          {selectedIds.map((id) => {
            const ticket = ticketOptions.find((option) => option.id === id);
            return (
              <button
                className="project-ticket-label-chip"
                key={id}
                onClick={() => onChange(selectedIds.filter((candidate) => candidate !== id))}
                type="button"
              >
                {ticket?.label ?? id}
                <IconX aria-hidden="true" />
              </button>
            );
          })}
        </div>
      ) : null}
      <Select
        onValueChange={(value) => {
          if (value && !selectedIds.includes(value)) {
            onChange([...selectedIds, value]);
          }
          setDraft("");
        }}
        value={draft}
      >
        <SelectTrigger size="sm">
          <SelectValue placeholder={`Add ${label.toLowerCase()} ticket`} />
        </SelectTrigger>
        <SelectContent>
          {available.map((option) => (
            <SelectItem key={option.id} value={option.id}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

function DependencySummary({
  blockedByIds,
  blockingIds,
  tickets,
}: {
  blockedByIds: string[];
  blockingIds: string[];
  tickets: BoardTicket[];
}) {
  if (blockedByIds.length === 0 && blockingIds.length === 0) {
    return null;
  }
  const labelFor = (id: string) => tickets.find((ticket) => ticket.id === id)?.displayId ?? id;
  return (
    <div className="project-ticket-dependencies">
      {blockedByIds.length > 0 ? (
        <p>
          <strong>Blocked by:</strong> {blockedByIds.map(labelFor).join(", ")}
        </p>
      ) : null}
      {blockingIds.length > 0 ? (
        <p>
          <strong>Blocking:</strong> {blockingIds.map(labelFor).join(", ")}
        </p>
      ) : null}
    </div>
  );
}

function ImagePreviewStrip({
  description,
  imagePreviewDataUrls,
  onRemove,
}: {
  description: string;
  imagePreviewDataUrls: Record<string, string>;
  onRemove?: (image: DescriptionImageReference) => void;
}) {
  const [openImage, setOpenImage] = useState<DescriptionImageReference | undefined>();
  const images = extractPreviewableDescriptionImageReferences(description);
  const openPreviewSrc = openImage ? imagePreviewDataUrls[openImage.src] : undefined;

  useEffect(() => {
    if (!openImage) {
      return;
    }
    if (!images.some((image) => image.id === openImage.id)) {
      setOpenImage(undefined);
      return;
    }
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpenImage(undefined);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [images, openImage]);

  if (images.length === 0) {
    return null;
  }

  return (
    <>
      <div className="project-ticket-image-strip" aria-label="Image previews">
        {images.map((image) => {
          const previewSrc = imagePreviewDataUrls[image.src];
          return (
            <div
              aria-label={previewSrc ? `Open image preview ${image.src}` : undefined}
              className="project-ticket-image-thumb"
              key={image.id}
              onClick={() => {
                if (previewSrc) {
                  setOpenImage(image);
                }
              }}
              onKeyDown={(event) => {
                if (!previewSrc) {
                  return;
                }
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  setOpenImage(image);
                }
              }}
              role={previewSrc ? "button" : undefined}
              tabIndex={previewSrc ? 0 : undefined}
            >
              {previewSrc ? <img alt="" src={previewSrc} /> : <span aria-hidden="true" />}
              {onRemove ? (
                <button
                  aria-label="Remove pasted image"
                  className="project-ticket-image-remove"
                  onClick={(event) => {
                    event.stopPropagation();
                    onRemove(image);
                    if (openImage?.id === image.id) {
                      setOpenImage(undefined);
                    }
                  }}
                  type="button"
                >
                  <IconX aria-hidden="true" />
                </button>
              ) : null}
            </div>
          );
        })}
      </div>
      {openImage && openPreviewSrc
        ? createPortal(
            <div
              className="project-ticket-image-popup"
              onClick={() => setOpenImage(undefined)}
              role="presentation"
            >
              <img alt="" src={openPreviewSrc} />
            </div>,
            document.body,
          )
        : null}
    </>
  );
}

function ConversationSection({
  action,
  agents,
  focusedSessionId,
  links,
  onAssociateFocusedSession,
  onJumpToConversation,
  onSelectedAgentChange,
  onUnlinkConversation,
  selectedAgentId,
}: {
  action: ConversationActionState;
  agents: ProjectBoardAgentOption[];
  focusedSessionId?: string;
  links: ProjectBoardConversationLinkView[];
  onAssociateFocusedSession: () => void;
  onJumpToConversation: (link: ProjectBoardConversationLinkView) => void;
  onSelectedAgentChange: (agentId: string) => void;
  onUnlinkConversation: (link: ProjectBoardConversationLinkView) => void;
  selectedAgentId: string;
}) {
  const isAssociating = action?.kind === "associate";
  const hasActiveConversationAction = Boolean(action);
  const agentSelectItems = useMemo(
    () =>
      agents.map((agent) => ({
        label: agent.label,
        value: agent.agentId,
      })),
    [agents],
  );
  return (
    <section className="project-ticket-conversations" aria-label="Linked conversations">
      <div className="project-ticket-section-title">Conversation</div>
      <div className="project-ticket-conversation-controls">
        <Select
          disabled={agents.length === 0}
          items={agentSelectItems}
          onValueChange={onSelectedAgentChange}
          value={selectedAgentId}
        >
          <SelectTrigger aria-label="Agent for Start work" size="sm">
            <SelectValue placeholder="Choose agent" />
          </SelectTrigger>
          <SelectContent>
            {agents.map((agent) => (
              <SelectItem key={agent.agentId} value={agent.agentId}>
                {agent.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          disabled={!focusedSessionId || hasActiveConversationAction}
          onClick={onAssociateFocusedSession}
          size="sm"
          type="button"
          variant="outline"
        >
          <IconLink data-icon="inline-start" />
          {isAssociating ? "Associating" : "Associate focused"}
        </Button>
      </div>
      {links.length > 0 ? (
        <TooltipProvider delayDuration={TOOLTIP_DELAY_MS}>
          <div className="project-ticket-conversation-list">
            {links.map((link) => {
              const label = conversationLinkLabel(link);
              return (
                <div className="project-ticket-conversation-row" key={link.id}>
                  <div className="project-ticket-conversation-main">
                    <ConversationLinkName
                      className="project-ticket-conversation-name"
                      label={label}
                    />
                    <span className="project-ticket-conversation-status">
                      {conversationLinkStatusText(link)}
                    </span>
                  </div>
                  <div className="project-ticket-conversation-actions">
                    <Button
                      aria-label="Jump to linked conversation"
                      disabled={!isUsableConversationLink(link) || hasActiveConversationAction}
                      onClick={() => onJumpToConversation(link)}
                      size="icon-sm"
                      type="button"
                      variant="ghost"
                    >
                      <IconExternalLink />
                    </Button>
                    <Button
                      aria-label="Unlink conversation"
                      disabled={hasActiveConversationAction}
                      onClick={() => onUnlinkConversation(link)}
                      size="icon-sm"
                      type="button"
                      variant="ghost"
                    >
                      <IconUnlink />
                    </Button>
                  </div>
                </div>
              );
            })}
          </div>
        </TooltipProvider>
      ) : (
        <p className="project-ticket-empty">No linked conversation yet.</p>
      )}
    </section>
  );
}

function conversationLinkLabel(link: ProjectBoardConversationLinkView): string {
  return link.sessionTitle || link.agentName || link.agentId || link.agentSessionId || "Agent session";
}

function isUsableConversationLink(link: ProjectBoardConversationLinkView | undefined): boolean {
  return Boolean(link?.isLive || link?.isRestorable);
}

function getPrimaryUsableConversationLink(
  links: ProjectBoardConversationLinkView[],
): ProjectBoardConversationLinkView | undefined {
  return links.find(isUsableConversationLink);
}

function ConversationLinkName({
  className,
  label,
}: {
  className: string;
  label: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger render={<span className={className}>{label}</span>} />
      <TooltipContent side="bottom">{label}</TooltipContent>
    </Tooltip>
  );
}

function conversationLinkStatusText(link: ProjectBoardConversationLinkView): string {
  const sessionStatus = link.isSleeping
    ? "Sleeping"
    : link.isLive
      ? "Live"
      : link.isRestorable
        ? "Restorable"
        : "Unavailable";
  const agentSessionPreview = link.agentSessionId ? ` · ${link.agentSessionId.slice(0, 8)}` : "";
  return `${sessionStatus}${agentSessionPreview}`;
}

function projectBoardCommentMetadataFromLink(
  link: ProjectBoardConversationLinkView | undefined,
): ProjectBoardCommentMetadata {
  /*
   * CDXC:ProjectBoardComments 2026-06-05-06:43:
   * UI-added comments should use the linked agent conversation as their metadata source so the rendered author line can show the agent beside the Beads user and the footer can show the resumable agent CLI session id instead of the truncated status preview.
   *
   * CDXC:ProjectBoardComments 2026-06-05-06:55:
   * The comment Session footer must be the saved session id from the agent CLI, not the Ghostex pane id. If the linked conversation has not reported an agent session id yet, omit the footer rather than displaying the wrong id as resumable.
   */
  if (!link) {
    return {};
  }
  return {
    agentName: link.agentName || link.agentId,
    sessionId: link.agentSessionId,
  };
}

function compareConversationLinksNewestFirst(
  left: ProjectBoardConversationLinkView,
  right: ProjectBoardConversationLinkView,
): number {
  const leftTime = Date.parse(left.updatedAt);
  const rightTime = Date.parse(right.updatedAt);
  return (Number.isNaN(rightTime) ? 0 : rightTime) - (Number.isNaN(leftTime) ? 0 : leftTime);
}

function compareAutomationRunsForTriage(left: AutomationRun, right: AutomationRun): number {
  const unreadDelta = Number(right.isUnread) - Number(left.isUnread);
  if (unreadDelta !== 0) {
    return unreadDelta;
  }
  const statusDelta = automationTriageStatusWeight(right.status) - automationTriageStatusWeight(left.status);
  if (statusDelta !== 0) {
    return statusDelta;
  }
  return compareAutomationRunsNewestFirst(left, right);
}

function selectAutomationRunsForTriage(runs: AutomationRun[]): AutomationRun[] {
  const selectedRuns = new Map<string, AutomationRun>();
  for (const run of runs.filter(isAutomationRunActionableInTriage).sort(compareAutomationRunsForTriage)) {
    selectedRuns.set(run.id, run);
  }
  for (const run of runs
    .filter(isAutomationRunRecentlyCompletedForTriage)
    .sort(compareAutomationRunsNewestFirst)
    .slice(0, PROJECT_AUTOMATION_TRIAGE_RECENT_COMPLETED_LIMIT)) {
    selectedRuns.set(run.id, run);
  }
  return [...selectedRuns.values()].sort(compareAutomationRunsForTriage);
}

function isAutomationRunActionableInTriage(run: AutomationRun): boolean {
  return (
    run.isUnread ||
    run.status === "findings" ||
    run.status === "needs_attention" ||
    run.status === "failed"
  );
}

function isAutomationRunRecentlyCompletedForTriage(run: AutomationRun): boolean {
  return Boolean(run.completedAt) && run.status !== "running" && run.status !== "queued";
}

function automationTriageStatusWeight(status: AutomationRun["status"]): number {
  switch (status) {
    case "needs_attention":
    case "failed":
      return 3;
    case "findings":
      return 2;
    default:
      return 1;
  }
}

function BoardLane({
  column,
  conversationAction,
  linksByBeadId,
  onAddTicket,
  onJumpToConversation,
  onOpenContextMenu,
  onOpenTicket,
  tickets,
}: {
  column: (typeof BOARD_COLUMNS)[number];
  conversationAction: ConversationActionState;
  linksByBeadId: Map<string, ProjectBoardConversationLinkView[]>;
  onAddTicket: (status: BoardStatusKey) => void;
  onJumpToConversation: (link: ProjectBoardConversationLinkView) => void;
  onOpenContextMenu: (ticket: BoardTicket, point: { x: number; y: number }) => void;
  onOpenTicket: (ticket: BoardTicket) => void;
  tickets: BoardTicket[];
}) {
  const { isDropTarget, ref } = useDroppable({
    accept: "ticket",
    data: { statusKey: column.key },
    id: column.key,
  });
  const visibleTickets = tickets.slice(0, PROJECT_BOARD_MAX_VISIBLE_TICKETS_PER_COLUMN);
  const hiddenTicketCount = tickets.length - visibleTickets.length;
  return (
    <section
      className="project-board-lane"
      data-drop-target={String(isDropTarget)}
      data-tone={column.tone}
      ref={ref}
    >
      <header className="project-board-lane-header">
        <div>
          <span className="project-board-lane-dot" />
          <h2>{column.label}</h2>
        </div>
        <div className="project-board-lane-header-action">
          <span className="project-board-lane-count">{tickets.length}</span>
          <Button
            aria-label={`Add ticket to ${column.label}`}
            className="project-board-lane-add"
            onClick={() => onAddTicket(column.key)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <IconPlus aria-hidden="true" />
          </Button>
        </div>
      </header>
      <div className="project-board-lane-scroll vertical-scroll-fade-mask">
        <div className="project-board-card-stack">
          {visibleTickets.map((ticket) => (
            <TicketCard
              conversationAction={conversationAction}
              key={ticket.id}
              links={linksByBeadId.get(ticket.id) ?? []}
              onJumpToConversation={onJumpToConversation}
              onOpenContextMenu={onOpenContextMenu}
              onOpenTicket={onOpenTicket}
              ticket={ticket}
            />
          ))}
          {hiddenTicketCount > 0 ? (
            <div className="project-board-lane-limit" role="status">
              Showing {visibleTickets.length} of {tickets.length}. Use search or status filters to narrow this lane.
            </div>
          ) : null}
        </div>
      </div>
    </section>
  );
}

function TicketCard({
  conversationAction,
  links,
  onJumpToConversation,
  onOpenContextMenu,
  onOpenTicket,
  ticket,
}: {
  conversationAction: ConversationActionState;
  links: ProjectBoardConversationLinkView[];
  onJumpToConversation: (link: ProjectBoardConversationLinkView) => void;
  onOpenContextMenu: (ticket: BoardTicket, point: { x: number; y: number }) => void;
  onOpenTicket: (ticket: BoardTicket) => void;
  ticket: BoardTicket;
}) {
  const { isDragging, ref } = useDraggable({
    data: { ticketId: ticket.id },
    id: ticket.id,
    type: "ticket",
  });
  const blockedByCount = ticket.dependency_count ?? getBlockedByIds(ticket).length;
  const blockingCount = ticket.dependent_count ?? 0;
  const primaryLink = getPrimaryUsableConversationLink(links) ?? links[0];
  const additionalLinkCount = primaryLink ? links.length - 1 : 0;
  const primaryLinkLabel = primaryLink ? conversationLinkLabel(primaryLink) : "";
  const jumpDisabled =
    !isUsableConversationLink(primaryLink) ||
    Boolean(conversationAction);

  return (
    <Card
      className="project-board-card"
      data-dragging={String(isDragging)}
      onClick={() => onOpenTicket(ticket)}
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
        onOpenContextMenu(ticket, { x: event.clientX, y: event.clientY });
      }}
      onKeyDown={(event) => {
        if (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10")) {
          return;
        }
        event.preventDefault();
        const bounds = event.currentTarget.getBoundingClientRect();
        onOpenContextMenu(ticket, {
          x: bounds.left + Math.min(32, bounds.width - 12),
          y: bounds.top + Math.min(28, bounds.height - 12),
        });
      }}
      ref={ref}
      role="button"
      size="sm"
      tabIndex={0}
    >
      <CardHeader className="project-board-card-header">
        <CardTitle>{ticket.title}</CardTitle>
        <CardDescription>{ticket.displayId}</CardDescription>
      </CardHeader>
      <CardContent className="project-board-card-content">
        <p>{ticket.description || "No prompt yet."}</p>
        {ticket.labels?.length ? (
          <div className="project-board-card-labels">
            {ticket.labels.map((label) => (
              <span className="project-board-card-label" key={label}>
                {label}
              </span>
            ))}
          </div>
        ) : null}
        <Separator />
        <div className="project-board-card-meta">
          <span className="project-board-priority">{priorityLabel(ticket.priority)}</span>
          {estimateToTshirt(ticket.estimate) ? (
            <span>{estimateToTshirt(ticket.estimate)}</span>
          ) : null}
          {blockedByCount > 0 ? <span>{blockedByCount} blocked</span> : null}
          {blockingCount > 0 ? <span>{blockingCount} blocking</span> : null}
          <span className="project-board-comments">
            <IconMessageCircle />
            {ticket.comment_count ?? ticket.comments?.length ?? 0}
          </span>
        </div>
        {primaryLink ? (
          <div className="project-board-card-conversation">
            <TooltipProvider delayDuration={TOOLTIP_DELAY_MS}>
              <span className="project-board-card-conversation-label">
                <IconLink />
                <ConversationLinkName
                  className="project-board-card-conversation-name"
                  label={primaryLinkLabel}
                />
                {additionalLinkCount > 0 ? (
                  <span className="project-board-card-conversation-extra">
                    +{additionalLinkCount}
                  </span>
                ) : null}
              </span>
            </TooltipProvider>
            <Button
              aria-label="Jump to linked conversation"
              disabled={jumpDisabled}
              onClick={(event) => {
                event.stopPropagation();
                onJumpToConversation(primaryLink);
              }}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              <IconExternalLink />
            </Button>
          </div>
        ) : null}
      </CardContent>
    </Card>
  );
}

function ProjectBoardTicketContextMenu({
  confirmingDelete,
  deleting,
  onDelete,
  onDismiss,
  onPrimaryAction,
  position,
  primaryActionDisabled,
  primaryActionLabel,
}: {
  confirmingDelete: boolean;
  deleting: boolean;
  onDelete: () => void;
  onDismiss: () => void;
  onPrimaryAction: () => void;
  position: Pick<TicketContextMenuState, "x" | "y">;
  primaryActionDisabled: boolean;
  primaryActionLabel: string;
}) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [menuStyle, setMenuStyle] = useState<CSSProperties>(() => ({
    left: `${position.x}px`,
    top: `${position.y}px`,
  }));

  useLayoutEffect(() => {
    const menu = menuRef.current;
    if (!menu) {
      return;
    }
    const bounds = menu.getBoundingClientRect();
    const left = Math.max(
      PROJECT_BOARD_CONTEXT_MENU_VIEWPORT_MARGIN_PX,
      Math.min(
        position.x,
        window.innerWidth - bounds.width - PROJECT_BOARD_CONTEXT_MENU_VIEWPORT_MARGIN_PX,
      ),
    );
    const top = Math.max(
      PROJECT_BOARD_CONTEXT_MENU_VIEWPORT_MARGIN_PX,
      Math.min(
        position.y,
        window.innerHeight - bounds.height - PROJECT_BOARD_CONTEXT_MENU_VIEWPORT_MARGIN_PX,
      ),
    );
    setMenuStyle({
      left: `${Math.round(left)}px`,
      top: `${Math.round(top)}px`,
    });
  }, [confirmingDelete, position.x, position.y, primaryActionLabel]);

  useEffect(() => {
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        onDismiss();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onDismiss]);

  return createPortal(
    <>
      <button
        aria-label="Dismiss ticket context menu"
        className="project-board-context-menu-backdrop"
        onClick={onDismiss}
        onContextMenu={(event) => {
          event.preventDefault();
          onDismiss();
        }}
        type="button"
      />
      <div
        className="project-board-ticket-context-menu"
        onClick={(event) => event.stopPropagation()}
        onContextMenu={(event) => event.preventDefault()}
        ref={menuRef}
        role="menu"
        style={menuStyle}
      >
        <button
          className="project-board-ticket-context-menu-item"
          disabled={primaryActionDisabled}
          onClick={onPrimaryAction}
          role="menuitem"
          type="button"
        >
          <IconPlayerPlay aria-hidden="true" />
          {primaryActionLabel}
        </button>
        <button
          className="project-board-ticket-context-menu-item project-board-ticket-context-menu-item-danger"
          disabled={deleting}
          onClick={onDelete}
          role="menuitem"
          type="button"
        >
          <IconTrash aria-hidden="true" />
          {confirmingDelete ? (deleting ? "Deleting" : "Confirm delete") : "Delete"}
        </button>
      </div>
    </>,
    document.body,
  );
}

function AutomationComingSoonOverlay({ surfaceName }: { surfaceName: string }) {
  return (
    <section
      aria-label={`${surfaceName} coming soon`}
      className="project-automation-coming-soon"
    >
      <div className="project-automation-coming-soon-panel" role="status">
        <div className="project-automation-coming-soon-icon">
          <IconCalendarTime aria-hidden="true" />
        </div>
        <span>Experimental</span>
        <h2>{surfaceName} is coming very soon</h2>
        <p>
          Enable Experimental Features in Settings to preview Automations
          Overview and project Automate pages before launch.
        </p>
      </div>
    </section>
  );
}

function AutomationEmptyState({
  action,
  description,
  icon: Icon,
  title,
  variant = "panel",
}: {
  action?: { label: string; onClick: () => void };
  description: string;
  icon: typeof IconCalendarTime;
  title: string;
  variant?: "detail" | "panel";
}) {
  return (
    <section
      className="project-automation-empty-state"
      data-variant={variant}
      {...(variant === "detail" ? { "aria-label": title } : {})}
    >
      <div className="project-automation-empty-state-icon">
        <Icon aria-hidden="true" />
      </div>
      <strong>{title}</strong>
      <p>{description}</p>
      {action ? (
        <Button
          className="project-automation-empty-action"
          onClick={action.onClick}
          size="sm"
          type="button"
          variant="secondary"
        >
          {action.label}
        </Button>
      ) : null}
    </section>
  );
}

function automationRunEmptyDescription(emptyTitle: string): string {
  if (emptyTitle.toLowerCase().includes("triage")) {
    return "When an automation reports findings or needs attention, the result appears here for review.";
  }
  return "Runs appear here after automations execute on their schedule or when you run them manually.";
}

function AutomationDefinitionList({
  actionId,
  agents,
  automations,
  onCreate,
  onDelete,
  onEdit,
  onRunNow,
  onSelect,
  onSetEnabled,
  projectNameById,
  runs,
  selectedAutomationId,
  showProjectLabels = false,
}: {
  actionId: string;
  agents: ProjectAutomationAgentOption[];
  automations: AutomationDefinition[];
  onCreate: () => void;
  onDelete: (automation: AutomationDefinition) => void;
  onEdit: (automation: AutomationDefinition) => void;
  onRunNow: (automation: AutomationDefinition) => void;
  onSelect: (automationId: string) => void;
  onSetEnabled: (automation: AutomationDefinition, enabled: boolean) => void;
  projectNameById?: ReadonlyMap<string, string>;
  runs: AutomationRun[];
  selectedAutomationId: string;
  showProjectLabels?: boolean;
}) {
  if (automations.length === 0) {
    return (
      <AutomationEmptyState
        action={{ label: "Create automation", onClick: onCreate }}
        description="Schedule agents to run recurring checks, reviews, or maintenance for this project."
        icon={IconCalendarTime}
        title="No automations yet"
      />
    );
  }
  return (
    <section className="project-automation-list vertical-scroll-fade-mask" aria-label="Automations">
      {automations.map((automation) => {
        const lastRun = runs.find((run) => run.automationId === automation.id);
        const unreadCount = runs.filter(
          (run) => run.automationId === automation.id && run.isUnread && !run.isArchived,
        ).length;
        const agent = agents.find((candidate) => candidate.agentId === automation.agentId);
        const agentLabel = agent?.label ?? automation.agentId;
        const automationProjectName = projectNameById?.get(automation.projectIds[0] ?? "");
        const isBusy = actionId === automation.id;
        return (
          <Card
            className="project-automation-card"
            data-selected={automation.id === selectedAutomationId}
            key={automation.id}
            onClick={() => onSelect(automation.id)}
            role="button"
            size="sm"
            tabIndex={0}
          >
            <CardContent>
              <div className="project-automation-card-main">
                <div>
                  <div className="project-automation-card-title">
                    <span data-enabled={automation.enabled}>{automation.enabled ? "Enabled" : "Paused"}</span>
                    <strong>{automation.name}</strong>
                  </div>
                  <div className="project-automation-card-tags">
                    {showProjectLabels && automationProjectName ? <span>{automationProjectName}</span> : null}
                    <span>{describeAutomationSchedule(automation.schedule)}</span>
                    <span>{describeAutomationMode(automation.executionMode)}</span>
                  </div>
                  <div className="project-automation-card-agent">
                    {agent && resolveAutomationAgentIcon(agent) ? (
                      <AutomationAgentIcon icon={resolveAutomationAgentIcon(agent)!} />
                    ) : null}
                    <span>{agentLabel}</span>
                  </div>
                </div>
                <div className="project-automation-card-meta">
                  <span>{automation.nextRunAt ? formatShortDate(automation.nextRunAt) : "No next run"}</span>
                  <span>{lastRun ? automationRunStatusLabel(lastRun.status) : "Never run"}</span>
                  {unreadCount > 0 ? <span data-unread="true">{unreadCount} unread</span> : null}
                </div>
              </div>
              <div className="project-automation-card-actions">
                <Button
                  aria-label={`Run ${automation.name}`}
                  disabled={isBusy}
                  onClick={(event) => {
                    event.stopPropagation();
                    onRunNow(automation);
                  }}
                  size="icon-sm"
                  type="button"
                  variant="ghost"
                >
                  <IconPlayerPlay />
                </Button>
                <div className="project-automation-card-toggle">
                  <Switch
                    checked={automation.enabled}
                    disabled={isBusy}
                    onCheckedChange={(enabled: boolean) => {
                      onSetEnabled(automation, enabled);
                    }}
                    onClick={(event) => event.stopPropagation()}
                    size="sm"
                  />
                  <span data-enabled={automation.enabled}>{automation.enabled ? "On" : "Off"}</span>
                </div>
                <Button
                  onClick={(event) => {
                    event.stopPropagation();
                    onEdit(automation);
                  }}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  Edit
                </Button>
                <Button
                  aria-label={`Delete ${automation.name}`}
                  disabled={isBusy}
                  onClick={(event) => {
                    event.stopPropagation();
                    onDelete(automation);
                  }}
                  size="icon-sm"
                  type="button"
                  variant="ghost"
                >
                  <IconTrash />
                </Button>
              </div>
            </CardContent>
          </Card>
        );
      })}
    </section>
  );
}

function AutomationRunList({
  actionId,
  agents,
  automations,
  emptyTitle,
  onArchive,
  onMarkRead,
  onOpenSession,
  onOpenWorktree,
  onSelect,
  projectName,
  runs,
  selectedRunId,
}: {
  actionId: string;
  agents: ProjectAutomationAgentOption[];
  automations: AutomationDefinition[];
  emptyTitle: string;
  onArchive: (run: AutomationRun) => void;
  onMarkRead: (run: AutomationRun) => void;
  onOpenSession: (run: AutomationRun) => void;
  onOpenWorktree: (run: AutomationRun) => void;
  onSelect: (runId: string) => void;
  projectName: string;
  runs: AutomationRun[];
  selectedRunId: string;
}) {
  if (runs.length === 0) {
    return (
      <AutomationEmptyState
        description={automationRunEmptyDescription(emptyTitle)}
        icon={IconBell}
        title={emptyTitle}
      />
    );
  }
  return (
    <section className="project-automation-run-list vertical-scroll-fade-mask" aria-label="Automation runs">
      {runs.map((run) => {
        const automation = automations.find((candidate) => candidate.id === run.automationId);
        const agentLabel = automation ? automationAgentLabel(agents, automation.agentId) : "Unknown agent";
        const isActiveRun = isAutomationRunActive(run);
        return (
          <Card
            className="project-automation-run-card"
            data-selected={run.id === selectedRunId}
            data-unread={run.isUnread}
            key={run.id}
            onClick={() => onSelect(run.id)}
            role="button"
            size="sm"
            tabIndex={0}
          >
            <CardContent>
              <div className="project-automation-run-main">
                <div className="project-automation-run-heading">
                  <span data-status={run.status}>{automationRunStatusLabel(run.status)}</span>
                  <strong>{automation?.name ?? run.automationId}</strong>
                </div>
                <p>{run.findingsSummary || run.errorMessage || "Run is waiting for agent output."}</p>
                <div className="project-automation-run-meta">
                  <span>{projectName}</span>
                  <span>{agentLabel}</span>
                  <span>{formatShortDate(run.completedAt ?? run.createdAt)}</span>
                  {run.sessionId ? <span>Session {run.sessionId}</span> : null}
                  {run.worktree ? <span>{run.worktree.branch}</span> : null}
                </div>
              </div>
              <div className="project-automation-run-actions">
                {run.sessionId ? (
                  <Button
                    aria-label="Open automation session"
                    disabled={actionId === run.id}
                    onClick={(event) => {
                      event.stopPropagation();
                      onOpenSession(run);
                    }}
                    size="icon-sm"
                    type="button"
                    variant="ghost"
                  >
                    <IconExternalLink />
                  </Button>
                ) : null}
                {run.worktree ? (
                  <Button
                    aria-label="Open automation worktree"
                    disabled={actionId === run.id}
                    onClick={(event) => {
                      event.stopPropagation();
                      onOpenWorktree(run);
                    }}
                    size="icon-sm"
                    type="button"
                    variant="ghost"
                  >
                    <IconFolderOpen />
                  </Button>
                ) : null}
                {run.isUnread ? (
                  <Button
                    aria-label="Mark run read"
                    disabled={actionId === run.id}
                    onClick={(event) => {
                      event.stopPropagation();
                      onMarkRead(run);
                    }}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    Read
                  </Button>
                ) : null}
                <Button
                  aria-label="Archive run"
                  disabled={actionId === run.id || isActiveRun}
                  onClick={(event) => {
                    event.stopPropagation();
                    onArchive(run);
                  }}
                  size="icon-sm"
                  type="button"
                  variant="ghost"
                >
                  <IconArchive />
                </Button>
              </div>
            </CardContent>
          </Card>
        );
      })}
    </section>
  );
}

function AutomationDefinitionDetail({
  actionId,
  agents,
  automation,
  onDelete,
  onEdit,
  onRunNow,
  onSetEnabled,
  projectNameById,
  runs,
  showProjectLabels = false,
}: {
  actionId: string;
  agents: ProjectAutomationAgentOption[];
  automation: AutomationDefinition | undefined;
  onDelete: (automation: AutomationDefinition) => void;
  onEdit: (automation: AutomationDefinition) => void;
  onRunNow: (automation: AutomationDefinition) => void;
  onSetEnabled: (automation: AutomationDefinition, enabled: boolean) => void;
  projectNameById?: ReadonlyMap<string, string>;
  runs: AutomationRun[];
  showProjectLabels?: boolean;
}) {
  if (!automation) {
    return (
      <section className="project-automation-detail project-automation-detail--empty" aria-label="Automation details">
        <AutomationEmptyState
          description="Select an automation from the list to see its schedule, prompt, and recent runs."
          icon={IconCalendarTime}
          title="No automation selected"
          variant="detail"
        />
      </section>
    );
  }
  const automationRuns = runs
    .filter((run) => run.automationId === automation.id)
    .slice(0, 5);
  const agent = agents.find((candidate) => candidate.agentId === automation.agentId);
  const agentLabel = agent?.label ?? automation.agentId;
  const agentIcon = agent ? resolveAutomationAgentIcon(agent) : undefined;
  const automationProjectName = projectNameById?.get(automation.projectIds[0] ?? "");
  const isBusy = actionId === automation.id;
  return (
    <section className="project-automation-detail vertical-scroll-fade-mask" aria-label="Automation details">
      <div className="project-automation-detail-header">
        <div>
          <span data-enabled={automation.enabled}>{automation.enabled ? "Enabled" : "Paused"}</span>
          <h2>{automation.name}</h2>
        </div>
        <div className="project-automation-detail-actions">
          <Button
            aria-label={`Run ${automation.name}`}
            disabled={isBusy}
            onClick={() => onRunNow(automation)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <IconPlayerPlay />
          </Button>
          <div className="project-automation-detail-toggle">
            <Switch
              checked={automation.enabled}
              disabled={isBusy}
              onCheckedChange={(enabled: boolean) => onSetEnabled(automation, enabled)}
              size="sm"
            />
            <span data-enabled={automation.enabled}>{automation.enabled ? "Enabled" : "Paused"}</span>
          </div>
          <Button onClick={() => onEdit(automation)} size="sm" type="button" variant="outline">
            Edit
          </Button>
          <Button
            aria-label={`Delete ${automation.name}`}
            disabled={isBusy}
            onClick={() => onDelete(automation)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <IconTrash />
          </Button>
        </div>
      </div>
      <dl className="project-automation-detail-grid">
        {showProjectLabels && automationProjectName ? (
          <div>
            <dt>Project</dt>
            <dd>{automationProjectName}</dd>
          </div>
        ) : null}
        <div>
          <dt>Schedule</dt>
          <dd>{describeAutomationSchedule(automation.schedule)}</dd>
        </div>
        <div>
          <dt>Next run</dt>
          <dd>{automation.nextRunAt ? formatShortDate(automation.nextRunAt) : "Not scheduled"}</dd>
        </div>
        <div>
          <dt>Agent</dt>
          <dd>
            {agentIcon ? <AutomationAgentIcon icon={agentIcon} /> : null}
            <span>{agentLabel}</span>
          </dd>
        </div>
        <div>
          <dt>Mode</dt>
          <dd>{describeAutomationMode(automation.executionMode)}</dd>
        </div>
        {automation.executionMode.kind === "worktree" && automation.executionMode.setupCommand ? (
          <div>
            <dt>Setup</dt>
            <dd>{automation.executionMode.setupCommand}</dd>
          </div>
        ) : null}
        {automation.executionMode.kind === "thread" ? (
          <div>
            <dt>Thread</dt>
            <dd>{automation.executionMode.sessionId}</dd>
          </div>
        ) : null}
        {automation.executionMode.kind === "thread" && automation.executionMode.expiresAt ? (
          <div>
            <dt>Expires</dt>
            <dd>{formatShortDate(automation.executionMode.expiresAt)}</dd>
          </div>
        ) : null}
      </dl>
      <Separator />
      <div className="project-automation-detail-section">
        <h3>Prompt</h3>
        <pre>{automation.prompt}</pre>
      </div>
      <div className="project-automation-detail-section">
        <h3>Recent runs</h3>
        {automationRuns.length > 0 ? (
          <div className="project-automation-detail-run-stack">
            {automationRuns.map((run) => (
              <div key={run.id}>
                <span data-status={run.status}>{automationRunStatusLabel(run.status)}</span>
                <p>{formatShortDate(run.completedAt ?? run.createdAt)}</p>
              </div>
            ))}
          </div>
        ) : (
          <p>No runs yet.</p>
        )}
      </div>
    </section>
  );
}

function AutomationRunDetail({
  actionId,
  agents,
  automation,
  onArchive,
  onMarkRead,
  onOpenSession,
  onOpenWorktree,
  projectName,
  run,
}: {
  actionId: string;
  agents: ProjectAutomationAgentOption[];
  automation: AutomationDefinition | undefined;
  onArchive: (run: AutomationRun) => void;
  onMarkRead: (run: AutomationRun) => void;
  onOpenSession: (run: AutomationRun) => void;
  onOpenWorktree: (run: AutomationRun) => void;
  projectName: string;
  run: AutomationRun | undefined;
}) {
  if (!run) {
    return (
      <section className="project-automation-detail project-automation-detail--empty" aria-label="Automation run details">
        <AutomationEmptyState
          description="Select a run from the list to review its status, summary, and linked session."
          icon={IconBell}
          title="No run selected"
          variant="detail"
        />
      </section>
    );
  }
  const agent = automation ? agents.find((candidate) => candidate.agentId === automation.agentId) : undefined;
  const agentLabel = agent?.label ?? (automation ? automation.agentId : "Unknown agent");
  const agentIcon = agent ? resolveAutomationAgentIcon(agent) : undefined;
  const isBusy = actionId === run.id;
  const isActiveRun = isAutomationRunActive(run);
  return (
    <section className="project-automation-detail vertical-scroll-fade-mask" aria-label="Automation run details">
      <div className="project-automation-detail-header">
        <div>
          <span data-status={run.status}>{automationRunStatusLabel(run.status)}</span>
          <h2>{automation?.name ?? run.automationId}</h2>
        </div>
        <div className="project-automation-detail-actions">
          {run.sessionId ? (
            <Button
              aria-label="Open automation session"
              disabled={isBusy}
              onClick={() => onOpenSession(run)}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              <IconExternalLink />
            </Button>
          ) : null}
          {run.worktree ? (
            <Button
              aria-label="Open automation worktree"
              disabled={isBusy}
              onClick={() => onOpenWorktree(run)}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              <IconFolderOpen />
            </Button>
          ) : null}
          {run.isUnread ? (
            <Button disabled={isBusy} onClick={() => onMarkRead(run)} size="sm" type="button" variant="outline">
              Read
            </Button>
          ) : null}
          <Button
            aria-label="Archive run"
            disabled={isBusy || isActiveRun}
            onClick={() => onArchive(run)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <IconArchive />
          </Button>
        </div>
      </div>
      <dl className="project-automation-detail-grid">
        <div>
          <dt>Project</dt>
          <dd>{projectName}</dd>
        </div>
        <div>
          <dt>Agent</dt>
          <dd>
            {agentIcon ? <AutomationAgentIcon icon={agentIcon} /> : null}
            <span>{agentLabel}</span>
          </dd>
        </div>
        <div>
          <dt>Created</dt>
          <dd>{formatShortDate(run.createdAt)}</dd>
        </div>
        <div>
          <dt>Completed</dt>
          <dd>{run.completedAt ? formatShortDate(run.completedAt) : "Still running"}</dd>
        </div>
        {run.sessionId ? (
          <div>
            <dt>Session</dt>
            <dd>
              <span>{run.sessionId}</span>
              <Button
                aria-label="Copy automation session id"
                onClick={() => void navigator.clipboard.writeText(run.sessionId ?? "")}
                size="icon-sm"
                type="button"
                variant="ghost"
              >
                <IconCopy />
              </Button>
            </dd>
          </div>
        ) : null}
        {run.worktree ? (
          <>
            <div>
              <dt>Branch</dt>
              <dd>
                <span>{run.worktree.branch}</span>
                <Button
                  aria-label="Copy automation worktree branch"
                  onClick={() => void navigator.clipboard.writeText(run.worktree?.branch ?? "")}
                  size="icon-sm"
                  type="button"
                  variant="ghost"
                >
                  <IconCopy />
                </Button>
              </dd>
            </div>
            <div>
              <dt>Worktree</dt>
              <dd>
                <span>{run.worktree.path}</span>
                <Button
                  aria-label="Copy automation worktree path"
                  onClick={() => void navigator.clipboard.writeText(run.worktree?.path ?? "")}
                  size="icon-sm"
                  type="button"
                  variant="ghost"
                >
                  <IconCopy />
                </Button>
              </dd>
            </div>
          </>
        ) : null}
      </dl>
      <Separator />
      <div className="project-automation-detail-section">
        <h3>Result</h3>
        <p>{run.findingsSummary || run.errorMessage || "Run is waiting for agent output."}</p>
      </div>
    </section>
  );
}

function ProjectBoardNotice({ message }: { message: string }) {
  const isMissingProject = /not initialized|no storage|not a beads|bd init|database|\.beads/i.test(message);
  const isMissingBeads =
    !isMissingProject &&
    /bd was not found|bundled bd|beads cli|executable|command not found|not found: bd|bd: not found|env: bd: no such file|cannot find/i.test(message);
  const command = isMissingProject ? "bd init" : "";
  const title = isMissingBeads
    ? "Beads CLI unavailable"
    : isMissingProject
      ? "Initialize Beads for this project"
      : "Project board unavailable";
  const bodyLines = isMissingBeads
    ? [
        "Packaged Ghostex includes the Beads CLI used to read and update Project tickets.",
        "Update or rebuild Ghostex so the bundled bd is staged. Source checkouts must stage the bundled bd instead of using PATH bd.",
      ]
    : isMissingProject
      ? [
          "This project does not have a Beads workspace yet. Run this once from the project root, then refresh the board.",
        ]
      : [message];
  return (
    <Card
      className="project-board-notice"
      data-kind={isMissingBeads ? "install" : isMissingProject ? "init" : "error"}
      role="status"
      size="sm"
    >
      <CardContent>
        {/*
          CDXC:ProjectBoard 2026-05-28-15:27:
          Initialization is a normal first-run state for Beads-backed projects, not an app failure.
          Present bd init as an explanatory setup callout with a copyable command so users understand what needs to happen before the board can load tickets.

          CDXC:ProjectBoard 2026-05-29-15:49:
          Missing-Beads setup should use the same polished notice shell but stay intentionally terse: one header and two lines below.
          Explain why Beads is required without adding a second control row.

          CDXC:ProjectBoardBeads 2026-06-08-10:46:
          Project/Kanban should work on first open in packaged Ghostex because the app now bundles the full upstream `bd` CLI. If bd is still unavailable, frame the notice as a stale/broken bundle or source-checkout setup issue instead of telling packaged users to install Homebrew Beads.
        */}
        <div className="project-board-notice-icon" aria-hidden="true">
          <IconAlertTriangle />
        </div>
        <div className="project-board-notice-body">
          <strong>{title}</strong>
          {bodyLines.map((line) => (
            <p key={line}>{line}</p>
          ))}
          {command ? (
            <div className="project-board-notice-command">
              <code>{command}</code>
              <Button
                aria-label={`Copy ${command}`}
                onClick={() => void navigator.clipboard.writeText(command)}
                size="icon-sm"
                type="button"
                variant="ghost"
              >
                <IconCopy />
              </Button>
            </div>
          ) : null}
        </div>
      </CardContent>
    </Card>
  );
}

function handleCmdEnter(event: KeyboardEvent, action: () => void) {
  if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
    event.preventDefault();
    action();
  }
}

function createAutomationDraft(input: Partial<AutomationDraft> = {}): AutomationDraft {
  return {
    agentId: input.agentId ?? "",
    cronExpression: input.cronExpression ?? "*/15 * * * *",
    enabled: input.enabled ?? true,
    expiresAt: input.expiresAt ?? "",
    executionKind: input.executionKind ?? "worktree",
    id: input.id,
    name: input.name ?? "",
    prompt: input.prompt ?? "",
    projectId: input.projectId ?? "",
    schedulePreset: input.schedulePreset ?? "15m",
    scheduleTime: input.scheduleTime ?? "09:00",
    setupCommand: input.setupCommand ?? "",
    threadSessionId: input.threadSessionId ?? "",
    weeklyDay: input.weeklyDay ?? "1",
  };
}

function resolveAutomationDraftAgentId(
  agents: readonly Pick<ProjectAutomationAgentOption, "agentId">[],
  defaultAgentId?: string,
): string {
  /*
   * CDXC:Automations 2026-06-30-19:16:
   * New automation drafts should select the user's Default Prompt Agent by default, but only when that agent is present in the launchable options for the selected project. An unavailable saved id should not render as "Choose agent" or be saved invisibly.
   */
  const normalizedDefaultAgentId = defaultAgentId?.trim();
  return (
    agents.find((agent) => agent.agentId === normalizedDefaultAgentId)?.agentId ??
    agents[0]?.agentId ??
    ""
  );
}

function resolveAutomationDraftProjectId(
  projects: readonly Pick<ProjectAutomationTargetOption, "projectId">[],
  currentProjectId: string | undefined,
  fallbackProjectId: string | undefined,
): string {
  /*
   * CDXC:Automations 2026-07-01-02:33:
   * The global Create automation dialog is hosted by the Quick Automations surface, but saved automation definitions must target a real automation project. Keep an existing draft project only when it is still present in the loaded target list, so opening the dialog before bridge hydration cannot preserve `quick-automations` as the selected project.
   */
  const normalizedCurrentProjectId = currentProjectId?.trim() ?? "";
  if (
    normalizedCurrentProjectId &&
    projects.some((project) => project.projectId === normalizedCurrentProjectId)
  ) {
    return normalizedCurrentProjectId;
  }
  return projects[0]?.projectId ?? fallbackProjectId?.trim() ?? "";
}

function createAutomationDraftFromDefinition(
  definition: AutomationDefinition,
  projectId: string,
): AutomationDraft {
  const schedulePreset = resolveAutomationSchedulePreset(definition.schedule);
  const schedule = definition.schedule;
  if (schedule.kind === "weekly") {
    return createAutomationDraftFromDefinitionSchedule(definition, schedulePreset, projectId, {
      scheduleTime: schedule.time,
      weeklyDay: String(schedule.days[0] ?? 1),
    });
  }
  if (schedule.kind === "daily") {
    return createAutomationDraftFromDefinitionSchedule(definition, schedulePreset, projectId, {
      scheduleTime: schedule.time,
    });
  }
  if (schedule.kind === "cron") {
    return createAutomationDraftFromDefinitionSchedule(definition, schedulePreset, projectId, {
      cronExpression: schedule.expression,
    });
  }
  return createAutomationDraftFromDefinitionSchedule(definition, schedulePreset, projectId);
}

function resolveAutomationSchedulePreset(schedule: AutomationSchedule): AutomationSchedulePreset {
  if (schedule.kind === "interval") {
    const matchedPreset = Object.entries(AUTOMATION_INTERVAL_MS_BY_PRESET).find(
      ([, everyMs]) => everyMs === schedule.everyMs,
    );
    return (matchedPreset?.[0] as AutomationSchedulePreset | undefined) ?? "hourly";
  }
  if (schedule.kind === "weekly") {
    const weekdayPreset = [1, 2, 3, 4, 5];
    if (
      schedule.days.length === weekdayPreset.length &&
      weekdayPreset.every((day) => schedule.days.includes(day))
    ) {
      return "weekdays";
    }
    return "weekly";
  }
  if (schedule.kind === "daily") {
    return "daily";
  }
  return "cron";
}

function createAutomationDraftFromDefinitionSchedule(
  definition: AutomationDefinition,
  schedulePreset: AutomationDraft["schedulePreset"],
  projectId: string,
  input: Partial<AutomationDraft> = {},
): AutomationDraft {
  return createAutomationDraft({
    ...input,
    agentId: definition.agentId,
    enabled: definition.enabled,
    expiresAt:
      definition.executionMode.kind === "thread" && definition.executionMode.expiresAt
        ? toDatetimeLocalValue(definition.executionMode.expiresAt)
        : "",
    executionKind: definition.executionMode.kind,
    id: definition.id,
    name: definition.name,
    prompt: definition.prompt,
    projectId,
    schedulePreset,
    setupCommand:
      definition.executionMode.kind === "worktree"
        ? definition.executionMode.setupCommand ?? ""
        : "",
    threadSessionId:
      definition.executionMode.kind === "thread" ? definition.executionMode.sessionId : "",
  });
}

function createAutomationDefinitionFromDraft(
  draft: AutomationDraft,
  input: { fallbackAgentId: string; projectId: string },
): AutomationDefinition | undefined {
  const name = draft.name.trim();
  const prompt = draft.prompt.trim();
  const agentId = draft.agentId.trim() || input.fallbackAgentId.trim();
  const schedule = createAutomationScheduleFromDraft(draft);
  if (!name || !prompt || !agentId || !schedule) {
    return undefined;
  }
  const now = new Date().toISOString();
  const executionMode: AutomationExecutionMode =
    draft.executionKind === "local"
      ? { kind: "local" }
      : draft.executionKind === "thread"
        ? {
            expiresAt: datetimeLocalToIso(draft.expiresAt),
            kind: "thread",
            sessionId: draft.threadSessionId.trim(),
          }
        : {
            kind: "worktree",
            setupCommand: draft.setupCommand.trim() || undefined,
          };
  if (executionMode.kind === "thread" && !executionMode.sessionId) {
    return undefined;
  }
  return {
    agentId,
    createdAt: now,
    enabled: draft.enabled,
    executionMode,
    id: draft.id ?? `automation-${crypto.randomUUID()}`,
    name,
    nextRunAt: draft.enabled ? computeNextRunAt(schedule) : undefined,
    projectIds: [input.projectId],
    prompt,
    schedule,
    updatedAt: now,
  };
}

function createAutomationScheduleFromDraft(draft: AutomationDraft): AutomationSchedule | undefined {
  const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || "local";
  const intervalMs = AUTOMATION_INTERVAL_MS_BY_PRESET[draft.schedulePreset];
  const schedule =
    intervalMs !== undefined
      ? { everyMs: intervalMs, kind: "interval" }
      : draft.schedulePreset === "cron"
        ? {
            expression: draft.cronExpression,
            kind: "cron",
            timezone,
          }
        : draft.schedulePreset === "weekly"
          ? {
              days: [Number(draft.weeklyDay)],
              kind: "weekly",
              time: draft.scheduleTime,
              timezone,
            }
          : draft.schedulePreset === "weekdays"
            ? {
                days: [1, 2, 3, 4, 5],
                kind: "weekly",
                time: draft.scheduleTime,
                timezone,
              }
            : {
                kind: "daily",
                time: draft.scheduleTime,
                timezone,
              };
  return normalizeAutomationSchedule(schedule);
}

function describeAutomationSchedule(schedule: AutomationSchedule): string {
  switch (schedule.kind) {
    case "interval": {
      const preset = Object.entries(AUTOMATION_INTERVAL_MS_BY_PRESET).find(
        ([, everyMs]) => everyMs === schedule.everyMs,
      );
      if (preset) {
        return AUTOMATION_SCHEDULE_PRESETS.find((option) => option.value === preset[0])?.label ?? preset[0];
      }
      if (schedule.everyMs % (60 * 60 * 1000) === 0) {
        const hours = schedule.everyMs / (60 * 60 * 1000);
        return hours === 1 ? "Hourly" : `Every ${hours} hours`;
      }
      return `Every ${Math.round(schedule.everyMs / 60_000)} minutes`;
    }
    case "daily":
      return `Daily at ${schedule.time}`;
    case "weekly": {
      const weekdayPreset = [1, 2, 3, 4, 5];
      if (
        schedule.days.length === weekdayPreset.length &&
        weekdayPreset.every((day) => schedule.days.includes(day))
      ) {
        return `Weekdays at ${schedule.time}`;
      }
      return `Weekly ${weekdayLabel(schedule.days[0] ?? 0)} at ${schedule.time}`;
    }
    case "cron":
      return schedule.expression;
  }
}

function describeAutomationMode(mode: AutomationExecutionMode): string {
  switch (mode.kind) {
    case "worktree":
      return "Worktree";
    case "thread":
      return "Thread";
    case "local":
      return "Local checkout";
  }
}

function automationRunStatusLabel(status: AutomationRun["status"]): string {
  switch (status) {
    case "no_findings":
      return "No findings";
    case "needs_attention":
      return "Needs attention";
    default:
      return status.replace(/_/gu, " ");
  }
}

function isAutomationRunActive(run: Pick<AutomationRun, "status">): boolean {
  return run.status === "queued" || run.status === "running";
}

function automationAgentLabel(agents: ProjectAutomationAgentOption[], agentId: string): string {
  return agents.find((agent) => agent.agentId === agentId)?.label ?? agentId;
}

function resolveAutomationAgentIcon(
  agent: Pick<ProjectAutomationAgentOption, "agentId" | "icon">,
): SidebarAgentIcon | undefined {
  return agent.icon ?? getSidebarAgentIconById(agent.agentId);
}

function AutomationAgentOptionLabel({ agent }: { agent: ProjectAutomationAgentOption }) {
  const icon = resolveAutomationAgentIcon(agent);
  return (
    <span className="project-automation-agent-option">
      {icon ? <AutomationAgentIcon icon={icon} /> : null}
      <span>{agent.label}</span>
    </span>
  );
}

function AutomationAgentIcon({ icon }: { icon: SidebarAgentIcon }) {
  return (
    <span
      aria-hidden="true"
      className="project-automation-agent-icon"
      data-agent-icon={icon}
      style={{
        backgroundColor: AGENT_LOGO_COLORS[icon],
        maskImage: `url("${AGENT_LOGOS[icon]}")`,
        WebkitMaskImage: `url("${AGENT_LOGOS[icon]}")`,
      }}
    />
  );
}

function weekdayLabel(day: number): string {
  return ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"][day] ?? "Weekly";
}

function datetimeLocalToIso(value: string): string | undefined {
  const trimmed = value.trim();
  if (!trimmed) {
    return undefined;
  }
  const parsedMs = Date.parse(trimmed);
  return Number.isFinite(parsedMs) ? new Date(parsedMs).toISOString() : undefined;
}

function toDatetimeLocalValue(value: string): string {
  const parsedMs = Date.parse(value);
  if (!Number.isFinite(parsedMs)) {
    return "";
  }
  const date = new Date(parsedMs);
  const pad = (part: number) => String(part).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function waitForProjectBoardRefreshIdle(isBusy: () => boolean): Promise<void> {
  return new Promise((resolve) => {
    const tick = () => {
      if (!isBusy()) {
        resolve();
        return;
      }
      window.setTimeout(tick, 25);
    };
    tick();
  });
}

function toCreatedBoardTicket(
  issue: BeadsIssue,
  knownIssues: BeadsIssue[],
  displayKey: string,
): BoardTicket | undefined {
  const issues = [...knownIssues.filter((candidate) => candidate.id !== issue.id), issue];
  return toBoardTickets(issues, displayKey).find((ticket) => ticket.id === issue.id);
}

function resolveCreatedIssueFromRefresh(
  issues: BeadsIssue[],
  issueIdsBeforeCreate: Set<string>,
  created: { description: string; title: string },
): BeadsIssue | undefined {
  return issues
    .filter((issue) => {
      if (!issue?.id || issueIdsBeforeCreate.has(issue.id)) {
        return false;
      }
      return issue.title === created.title && (issue.description ?? "") === created.description;
    })
    .sort((left, right) => {
      const leftTime = Date.parse(left.created_at ?? left.updated_at ?? "");
      const rightTime = Date.parse(right.created_at ?? right.updated_at ?? "");
      return (Number.isFinite(rightTime) ? rightTime : 0) - (Number.isFinite(leftTime) ? leftTime : 0);
    })[0];
}

function stringifyProjectBoardDebugDetails(details: Record<string, unknown> | undefined): string | undefined {
  if (details === undefined) {
    return undefined;
  }
  try {
    return JSON.stringify(details);
  } catch {
    return JSON.stringify({ serializationFailed: true });
  }
}

function projectBoardTitleGenerationFailureDetails(error: unknown): Record<string, unknown> {
  const text = error instanceof Error ? error.message : String(error);
  return {
    errorClass: projectBoardTitleGenerationErrorClass(text),
    errorLength: text.length,
    isGenericPromptAgentFailure: text === "Prompt-agent title generation failed.",
  };
}

function projectBoardPromptAgentKind(agentId: string): string {
  switch (agentId.trim().toLowerCase()) {
    case "codex":
    case "claude":
    case "cursor":
    case "gemini":
      return agentId.trim().toLowerCase();
    case "":
      return "none";
    default:
      return "custom";
  }
}

function projectBoardTitleGenerationErrorClass(text: string): string {
  const normalized = text.toLowerCase();
  if (!normalized) {
    return "empty";
  }
  if (normalized.includes("command not found")) {
    return "commandNotFound";
  }
  if (normalized.includes("permission") || normalized.includes("operation not permitted")) {
    return "permission";
  }
  if (normalized.includes("auth") || normalized.includes("login") || normalized.includes("api key")) {
    return "auth";
  }
  if (normalized.includes("rate limit") || normalized.includes("429")) {
    return "rateLimit";
  }
  if (normalized.includes("timed out") || normalized.includes("timeout")) {
    return "timeout";
  }
  if (normalized === "prompt-agent title generation failed.") {
    return "genericPromptAgentFailure";
  }
  return "reported";
}

function createIssuesSignature(issues: BeadsIssue[]): string {
  return issues
    .map((issue) =>
      [
        issue.id,
        issue.status,
        issue.updated_at ?? "",
        issue.title,
        String(issue.priority ?? ""),
        String(issue.estimate ?? ""),
        String(issue.comment_count ?? issue.comments?.length ?? ""),
        String(issue.dependency_count ?? ""),
        String(issue.dependent_count ?? ""),
        (issue.labels ?? []).join(","),
      ].join("\u001f"),
    )
    .join("\u001e");
}

function mergeKnownLabels(current: string[], labels: readonly string[] | undefined): string[] {
  const next = new Set(current);
  for (const label of labels ?? []) {
    const normalized = typeof label === "string" ? label.trim() : "";
    if (normalized) {
      next.add(normalized);
    }
  }
  return [...next].sort((left, right) => left.localeCompare(right));
}

function deriveKnownLabelsFromIssues(issues: BeadsIssue[]): string[] {
  return mergeKnownLabels([], issues.flatMap((issue) => issue.labels ?? []));
}

function prioritizeDependencyTickets(tickets: BoardTicket[]): BoardTicket[] {
  const activeTickets = tickets.filter((ticket) => ticket.boardStatus !== "done");
  const doneTickets = tickets.filter((ticket) => ticket.boardStatus === "done");
  return [...activeTickets, ...doneTickets];
}

function hasProjectBoardImagePastePayload(clipboardData: DataTransfer): boolean {
  /**
   * CDXC:ProjectBoardImagePaste 2026-05-28-08:18:
   * Image paste detection must stay synchronous so the caller prevents the browser's default data-URI Markdown insertion before native resolves the clipboard to a durable image path.
   *
   * CDXC:ProjectBoardImagePaste 2026-05-28-08:27:
   * New Project Board image pastes should persist a path, not a base64 payload. If the clipboard has a file or path, native returns that path; if it only has bitmap data, native saves the bitmap under the resolved Ghostex image directory like the rich prompt editor and returns the saved path.
   */
  const files = [...clipboardData.files];
  if (files.some((file) => file.type.startsWith("image/") || isDescriptionImageSource(file.name))) {
    return true;
  }
  const items = [...clipboardData.items];
  if (items.some((entry) => entry.type.startsWith("image/") || entry.type === "public.file-url")) {
    return true;
  }
  const uriList = clipboardData.getData("text/uri-list").trim();
  if (uriList.startsWith("file:") && isDescriptionImageSource(uriList)) {
    return true;
  }
  const plainText = clipboardData.getData("text/plain").trim();
  return isDescriptionImageSource(plainText);
}

function sendBeadsRequest(
  request: Omit<BeadsBridgeRequest, "requestId">,
): Promise<BeadsBridgeResponse> {
  return new Promise((resolve, reject) => {
    const requestId = crypto.randomUUID();
    const timeout = window.setTimeout(() => {
      window.removeEventListener(BRIDGE_RESPONSE_EVENT, onResponse);
      reject(new Error("Beads command timed out."));
    }, 60_000);
    const onResponse = (event: Event) => {
      const response = (event as CustomEvent<BeadsBridgeResponse>).detail;
      if (response?.requestId !== requestId) {
        return;
      }
      window.clearTimeout(timeout);
      window.removeEventListener(BRIDGE_RESPONSE_EVENT, onResponse);
      resolve(response);
    };
    window.addEventListener(BRIDGE_RESPONSE_EVENT, onResponse);
    const message = { ...request, requestId };
    const projectBeadsBridge = (window as ProjectBeadsWebKitWindow).webkit?.messageHandlers
      ?.ghostexProjectBeads;
    if (projectBeadsBridge) {
      projectBeadsBridge.postMessage(message);
      return;
    }
    if (request.action === "listIssues" && request.cwd) {
      void fetch(
        `file://${request.cwd}/.beads/issues.jsonl`,
      ).then(() => reject(new Error("Beads bridge is unavailable outside Ghostex."))).catch(() => {
        reject(new Error("Beads bridge is unavailable outside Ghostex."));
      });
      return;
    }
    console.info(`${BRIDGE_REQUEST_PREFIX}${JSON.stringify(message)}`);
    reject(new Error("Beads bridge is unavailable outside Ghostex."));
  });
}

function sendProjectBoardRequest<TPayload = ProjectBoardConversationState>(
  request: Omit<ProjectBoardBridgeRequest, "requestId">,
): Promise<ProjectBoardBridgeResponse<TPayload>> {
  return new Promise((resolve, reject) => {
    const requestId = crypto.randomUUID();
    const timeout = window.setTimeout(() => {
      window.removeEventListener(PROJECT_BOARD_RESPONSE_EVENT, onResponse);
      reject(new Error("Project board bridge timed out."));
    }, 60_000);
    const onResponse = (event: Event) => {
      const response = (event as CustomEvent<ProjectBoardBridgeResponse<TPayload>>).detail;
      if (response?.requestId !== requestId) {
        return;
      }
      window.clearTimeout(timeout);
      window.removeEventListener(PROJECT_BOARD_RESPONSE_EVENT, onResponse);
      resolve(response);
    };
    window.addEventListener(PROJECT_BOARD_RESPONSE_EVENT, onResponse);
    const message = { ...request, requestId };
    const projectBoardBridge = (window as ProjectBeadsWebKitWindow).webkit?.messageHandlers
      ?.ghostexProjectBoard;
    if (projectBoardBridge) {
      projectBoardBridge.postMessage(message);
      return;
    }
    window.clearTimeout(timeout);
    window.removeEventListener(PROJECT_BOARD_RESPONSE_EVENT, onResponse);
    reject(new Error("Project board bridge is unavailable outside Ghostex."));
  });
}

function sendProjectBoardImageRequest(
  request: Omit<ProjectBoardImageBridgeRequest, "requestId">,
): Promise<ProjectBoardImageBridgeResponse> {
  return new Promise((resolve, reject) => {
    const requestId = crypto.randomUUID();
    const timeout = window.setTimeout(() => {
      window.removeEventListener(PROJECT_BOARD_IMAGE_RESPONSE_EVENT, onResponse);
      reject(new Error("Project board image bridge timed out."));
    }, 30_000);
    const onResponse = (event: Event) => {
      const response = (event as CustomEvent<ProjectBoardImageBridgeResponse>).detail;
      if (response?.requestId !== requestId) {
        return;
      }
      window.clearTimeout(timeout);
      window.removeEventListener(PROJECT_BOARD_IMAGE_RESPONSE_EVENT, onResponse);
      resolve(response);
    };
    window.addEventListener(PROJECT_BOARD_IMAGE_RESPONSE_EVENT, onResponse);
    const message = { ...request, requestId };
    const projectBoardImagesBridge = (window as ProjectBeadsWebKitWindow).webkit?.messageHandlers
      ?.ghostexProjectBoardImages;
    if (projectBoardImagesBridge) {
      projectBoardImagesBridge.postMessage(message);
      return;
    }
    window.clearTimeout(timeout);
    window.removeEventListener(PROJECT_BOARD_IMAGE_RESPONSE_EVENT, onResponse);
    reject(new Error("Project board image bridge is unavailable outside Ghostex."));
  });
}

const styleElement = document.createElement("style");
styleElement.textContent = `
  :root {
    color-scheme: dark;
    background: var(--app-background, #191919);
    color: #f4f4f5;
    font-family: Inter Variable, -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    --background: var(--app-background, #191919);
    --foreground: oklch(0.985 0 0);
    --card: #171717;
    --card-foreground: oklch(0.985 0 0);
    --popover: #171717;
    --popover-foreground: oklch(0.985 0 0);
    --primary: oklch(0.922 0 0);
    --primary-foreground: oklch(0.205 0 0);
    --secondary: #242424;
    --secondary-foreground: oklch(0.985 0 0);
    --muted: #242424;
    --muted-foreground: oklch(0.708 0 0);
    --accent: #242424;
    --accent-foreground: oklch(0.985 0 0);
    --destructive: oklch(0.704 0.191 22.216);
    --border: oklch(1 0 0 / 10%);
    --input: oklch(1 0 0 / 15%);
    --ring: oklch(0.556 0 0);
    --radius: 6px;
    --project-board-bg: var(--app-background, #191919);
    --project-board-panel: #171717;
    --project-board-panel-hover: #1d1d1d;
    /*
     * CDXC:ProjectBoardCards 2026-06-19-09:14:
     * Kanban card surfaces need a brighter resting background than their lane panels so cards stand out in the macOS Project board.
     * Keep hover one step brighter than the resting card color so hover feedback remains visible after raising the base card tone.
     */
    --project-board-card: #242424;
    --project-board-card-hover: #2b2b2b;
    --project-board-border: rgba(255, 255, 255, 0.1);
    --project-board-border-strong: rgba(255, 255, 255, 0.16);
    --project-board-control-height: 36px;
    --project-board-scrollbar: rgba(255, 255, 255, 0.28);
    /*
     * CDXC:ProjectBoardRoundness 2026-06-29-20:55:
     * The Kanban ticket dialog and board cards/controls should adopt the Settings surface roundness instead of the global square theme.
     * Small chips/labels use the compact radius, interactive controls/cards/inputs use the control radius, and the dialog plus dropdown popups use the section/control radius. Field focus reuses a neutral dimmed border like Settings rather than a saturated focus ring.
     */
    --project-board-radius-compact: 4px;
    --project-board-radius-control: 6px;
    --project-board-radius-section: 10px;
    --project-board-focus-border: color-mix(in srgb, #f4f4f5 58%, var(--project-board-border) 42%);
  }

  * { box-sizing: border-box; }

  body {
    background: var(--project-board-bg);
    margin: 0;
    min-height: 100vh;
    overflow: hidden;
  }

  /*
   * CDXC:ProjectBoard 2026-06-13-13:37:
   * Kanban bead context menus should feel like Ghostex sidebar menus while staying owned by the standalone Project board bundle.
   * Use a transparent fixed backdrop to dismiss the menu and fixed menu coordinates so right-click placement is independent of lane scroll positions.
   */
  .project-board-context-menu-backdrop {
    background: transparent;
    border: 0;
    cursor: default;
    inset: 0;
    margin: 0;
    padding: 0;
    position: fixed;
    z-index: 1190;
  }

  .project-board-ticket-context-menu {
    background: color-mix(in srgb, var(--project-board-panel) 92%, #000 8%);
    border: 1px solid rgba(255, 255, 255, 0.13);
    box-shadow:
      0 14px 28px rgba(0, 0, 0, 0.32),
      0 0 0 1px rgba(255, 255, 255, 0.04);
    display: grid;
    gap: 2px;
    min-width: 164px;
    padding: 6px;
    position: fixed;
    z-index: 1200;
  }

  .project-board-ticket-context-menu-item {
    align-items: center;
    background: transparent;
    border: 0;
    color: rgba(244, 244, 245, 0.88);
    display: flex;
    font: inherit;
    font-size: 12px;
    font-weight: 620;
    gap: 8px;
    min-height: 32px;
    padding: 8px 10px;
    text-align: left;
    white-space: nowrap;
    width: 100%;
  }

  .project-board-ticket-context-menu-item svg {
    flex: 0 0 auto;
    height: 14px;
    width: 14px;
  }

  .project-board-ticket-context-menu-item:hover,
  .project-board-ticket-context-menu-item:focus-visible {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(250, 250, 250, 0.96);
    outline: none;
  }

  .project-board-ticket-context-menu-item:disabled {
    color: rgba(244, 244, 245, 0.34);
    cursor: not-allowed;
  }

  .project-board-ticket-context-menu-item:disabled:hover {
    background: transparent;
  }

  .project-board-ticket-context-menu-item-danger {
    color: rgba(255, 158, 158, 0.92);
  }

  .project-board-ticket-context-menu-item-danger:hover,
  .project-board-ticket-context-menu-item-danger:focus-visible {
    background: rgba(235, 87, 87, 0.16);
    color: #ffd2d2;
  }

  .project-board-shell {
    background: var(--project-board-bg);
    display: flex;
    flex-direction: column;
    gap: 14px;
    height: 100vh;
    min-height: 0;
    overflow: hidden;
    padding: 22px 24px 24px;
  }

  .project-board-shell * {
    border-radius: 0 !important;
  }

  /*
   * CDXC:ProjectBoardRoundness 2026-06-29-20:55:
   * Match the Settings surface look: round the Kanban ticket dialog plus the board's interactive controls and bead cards while large swimlane panels stay square so adjacent lanes keep one shared separator line.
   * Board opt-ins need higher specificity than the .project-board-shell square reset, so they reassert the radius with !important; the ticket dialog and portaled dropdown popups live outside the shell and round without it.
   */
  .project-board-card,
  .project-board-card-conversation,
  .project-board-card [data-slot="button"],
  .project-automation-card,
  .project-automation-run-card,
  .project-automation-card [data-slot="button"],
  .project-automation-run-card [data-slot="button"],
  .project-automation-detail-actions [data-slot="button"],
  .project-automation-detail-section pre,
  .project-automation-detail-run-stack div,
  .project-automation-empty-state-icon,
  .project-automation-empty-action,
  .project-automation-coming-soon-panel,
  .project-automation-coming-soon-icon,
  .project-automation-tab,
  .project-automation-tabs,
  .project-automation-toolbar-button,
  .project-board-toolbar-actions [data-slot="button"],
  .project-board-lane-header-action,
  .project-board-search input,
  .project-board-filter-select {
    border-radius: var(--project-board-radius-control) !important;
  }

  .project-automation-panel,
  .project-automation-split,
  .project-automation-coming-soon {
    border-radius: var(--project-board-radius-section) !important;
  }

  .project-board-card-label {
    border-radius: var(--project-board-radius-compact) !important;
  }

  [data-slot="select-content"],
  [data-slot="popover-content"] {
    border-radius: var(--project-board-radius-control) !important;
  }

  .project-ticket-dialog .rounded-none,
  .project-ticket-dialog [data-slot="input"],
  .project-ticket-dialog [data-slot="textarea"],
  .project-ticket-dialog [data-slot="select-trigger"],
  .project-ticket-dialog [data-slot="button"],
  .project-ticket-dialog [data-slot="dialog-close"],
  .project-ticket-dialog .project-ticket-image-thumb,
  .project-ticket-dialog .project-ticket-image-remove,
  .project-ticket-dialog .project-ticket-comment-list,
  .project-ticket-dialog .project-ticket-comment,
  .project-ticket-dialog .project-ticket-conversation-row {
    border-radius: var(--project-board-radius-control);
  }

  .project-ticket-dialog .project-ticket-label-chip {
    border-radius: var(--project-board-radius-compact);
  }

  /*
   * CDXC:ProjectBoardRoundness 2026-06-29-20:55:
   * Give Kanban form controls the Settings field treatment: a subtle translucent fill, a visible neutral border (select triggers ship transparent borders by default), and a dimmed neutral focus border without the saturated shadcn focus ring.
   */
  .project-ticket-dialog [data-slot="input"],
  .project-ticket-dialog [data-slot="textarea"],
  .project-ticket-dialog [data-slot="select-trigger"],
  .project-board-shell .project-board-search input,
  .project-board-shell .project-board-filter-select {
    background: color-mix(in srgb, var(--input) 30%, transparent);
    border: 1px solid var(--input);
  }

  .project-ticket-dialog [data-slot="input"]:is(:focus, :focus-visible),
  .project-ticket-dialog [data-slot="textarea"]:is(:focus, :focus-visible),
  .project-ticket-dialog [data-slot="select-trigger"]:is(:focus, :focus-visible),
  .project-board-shell .project-board-search input:is(:focus, :focus-visible),
  .project-board-shell .project-board-filter-select:is(:focus, :focus-visible) {
    border-color: var(--project-board-focus-border);
    box-shadow: none;
    outline: none;
  }

  .project-ticket-comment-list [data-slot="scroll-area-viewport"] {
    scrollbar-color: transparent transparent;
    scrollbar-width: none;
  }

  .project-ticket-comment-list:hover [data-slot="scroll-area-viewport"],
  .project-ticket-comment-list:focus-within [data-slot="scroll-area-viewport"] {
    scrollbar-color: var(--project-board-scrollbar) transparent;
  }

  .project-ticket-comment-list [data-slot="scroll-area-viewport"]::-webkit-scrollbar {
    height: 2px;
    width: 2px;
  }

  /*
   * CDXC:BoardScrollbars 2026-08-07:
   * The board strip and every lane body keep the browser's own scrollbar so the
   * bar stays clickable and draggable instead of wheel-only. Chromium paints
   * ::-webkit-scrollbar geometry only while the scroller keeps scrollbar-width
   * at auto and leaves scrollbar-color unset; either one hands rendering to the
   * standard scrollbar and collapses the gutter to 0px, which is why these two
   * scrollers stay out of the hidden-scrollbar rules above. The 8px box is the
   * mouse target and the thumb's transparent borders keep the painted rail at
   * the board's 2px width.
   *
   * CDXC:DialogScrollbar 2026-08-07:
   * The ticket dialog body sat in the hidden-scrollbar rules above, and
   * measuring it in Chromium showed the same wheel-only failure the board had:
   * a 0px gutter, and no scroll from a track click or a thumb drag at any x
   * offset along its right edge. It joins the real-scrollbar rules here. The
   * comment list stays hidden above because its Radix ScrollArea paints its own
   * interactable bar.
   */
  .project-board-lanes,
  .project-board-lane-scroll,
  .project-ticket-dialog-body {
    scrollbar-width: auto;
  }

  .project-board-lanes::-webkit-scrollbar,
  .project-board-lane-scroll::-webkit-scrollbar,
  .project-ticket-dialog-body::-webkit-scrollbar {
    background: transparent;
    display: block;
    height: 8px;
    width: 8px;
  }

  .project-board-lanes::-webkit-scrollbar-track,
  .project-board-lane-scroll::-webkit-scrollbar-track,
  .project-ticket-dialog-body::-webkit-scrollbar-track,
  .project-ticket-comment-list [data-slot="scroll-area-viewport"]::-webkit-scrollbar-track {
    background: transparent;
  }

  .project-board-lanes::-webkit-scrollbar-thumb,
  .project-board-lane-scroll::-webkit-scrollbar-thumb,
  .project-ticket-dialog-body::-webkit-scrollbar-thumb,
  .project-ticket-comment-list [data-slot="scroll-area-viewport"]::-webkit-scrollbar-thumb {
    background: transparent;
  }

  .project-ticket-comment-list:hover [data-slot="scroll-area-viewport"]::-webkit-scrollbar-thumb,
  .project-ticket-comment-list:focus-within [data-slot="scroll-area-viewport"]::-webkit-scrollbar-thumb {
    background: var(--project-board-scrollbar);
  }

  .project-board-lanes::-webkit-scrollbar-thumb {
    background-clip: content-box;
    border-bottom: 3px solid transparent;
    border-top: 3px solid transparent;
  }

  .project-board-lane-scroll::-webkit-scrollbar-thumb,
  .project-ticket-dialog-body::-webkit-scrollbar-thumb {
    background-clip: content-box;
    border-left: 3px solid transparent;
    border-right: 3px solid transparent;
  }

  .project-board-lanes:hover::-webkit-scrollbar-thumb,
  .project-board-lanes:focus-within::-webkit-scrollbar-thumb,
  .project-board-lane:hover .project-board-lane-scroll::-webkit-scrollbar-thumb,
  .project-board-lane:focus-within .project-board-lane-scroll::-webkit-scrollbar-thumb,
  .project-ticket-dialog-body:hover::-webkit-scrollbar-thumb,
  .project-ticket-dialog-body:focus-within::-webkit-scrollbar-thumb {
    background-color: var(--project-board-scrollbar);
  }

  .project-ticket-comment-list [data-slot="scroll-area-scrollbar"] {
    opacity: 0;
    transition: opacity 120ms ease;
    width: 5px;
  }

  .project-ticket-comment-list:hover [data-slot="scroll-area-scrollbar"],
  .project-ticket-comment-list:focus-within [data-slot="scroll-area-scrollbar"] {
    opacity: 1;
  }

  .project-ticket-comment-list [data-slot="scroll-area-thumb"] {
    background: var(--project-board-scrollbar);
  }

  .project-board-toolbar {
    align-items: center;
    display: grid;
    flex: 0 0 auto;
    gap: 12px;
    grid-template-columns: minmax(0, 1fr) auto;
    min-height: 40px;
  }

  .project-board-toolbar[data-surface="automations"] {
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  }

  .project-board-toolbar-heading {
    display: grid;
    gap: 4px;
    justify-self: start;
    min-width: 0;
  }

  .project-automation-eyebrow {
    color: rgba(244, 244, 245, 0.48);
    font-size: 10px;
    font-weight: 750;
    letter-spacing: 0.08em;
    line-height: 1;
    text-transform: uppercase;
  }

  .project-board-toolbar-title {
    color: rgba(250, 250, 250, 0.96);
    font-size: 21px;
    font-weight: 650;
    line-height: 1.15;
    margin: 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-board-toolbar-actions {
    align-items: center;
    display: flex;
    gap: 8px;
    justify-self: end;
  }

  /*
   * CDXC:Automations 2026-06-29-15:55:
   * The first shipped Automation page uses a compact local nav for gxserver
   * definitions, run history, and triage while keeping the Kanban board header
   * unchanged.
   *
   * CDXC:Automations 2026-06-30-10:55:
   * The Automation page tabs should read as a flat Kanban segmented control, not a gradient-backed strip. The Create automation and + Automation actions should use the same height and radius as Kanban's new-ticket action buttons.
   *
   * CDXC:Automations 2026-06-30-21:10:
   * Automations, Runs, and Triage tabs must share the widest tab width so the segmented control feels stable while still sizing from its labels instead of a hard-coded pixel width.
   */
  .project-automation-tabs {
    align-items: center;
    background: var(--project-board-panel);
    border: 1px solid var(--project-board-border);
    display: inline-grid;
    flex: 0 0 auto;
    gap: 3px;
    grid-auto-columns: 1fr;
    grid-auto-flow: column;
    height: var(--project-board-control-height);
    justify-self: center;
    padding: 3px;
    width: fit-content;
  }

  .project-automation-tab {
    align-items: center;
    background: transparent;
    border: 1px solid transparent;
    color: rgba(250, 250, 250, 0.68);
    cursor: pointer;
    display: inline-flex;
    font-size: 12px;
    font-weight: 650;
    height: 28px;
    justify-content: center;
    line-height: 1;
    padding: 0 12px;
    white-space: nowrap;
  }

  .project-automation-tab:hover {
    background: var(--project-board-panel-hover);
    border-color: rgba(255, 255, 255, 0.1);
    color: rgba(250, 250, 250, 0.88);
  }

  .project-automation-tab[data-active="true"] {
    background: var(--project-board-card);
    border-color: var(--project-board-border-strong);
    color: rgba(250, 250, 250, 0.94);
  }

  .project-automation-empty-action,
  .project-automation-toolbar-button {
    height: var(--project-board-control-height);
    min-height: var(--project-board-control-height);
  }

  /*
   * CDXC:ProjectAutomations 2026-06-09-18:40:
   * Automation views use one connected shell: a darker list sidebar on the left and a detail pane on the right with no gutter between them. Both columns share the same height so empty states stay vertically centered together.
   *
   * CDXC:ProjectAutomations 2026-06-09-15:40:
   * Automation split views need centered empty states with icon, title, helper copy, and optional create action so blank Automations/Triage/Runs panels do not look like misaligned top-left placeholders.
   *
   * CDXC:Automations 2026-06-30-10:50:
   * Automation pages should share Kanban's rounded card/control language instead of inheriting the shell's square reset. Use flat Project Board panel/card colors and explicit radius opt-ins, with no gradient backgrounds.
   */
  .project-automation-split {
    background: var(--project-board-panel);
    border: 1px solid var(--project-board-border);
    display: grid;
    flex: 1 1 auto;
    gap: 0;
    grid-template-columns: minmax(280px, 0.9fr) minmax(320px, 1.1fr);
    grid-template-rows: minmax(0, 1fr);
    min-height: 0;
    overflow: hidden;
  }

  .project-automation-panel {
    background: var(--project-board-panel);
    border: 1px solid var(--project-board-border);
    display: flex;
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
  }

  /*
   * CDXC:Automations 2026-07-01-03:24:
   * Automations Overview and project Automate are openable discovery pages, but
   * their real content must stay covered until Enable Experimental Features is
   * on. Use an opaque first-party panel instead of a transparent overlay so
   * disabled users cannot inspect automation definitions, runs, or triage data.
   */
  .project-automation-coming-soon {
    align-items: center;
    background: var(--project-board-panel);
    border: 1px solid var(--project-board-border);
    display: flex;
    flex: 1 1 auto;
    justify-content: center;
    min-height: 0;
    overflow: hidden;
    padding: 28px;
  }

  .project-automation-coming-soon-panel {
    align-items: center;
    background: color-mix(in srgb, var(--project-board-panel) 92%, #fff 8%);
    border: 1px solid var(--project-board-border-strong);
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 420px;
    padding: 28px;
    text-align: center;
  }

  .project-automation-coming-soon-icon {
    align-items: center;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: rgba(244, 244, 245, 0.52);
    display: flex;
    height: 52px;
    justify-content: center;
    width: 52px;
  }

  .project-automation-coming-soon-icon svg {
    height: 26px;
    width: 26px;
  }

  .project-automation-coming-soon-panel span {
    color: rgba(244, 244, 245, 0.48);
    font-size: 10px;
    font-weight: 750;
    letter-spacing: 0.08em;
    line-height: 1;
    text-transform: uppercase;
  }

  .project-automation-coming-soon-panel h2 {
    color: rgba(250, 250, 250, 0.96);
    font-size: 20px;
    font-weight: 650;
    line-height: 1.2;
    margin: 0;
  }

  .project-automation-coming-soon-panel p {
    color: rgba(244, 244, 245, 0.58);
    font-size: 13px;
    line-height: 1.5;
    margin: 0;
    max-width: 340px;
  }

  .project-automation-split > * {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }

  .project-automation-split > :first-child {
    background: var(--project-board-panel);
    border-right: 1px solid var(--project-board-border);
  }

  .project-automation-split > :last-child {
    background: color-mix(in srgb, var(--project-board-panel) 94%, #fff 6%);
  }

  .project-automation-empty-state {
    align-items: center;
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 10px;
    justify-content: center;
    height: 100%;
    min-height: 0;
    padding: 36px 28px;
    text-align: center;
  }

  .project-automation-split > .project-automation-empty-state {
    background: transparent;
    border: none;
  }

  .project-automation-empty-state[data-variant="detail"] {
    padding: 24px;
  }

  .project-automation-empty-state-icon {
    align-items: center;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 12px;
    color: rgba(244, 244, 245, 0.46);
    display: flex;
    height: 52px;
    justify-content: center;
    margin-bottom: 4px;
    width: 52px;
  }

  .project-automation-empty-state-icon svg {
    height: 26px;
    width: 26px;
  }

  .project-automation-empty-state strong {
    color: rgba(250, 250, 250, 0.94);
    font-size: 15px;
    font-weight: 650;
    line-height: 1.25;
  }

  .project-automation-empty-state p {
    color: rgba(244, 244, 245, 0.54);
    font-size: 13px;
    line-height: 1.5;
    margin: 0;
    max-width: 300px;
  }

  .project-automation-split .project-automation-detail {
    background: transparent;
    border: none;
    flex: 1 1 auto;
    min-height: 0;
  }

  .project-automation-split .project-automation-detail:not(.project-automation-detail--empty) {
    --edge-fade-distance: 16px;
    overflow: auto;
    padding: 16px;
  }

  .project-automation-detail--empty {
    align-items: center;
    display: flex;
    flex: 1 1 auto;
    height: 100%;
    justify-content: center;
    min-height: 0;
    padding: 0;
  }

  .project-automation-list,
  .project-automation-run-list {
    --edge-fade-distance: 16px;
    display: grid;
    flex: 1 1 auto;
    gap: 10px;
    grid-auto-rows: min-content;
    min-height: 0;
    overflow: auto;
    padding: 12px;
  }

  .project-automation-card,
  .project-automation-run-card {
    background: var(--project-board-card);
    border-color: var(--project-board-border);
  }

  .project-automation-card:hover,
  .project-automation-run-card:hover {
    background: var(--project-board-card-hover);
  }

  .project-automation-card[data-selected="true"],
  .project-automation-run-card[data-selected="true"] {
    border-color: rgba(244, 244, 245, 0.32);
    box-shadow: inset 0 0 0 1px rgba(244, 244, 245, 0.18);
  }

  .project-automation-card [data-slot="card-content"],
  .project-automation-run-card [data-slot="card-content"] {
    align-items: flex-start;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
  }

  .project-automation-card-main,
  .project-automation-run-main {
    display: grid;
    gap: 6px;
    min-width: 0;
    width: 100%;
  }

  .project-automation-card-title,
  .project-automation-run-heading {
    align-items: center;
    display: flex;
    gap: 8px;
    min-width: 0;
  }

  .project-automation-card-title strong,
  .project-automation-run-heading strong {
    color: rgba(250, 250, 250, 0.96);
    font-size: 14px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-automation-card-title span,
  .project-automation-run-heading span {
    border-radius: 999px;
    color: rgba(250, 250, 250, 0.86);
    flex: 0 0 auto;
    font-size: 11px;
    font-weight: 700;
    padding: 3px 7px;
    text-transform: capitalize;
  }

  .project-automation-card-title span[data-enabled="true"],
  .project-automation-run-heading span[data-status="findings"] {
    background: rgba(111, 207, 151, 0.18);
    color: #8ee4ad;
  }

  .project-automation-card-title span[data-enabled="false"],
  .project-automation-run-heading span[data-status="failed"],
  .project-automation-run-heading span[data-status="needs_attention"] {
    background: rgba(235, 87, 87, 0.18);
    color: #ff9a9a;
  }

  .project-automation-card-main p,
  .project-automation-run-main p,
  .project-automation-card-meta,
  .project-automation-run-meta {
    color: rgba(244, 244, 245, 0.58);
    font-size: 12px;
    margin: 0;
  }

  .project-automation-card-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 2px;
  }

  .project-automation-card-tags span,
  .project-automation-card-meta span,
  .project-automation-run-meta span {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 999px;
    color: rgba(244, 244, 245, 0.68);
    font-size: 11px;
    font-weight: 600;
    padding: 2px 8px;
  }

  .project-automation-card-meta span[data-unread="true"] {
    background: rgba(111, 207, 151, 0.14);
    border-color: rgba(111, 207, 151, 0.24);
    color: #8ee4ad;
  }

  .project-automation-card-agent {
    align-items: center;
    color: rgba(244, 244, 245, 0.72);
    display: inline-flex;
    font-size: 12px;
    gap: 6px;
    margin-top: 4px;
  }

  .project-automation-card-meta,
  .project-automation-run-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 4px;
  }

  .project-automation-card-actions {
    align-items: center;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    display: flex;
    flex: 0 0 auto;
    gap: 6px;
    justify-content: flex-end;
    padding-top: 10px;
    width: 100%;
  }

  .project-automation-run-actions {
    align-items: center;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    display: flex;
    flex: 0 0 auto;
    gap: 6px;
    justify-content: flex-end;
    padding-top: 10px;
    width: 100%;
  }

  .project-automation-detail {
    display: grid;
    gap: 14px;
    grid-auto-rows: min-content;
    min-height: 0;
  }

  .project-automation-detail:not(.project-automation-detail--empty) {
    --edge-fade-distance: 16px;
    overflow: auto;
  }

  .project-automation-detail-header {
    align-items: flex-start;
    display: flex;
    gap: 12px;
    justify-content: space-between;
    min-width: 0;
  }

  .project-automation-detail-header h2 {
    color: rgba(250, 250, 250, 0.96);
    font-size: 18px;
    line-height: 1.2;
    margin: 6px 0 0;
  }

  .project-automation-detail-header span,
  .project-automation-detail-run-stack span {
    border-radius: 999px;
    color: rgba(250, 250, 250, 0.86);
    display: inline-flex;
    font-size: 11px;
    font-weight: 700;
    padding: 3px 7px;
    text-transform: capitalize;
  }

  .project-automation-detail-header span[data-enabled="true"],
  .project-automation-detail-header span[data-status="findings"],
  .project-automation-detail-run-stack span[data-status="findings"] {
    background: rgba(111, 207, 151, 0.18);
    color: #8ee4ad;
  }

  .project-automation-detail-header span[data-enabled="false"],
  .project-automation-detail-header span[data-status="failed"],
  .project-automation-detail-header span[data-status="needs_attention"],
  .project-automation-detail-run-stack span[data-status="failed"],
  .project-automation-detail-run-stack span[data-status="needs_attention"] {
    background: rgba(235, 87, 87, 0.18);
    color: #ff9a9a;
  }

  .project-automation-detail-actions {
    align-items: center;
    display: flex;
    flex: 0 0 auto;
    gap: 6px;
  }

  .project-automation-detail-grid {
    display: grid;
    gap: 10px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    margin: 0;
  }

  .project-automation-detail-grid div {
    min-width: 0;
  }

  .project-automation-detail-grid dt,
  .project-automation-detail-section h3 {
    color: rgba(244, 244, 245, 0.52);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0;
    margin: 0 0 4px;
    text-transform: uppercase;
  }

  .project-automation-detail-grid dd,
  .project-automation-detail-section p,
  .project-automation-detail-run-stack p {
    color: rgba(244, 244, 245, 0.78);
    font-size: 12px;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .project-automation-detail-grid dd {
    align-items: center;
    display: flex;
    gap: 6px;
    min-width: 0;
  }

  .project-automation-detail-grid dd span {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .project-automation-detail-section {
    display: grid;
    gap: 8px;
    min-width: 0;
  }

  .project-automation-detail-section pre {
    background: rgba(0, 0, 0, 0.22);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 7px;
    color: rgba(244, 244, 245, 0.82);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 12px;
    line-height: 1.45;
    margin: 0;
    max-height: 220px;
    overflow: auto;
    padding: 10px;
    white-space: pre-wrap;
  }

  .project-automation-detail-run-stack {
    display: grid;
    gap: 8px;
  }

  .project-automation-detail-run-stack div {
    align-items: center;
    background: rgba(255, 255, 255, 0.045);
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 7px;
    display: flex;
    justify-content: space-between;
    padding: 8px 10px;
  }

  .project-automation-dialog {
    max-width: min(780px, calc(100vw - 44px));
    width: 780px;
  }

  .project-automation-form {
    gap: 14px;
  }

  .project-automation-form label,
  .project-automation-field-full {
    color: rgba(244, 244, 245, 0.72);
    display: grid;
    font-size: 12px;
    font-weight: 650;
    gap: 6px;
  }

  .project-automation-field-full {
    grid-column: 1 / -1;
  }

  .project-automation-form-grid {
    display: grid;
    gap: 12px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .project-automation-form-section {
    display: grid;
    gap: 10px;
  }

  .project-automation-form-section-title {
    color: rgba(244, 244, 245, 0.52);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .project-automation-select {
    height: var(--project-board-control-height);
    min-width: 0;
    width: 100%;
  }

  .project-automation-agent-option {
    align-items: center;
    display: inline-flex;
    gap: 8px;
    min-width: 0;
  }

  .project-automation-agent-icon {
    display: block;
    flex: 0 0 auto;
    height: 14px;
    mask-position: center;
    mask-repeat: no-repeat;
    mask-size: contain;
    width: 14px;
    -webkit-mask-position: center;
    -webkit-mask-repeat: no-repeat;
    -webkit-mask-size: contain;
  }

  .project-automation-prompt-field textarea {
    min-height: 150px;
  }

  .project-automation-dialog [data-slot="input"],
  .project-automation-dialog [data-slot="textarea"],
  .project-automation-dialog [data-slot="select-trigger"] {
    background: color-mix(in srgb, var(--input) 30%, transparent);
    border: 1px solid var(--input);
  }

  .project-automation-dialog [data-slot="input"]:is(:focus, :focus-visible),
  .project-automation-dialog [data-slot="textarea"]:is(:focus, :focus-visible),
  .project-automation-dialog [data-slot="select-trigger"]:is(:focus, :focus-visible) {
    border-color: var(--project-board-focus-border);
    box-shadow: none;
    outline: none;
  }

  .project-automation-segmented {
    background: rgba(255, 255, 255, 0.055);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    padding: 3px;
  }

  .project-automation-segmented button {
    background: transparent;
    border: 0;
    border-radius: 6px;
    color: rgba(244, 244, 245, 0.72);
    font: inherit;
    font-size: 12px;
    font-weight: 700;
    height: 30px;
  }

  .project-automation-segmented button[data-active="true"] {
    background: rgba(244, 244, 245, 0.9);
    color: #151617;
  }

  .project-automation-segmented button:disabled {
    color: rgba(244, 244, 245, 0.32);
    cursor: not-allowed;
  }

  .project-automation-segmented button:disabled[data-active="true"] {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(244, 244, 245, 0.42);
  }

  .project-automation-inline-note {
    color: rgba(244, 244, 245, 0.54);
    font-size: 12px;
    line-height: 1.4;
    margin: -4px 0 0;
  }

  .project-automation-enabled {
    align-items: center;
    display: flex !important;
    flex-direction: row;
    gap: 8px;
  }

  .project-automation-card-toggle {
    align-items: center;
    display: flex;
    flex-direction: row;
    gap: 6px;
  }

  .project-automation-card-toggle span,
  .project-automation-detail-toggle span {
    color: var(--project-board-muted);
    font-size: 12px;
    line-height: 1;
  }

  .project-automation-card-toggle span[data-enabled="true"],
  .project-automation-detail-toggle span[data-enabled="true"] {
    color: var(--project-board-accent);
  }

  .project-automation-detail-toggle {
    align-items: center;
    display: flex;
    flex-direction: row;
    gap: 8px;
  }

  @media (max-width: 860px) {
    .project-automation-split {
      grid-template-columns: 1fr;
      grid-template-rows: auto minmax(0, 1fr);
    }

    .project-automation-split > :first-child {
      border-bottom: 1px solid rgba(255, 255, 255, 0.08);
      border-right: none;
    }

    .project-automation-form-grid {
      grid-template-columns: 1fr;
    }

    .project-automation-select {
      width: 100%;
    }
  }

  .project-board-filters {
    align-items: center;
    display: flex;
    flex: 0 0 auto;
    gap: 10px;
    min-width: 0;
  }

  .project-board-search {
    align-items: center;
    display: flex;
    flex: 1 1 auto;
    min-width: 0;
    position: relative;
  }

  .project-board-search-icon {
    color: rgba(244, 244, 245, 0.42);
    height: 16px;
    pointer-events: none;
    position: absolute;
    right: 12px;
    width: 16px;
    z-index: 1;
  }

  .project-board-search input {
    height: var(--project-board-control-height);
    padding-right: 36px;
  }

  .project-board-search-clear-button {
    align-items: center;
    background: transparent;
    border: none;
    border-radius: 0;
    color: rgba(244, 244, 245, 0.42);
    display: inline-flex;
    height: 24px;
    justify-content: center;
    padding: 0;
    position: absolute;
    right: 8px;
    top: 50%;
    transform: translateY(-50%);
    width: 24px;
    z-index: 1;
  }

  .project-board-search-clear-button:hover,
  .project-board-search-clear-button:focus-visible {
    color: rgba(244, 244, 245, 0.78);
    outline: none;
  }

  .project-board-search-clear-button svg {
    height: 16px;
    pointer-events: none;
    width: 16px;
  }

  .project-board-filter-select,
  .project-board-ticket-button {
    height: var(--project-board-control-height);
    min-width: 124px;
  }

  .project-board-ticket-button {
    min-width: 0;
  }

  .project-board-board-region {
    display: flex;
    flex: 1 1 auto;
    min-height: 0;
    min-width: 0;
    position: relative;
  }

  .project-board-loading-overlay {
    align-items: center;
    background: rgba(10, 10, 10, 0.48);
    color: rgba(244, 244, 245, 0.9);
    display: flex;
    inset: 0;
    justify-content: center;
    pointer-events: auto;
    position: absolute;
    z-index: 20;
  }

  .project-board-loading-spinner {
    animation: project-board-loading-spin 850ms linear infinite;
    filter: drop-shadow(0 2px 8px rgba(0, 0, 0, 0.38));
  }

  @keyframes project-board-loading-spin {
    to { transform: rotate(360deg); }
  }

  .project-board-lanes {
    align-items: stretch;
    display: grid;
    flex: 1 1 auto;
    /*
     * CDXC:ProjectBoardLanes 2026-06-19-09:59:
     * Kanban cards need more usable width, so swimlanes should sit directly beside each other instead of spending horizontal space on gutters.
     * Keep the existing lane grid structure and let the lane border act as the visible separator.
     *
     * CDXC:ScrollFades 2026-06-19-14:16:
     * The Project Board should use the same Codex-style edge fade as the
     * sidebar scroll surface. The board strip owns the horizontal fade while
     * each lane body owns its vertical fade, leaving lane headers and custom
     * scrollbars unmasked.
     *
     * CDXC:BoardScrollbars 2026-08-07:
     * The lane bar is the scroller's own scrollbar now, so it lives inside the
     * mask and fades at the very ends of its travel like the ticket dialog's
     * scrollbar already does.
     */
    --edge-fade-distance: 18px;
    gap: 0;
    grid-template-columns: repeat(6, minmax(218px, 1fr));
    min-height: 0;
    overflow-x: auto;
    overflow-y: hidden;
    padding-bottom: 0;
  }

  .project-board-lane {
    background: var(--project-board-panel);
    border: 1px solid var(--project-board-border);
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 218px;
    overflow: hidden;
    position: relative;
  }

  .project-board-lane + .project-board-lane {
    /*
     * CDXC:ProjectBoardLanes 2026-06-19-09:59:
     * Adjacent zero-gap swimlanes must meet on one separator line, not two stacked borders.
     * Remove the following lane's left border so the previous lane's right border owns the shared boundary.
     */
    border-left-width: 0;
  }

  .project-board-lane[data-drop-target="true"] {
    background: var(--project-board-panel-hover);
    border-color: var(--project-board-border-strong);
  }

  .project-board-lane-header {
    align-items: center;
    display: flex;
    flex: 0 0 auto;
    justify-content: space-between;
    min-height: 44px;
    padding: 0 12px;
  }

  .project-board-lane-header div {
    align-items: center;
    display: flex;
    gap: 8px;
    min-width: 0;
  }

  .project-board-lane-header h2,
  .project-board-lane-header span {
    color: rgba(244, 244, 245, 0.68);
    font-size: 12px;
    font-weight: 650;
    margin: 0;
  }

  .project-board-lane-header-action {
    height: 28px;
    justify-content: flex-end;
    margin-right: 4px;
    position: relative;
    width: 28px;
  }

  .project-board-lane-count,
  .project-board-lane-add {
    transition: opacity 120ms ease;
  }

  .project-board-lane-count {
    display: block;
    min-width: 100%;
    opacity: 1;
    text-align: right;
  }

  .project-board-lane-add {
    opacity: 0;
    pointer-events: none;
    position: absolute;
    right: -3px;
    top: 0;
  }

  .project-board-lane:hover .project-board-lane-count,
  .project-board-lane:focus-within .project-board-lane-count {
    opacity: 0;
  }

  .project-board-lane:hover .project-board-lane-add,
  .project-board-lane:focus-within .project-board-lane-add {
    opacity: 1;
    pointer-events: auto;
  }

  .project-board-lane-dot {
    background: rgba(244, 244, 245, 0.42);
    display: inline-block;
    height: 7px;
    width: 7px;
  }

  .project-board-lane[data-tone="muted"] .project-board-lane-dot { background: #8f9aa7; }
  .project-board-lane[data-tone="blue"] .project-board-lane-dot { background: #5ea4ff; }
  .project-board-lane[data-tone="amber"] .project-board-lane-dot { background: #e7b85b; }
  .project-board-lane[data-tone="violet"] .project-board-lane-dot { background: #b18cff; }
  .project-board-lane[data-tone="green"] .project-board-lane-dot { background: #95d7f6; }

  .project-board-lane-scroll {
    --edge-fade-distance: 18px;
    flex: 1 1 auto;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    padding-right: 0;
  }

  .project-board-card-stack {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    padding: 0 10px 10px;
  }

  .project-board-lane-limit {
    border: 1px dashed rgba(255, 255, 255, 0.12);
    color: rgba(244, 244, 245, 0.48);
    font-size: 11px;
    line-height: 1.4;
    padding: 10px 12px;
  }

  .project-board-card {
    /*
     * CDXC:ProjectBoardCards 2026-06-13-13:55:
     * Kanban bead cards are click, drag, and context-menu targets, so their text should not become selected by accidental pointer movement.
     * Disable selection at the card surface while keeping editable ticket dialog fields selectable.
     */
    background: var(--project-board-card);
    border: 1px solid var(--project-board-border);
    box-shadow: 0 1px 0 rgba(0, 0, 0, 0.28);
    cursor: default;
    gap: 0;
    max-width: 100%;
    min-width: 0;
    padding: 0;
    user-select: none;
    width: 100%;
  }

  .project-board-card:hover { background-color: var(--project-board-card-hover); }
  .project-board-card[data-dragging="true"] { opacity: 0.55; }

  .project-board-card-header {
    gap: 5px;
    min-width: 0;
    padding: 11px 12px 0;
  }

  .project-board-card-header [data-slot="card-title"] {
    color: rgba(250, 250, 250, 0.91);
    font-size: 13px;
    font-weight: 560;
    line-height: 1.35;
    overflow-wrap: anywhere;
    word-break: break-word;
  }

  .project-board-card-header [data-slot="card-description"] {
    color: rgba(244, 244, 245, 0.39);
    font-size: 11px;
  }

  .project-board-card-content {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
    padding: 8px 12px 11px;
  }

  .project-board-card-content p {
    color: rgba(244, 244, 245, 0.55);
    display: -webkit-box;
    font-size: 12px;
    line-height: 1.42;
    margin: 0;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    overflow: hidden;
    overflow-wrap: anywhere;
    word-break: break-word;
  }

  .project-board-card-labels {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .project-board-card-label {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(244, 244, 245, 0.72);
    font-size: 10px;
    line-height: 1;
    padding: 4px 7px;
  }

  .project-board-card-meta {
    align-items: center;
    color: rgba(244, 244, 245, 0.46);
    display: flex;
    flex-wrap: wrap;
    font-size: 11px;
    gap: 8px;
    line-height: 1;
  }

  .project-board-priority {
    color: rgba(244, 244, 245, 0.72);
    font-weight: 680;
  }

  .project-board-comments {
    align-items: center;
    display: inline-flex;
    gap: 4px;
    margin-left: auto;
  }

  .project-board-comments svg {
    height: 13px;
    width: 13px;
  }

  .project-board-card-conversation {
    align-items: center;
    background: rgba(80, 160, 255, 0.08);
    border: 1px solid rgba(120, 180, 255, 0.15);
    color: rgba(218, 235, 255, 0.86);
    display: flex;
    gap: 8px;
    justify-content: space-between;
    min-height: 30px;
    min-width: 0;
    padding: 4px 5px 4px 8px;
  }

  .project-board-card-conversation span {
    align-items: center;
    display: inline-flex;
    font-size: 11px;
    gap: 5px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-board-card-conversation-label {
    /*
     * CDXC:ProjectBoard 2026-05-28-10:14:
     * Board-card associated session names must show a literal ellipsis when
     * the card is too narrow, while the trailing jump button remains visible.
     * Give the text cluster a zero flex basis and override the broader span
     * rule on the actual tooltip trigger so Chromium/WebKit calculate
     * text-overflow instead of clipping the label.
     */
    flex: 1 1 0;
    max-width: 100%;
    min-width: 0;
    overflow: hidden;
  }

  .project-board-card-conversation-label .project-board-card-conversation-name {
    display: block;
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-board-card-conversation-extra {
    flex: 0 0 auto;
  }

  .project-board-card-conversation svg {
    flex: 0 0 auto;
    height: 13px;
    width: 13px;
  }

  .project-board-notice {
    background: var(--project-board-panel);
    border: 1px solid var(--project-board-border);
    box-shadow: 0 12px 34px rgba(0, 0, 0, 0.22);
    color: rgba(244, 244, 245, 0.9);
    flex: 0 0 auto;
  }

  .project-board-notice[data-kind="init"] {
    border-color: rgba(231, 184, 91, 0.28);
  }

  .project-board-notice[data-kind="install"] {
    border-color: rgba(94, 164, 255, 0.26);
  }

  .project-board-notice[data-kind="install"] .project-board-notice-icon {
    background: rgba(94, 164, 255, 0.12);
    border-color: rgba(94, 164, 255, 0.2);
    color: #7ab7ff;
  }

  .project-board-notice [data-slot="card-content"] {
    align-items: flex-start;
    display: flex;
    gap: 12px;
    padding: 14px;
  }

  .project-board-notice-icon {
    align-items: center;
    background: rgba(231, 184, 91, 0.13);
    border: 1px solid rgba(231, 184, 91, 0.2);
    color: #e7b85b;
    display: flex;
    flex: 0 0 auto;
    height: 34px;
    justify-content: center;
    width: 34px;
  }

  .project-board-notice-icon svg {
    height: 17px;
    width: 17px;
  }

  .project-board-notice-body {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }

  .project-board-notice strong {
    color: rgba(250, 250, 250, 0.94);
    font-size: 13px;
    font-weight: 680;
    letter-spacing: 0;
    line-height: 1.2;
  }

  .project-board-notice p {
    color: rgba(244, 244, 245, 0.64);
    font-size: 12px;
    line-height: 1.45;
    margin: 0;
    max-width: 660px;
  }

  .project-board-notice-command {
    align-items: center;
    align-self: flex-start;
    background: rgba(0, 0, 0, 0.22);
    border: 1px solid rgba(255, 255, 255, 0.08);
    display: inline-flex;
    gap: 7px;
    min-height: 30px;
    padding: 3px 4px 3px 9px;
  }

  .project-board-notice-command code {
    color: rgba(250, 250, 250, 0.9);
    font-family: "SF Mono", ui-monospace, monospace;
    font-size: 12px;
    line-height: 1;
    white-space: nowrap;
  }

  .project-board-notice-command button {
    color: rgba(244, 244, 245, 0.58);
    height: 22px;
    width: 22px;
  }

  .project-board-notice-command button:hover {
    color: rgba(250, 250, 250, 0.92);
  }

  .project-ticket-dialog {
    /*
     * CDXC:ProjectBoard 2026-05-28-13:52:
     * Project ticket edit/create dialogs should use the same modal background
     * as the rest of Ghostex app-modal surfaces.
     *
     * CDXC:SidebarTheme 2026-06-15-01:43:
     * Project Board dialogs follow --app-modal-background so Dark 1 uses
     * #191919 while Dark 2 preserves the previous #0e0e0e surface.
     */
    background: var(--app-modal-background, #191919);
    background-color: var(--app-modal-background, #191919);
    border-radius: var(--project-board-radius-section);
    max-width: min(780px, calc(100vw - 44px));
    overflow: hidden;
    width: 780px;
  }

  .project-ticket-dialog-body {
    --edge-fade-distance: 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-height: min(72vh, 760px);
    min-height: 0;
    overflow: auto;
  }

  .project-ticket-dialog-footer {
    /*
     * CDXC:ProjectBoardTicketEditor 2026-05-28-08:02:
     * The ticket editor footer should not distribute Delete, Start work, and Save as left, center, and right islands. Keep the destructive Delete action isolated while grouping the workflow and save actions together at the right edge.
     */
    align-items: center;
    flex-direction: row;
    flex-wrap: wrap;
    gap: 8px;
    justify-content: space-between;
  }

  .project-ticket-dialog-primary-actions {
    align-items: center;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    justify-content: flex-end;
    margin-left: auto;
  }

  .project-ticket-create-footer {
    /*
     * CDXC:ProjectBoard 2026-05-28-12:32:
     * New-ticket creation now has two outcomes: queue the bead, or create it and
     * immediately launch work in the selected execution location. Keep agent and
     * location controls grouped with Create & Start so plain Create remains a
     * simple board operation while the start path is explicit.
     */
    align-items: end;
    display: grid;
    gap: 12px;
    grid-template-columns: minmax(0, 1fr) auto;
  }

  .project-ticket-create-start {
    display: flex;
    flex-direction: column;
    gap: 7px;
    min-width: 0;
  }

  .project-ticket-create-start-controls {
    align-items: center;
    display: grid;
    gap: 8px;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    justify-items: stretch;
    min-width: 0;
  }

  .project-ticket-footer-select,
  .project-ticket-meta-grid [data-slot="select-trigger"],
  .project-ticket-conversation-controls [data-slot="select-trigger"] {
    height: var(--project-board-control-height);
    min-width: 0;
    width: 100%;
  }

  .project-ticket-title-input,
  .project-ticket-label-editor input {
    height: var(--project-board-control-height);
    min-height: var(--project-board-control-height);
  }

  .project-ticket-create-actions {
    align-items: center;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .project-ticket-create-actions {
    justify-content: flex-end;
  }

  .project-ticket-dialog-footer [data-slot="button"],
  .project-ticket-create-actions > [data-slot="button"],
  .project-ticket-label-editor > [data-slot="button"],
  .project-ticket-conversation-controls > [data-slot="button"] {
    /*
     * CDXC:ProjectBoardForms 2026-06-21-15:30:
     * New-ticket and edit-ticket action buttons must match the adjacent Project Board dropdown height so macOS Kanban dialog control rows align instead of mixing shadcn's default button height with taller select triggers.
     *
     * CDXC:ProjectBoardForms 2026-06-22-02:17:
     * Top-of-dialog Kanban modal dropdowns, label add controls, and ticket title text fields must use the same Project Board control height as the footer buttons so the create/edit dialogs do not show mismatched control rows.
     */
    height: var(--project-board-control-height);
  }

  .project-ticket-meta-grid {
    display: grid;
    gap: 12px;
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .project-ticket-field {
    color: rgba(244, 244, 245, 0.58);
    display: flex;
    flex-direction: column;
    font-size: 12px;
    font-weight: 600;
    gap: 7px;
    min-width: 0;
  }

  .project-ticket-field-inline {
    gap: 6px;
  }

  .project-ticket-field textarea,
  .project-ticket-field input {
    color: rgba(250, 250, 250, 0.92);
    max-width: 100%;
    min-width: 0;
    overflow-wrap: anywhere;
    word-break: break-word;
  }

  .project-ticket-prompt-input {
    min-height: 190px;
  }

  .project-ticket-title-input {
    /*
    CDXC:ProjectBoardTickets 2026-06-15-21:00:
    Ticket title editing is a single-line text field. Keep the create/edit title control at one input row so it does not inherit prompt textarea height or wrap its value like long-form content.
    */
    line-height: 18px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-ticket-label-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .project-ticket-label-chip {
    align-items: center;
    background: rgba(255, 255, 255, 0.08);
    border: 0;
    color: rgba(244, 244, 245, 0.82);
    cursor: pointer;
    display: inline-flex;
    font-size: 11px;
    gap: 4px;
    padding: 4px 8px;
  }

  .project-ticket-label-chip svg {
    height: 12px;
    width: 12px;
  }

  .project-ticket-label-editor {
    align-items: center;
    display: flex;
    gap: 8px;
  }

  .project-ticket-label-editor input {
    flex: 1 1 auto;
  }

  .project-ticket-image-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  /*
   * CDXC:ProjectBoard 2026-05-31-07:15:
   * Prompt image thumbnails below the ticket Prompt field open a full-screen
   * preview on click with a dark overlay; any click on the overlay dismisses
   * the preview and the enlarged image is capped at 90vw by 90vh.
   */
  .project-ticket-image-popup {
    align-items: center;
    background: rgb(0 0 0 / 74%);
    display: flex;
    inset: 0;
    justify-content: center;
    padding: 28px;
    position: fixed;
    z-index: 2000;
  }

  .project-ticket-image-popup img {
    box-shadow: 0 18px 60px rgb(0 0 0 / 50%);
    max-height: 90vh;
    max-width: 90vw;
    object-fit: contain;
  }

  .project-ticket-image-thumb {
    background: rgba(0, 0, 0, 0.24);
    border: 1px solid rgba(255, 255, 255, 0.1);
    display: block;
    height: 72px;
    overflow: hidden;
    position: relative;
    width: 72px;
  }

  .project-ticket-image-thumb[role="button"] {
    cursor: pointer;
  }

  .project-ticket-image-thumb[role="button"]:hover,
  .project-ticket-image-thumb[role="button"]:focus-visible {
    border-color: rgba(255, 255, 255, 0.28);
  }

  .project-ticket-image-thumb img {
    height: 100%;
    object-fit: cover;
    width: 72px;
  }

  .project-ticket-image-thumb span {
    background: linear-gradient(135deg, rgba(255, 255, 255, 0.08), rgba(255, 255, 255, 0.02));
    display: block;
    height: 100%;
    width: 100%;
  }

  .project-ticket-image-remove {
    align-items: center;
    background: rgba(10, 10, 12, 0.78);
    border: 1px solid rgba(255, 255, 255, 0.16);
    color: rgba(255, 255, 255, 0.9);
    cursor: pointer;
    display: inline-flex;
    height: 22px;
    justify-content: center;
    padding: 0;
    position: absolute;
    right: 4px;
    top: 4px;
    width: 22px;
  }

  .project-ticket-image-remove svg {
    height: 13px;
    width: 13px;
  }

  .project-ticket-image-remove:hover {
    background: rgba(32, 32, 36, 0.94);
  }

  .project-ticket-dependencies {
    color: rgba(244, 244, 245, 0.62);
    font-size: 12px;
  }

  .project-ticket-dependencies p {
    margin: 0 0 4px;
  }

  .project-ticket-conversations {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .project-ticket-conversation-controls {
    align-items: center;
    display: grid;
    gap: 8px;
    grid-template-columns: minmax(150px, 1fr) auto;
  }

  .project-ticket-conversation-list {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .project-ticket-conversation-row {
    align-items: center;
    background: rgba(255, 255, 255, 0.035);
    border: 1px solid rgba(255, 255, 255, 0.08);
    display: grid;
    gap: 10px;
    grid-template-columns: minmax(0, 1fr) auto;
    min-height: 42px;
    padding: 7px 8px 7px 10px;
  }

  .project-ticket-conversation-main {
    /*
     * CDXC:ProjectBoard 2026-05-28-09:17:
     * Ticket conversation rows must preserve the right-side jump/unlink controls
     * at narrow widths while the associated session name truncates with an
     * ellipsis and exposes the full name through the hover tooltip.
     *
     * CDXC:ProjectBoard 2026-05-28-10:14:
     * The associated-session tooltip should open below the session name so it
     * does not cover the title area while inspecting a ticket.
     */
    min-width: 0;
    overflow: hidden;
  }

  .project-ticket-conversation-name,
  .project-ticket-conversation-status {
    display: block;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-ticket-conversation-name {
    color: rgba(250, 250, 250, 0.9);
    font-size: 12px;
    font-weight: 620;
  }

  .project-ticket-conversation-status {
    color: rgba(244, 244, 245, 0.46);
    font-size: 11px;
    margin-top: 2px;
  }

  .project-ticket-conversation-actions {
    align-items: center;
    display: flex;
    gap: 4px;
  }

  .project-ticket-comments {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .project-ticket-section-title {
    color: rgba(244, 244, 245, 0.58);
    font-size: 12px;
    font-weight: 650;
  }

  .project-ticket-comment-list {
    --edge-fade-distance: 14px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.08);
    max-height: 180px;
    min-height: 92px;
    padding: 6px;
  }

  .project-ticket-comment-list [data-slot="scroll-area-viewport"] > div {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /*
   * CDXC:ProjectBoardComments 2026-06-05-06:43:
   * Ticket comments in the edit dialog need readable author/date separation, author (agent) attribution, and a bottom-aligned full session id while preserving multiline comment text.
   */
  .project-ticket-comment {
    background: rgba(250, 250, 250, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-left: 2px solid rgba(125, 211, 252, 0.72);
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
  }

  .project-ticket-empty {
    padding: 12px;
  }

  .project-ticket-comment-header {
    align-items: baseline;
    display: flex;
    gap: 10px;
    justify-content: space-between;
    min-width: 0;
  }

  .project-ticket-comment-author-row {
    align-items: baseline;
    display: flex;
    gap: 4px;
    min-width: 0;
  }

  .project-ticket-comment-author {
    color: rgba(250, 250, 250, 0.94);
    font-size: 13px;
    font-weight: 700;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-ticket-comment-agent {
    color: rgba(186, 230, 253, 0.86);
    font-size: 12px;
    font-weight: 620;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .project-ticket-comment-date {
    color: rgba(244, 244, 245, 0.46);
    flex: 0 0 auto;
    font-size: 11px;
    font-weight: 600;
  }

  .project-ticket-comment p,
  .project-ticket-empty {
    color: rgba(244, 244, 245, 0.72);
    font-size: 13px;
    line-height: 1.45;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .project-ticket-comment p {
    margin: 0;
  }

  .project-ticket-comment-session {
    align-items: center;
    border-top: 1px solid rgba(255, 255, 255, 0.07);
    color: rgba(244, 244, 245, 0.48);
    display: flex;
    gap: 8px;
    justify-content: space-between;
    min-width: 0;
    padding-top: 8px;
  }

  .project-ticket-comment-session span {
    flex: 0 0 auto;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0;
    text-transform: uppercase;
  }

  .project-ticket-comment-session code {
    color: rgba(244, 244, 245, 0.74);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
    font-size: 11px;
    min-width: 0;
    overflow-wrap: anywhere;
    text-align: right;
  }

  @media (max-width: 900px) {
    .project-board-shell { padding: 18px 16px; }
    .project-ticket-create-footer,
    .project-ticket-create-start-controls {
      grid-template-columns: 1fr;
    }
    .project-ticket-create-actions {
      justify-content: stretch;
    }
    .project-ticket-create-actions > button {
      flex: 1 1 auto;
    }
    .project-ticket-conversation-controls { grid-template-columns: 1fr; }
    .project-ticket-meta-grid { grid-template-columns: 1fr; }
  }
`;
document.head.append(styleElement);

createRoot(document.getElementById("root")!).render(<ProjectBoardApp />);
