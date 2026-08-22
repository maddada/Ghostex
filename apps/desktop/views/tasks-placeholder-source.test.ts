import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const tasksPlaceholderSource = readFileSync(new URL("./tasks-placeholder.tsx", import.meta.url), "utf8");

function sourceBetweenIn(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

function sourceBetween(start: string, end: string): string {
  return sourceBetweenIn(tasksPlaceholderSource, start, end);
}

function collectFunctionalUpdaterCalls(source: string, setterCallStart: string): string[] {
  const calls: string[] = [];
  let searchIndex = 0;
  while (searchIndex < source.length) {
    const startIndex = source.indexOf(setterCallStart, searchIndex);
    if (startIndex === -1) {
      break;
    }
    const openParenIndex = source.indexOf("(", startIndex);
    expect(openParenIndex).toBeGreaterThan(startIndex);
    let depth = 0;
    let endIndex = openParenIndex;
    for (; endIndex < source.length; endIndex += 1) {
      const char = source[endIndex];
      if (char === "(") {
        depth += 1;
      } else if (char === ")") {
        depth -= 1;
        if (depth === 0) {
          endIndex += 1;
          break;
        }
      }
    }
    expect(depth).toBe(0);
    calls.push(source.slice(startIndex, endIndex));
    searchIndex = endIndex;
  }
  return calls;
}

describe("Project Board form event handling", () => {
  test("keeps Kanban swimlanes adjacent with a single separator", () => {
    /*
     * CDXC:ProjectBoardLanes 2026-06-19-09:59:
     * Kanban swimlanes should remove horizontal gutters to give cards more width while avoiding doubled borders where lanes touch.
     */
    const laneLayoutSource = sourceBetween(".project-board-lanes {", ".project-board-lane-header");

    expect(laneLayoutSource).toContain("gap: 0;");
    expect(laneLayoutSource).toContain(".project-board-lane + .project-board-lane");
    expect(laneLayoutSource).toContain("border-left-width: 0;");
  });

  test("keeps the Kanban board scrollbars grabbable with the mouse", () => {
    /*
     * CDXC:BoardScrollbars 2026-08-07:
     * The board strip and the lane bodies must keep a real scrollbar box, or the
     * board is wheel-only: Chromium measured a 0px scroll gutter (nothing to
     * click or drag) whenever the scroller carried scrollbar-width: none, a
     * scrollbar-color, or a zero-sized ::-webkit-scrollbar. A decorative
     * pointer-events: none overlay is not a scrollbar and must not come back.
     */
    const boardScrollbarSource = sourceBetween(
      ".project-board-lanes,\n  .project-board-lane-scroll,\n  .project-ticket-dialog-body {",
      ".project-board-toolbar {",
    );

    expect(boardScrollbarSource).toContain("scrollbar-width: auto;");
    expect(boardScrollbarSource).not.toContain("scrollbar-color");
    expect(boardScrollbarSource).toContain("height: 8px;");
    expect(boardScrollbarSource).toContain("width: 8px;");
    expect(tasksPlaceholderSource).not.toContain("project-board-lane-scrollbar");
  });

  test("keeps the ticket dialog body scrollbar grabbable with the mouse", () => {
    /*
     * CDXC:DialogScrollbar 2026-08-07:
     * The ticket dialog body shared the comment list's hidden-scrollbar rules,
     * so Chromium measured a 0px scroll gutter on it and neither a track click
     * nor a thumb drag anywhere along its right edge moved it — wheel-only. It
     * must stay on the board's real-scrollbar rules. The comment list must stay
     * hidden here because its Radix ScrollArea paints its own grabbable bar, and
     * the dialog thumb's hover reveal must set background-color rather than the
     * background shorthand, which would reset the content-box clip that keeps
     * the painted rail at 2px.
     */
    const hiddenScrollbarSource = sourceBetween(
      '.project-ticket-comment-list [data-slot="scroll-area-viewport"] {',
      ".project-board-lanes,",
    );
    const dialogScrollbarSource = sourceBetween(
      ".project-board-lanes,\n  .project-board-lane-scroll,\n  .project-ticket-dialog-body {",
      ".project-board-toolbar {",
    );
    const dialogThumbHoverSource = sourceBetween(
      ".project-ticket-dialog-body:focus-within::-webkit-scrollbar-thumb {",
      '.project-ticket-comment-list [data-slot="scroll-area-scrollbar"] {',
    );

    expect(hiddenScrollbarSource).not.toContain("project-ticket-dialog-body");
    expect(hiddenScrollbarSource).toContain("scrollbar-width: none;");
    expect(hiddenScrollbarSource).toContain("width: 2px;");
    expect(dialogScrollbarSource).toContain(".project-ticket-dialog-body::-webkit-scrollbar {");
    expect(dialogScrollbarSource).toContain(".project-ticket-dialog-body::-webkit-scrollbar-thumb {");
    expect(dialogThumbHoverSource).toContain("background-color: var(--project-board-scrollbar);");
  });

  test("shows the first-open Kanban loading overlay until initial load finishes", () => {
    /*
     * CDXC:ProjectBoardLoading 2026-06-20-18:21:
     * The first macOS Kanban open should cover the mounted lanes with a spinner until initial Beads loading finishes, while later refreshes should not replay that mask.
     */
    const projectBoardSource = sourceBetween("function ProjectBoardApp()", "function TicketMetaFields(");
    const overlayStyleSource = sourceBetween(".project-board-board-region {", ".project-board-lanes {");

    expect(projectBoardSource).toContain(
      "const [hasCompletedInitialBoardLoad, setHasCompletedInitialBoardLoad] = useState(false);",
    );
    expect(projectBoardSource).toContain('if (mode === "initial") {');
    expect(projectBoardSource).toContain("setHasCompletedInitialBoardLoad(true);");
    expect(projectBoardSource).toContain(
      'activeSurfaceTab === "board" && loadState === "loading" && !hasCompletedInitialBoardLoad',
    );
    expect(projectBoardSource).toContain('className="project-board-loading-overlay"');
    expect(projectBoardSource).toContain('role="status"');
    expect(projectBoardSource).toContain("IconLoader2");
    expect(overlayStyleSource).toContain("position: relative;");
    expect(overlayStyleSource).toContain(".project-board-loading-overlay");
    expect(overlayStyleSource).toContain("position: absolute;");
    expect(overlayStyleSource).toContain(".project-board-loading-spinner");
    expect(overlayStyleSource).toContain("@keyframes project-board-loading-spin");
  });

  test("keeps ticket dialog controls aligned with footer buttons", () => {
    /*
     * CDXC:ProjectBoardForms 2026-06-21-15:30:
     * New-ticket and edit-ticket action rows should use one shared Project Board control height for buttons and adjacent dropdowns so the macOS Kanban dialogs do not mix default shadcn button height with taller select triggers.
     *
     * CDXC:ProjectBoardForms 2026-06-22-02:17:
     * Kanban modal controls above the prompt editor should match footer button height too, including top metadata dropdowns, the label add row, and the ticket title text field.
     */
    const ticketDialogControlSource = sourceBetween(
      ".project-ticket-footer-select,",
      ".project-ticket-meta-grid {",
    );

    expect(ticketDialogControlSource).toContain('.project-ticket-meta-grid [data-slot="select-trigger"]');
    expect(ticketDialogControlSource).toContain(
      '.project-ticket-conversation-controls [data-slot="select-trigger"]',
    );
    expect(ticketDialogControlSource).toContain(".project-ticket-title-input");
    expect(ticketDialogControlSource).toContain(".project-ticket-label-editor input");
    expect(ticketDialogControlSource).toContain('height: var(--project-board-control-height);');
    expect(ticketDialogControlSource).toContain('min-height: var(--project-board-control-height);');
    expect(ticketDialogControlSource).toContain('.project-ticket-dialog-footer [data-slot="button"]');
    expect(ticketDialogControlSource).toContain('.project-ticket-create-actions > [data-slot="button"]');
    expect(ticketDialogControlSource).toContain('.project-ticket-label-editor > [data-slot="button"]');
    expect(ticketDialogControlSource).toContain('.project-ticket-conversation-controls > [data-slot="button"]');
  });

  test("uses brighter Kanban bead card surfaces than lane panels", () => {
    /*
     * CDXC:ProjectBoardCards 2026-06-19-09:14:
     * Kanban cards should stand out from the macOS Project board lanes before hover, so the card background token must stay visibly brighter than the lane panel token.
     */
    const variableSource = sourceBetween(":root {", "* { box-sizing: border-box; }");

    expect(variableSource).toContain("--project-board-panel: #171717;");
    expect(variableSource).toContain("--project-board-card: #242424;");
    expect(variableSource).toContain("--project-board-card-hover: #2b2b2b;");
  });

  test("prevents accidental text selection inside Kanban bead cards", () => {
    /*
     * CDXC:ProjectBoardCards 2026-06-13-13:55:
     * Kanban bead cards are draggable and right-clickable, so card text should not be user-selectable by accidental pointer movement.
     */
    const cardStyleSource = sourceBetween(".project-board-card {", ".project-board-card:hover");

    expect(cardStyleSource).toContain("user-select: none;");
  });

  test("renders Kanban bead context-menu Start work and Delete actions", () => {
    /*
     * CDXC:ProjectBoard 2026-06-13-13:37:
     * Right-clicking a Kanban bead card should expose Start work and Delete in the card context menu while reusing the existing ticket start/delete handlers.
     */
    const projectBoardSource = sourceBetween("function ProjectBoardApp()", "function TicketMetaFields(");
    const ticketCardSource = sourceBetween("function TicketCard(", "function ProjectBoardTicketContextMenu(");
    const contextMenuSource = sourceBetween("function ProjectBoardTicketContextMenu(", "function AutomationEmptyState(");

    expect(ticketCardSource).toContain("onContextMenu={(event) =>");
    expect(ticketCardSource).toContain("onOpenContextMenu(ticket");
    expect(projectBoardSource).toContain("void startTicketWork(contextMenuTicket)");
    expect(projectBoardSource).toContain("void deleteTicket(contextMenuTicket)");
    expect(projectBoardSource).toContain('"Start work"');
    expect(contextMenuSource).toContain('"Delete"');
  });

  test("shows the Beads creator distinctly from the assignee", () => {
    /*
     * CDXC:ProjectBoardCreator 2026-08-07-07:52:
     * Cards and Edit ticket show who created a bead next to who works it, so the two must read
     * differently: the creator is muted "by <name>" text with no person icon, the assignee keeps its
     * icon chip. Both resolve the creator through ticketCreatorName so it disappears when it is unset
     * or the same person as the assignee, and the assignee chip spells its role out in the tooltip.
     */
    const ticketCardSource = sourceBetween("function TicketCard(", "function ProjectBoardTicketContextMenu(");
    const metaFieldsSource = sourceBetween("function TicketMetaFields(", "function DependencyPicker(");
    const creatorStyleSource = sourceBetween(
      ".project-board-card-creator {",
      ".project-board-card-assignee {",
    );

    expect(ticketCardSource).toContain("ticketCreatorName(ticket.created_by, ticket.assignee)");
    expect(ticketCardSource).toContain('className="project-board-card-creator"');
    expect(ticketCardSource).toContain("by {creator}");
    expect(ticketCardSource).toContain("title={`Assigned to ${ticket.assignee}`}");
    expect(metaFieldsSource).toContain("ticketCreatorName(createdBy, assignee)");
    expect(metaFieldsSource).toContain("{creator ? (");
    expect(metaFieldsSource).toContain("<span>Created by</span>");
    expect(metaFieldsSource).toContain('className="project-ticket-creator-value"');
    expect(creatorStyleSource).toContain("text-overflow: ellipsis;");
    expect(tasksPlaceholderSource).toContain("createdBy={detail.ticket?.created_by}");
  });

  test("reports sanitized focus-owner events for native Kanban focus arbitration", () => {
    /*
     * CDXC:ProjectBoardFocus 2026-06-12-08:44:
     * Kanban typing focus must notify native with event categories only so focus arbitration can protect board input without recording user text, paths, URLs, ticket titles, or command content.
     */
    const focusOwnerSource = sourceBetween(
      "function postProjectBoardFocusOwnerChanged",
      "function createEmptyDetailDraft()",
    );
    const focusEffectSource = sourceBetween(
      "useEffect(() => {\n    let lastPostedAt = 0;",
      "  const openNewTicket = useCallback",
    );

    expect(focusOwnerSource).toContain('action: "projectEditorFocusOwnerChanged"');
    expect(focusOwnerSource).toContain("event,");
    expect(focusOwnerSource).toContain("projectEditorId,");
    expect(focusOwnerSource).toContain("projectId,");
    expect(focusOwnerSource).not.toContain("projectPath");
    expect(focusOwnerSource).not.toContain("details");
    expect(focusOwnerSource).not.toContain("ticketTitle");
    expect(focusEffectSource).toContain('window.addEventListener("pointerdown", handlePointerDown, true)');
    expect(focusEffectSource).toContain('window.addEventListener("focusin", handleFocusIn, true)');
    expect(focusEffectSource).toContain('window.addEventListener("keydown", handleKeyDown, true)');
    expect(focusEffectSource).toContain('postFocusOwnerChanged("keydown", event.target)');
    expect(focusEffectSource).toContain('event !== "pointerdown" && !isProjectBoardEditableFocusTarget(target)');
  });

  test("logs Kanban title-generation diagnostics without raw prompt-agent output", () => {
    /*
     * CDXC:ProjectBoardDiagnostics 2026-06-21-03:56:
     * Empty-title Kanban ticket diagnostics should join webview and native
     * title-generation attempts by ids, counts, lengths, and failure classes,
     * not by persisting prompt text, command text, stdout, stderr, or raw error
     * output in the support bundle.
     */
    const titleGenerationSource = sourceBetween(
      "const generateCreatedTicketTitle = async",
      "if (!createdIssue?.id)",
    );

    expect(titleGenerationSource).toContain("titleGenerationDebugDetails");
    expect(titleGenerationSource).toContain("defaultAgentKind");
    expect(titleGenerationSource).toContain("promptLength: prompt.length");
    expect(titleGenerationSource).toContain("promptAgentCommandLength");
    expect(titleGenerationSource).toContain("resolvedAgentKind");
    expect(titleGenerationSource).toContain("selectedAgentKind");
    expect(titleGenerationSource).toContain("issueId,");
    expect(titleGenerationSource).toContain("projectBoardTitleGenerationFailureDetails(error)");
    expect(titleGenerationSource).not.toContain("error: error instanceof Error ? error.message : String(error)");
    expect(tasksPlaceholderSource).toContain("function projectBoardPromptAgentKind");
    expect(tasksPlaceholderSource).toContain("function projectBoardTitleGenerationFailureDetails");
    expect(tasksPlaceholderSource).toContain("function projectBoardTitleGenerationErrorClass");
  });

  test("keeps generated Kanban titles background-only after deterministic draft creation", () => {
    /*
     * CDXC:ProjectBoardTitleGeneration 2026-06-21-16:56:
     * Empty-title Kanban ticket creation should use a deterministic draft title immediately, schedule prompt-agent title generation as detached background work, and patch only that card when the generated title lands.
     */
    const draftTitleSource = sourceBetween(
      "function createProjectBoardDraftTitle",
      "function applyPendingBoardStatusMoves",
    );
    const createTicketSource = sourceBetween("const createTicket = async", "const deleteTicket = async");
    const titleGenerationSource = sourceBetween(
      "const generateCreatedTicketTitle = async",
      "if (!createdIssue?.id)",
    );

    expect(tasksPlaceholderSource).not.toContain("PROJECT_BOARD_GENERATING_TITLE");
    expect(draftTitleSource).toContain("PROJECT_BOARD_DRAFT_TITLE_MAX_LENGTH");
    expect(draftTitleSource).toContain('return "New ticket";');
    expect(createTicketSource).toContain(
      "const title = shouldGenerateTitle ? createProjectBoardDraftTitle(prompt) : requestedTitle;",
    );
    expect(createTicketSource).toContain("void reconcileCreatedTicket();");
    expect(createTicketSource).toContain("scheduleProjectBoardGeneratedTitle(() =>");
    expect(createTicketSource).not.toContain("void reconcileCreatedTicket().then");
    expect(titleGenerationSource).toContain("setLocalTicketTitle(issueId, generatedTitle);");
    expect(titleGenerationSource).not.toContain('loadTickets({ mode: "background" })');
    expect(titleGenerationSource).not.toContain("setErrorMessage(");
  });

  test("closes edit-ticket Save before background persistence finishes", () => {
    /*
     * CDXC:ProjectBoardLocalFirst 2026-06-27-18:02:
     * Edit-ticket Save should dismiss the Kanban modal immediately, update the visible card optimistically, and use a native error toast plus the saved draft if detached Beads persistence fails.
     */
    const saveSource = sourceBetween("const persistTicketDetail = async", "const createTicket = async");
    const toastSource = sourceBetween("const showProjectBoardToast = useCallback", "const setLocalTicketStatus");

    expect(toastSource).toContain('action: "showToast"');
    expect(saveSource).toContain("setDetail(createEmptyDetailDraft());");
    expect(saveSource).toContain("upsertLocalIssue(optimisticIssue);");
    expect(saveSource).toContain("void persistTicketDetail(draft).catch");
    expect(saveSource).toContain('showProjectBoardToast("error", "Ticket save failed", message);');
    expect(saveSource).toContain("detailSaveSerialRef.current === saveToken");
    expect(saveSource).toContain("setDetail({\n          ...draft,\n          isDeleting: false,\n          isSaving: false,");
    expect(saveSource).not.toContain("setDetail((current) => ({ ...current, isSaving: true }))");
    expect(saveSource).not.toContain('await loadTickets({ mode: "mutation" })');
  });

  test("starts ticket work with the agent the bead is assigned to", () => {
    /*
     * CDXC:ProjectBoardStartWork 2026-08-07-07:01:
     * Opening a ticket assigned to a configured agent should show that agent in the
     * Start work select and start that agent from the card context menu too, while
     * an agent the user picked for that ticket this session stays selected.
     */
    const projectBoardSource = sourceBetween("function ProjectBoardApp()", "function TicketMetaFields(");
    const startWorkSource = sourceBetween("const startTicketWork = async", "const selectTicketAgent =");

    expect(projectBoardSource).toContain(
      "const pickedAgentIdByBeadIdRef = useRef(new Map<string, string>());",
    );
    expect(projectBoardSource).toContain(
      "pickedAgentIdByBeadIdRef.current.get(ticket.id) ??\n    resolveAssignedAgentId(ticket.assignee, conversationState.agents)",
    );
    expect(projectBoardSource).toContain("const nextAgentId = assignedAgentIdForTicket(ticket);");
    expect(projectBoardSource).toContain("pickedAgentIdByBeadIdRef.current.set(detail.ticket.id, agentId);");
    expect(projectBoardSource).toContain("onSelectedAgentChange={selectTicketAgent}");
    expect(startWorkSource).toContain(
      "assignedAgentIdForTicket(ticket) || selectedAgentId || conversationState.defaultAgentId",
    );
    expect(startWorkSource).toContain("agentId: startAgentId,");
  });

  test("sorts lanes in the board toolbar before the visible ticket limit", () => {
    /*
     * CDXC:ProjectBoardSort 2026-08-07:
     * The Kanban toolbar owns ticket order alongside the existing filters, and lane grouping must
     * sort before BoardLane slices to PROJECT_BOARD_MAX_VISIBLE_TICKETS_PER_COLUMN so the newest
     * closed beads stay visible in Done.
     */
    const projectBoardSource = sourceBetween("function ProjectBoardApp()", "function TicketMetaFields(");
    const filtersSource = sourceBetween(
      '<section className="project-board-filters"',
      '{activeSurfaceTab === "triage" ? (',
    );
    const laneSource = sourceBetween("function BoardLane({", "function TicketCard(");

    expect(projectBoardSource).toContain(
      "const [sortOption, setSortOption] = useState<BoardSortOption>(storedViewPreferences.sortOption);",
    );
    expect(projectBoardSource).toContain("result[column.key] = sortBoardTickets(");
    expect(projectBoardSource).toContain("sortOption,\n          column.key,");
    expect(filtersSource).toContain('aria-label="Sort tickets"');
    expect(filtersSource).toContain("PROJECT_BOARD_SORT_SELECT_ITEMS");
    expect(filtersSource).toContain("setSortOption(value as BoardSortOption)");
    expect(laneSource).toContain(
      "const visibleTickets = tickets.slice(0, PROJECT_BOARD_MAX_VISIBLE_TICKETS_PER_COLUMN);",
    );
  });

  test("restores and stores the board toolbar selections across projects", () => {
    /*
     * CDXC:ProjectBoardViewPreferences 2026-08-07:
     * Switching away from the Kanban tab unmounts the board surface, so the toolbar must seed its
     * priority, estimate, sort, and tag state from the stored preferences and write every later
     * selection back. The key and the write are project-independent so the selections follow the
     * user into every board. Search stays out of that payload.
     */
    const projectBoardSource = sourceBetween("function ProjectBoardApp()", "function TicketMetaFields(");

    expect(tasksPlaceholderSource).toContain(
      "function readProjectBoardViewPreferences(): ProjectBoardViewPreferences {",
    );
    expect(tasksPlaceholderSource).toContain(
      'JSON.parse(window.localStorage.getItem(PROJECT_BOARD_VIEW_PREFERENCES_STORAGE_KEY) || "null"),',
    );
    expect(tasksPlaceholderSource).toContain("return DEFAULT_PROJECT_BOARD_VIEW_PREFERENCES;");
    expect(projectBoardSource).toContain(
      "const storedViewPreferences = useMemo(() => readProjectBoardViewPreferences(), []);",
    );
    expect(projectBoardSource).toContain(
      "useState<BoardPriorityFilter>(\n    storedViewPreferences.priorityFilter,\n  );",
    );
    expect(projectBoardSource).toContain(
      "useState<BoardEstimateFilter>(\n    storedViewPreferences.estimateFilter,\n  );",
    );
    expect(projectBoardSource).toContain("useState<BoardSortOption>(storedViewPreferences.sortOption);");
    expect(projectBoardSource).toContain("useState<BoardTagFilter>(storedViewPreferences.tagFilter);");
    expect(projectBoardSource).toContain(
      "try {\n      window.localStorage.setItem(\n        PROJECT_BOARD_VIEW_PREFERENCES_STORAGE_KEY,\n        JSON.stringify({ estimateFilter, priorityFilter, sortOption, tagFilter }),\n      );\n    } catch {",
    );
    expect(projectBoardSource).toContain("}, [estimateFilter, priorityFilter, sortOption, tagFilter]);");
    expect(projectBoardSource).not.toContain("JSON.stringify({ estimateFilter, priorityFilter, searchQuery");
    expect(projectBoardSource).toContain('const [searchQuery, setSearchQuery] = useState("");');
  });

  test("snapshots form values before functional state updaters", () => {
    /*
     * CDXC:ProjectBoardForms 2026-06-09-15:36:
     * New automation and ticket text entry should keep the Kanban page mounted even when React defers functional state updaters.
     * Updater closures must use already-captured primitives instead of reading value or checked from the React event object.
     */
    const projectBoardSource = sourceBetween("function ProjectBoardApp()", "function TicketMetaFields(");
    const updaterCalls = [
      ...collectFunctionalUpdaterCalls(projectBoardSource, "setAutomationDraft((current) =>"),
      ...collectFunctionalUpdaterCalls(projectBoardSource, "setDetail((current) =>"),
      ...collectFunctionalUpdaterCalls(projectBoardSource, "setNewTicket((current) =>"),
    ];

    expect(updaterCalls).not.toHaveLength(0);
    expect(
      updaterCalls.filter((call) => /event\.(?:currentTarget|target)\.(?:checked|value)/u.test(call)),
    ).toEqual([]);
  });
});
