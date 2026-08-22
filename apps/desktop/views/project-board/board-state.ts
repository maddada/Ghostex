import {
  isDescriptionImageSource,
  toBoardTickets,
  type BoardColumn,
  type BeadsIssue,
  type BoardTicket,
} from "../project-board-shared";
import {
  type DetailDraft,
  type TicketFormDraft,
  type PendingBoardStatusMove,
  type ProjectBoardFocusOwnerEvent,
  type ProjectBeadsWebKitWindow,
} from "./types";
import {
  PROJECT_BOARD_GENERATED_TITLE_DELAY_MS,
  PROJECT_BOARD_GENERATED_TITLE_IDLE_TIMEOUT_MS,
  PROJECT_BOARD_DRAFT_TITLE_MAX_LENGTH,
} from "./constants";

export type ProjectBoardIdleWindow = Window & {
  requestIdleCallback?: (
    callback: () => void,
    options?: { timeout?: number },
  ) => number;
};

export function isProjectBoardEditableFocusTarget(target: EventTarget | null): boolean {
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

export function postProjectBoardFocusOwnerChanged({
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

export function createEmptyDetailDraft(): DetailDraft {
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

export function createEmptyTicketFormDraft(): TicketFormDraft {
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
export function createProjectBoardDraftTitle(prompt: string): string {
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

export function boardColumnsSignature(columns: ReadonlyArray<BoardColumn>): string {
  return columns.map((column) => column.key).join("\u001f");
}

export function applyPendingBoardStatusMoves(
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

export function upsertProjectBoardIssue(issues: BeadsIssue[], issue: BeadsIssue): BeadsIssue[] {
  const index = issues.findIndex((candidate) => candidate.id === issue.id);
  if (index === -1) {
    return [...issues, issue];
  }
  const nextIssues = [...issues];
  nextIssues[index] = { ...nextIssues[index], ...issue };
  return nextIssues;
}

export function upsertProjectBoardTicket(tickets: BoardTicket[], ticket: BoardTicket): BoardTicket[] {
  const index = tickets.findIndex((candidate) => candidate.id === ticket.id);
  if (index === -1) {
    return [...tickets, ticket];
  }
  const nextTickets = [...tickets];
  nextTickets[index] = { ...nextTickets[index], ...ticket };
  return nextTickets;
}

export function scheduleProjectBoardGeneratedTitle(work: () => void): void {
  window.setTimeout(() => {
    const requestIdleCallback = (window as ProjectBoardIdleWindow).requestIdleCallback;
    if (requestIdleCallback) {
      requestIdleCallback(work, { timeout: PROJECT_BOARD_GENERATED_TITLE_IDLE_TIMEOUT_MS });
      return;
    }
    work();
  }, PROJECT_BOARD_GENERATED_TITLE_DELAY_MS);
}

export function waitForProjectBoardRefreshIdle(isBusy: () => boolean): Promise<void> {
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

export function toCreatedBoardTicket(
  issue: BeadsIssue,
  knownIssues: BeadsIssue[],
  displayKey: string,
  columns: ReadonlyArray<BoardColumn>,
): BoardTicket | undefined {
  const issues = [...knownIssues.filter((candidate) => candidate.id !== issue.id), issue];
  return toBoardTickets(issues, displayKey, columns).find((ticket) => ticket.id === issue.id);
}

export function resolveCreatedIssueFromRefresh(
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

export function stringifyProjectBoardDebugDetails(details: Record<string, unknown> | undefined): string | undefined {
  if (details === undefined) {
    return undefined;
  }
  try {
    return JSON.stringify(details);
  } catch {
    return JSON.stringify({ serializationFailed: true });
  }
}

export function projectBoardTitleGenerationFailureDetails(error: unknown): Record<string, unknown> {
  const text = error instanceof Error ? error.message : String(error);
  return {
    errorClass: projectBoardTitleGenerationErrorClass(text),
    errorLength: text.length,
    isGenericPromptAgentFailure: text === "Prompt-agent title generation failed.",
  };
}

export function projectBoardPromptAgentKind(agentId: string): string {
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

export function projectBoardTitleGenerationErrorClass(text: string): string {
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

export function createIssuesSignature(issues: BeadsIssue[]): string {
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

export function mergeKnownLabels(current: string[], labels: readonly string[] | undefined): string[] {
  const next = new Set(current);
  for (const label of labels ?? []) {
    const normalized = typeof label === "string" ? label.trim() : "";
    if (normalized) {
      next.add(normalized);
    }
  }
  return [...next].sort((left, right) => left.localeCompare(right));
}

export function deriveKnownLabelsFromIssues(issues: BeadsIssue[]): string[] {
  return mergeKnownLabels([], issues.flatMap((issue) => issue.labels ?? []));
}

export function prioritizeDependencyTickets(tickets: BoardTicket[]): BoardTicket[] {
  const activeTickets = tickets.filter((ticket) => ticket.boardStatus !== "done");
  const doneTickets = tickets.filter((ticket) => ticket.boardStatus === "done");
  return [...activeTickets, ...doneTickets];
}

export function hasProjectBoardImagePastePayload(clipboardData: DataTransfer): boolean {
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