import Fuse from "fuse.js";

import type { ProjectBoardConversationLinkView } from "../../shared/bead-conversation-links";

/*
  CDXC:ProjectBoard 2026-05-23-14:10:
  Shared Beads board helpers keep display-id formatting, t-shirt estimate mapping, and filter logic consistent between the Project WKWebView surface and future Storybook coverage.
*/

export type BoardStatusKey = "backlog" | "todo" | "in_progress" | "test" | "review" | "done";

export type BeadsComment = {
  author?: string;
  created_at?: string;
  text?: string;
};

export type ProjectBoardCommentMetadata = {
  agentName?: string;
  sessionId?: string;
};

export type ParsedProjectBoardComment = ProjectBoardCommentMetadata & {
  body: string;
};

export type BeadsDependency = {
  created_at?: string;
  created_by?: string;
  depends_on_id: string;
  issue_id: string;
  type?: string;
};

export type BeadsIssue = {
  assignee?: string;
  blocked_by?: string[];
  blocks?: string[];
  closed_at?: string;
  comment_count?: number;
  comments?: BeadsComment[];
  created_at?: string;
  created_by?: string;
  dependencies?: BeadsDependency[];
  dependency_count?: number;
  dependent_count?: number;
  description?: string;
  estimate?: number | null;
  id: string;
  issue_type?: string;
  labels?: string[];
  priority?: number;
  status: string;
  title: string;
  updated_at?: string;
};

export type BoardTicket = BeadsIssue & {
  boardStatus: BoardStatusKey;
  displayId: string;
};

export type BeadsBridgeAction =
  | "addComment"
  | "addLabel"
  | "configGet"
  | "configGetIssuePrefix"
  | "configSet"
  | "create"
  | "delete"
  | "depAdd"
  | "depRemove"
  | "generateTitle"
  | "list"
  | "listIssues"
  | "listAllLabels"
  | "renamePrefix"
  | "removeLabel"
  | "search"
  | "setLabels"
  | "show"
  | "updateDescription"
  | "updateEstimate"
  | "updatePriority"
  | "updateStatus"
  | "updateTitle";

export type BeadsBridgeRequest = {
  /*
   * CDXC:PromptAgents 2026-05-29-10:53:
   * Project-board generated ticket titles should use the board's selected/default
   * prompt agent instead of hardcoding Codex in the board bridge request.
   *
   * CDXC:ProjectBoard 2026-06-02-15:18:
   * This request shape is a WebKit/UI bridge contract only. gxserver owns the Beads action execution behind the bridge, so request fields must describe intent rather than native subprocess commands.
   */
  action: BeadsBridgeAction;
  agentCommand?: string;
  agentId?: string;
  comment?: string;
  cwd: string;
  dependsOnId?: string;
  depType?: string;
  description?: string;
  estimate?: number;
  issueId?: string;
  label?: string;
  labels?: string[];
  priority?: string;
  projectId?: string;
  prompt?: string;
  query?: string;
  remoteMachineId?: string;
  requestId: string;
  status?: string;
  title?: string;
  value?: string;
};

export type BeadsBridgeResponse = {
  error?: string;
  exitCode: number;
  requestId: string;
  stderr: string;
  stdout: string;
};

export const BOARD_COLUMNS: Array<{
  key: BoardStatusKey;
  label: string;
  beadsStatus: string;
  tone: string;
}> = [
  /*
    CDXC:ProjectBoard 2026-05-30-08:58:
    The Kanban Project view needs a Backlog swim lane positioned before Todo, persisted as the Beads custom status `backlog` so drag/drop, edit-status selects, and reloads all share the same workflow state.
    New ticket creation remains in Todo; Backlog is an explicit planning lane users move work into.
  */
  { key: "backlog", label: "Backlog", beadsStatus: "backlog", tone: "muted" },
  { key: "todo", label: "Todo", beadsStatus: "open", tone: "neutral" },
  { key: "in_progress", label: "In Progress", beadsStatus: "in_progress", tone: "blue" },
  { key: "test", label: "Test", beadsStatus: "test", tone: "amber" },
  { key: "review", label: "Review", beadsStatus: "review", tone: "violet" },
  { key: "done", label: "Done", beadsStatus: "closed", tone: "green" },
];

export const PRIORITY_OPTIONS = [
  /*
    CDXC:ProjectBoard 2026-05-28-09:18:
    Project board priority controls must show user-facing urgency labels instead of Beads' numeric P0/P1/P2/P3/P4 shorthand. Keep persisted priority values numeric for bd compatibility and collapse legacy lowest-priority value 4 into the visible Low tier.
  */
  { label: "Urgent", value: "0" },
  { label: "High", value: "1" },
  { label: "Medium", value: "2" },
  { label: "Low", value: "3" },
] as const;

export const TSHIRT_OPTIONS = [
  { label: "XS", minutes: 15 },
  { label: "S", minutes: 30 },
  { label: "M", minutes: 60 },
  { label: "L", minutes: 120 },
  { label: "XL", minutes: 240 },
] as const;

export type TshirtSize = (typeof TSHIRT_OPTIONS)[number]["label"];
export type BoardPriorityFilter = "all" | (typeof PRIORITY_OPTIONS)[number]["value"];
export type BoardEstimateFilter = "all" | "none" | TshirtSize;
export type BoardTagFilter = string;
export type BoardSortDirection = "asc" | "desc";
export type BoardSortKey = "created" | "priority" | "updated";
export type BoardSortOption = "default" | `${BoardSortKey}-${BoardSortDirection}`;

/*
  CDXC:ProjectBoardSort 2026-08-07:
  Each sort key is offered in both directions as its own option instead of a separate direction toggle, because the toolbar's other controls are single dropdowns and a direction control would have nothing to act on while Default order is selected.
  Direction values describe the underlying field, so `asc` means oldest first for timestamps and urgent first for priority; the visible labels carry that meaning for users.
*/
export const BOARD_SORT_OPTIONS: ReadonlyArray<{ label: string; value: BoardSortOption }> = [
  { label: "Default order", value: "default" },
  { label: "Last updated (newest first)", value: "updated-desc" },
  { label: "Last updated (oldest first)", value: "updated-asc" },
  { label: "Created (newest first)", value: "created-desc" },
  { label: "Created (oldest first)", value: "created-asc" },
  { label: "Priority (urgent first)", value: "priority-asc" },
  { label: "Priority (low first)", value: "priority-desc" },
];

export type ProjectBoardViewPreferences = {
  estimateFilter: BoardEstimateFilter;
  priorityFilter: BoardPriorityFilter;
  sortOption: BoardSortOption;
  tagFilter: BoardTagFilter;
};

/*
  CDXC:ProjectBoardViewPreferences 2026-08-07:
  The Kanban is its own web surface, so leaving the board tears it down and every toolbar selection dies with it. Priority, estimate, and sort are durable view settings and are restored on the next mount; ticket search stays ephemeral because a restored query would hide most of the board without an obvious cause.
  The three selections describe how the user wants to read a board rather than anything about a particular project, so one app-wide set follows them into every project instead of each board keeping its own.
  Stored values outlive the option lists that produced them, so a preference that no longer matches a current option falls back to its default instead of leaving the toolbar showing a value the board cannot filter or sort by.

  CDXC:ProjectBoardTagFilter 2026-08-21:
  Tags are the only ticket metadata the board could write but never read back, so a board of mixed work had to be scrolled rather than narrowed. The tag selection joins the other three under the same storage key and the same app-wide scope.
  Unlike priority, estimate, and sort, the tag options are not a fixed list: they are the labels the loaded tickets actually carry, so validity is only knowable once a board has loaded. Normalisation therefore only rejects values that could never be a tag, and a stored tag that the loaded board does not offer is resolved to "all" at read time by resolveBoardTagFilter rather than being written over, so returning to the board that has the tag restores the selection.
  The tag filter only ever includes: there is no hide-by-tag mode, because a board silently omitting cards the user never asked to hide is the failure this control exists to fix.
*/
export const DEFAULT_PROJECT_BOARD_VIEW_PREFERENCES: ProjectBoardViewPreferences = {
  estimateFilter: "all",
  priorityFilter: "all",
  sortOption: "default",
  tagFilter: "all",
};

export const PROJECT_BOARD_VIEW_PREFERENCES_STORAGE_KEY = "ghostex-project-board-view";
const BOARD_PRIORITY_FILTER_VALUES: ReadonlyArray<BoardPriorityFilter> = [
  "all",
  ...PRIORITY_OPTIONS.map((option) => option.value),
];
const BOARD_ESTIMATE_FILTER_VALUES: ReadonlyArray<BoardEstimateFilter> = [
  "all",
  "none",
  ...TSHIRT_OPTIONS.map((option) => option.label),
];
const BOARD_SORT_OPTION_VALUES: ReadonlyArray<BoardSortOption> = BOARD_SORT_OPTIONS.map(
  (option) => option.value,
);

const REQUIRED_CUSTOM_STATUS_CONFIG = "backlog,test,review";
const PROJECT_BOARD_COMMENT_METADATA_SEPARATOR = "---";
const PROJECT_BOARD_COMMENT_AGENT_PREFIX = "Agent:";
const PROJECT_BOARD_COMMENT_SESSION_PREFIX = "Session:";

export function beadsStatusToBoardStatus(status: string): BoardStatusKey {
  switch (status) {
    case "backlog":
      return "backlog";
    case "closed":
      return "done";
    case "in_progress":
      return "in_progress";
    case "review":
      return "review";
    case "test":
      return "test";
    default:
      return "todo";
  }
}

export function boardStatusLabel(status: BoardStatusKey): string {
  return BOARD_COLUMNS.find((column) => column.key === status)?.label ?? "Todo";
}

export function boardStatusBeadsValue(status: BoardStatusKey): string {
  return BOARD_COLUMNS.find((column) => column.key === status)?.beadsStatus ?? "open";
}

export function normalizeIssuePrefix(value: string | undefined): string {
  const normalized = (value ?? "")
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9-]/gu, "")
    .slice(0, 8);
  if (!normalized) {
    return "zmux";
  }
  return /^[a-z]/u.test(normalized) ? normalized : `p-${normalized}`;
}

/*
 * CDXC:ProjectBoardBeads 2026-07-31:
 * Prefix reconciliation exists to replace stale bootstrap defaults (for example gxserver, or an
 * unset value that normalizes to zmux) with the active project's prefix. A board that already has
 * any other established prefix must keep it: projectBoardConfig.beadsDirectory can point several
 * projects at one shared board, and renaming it to whichever project was focused last mass-renames
 * every issue id and all text references on each board load — which also breaks the beadId values
 * persisted in beadConversationLinks.
 */
const BOOTSTRAP_ISSUE_PREFIXES = new Set(["gxserver", "zmux"]);

export async function ensureIssuePrefix(
  runBeads: (request: Omit<BeadsBridgeRequest, "cwd" | "requestId">) => Promise<unknown>,
  desiredPrefix: string,
): Promise<void> {
  const normalizedDesiredPrefix = normalizeIssuePrefix(desiredPrefix);
  const payload = await runBeads({ action: "configGetIssuePrefix" });
  const currentValue = normalizeBeadsConfigString(payload);
  const normalizedCurrentPrefix = normalizeIssuePrefix(currentValue);
  if (
    normalizedCurrentPrefix !== normalizedDesiredPrefix &&
    BOOTSTRAP_ISSUE_PREFIXES.has(normalizedCurrentPrefix)
  ) {
    await runBeads({ action: "renamePrefix", value: beadsRenamePrefixValue(normalizedDesiredPrefix) });
  }
}

export function normalizeDisplayIssueKey(value: string | undefined): string {
  const normalized = (value ?? "")
    .trim()
    .toUpperCase()
    .replace(/[^A-Z0-9]/gu, "")
    .slice(0, 3);
  return normalized || "PRJ";
}

export function buildDisplayIdMap(issues: BeadsIssue[]): Map<string, string> {
  const sorted = [...issues].sort((left, right) => {
    const leftTime = Date.parse(left.created_at ?? "");
    const rightTime = Date.parse(right.created_at ?? "");
    if (Number.isFinite(leftTime) && Number.isFinite(rightTime) && leftTime !== rightTime) {
      return leftTime - rightTime;
    }
    return left.id.localeCompare(right.id);
  });
  return new Map(sorted.map((issue, index) => [issue.id, String(index + 1)]));
}

export function formatTicketDisplayId(
  issue: Pick<BeadsIssue, "id">,
  displayKey: string,
  serialByIssueId: Map<string, string>,
): string {
  const serial = serialByIssueId.get(issue.id);
  return serial ? `${displayKey}-${serial}` : issue.id;
}

export function toBoardTickets(
  issues: BeadsIssue[],
  displayKey: string,
): BoardTicket[] {
  const serialByIssueId = buildDisplayIdMap(issues);
  return issues
    .filter((issue) => issue && typeof issue.id === "string")
    .map((issue) => ({
      ...issue,
      boardStatus: beadsStatusToBoardStatus(issue.status),
      displayId: formatTicketDisplayId(issue, displayKey, serialByIssueId),
    }));
}

/*
  CDXC:ProjectBoardCreator 2026-08-07-07:52:
  Cards and Edit ticket show who created a bead next to who is assigned to it. The creator is
  redundant noise when it is the same person as the assignee, so display surfaces resolve the
  creator through here instead of each deciding when to hide it.
*/
export function ticketCreatorName(
  createdBy: string | undefined,
  assignee: string | undefined,
): string | undefined {
  return createdBy && createdBy !== assignee ? createdBy : undefined;
}

export function estimateToTshirt(estimate: number | null | undefined): TshirtSize | undefined {
  if (estimate === null || estimate === undefined) {
    return undefined;
  }
  return TSHIRT_OPTIONS.find((option) => option.minutes === estimate)?.label;
}

export function tshirtToEstimate(label: TshirtSize | undefined): number | undefined {
  if (!label) {
    return undefined;
  }
  return TSHIRT_OPTIONS.find((option) => option.label === label)?.minutes;
}

export function priorityLabel(priority: number | undefined): string {
  const value = priority ?? 2;
  return PRIORITY_OPTIONS.find((option) => Number(option.value) === value)?.label ?? "Low";
}

export function prioritySelectValue(priority: number | undefined): string {
  const value = priority ?? 2;
  return PRIORITY_OPTIONS.some((option) => Number(option.value) === value) ? String(value) : "3";
}

export function parseBeadsJson(stdout: string): unknown {
  const trimmed = stdout.trim();
  if (!trimmed) {
    return undefined;
  }
  return JSON.parse(trimmed);
}

export function normalizeBeadsPayload<T>(payload: unknown, fallback: T): T {
  if (isRecord(payload) && "data" in payload) {
    return payload.data as T;
  }
  return (payload ?? fallback) as T;
}

function normalizeBeadsConfigString(payload: unknown): string {
  const normalized = normalizeBeadsPayload<unknown>(payload, undefined);
  if (typeof normalized === "string") {
    return normalized;
  }
  if (isRecord(normalized) && typeof normalized.value === "string") {
    return normalized.value;
  }
  return "";
}

function beadsRenamePrefixValue(value: string): string {
  return `${normalizeIssuePrefix(value)}-`;
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function beadsJsonEnvelope(line: string): Record<string, unknown> | undefined {
  if (!line.startsWith("{")) {
    return undefined;
  }
  try {
    const payload: unknown = JSON.parse(line);
    return isRecord(payload) ? payload : undefined;
  } catch {
    return undefined;
  }
}

function beadsEnvelopeError(payload: Record<string, unknown>): string {
  const body = isRecord(payload.data) ? payload.data : payload;
  const firstFailure = (Array.isArray(body.failed) ? body.failed : []).find(isRecord);
  if (firstFailure && typeof firstFailure.error === "string" && firstFailure.error) {
    return firstFailure.error.replace(/^updating issue:\s*/iu, "");
  }
  if (typeof body.error === "string" && body.error) {
    return body.error;
  }
  if (typeof payload.error === "string" && payload.error) {
    return payload.error;
  }
  return "";
}

function beadsReportableLines(trimmed: string): string[] {
  /*
   * CDXC:ProjectBoardBeadsEnvelope 2026-08-20:
   * Beads writes advisory `warning:` lines to the same stderr as the failure
   * (unconfigured beads.role, auto-import notes, ungated workspace), each
   * optionally followed by indented "Fix:"/"Or:" continuations. They describe
   * the environment, not the refusal, so folding them into the message buries
   * the sentence the operator has to act on.
   */
  const lines: string[] = [];
  let inWarning = false;
  for (const rawLine of trimmed.split(/\r?\n/u)) {
    const line = rawLine.trim();
    if (!line) {
      continue;
    }
    if (/^warning:/iu.test(line)) {
      inWarning = true;
      continue;
    }
    if (inWarning && rawLine.startsWith(" ")) {
      continue;
    }
    inWarning = false;
    lines.push(line);
  }
  return lines;
}

export function beadsErrorMessage(message: string): string {
  const trimmed = message.trim();
  if (!trimmed) {
    return "The Beads command failed.";
  }
  const lines = beadsReportableLines(trimmed);
  if (lines.length === 0) {
    return trimmed;
  }
  /*
   * CDXC:ProjectBoardBeadsEnvelope 2026-08-20:
   * A failing `bd` writes its human sentence first and its JSON result envelope
   * after it, so the combined payload never parses as JSON and the raw envelope
   * used to be pasted into the notice behind the sentence. Split at the start of
   * the envelope instead: keep the prose when there is any, and read the
   * envelope's nested per-issue error only when Beads printed none. The envelope
   * is parsed as one block because `--json` pretty-prints it across lines.
   */
  const envelopeStart = lines.findIndex((line) => line.startsWith("{"));
  const proseLines = envelopeStart === -1 ? lines : lines.slice(0, envelopeStart);
  const envelopeText = envelopeStart === -1 ? "" : lines.slice(envelopeStart).join("\n");
  const envelope = envelopeText ? beadsJsonEnvelope(envelopeText) : undefined;
  /*
   * CDXC:ProjectBoardBeadsMigration 2026-08-12:
   * Preserve Beads' structured remote-migration gate for the Project Board
   * notice to render as an operator decision. Other JSON-envelope failures
   * should expose their nested human error instead of dumping the envelope.
   */
  if (envelope && isRecord(envelope.remote_migrate_gate)) {
    return envelopeText;
  }
  if (proseLines.length > 0) {
    return proseLines.join(" ");
  }
  return (envelope ? beadsEnvelopeError(envelope) : "") || lines.join(" ");
}

/*
 * CDXC:ProjectBoardBeadsRejection 2026-08-20:
 * Beads refuses some board operations for domain reasons: a close guarded by
 * open blockers or open children, a dependency edge that cannot exist, an id
 * that names no issue. Every one of those used to reach the generic
 * "Project board unavailable" notice, which tells the operator to reinstall
 * Beads — advice that can never resolve any of them. Classify the refusals
 * here so the board can state what was refused and offer the real remedy, and
 * so the generic notice is left to genuine environment failures.
 */
export type BeadsRejection =
  | { blockerIds: string[]; issueId: string; kind: "close-blocked" }
  | { issueId: string; kind: "close-open-children"; openChildren: number }
  | { kind: "dependency-cycle" }
  | { blockerId: string; issueId: string; kind: "dependency-hierarchy"; relation: "ancestor" | "descendant" }
  | { issueId: string; kind: "dependency-self" }
  | {
      dependsOnId: string;
      existingType: string;
      issueId: string;
      kind: "dependency-type-conflict";
      requestedType: string;
    }
  | { issueId: string; kind: "issue-missing" };

export function parseBeadsRejection(message: string): BeadsRejection | undefined {
  const closeBlocked =
    /cannot close blocked issue:\s*(?<issueId>[^\s:]+)\s+is blocked by\s*\[(?<blockers>[^\]]*)\]/iu.exec(message);
  if (closeBlocked?.groups?.issueId) {
    const blockerIds = (closeBlocked.groups.blockers ?? "")
      .split(/[,\s]+/u)
      .map((blockerId) => blockerId.trim())
      .filter(Boolean);
    if (blockerIds.length > 0) {
      return { blockerIds, issueId: closeBlocked.groups.issueId, kind: "close-blocked" };
    }
  }
  const openChildren =
    /cannot close\s+(?<issueId>[^\s:]+):\s*(?<count>\d+)\s+open child issue/iu.exec(message);
  if (openChildren?.groups?.issueId) {
    return {
      issueId: openChildren.groups.issueId,
      kind: "close-open-children",
      openChildren: Number.parseInt(openChildren.groups.count ?? "0", 10) || 0,
    };
  }
  const typeConflict =
    /dependency\s+(?<issueId>\S+)\s+->\s+(?<dependsOnId>\S+)\s+already exists with type\s+"(?<existingType>[^"]*)"\s+\(requested\s+"(?<requestedType>[^"]*)"\)/iu.exec(
      message,
    );
  if (typeConflict?.groups?.issueId && typeConflict.groups.dependsOnId) {
    return {
      dependsOnId: typeConflict.groups.dependsOnId,
      existingType: typeConflict.groups.existingType ?? "",
      issueId: typeConflict.groups.issueId,
      kind: "dependency-type-conflict",
      requestedType: typeConflict.groups.requestedType ?? "",
    };
  }
  const hierarchy =
    /(?<issueId>\S+)\s+cannot be blocked by its\s+(?<relation>ancestor|descendant)\s+(?<blockerId>[^\s:]+)/iu.exec(
      message,
    );
  if (hierarchy?.groups?.issueId && hierarchy.groups.blockerId) {
    return {
      blockerId: hierarchy.groups.blockerId,
      issueId: hierarchy.groups.issueId,
      kind: "dependency-hierarchy",
      relation: hierarchy.groups.relation?.toLowerCase() === "ancestor" ? "ancestor" : "descendant",
    };
  }
  const selfDependency = /cannot add self-dependency(?::\s*(?<issueId>\S+))?/iu.exec(message);
  if (selfDependency) {
    return { issueId: selfDependency.groups?.issueId ?? "", kind: "dependency-self" };
  }
  if (/adding dependency would create a cycle/iu.test(message)) {
    return { kind: "dependency-cycle" };
  }
  const missingIssue = /no issue found matching\s+"(?<issueId>[^"]*)"/iu.exec(message);
  if (missingIssue) {
    return { issueId: missingIssue.groups?.issueId ?? "", kind: "issue-missing" };
  }
  if (/no issues found matching the provided IDs/iu.test(message)) {
    return { issueId: "", kind: "issue-missing" };
  }
  return undefined;
}

export function projectBoardRawProjectIdFromUrlParam(projectId: string): string {
  /*
   * CDXC:ProjectBoardRouting 2026-06-04-23:51:
   * Project Board URLs created before the raw-id/editor-id split stored the native editor id in projectId. Normalize those old URLs at the web surface boundary so Beads requests use the canonical gxserver/native project id.
   */
  const match = /^project-editor:(?<projectId>.+):(?<mode>code|git|tasks)$/u.exec(projectId);
  const encodedProjectId = match?.groups?.projectId;
  if (!encodedProjectId) {
    return projectId;
  }
  try {
    return decodeURIComponent(encodedProjectId);
  } catch {
    return projectId;
  }
}

export function normalizeProjectBoardViewPreferences(
  candidate: unknown,
): ProjectBoardViewPreferences {
  const stored =
    typeof candidate === "object" && candidate !== null
      ? (candidate as Record<string, unknown>)
      : {};
  return {
    estimateFilter: normalizeBoardViewPreference(
      stored.estimateFilter,
      BOARD_ESTIMATE_FILTER_VALUES,
      DEFAULT_PROJECT_BOARD_VIEW_PREFERENCES.estimateFilter,
    ),
    priorityFilter: normalizeBoardViewPreference(
      stored.priorityFilter,
      BOARD_PRIORITY_FILTER_VALUES,
      DEFAULT_PROJECT_BOARD_VIEW_PREFERENCES.priorityFilter,
    ),
    sortOption: normalizeBoardViewPreference(
      stored.sortOption,
      BOARD_SORT_OPTION_VALUES,
      DEFAULT_PROJECT_BOARD_VIEW_PREFERENCES.sortOption,
    ),
    tagFilter:
      typeof stored.tagFilter === "string" && stored.tagFilter.trim().length > 0
        ? stored.tagFilter
        : DEFAULT_PROJECT_BOARD_VIEW_PREFERENCES.tagFilter,
  };
}

function normalizeBoardViewPreference<TValue extends string>(
  candidate: unknown,
  allowedValues: ReadonlyArray<TValue>,
  fallback: TValue,
): TValue {
  return allowedValues.includes(candidate as TValue) ? (candidate as TValue) : fallback;
}

export async function ensureWorkflowStatuses(
  runBeads: (request: Omit<BeadsBridgeRequest, "cwd" | "requestId">) => Promise<unknown>,
): Promise<void> {
  const payload = await runBeads({ action: "configGet" });
  const currentValue = normalizeBeadsPayload<{ value?: string }>(payload, {}).value ?? "";
  const requiredEntries = REQUIRED_CUSTOM_STATUS_CONFIG.split(",");
  const requiredNames = new Set(requiredEntries.map((entry) => entry.split(":")[0]));
  const currentEntries = currentValue
    .split(",")
    .map((entry) => entry.trim())
    .filter(Boolean);
  const currentNames = new Set(currentEntries.map((entry) => entry.split(":")[0]));
  const nextEntries = currentEntries.map((entry) => {
    const name = entry.split(":")[0];
    return requiredNames.has(name) ? name : entry;
  });
  for (const entry of requiredEntries) {
    const name = entry.split(":")[0];
    if (!currentNames.has(name)) {
      nextEntries.push(entry);
    }
  }
  const nextValue = nextEntries.join(",");
  if (nextValue !== currentValue) {
    await runBeads({ action: "configSet", value: nextValue });
  }
}

/*
  CDXC:ProjectBoardTagFilter 2026-08-21:
  The tag dropdown offers the labels the loaded tickets actually carry rather than every label the project has ever defined, so selecting one can never produce an empty board and the list shrinks as retired tags stop being used.
*/
export function boardTagFilterOptions(tickets: BoardTicket[]): BoardTagFilter[] {
  const tags = new Set<string>();
  for (const ticket of tickets) {
    for (const label of ticket.labels ?? []) {
      tags.add(label);
    }
  }
  return ["all", ...Array.from(tags).sort((left, right) => left.localeCompare(right))];
}

export function resolveBoardTagFilter(
  tagFilter: BoardTagFilter,
  options: ReadonlyArray<BoardTagFilter>,
): BoardTagFilter {
  return options.includes(tagFilter) ? tagFilter : DEFAULT_PROJECT_BOARD_VIEW_PREFERENCES.tagFilter;
}

export function filterBoardTickets(
  tickets: BoardTicket[],
  query: string,
  priorityFilter: BoardPriorityFilter,
  estimateFilter: BoardEstimateFilter,
  tagFilter: BoardTagFilter,
): BoardTicket[] {
  const normalizedQuery = query.trim();
  /*
    CDXC:ProjectBoardFilters 2026-05-30-08:31:
    The Project board toolbar should filter by planning metadata instead of lane status so the visible swimlanes remain the workflow source of truth.
    Priority matching uses the same normalized visible tier as ticket controls, and estimate matching treats missing estimates as their own selectable state.
  */
  let filtered =
    priorityFilter === "all"
      ? tickets
      : tickets.filter((ticket) => prioritySelectValue(ticket.priority) === priorityFilter);
  filtered =
    estimateFilter === "all"
      ? filtered
      : filtered.filter((ticket) => {
          const ticketEstimate = estimateToTshirt(ticket.estimate);
          return estimateFilter === "none" ? ticketEstimate === undefined : ticketEstimate === estimateFilter;
        });
  filtered =
    tagFilter === "all"
      ? filtered
      : filtered.filter((ticket) => ticket.labels?.includes(tagFilter) ?? false);
  if (!normalizedQuery) {
    return filtered;
  }
  const fuse = new Fuse(filtered, {
    keys: ["title", "description", "id", "displayId", "labels"],
    threshold: 0.38,
  });
  return fuse.search(normalizedQuery).map((result) => result.item);
}

/*
  CDXC:ProjectBoardSort 2026-08-07:
  Lanes only render their first PROJECT_BOARD_MAX_VISIBLE_TICKETS_PER_COLUMN cards, so ticket order is a board-level concern rather than a lane-render detail.
  Beads returns no meaningful order for `list --all`, which leaves a Done lane of hundreds of closed beads hiding the work that just finished behind that cap.
  Done therefore defaults to newest-closed-first while the other lanes keep the Beads order they have always shown, and an explicit sort selection applies to every lane in its chosen direction.
  Direction also drives the priority tie-break so the two priority views are exact reverses of each other rather than differing only in their tiers.
*/
export function sortBoardTickets(
  tickets: BoardTicket[],
  sort: BoardSortOption,
  column: BoardStatusKey,
): BoardTicket[] {
  if (sort === "default") {
    return column === "done"
      ? [...tickets].sort((left, right) =>
          compareBoardTicketTimes(boardTicketClosedTime(left), boardTicketClosedTime(right), "desc"),
        )
      : tickets;
  }
  const [sortKey, sortDirection] = sort.split("-") as [BoardSortKey, BoardSortDirection];
  if (sortKey === "priority") {
    return [...tickets].sort((left, right) => {
      const priorityDelta =
        Number(prioritySelectValue(left.priority)) - Number(prioritySelectValue(right.priority));
      return priorityDelta !== 0
        ? applyBoardSortDirection(priorityDelta, sortDirection)
        : compareBoardTicketTimes(
            boardTicketUpdatedTime(left),
            boardTicketUpdatedTime(right),
            sortDirection,
          );
    });
  }
  const ticketTime = sortKey === "created" ? boardTicketCreatedTime : boardTicketUpdatedTime;
  return [...tickets].sort((left, right) =>
    compareBoardTicketTimes(ticketTime(left), ticketTime(right), sortDirection),
  );
}

function boardTicketClosedTime(ticket: BoardTicket): number {
  return parseBoardTicketTime(ticket.closed_at ?? ticket.updated_at ?? ticket.created_at);
}

function boardTicketUpdatedTime(ticket: BoardTicket): number {
  return parseBoardTicketTime(ticket.updated_at ?? ticket.created_at);
}

function boardTicketCreatedTime(ticket: BoardTicket): number {
  return parseBoardTicketTime(ticket.created_at);
}

function parseBoardTicketTime(value: string | undefined): number {
  const parsed = Date.parse(value ?? "");
  return Number.isFinite(parsed) ? parsed : Number.NEGATIVE_INFINITY;
}

function compareBoardTicketTimes(
  left: number,
  right: number,
  direction: BoardSortDirection,
): number {
  if (left === right) {
    return 0;
  }
  /*
    CDXC:ProjectBoardSort 2026-08-07:
    A bead whose timestamp is missing or unparseable is unknown rather than oldest, so it stays at the bottom of the lane in both directions instead of leading the oldest-first views.
  */
  if (left === Number.NEGATIVE_INFINITY) {
    return 1;
  }
  if (right === Number.NEGATIVE_INFINITY) {
    return -1;
  }
  return applyBoardSortDirection(left > right ? 1 : -1, direction);
}

function applyBoardSortDirection(ascendingComparison: number, direction: BoardSortDirection): number {
  return direction === "asc" ? ascendingComparison : -ascendingComparison;
}

export function appendImageMarkdownToDescription(
  description: string,
  imagePath: string,
  selectionStart?: number,
  selectionEnd?: number,
): string {
  const snippet = `[Image #${getNextDescriptionImageIndex(description)}](${imagePath})`;
  /**
   * CDXC:ProjectBoardImagePaste 2026-05-28-08:48:
   * Project Board image references are editable prompt text, not hidden metadata.
   * Paste images as visible [Image #N](path) references at the caret so users can
   * write prose around them and refer to each image explicitly in the prompt.
   */
  return insertDescriptionSnippet(description, snippet, selectionStart, selectionEnd);
}

export type DescriptionImageReference = {
  endOffset: number;
  id: string;
  markdown: string;
  src: string;
  startOffset: number;
};

const descriptionImageFileExtensionPattern = /\.(avif|gif|heic|heif|jpe?g|png|svg|tiff?|webp)(?:[?#].*)?$/iu;

function descriptionImageMarkdownPattern(): RegExp {
  return /!?\[[^\]\n]*\]\(([^)\n]+)\)/gu;
}

export function extractDescriptionImageReferences(description: string): DescriptionImageReference[] {
  const references: DescriptionImageReference[] = [];
  for (const match of description.matchAll(descriptionImageMarkdownPattern())) {
    const markdown = match[0];
    const src = (match[1] ?? "").trim();
    if (!isDescriptionImageSource(src)) {
      continue;
    }
    const startOffset = match.index ?? 0;
    references.push({
      endOffset: startOffset + markdown.length,
      id: `${startOffset}:${markdown.length}:${src.slice(0, 64)}`,
      markdown,
      src,
      startOffset,
    });
  }

  let lineStartOffset = 0;
  for (const line of description.split(/(\n)/u)) {
    if (line === "\n") {
      lineStartOffset += line.length;
      continue;
    }
    const src = line.trim();
    if (
      isDescriptionImageSource(src) &&
      !references.some(
        (reference) => lineStartOffset <= reference.startOffset && reference.endOffset <= lineStartOffset + line.length,
      )
    ) {
      const leadingWhitespaceLength = line.length - line.trimStart().length;
      const startOffset = lineStartOffset + leadingWhitespaceLength;
      references.push({
        endOffset: startOffset + src.length,
        id: `${startOffset}:${src.length}:${src.slice(0, 64)}`,
        markdown: src,
        src,
        startOffset,
      });
    }
    lineStartOffset += line.length;
  }

  return references.sort((left, right) => left.startOffset - right.startOffset);
}

export function extractDescriptionImagePreviews(description: string): string[] {
  /**
   * CDXC:ProjectBoardImagePaste 2026-05-28-08:50:
   * The preview strip must update from the image paths users type or paste in
   * the prompt text, including visible [Image #N](path) references and plain
   * standalone image-path lines.
   */
  return previewableDescriptionImageReferences(description).map((reference) => reference.src);
}

export function extractPreviewableDescriptionImageReferences(description: string): DescriptionImageReference[] {
  return previewableDescriptionImageReferences(description);
}

export function removeDescriptionImageReference(description: string, imageId: string): string {
  const reference = extractDescriptionImageReferences(description).find((candidate) => candidate.id === imageId);
  if (!reference) {
    return description;
  }
  return `${description.slice(0, reference.startOffset)}${description.slice(reference.endOffset)}`
    .replace(/[ \t]+\n/gu, "\n")
    .replace(/\n{3,}/gu, "\n\n")
    .trim();
}

export function isDescriptionImageSource(source: string): boolean {
  const trimmed = source.trim();
  if (!trimmed) {
    return false;
  }
  if (trimmed.toLowerCase().startsWith("data:image/")) {
    return true;
  }
  if (
    trimmed.startsWith("/") ||
    trimmed.startsWith("~/") ||
    trimmed.startsWith("file://") ||
    trimmed.startsWith("~/.ghostex/i/")
  ) {
    return descriptionImageFileExtensionPattern.test(trimmed);
  }
  return false;
}

function getNextDescriptionImageIndex(description: string): number {
  let highestIndex = 0;
  for (const match of description.matchAll(/\[Image #(\d+)\]\(/gu)) {
    const index = Number.parseInt(match[1] ?? "", 10);
    if (Number.isFinite(index)) {
      highestIndex = Math.max(highestIndex, index);
    }
  }
  return highestIndex + 1;
}

function insertDescriptionSnippet(
  description: string,
  snippet: string,
  selectionStart?: number,
  selectionEnd?: number,
): string {
  const start =
    typeof selectionStart === "number" && Number.isFinite(selectionStart)
      ? Math.max(0, Math.min(description.length, selectionStart))
      : description.length;
  const end =
    typeof selectionEnd === "number" && Number.isFinite(selectionEnd)
      ? Math.max(start, Math.min(description.length, selectionEnd))
      : start;
  const prefix = description.slice(0, start);
  const suffix = description.slice(end);
  const before = prefix.length === 0 || prefix.endsWith("\n") ? prefix : `${prefix}\n\n`;
  const after = suffix.length === 0 || suffix.startsWith("\n") ? suffix : `\n\n${suffix}`;
  return `${before}${snippet}${after}`;
}

function persistableDescriptionImageReferences(description: string): DescriptionImageReference[] {
  const references = extractDescriptionImageReferences(description);
  const hasPathReference = references.some((reference) => !isLegacyDataImageSource(reference.src));
  return hasPathReference
    ? references.filter((reference) => !isLegacyDataImageSource(reference.src))
    : references;
}

function previewableDescriptionImageReferences(description: string): DescriptionImageReference[] {
  return persistableDescriptionImageReferences(description);
}

function isLegacyDataImageSource(source: string): boolean {
  return source.trim().toLowerCase().startsWith("data:image/");
}

/*
 * CDXC:ProjectBoard 2026-05-30-09:25:
 * Start Work must tell agents to leave bead comments after each turn so humans can follow ticket progress without reading the full agent transcript.
 * Comments should capture user-facing outcomes and high-level technical decisions, not per-file diffs.
 *
 * CDXC:ProjectBoardComments 2026-06-05-06:43:
 * Agent-authored bead comments should carry a parseable agent label and a resumable agent CLI session id at the bottom of the comment. Keep the stored Beads comment as plain text while letting the ticket editor render `madda (Cursor CLI)`-style attribution and a dedicated session footer.
 *
 * CDXC:ProjectBoardComments 2026-06-05-06:55:
 * The Session footer is the saved session identity from the agent CLI that authored the comment, such as a Codex thread id or Cursor chat id, not the Ghostex pane/provider session id. Users need this id to resume the actual agent session that made the comment.
 *
 * CDXC:ProjectBoardBeads 2026-06-10-09:31:
 * Start Work prompts use the machine-installed `bd`, matching the Project/Kanban runtime and avoiding a second Ghostex-owned Beads binary.
 */
export function buildAgentWorkPrompt(ticket: BoardTicket): string {
  const beadId = ticket.id;
  return [
    `Work on bead ${beadId} (${ticket.displayId}): ${ticket.title}`,
    "",
    ticket.description?.trim() || "No prompt provided.",
    "",
    "After each turn where you made progress on this bead, add a bead comment summarizing what you did:",
    `- \`bd comment ${beadId} "<summary>"\``,
    "- Focus on user-facing requirements delivered and high-level technical approach.",
    "- Do not list specific files or line numbers.",
    "- End the comment with `Agent: <agent name>` and `Session: <saved agent CLI session id>` lines so the ticket view can show the agent after the user name and the resumable agent session id at the bottom.",
    "",
    "Status workflow for this project board:",
    `- Park for later: \`bd update ${beadId} --status backlog\``,
    `- When you start: \`bd update ${beadId} --status in_progress\``,
    `- When implementation is ready for test: \`bd update ${beadId} --status test\``,
    `- When ready for review: \`bd update ${beadId} --status review\``,
    `- When done: \`bd close ${beadId}\``,
  ].join("\n");
}

/*
 * CDXC:ProjectBoardComments 2026-06-05-06:43:
 * The ticket editor stores agent/session attribution in a bd-compatible plain-text footer because Beads comments only expose author, timestamp, and text. Parse that footer at the display boundary so old comments still render, while new comments get structured UI treatment without changing Beads storage.
 */
export function formatProjectBoardCommentText(
  body: string,
  metadata: ProjectBoardCommentMetadata = {},
): string {
  const trimmedBody = body.trim();
  const agentName = normalizeCommentMetadataValue(metadata.agentName);
  const sessionId = normalizeCommentMetadataValue(metadata.sessionId);
  const metadataLines = [
    agentName ? `${PROJECT_BOARD_COMMENT_AGENT_PREFIX} ${agentName}` : undefined,
    sessionId ? `${PROJECT_BOARD_COMMENT_SESSION_PREFIX} ${sessionId}` : undefined,
  ].filter((line): line is string => Boolean(line));
  if (metadataLines.length === 0) {
    return trimmedBody;
  }
  return [
    trimmedBody,
    "",
    PROJECT_BOARD_COMMENT_METADATA_SEPARATOR,
    ...metadataLines,
  ].join("\n");
}

export function parseProjectBoardCommentText(text: string | undefined): ParsedProjectBoardComment {
  const originalBody = (text ?? "").trim();
  if (!originalBody) {
    return { body: "" };
  }
  const lines = originalBody.split(/\r?\n/u);
  let cursor = lines.length - 1;
  let sessionId: string | undefined;
  let agentName: string | undefined;
  let hasMetadataSeparator = false;

  const sessionLine = lines[cursor]?.trim() ?? "";
  if (sessionLine.startsWith(PROJECT_BOARD_COMMENT_SESSION_PREFIX)) {
    sessionId = normalizeCommentMetadataValue(sessionLine.slice(PROJECT_BOARD_COMMENT_SESSION_PREFIX.length));
    cursor -= 1;
  }

  const agentLine = lines[cursor]?.trim() ?? "";
  if (agentLine.startsWith(PROJECT_BOARD_COMMENT_AGENT_PREFIX)) {
    agentName = normalizeCommentMetadataValue(agentLine.slice(PROJECT_BOARD_COMMENT_AGENT_PREFIX.length));
    cursor -= 1;
  }

  if (!sessionId && !agentName) {
    return { body: originalBody };
  }

  while (cursor >= 0 && lines[cursor]?.trim() === "") {
    cursor -= 1;
  }
  if (lines[cursor]?.trim() === PROJECT_BOARD_COMMENT_METADATA_SEPARATOR) {
    hasMetadataSeparator = true;
    cursor -= 1;
  }
  if (sessionId && !agentName && !hasMetadataSeparator) {
    return { body: originalBody };
  }
  while (cursor >= 0 && lines[cursor]?.trim() === "") {
    cursor -= 1;
  }

  return {
    agentName,
    body: lines.slice(0, cursor + 1).join("\n").trim(),
    sessionId,
  };
}

function normalizeCommentMetadataValue(value: string | undefined): string | undefined {
  const normalized = value?.trim();
  return normalized || undefined;
}

/*
 * CDXC:ProjectBoardStartWork 2026-08-07-07:01:
 * A bead's assignee names who should do the work, so opening a ticket must
 * preselect the configured agent that assignee refers to instead of always
 * showing the board default agent.
 * Compare the assignee case-insensitively against each configured agent's label
 * and agent id so both a custom agent named "Dobby" and a built-in agent id like
 * "claude" resolve, and return nothing when no agent matches so unassigned and
 * human-assigned beads keep the existing default.
 */
/*
 * CDXC:ProjectBoardStartWorkToolSuffix 2026-08-08-15:22:
 * Boards name an assignee after the tool a worker runs, and that name usually
 * carries the suffix the product ships with — "claude-code" for Claude Code,
 * "gemini-cli" for Gemini CLI — while the configured agent is the bare "claude"
 * or "gemini". An exact match alone leaves those beads on the board default,
 * which is the one case this preselect exists to fix.
 * Retry once with a trailing "-code" or "-cli" dropped, and only after the exact
 * pass fails, so an agent literally named "claude-code" still wins over "claude"
 * and no bead resolves to an agent the exact pass could already reach.
 */
const AGENT_TOOL_NAME_SUFFIXES = ["-code", "-cli"] as const;

export function resolveAssignedAgentId(
  assignee: string | undefined,
  agents: readonly { agentId: string; label: string }[],
): string | undefined {
  const normalizedAssignee = assignee?.trim().toLowerCase();
  if (!normalizedAssignee) {
    return undefined;
  }
  const matchAgentName = (candidate: string): string | undefined =>
    agents.find(
      (agent) =>
        agent.label.trim().toLowerCase() === candidate ||
        agent.agentId.trim().toLowerCase() === candidate,
    )?.agentId;
  const exactMatch = matchAgentName(normalizedAssignee);
  if (exactMatch) {
    return exactMatch;
  }
  for (const suffix of AGENT_TOOL_NAME_SUFFIXES) {
    if (normalizedAssignee.endsWith(suffix) && normalizedAssignee.length > suffix.length) {
      const withoutSuffix = normalizedAssignee.slice(0, -suffix.length);
      const suffixMatch = matchAgentName(withoutSuffix);
      if (suffixMatch) {
        return suffixMatch;
      }
    }
  }
  return undefined;
}

export function getBlockedByIds(issue: BeadsIssue): string[] {
  return (issue.dependencies ?? []).map((dependency) => dependency.depends_on_id).filter(Boolean);
}

export function getBlockingIds(issueId: string, issues: BeadsIssue[]): string[] {
  return issues
    .filter((candidate) =>
      (candidate.dependencies ?? []).some((dependency) => dependency.depends_on_id === issueId),
    )
    .map((candidate) => candidate.id);
}

export function formatShortDate(value?: string): string {
  if (!value) {
    return "";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "";
  }
  return date.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

export function conversationLinkLabel(link: ProjectBoardConversationLinkView): string {
  return link.sessionTitle || link.agentName || link.agentId || link.agentSessionId || "Agent session";
}

export type ProjectBoardConversationLinkActionKind = "jump" | "none" | "resume";

export function conversationLinkActionKind(
  link: ProjectBoardConversationLinkView | undefined,
): ProjectBoardConversationLinkActionKind {
  /*
   * CDXC:ProjectBoardBeads 2026-08-07:
   * Ghostex can open a live or restorable session directly. When the session
   * row is gone from restorable history the agent conversation it worked can
   * still be resumed into a fresh session, which is a different promise to the
   * user and gets its own affordance.
   */
  if (link?.isLive || link?.isRestorable) {
    return "jump";
  }
  return link?.isResumable ? "resume" : "none";
}

export function isUsableConversationLink(
  link: ProjectBoardConversationLinkView | undefined,
): boolean {
  return conversationLinkActionKind(link) !== "none";
}

export function conversationLinkStatusText(link: ProjectBoardConversationLinkView): string {
  /*
   * CDXC:ProjectBoardBeads 2026-08-07:
   * A closed agent session is the normal end state of bead work, not a broken
   * link, so the card keeps the worker as history ("Last worked 6 Aug")
   * instead of showing a dangling "Unavailable".
   */
  const lastWorkedDate = formatShortDate(link.updatedAt);
  const sessionStatus = link.isSleeping
    ? "Sleeping"
    : link.isLive
      ? "Live"
      : link.isRestorable
        ? "Restorable"
        : lastWorkedDate
          ? `Last worked ${lastWorkedDate}`
          : "Last worked";
  const agentSessionPreview = link.agentSessionId ? ` · ${link.agentSessionId.slice(0, 8)}` : "";
  return `${sessionStatus}${agentSessionPreview}`;
}
