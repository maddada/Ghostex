import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const stylesSource = readFileSync(new URL('./project-board/styles.ts', import.meta.url), 'utf8');
const projectBoardAppSource = readFileSync(new URL('./project-board/project-board-app.tsx', import.meta.url), 'utf8');
const boardStateSource = readFileSync(new URL('./project-board/board-state.ts', import.meta.url), 'utf8');
const ticketDetailSource = readFileSync(new URL('./project-board/ticket-detail.tsx', import.meta.url), 'utf8');
const boardLaneCardSource = readFileSync(new URL('./project-board/board-lane-card.tsx', import.meta.url), 'utf8');
const constantsSource = readFileSync(new URL('./project-board/constants.ts', import.meta.url), 'utf8');
const ticketDialogsSource = readFileSync(new URL('./project-board/ticket-dialogs.tsx', import.meta.url), 'utf8');
const automationDialogSource = readFileSync(new URL('./project-board/automation-dialog.tsx', import.meta.url), 'utf8');
const boardColumnsDialogSource = readFileSync(
  new URL('./project-board/board-columns-dialog.tsx', import.meta.url),
  'utf8'
);

function sourceBetweenIn(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

function sourceFrom(source: string, start: string): string {
  const startIndex = source.indexOf(start);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  return source.slice(startIndex);
}

function collectFunctionalUpdaterCalls(source: string, setterCallStart: string): string[] {
  const calls: string[] = [];
  let searchIndex = 0;
  while (searchIndex < source.length) {
    const startIndex = source.indexOf(setterCallStart, searchIndex);
    if (startIndex === -1) {
      break;
    }
    const openParenIndex = source.indexOf('(', startIndex);
    expect(openParenIndex).toBeGreaterThan(startIndex);
    let depth = 0;
    let endIndex = openParenIndex;
    for (; endIndex < source.length; endIndex += 1) {
      const char = source[endIndex];
      if (char === '(') {
        depth += 1;
      } else if (char === ')') {
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

describe('Project Board form event handling', () => {
  test('renders Kanban swimlanes as rounded panels with a gutter', () => {
    /*
     * CDXC:ProjectBoardRedesign 2026-08-23:
     * The Codex-style board separates swimlanes with a gutter and gives each
     * lane its own rounded panel, replacing the old zero-gap shared-border
     * strip. Layout lives in Tailwind on the components, not styles.ts.
     */
    const lanesSource = sourceBetweenIn(projectBoardAppSource, 'aria-label="Project issue board"', '</section>');
    const laneSource = sourceBetweenIn(boardLaneCardSource, 'function BoardLane({', 'function TicketCard(');

    expect(projectBoardAppSource).toContain('gap-2.5 overflow-x-auto');
    expect(lanesSource).toContain('<BoardLane');
    expect(laneSource).toContain('rounded-xl border border-border/80');
  });

  test('keeps the Kanban board scrollbars grabbable with the mouse', () => {
    /*
     * CDXC:BoardScrollbars 2026-08-07:
     * The board strip and the lane bodies must keep a real scrollbar box, or the
     * board is wheel-only: Chromium measured a 0px scroll gutter (nothing to
     * click or drag) whenever the scroller carried scrollbar-width: none, a
     * scrollbar-color, or a zero-sized ::-webkit-scrollbar. A decorative
     * pointer-events: none overlay is not a scrollbar and must not come back.
     */
    const boardScrollbarSource = sourceBetweenIn(
      stylesSource,
      '.project-board-lanes,\n  .project-board-lane-scroll,\n  .project-ticket-dialog-body {',
      '.project-automation-dialog {'
    );

    expect(boardScrollbarSource).toContain('scrollbar-width: auto;');
    expect(boardScrollbarSource).not.toContain('scrollbar-color');
    expect(boardScrollbarSource).toContain('height: 8px;');
    expect(boardScrollbarSource).toContain('width: 8px;');
    expect(stylesSource).not.toContain('project-board-lane-scrollbar');
  });

  test('keeps the ticket dialog body scrollbar grabbable with the mouse', () => {
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
    const hiddenScrollbarSource = sourceBetweenIn(
      stylesSource,
      '.project-ticket-comment-list [data-slot="scroll-area-viewport"] {',
      '.project-board-lanes,'
    );
    const dialogScrollbarSource = sourceBetweenIn(
      stylesSource,
      '.project-board-lanes,\n  .project-board-lane-scroll,\n  .project-ticket-dialog-body {',
      '.project-automation-dialog {'
    );
    /*
     * CDXC:ProjectBoardDialogRedesign 2026-08-24:
     * The dialog body thumb reveals on :hover only; :focus-within kept the bar
     * permanently visible because a form dialog always has a focused field.
     */
    const dialogThumbHoverSource = sourceBetweenIn(
      stylesSource,
      '.project-ticket-dialog-body:hover::-webkit-scrollbar-thumb {',
      '.project-ticket-comment-list [data-slot="scroll-area-scrollbar"] {'
    );

    expect(hiddenScrollbarSource).not.toContain('project-ticket-dialog-body');
    expect(hiddenScrollbarSource).toContain('scrollbar-width: none;');
    expect(hiddenScrollbarSource).toContain('width: 2px;');
    expect(dialogScrollbarSource).toContain('.project-ticket-dialog-body::-webkit-scrollbar {');
    expect(dialogScrollbarSource).toContain('.project-ticket-dialog-body::-webkit-scrollbar-thumb {');
    expect(dialogThumbHoverSource).toContain('background-color: var(--project-board-scrollbar);');
  });

  test('shows the first-open Kanban loading overlay until initial load finishes', () => {
    /*
     * CDXC:ProjectBoardLoading 2026-06-20-18:21:
     * The first macOS Kanban open should cover the mounted lanes with a spinner until initial Beads loading finishes, while later refreshes should not replay that mask.
     */
    const projectBoardSource = sourceFrom(projectBoardAppSource, 'function ProjectBoardApp()');
    const overlayStyleSource = sourceBetweenIn(
      stylesSource,
      '.project-board-board-region {',
      '.project-board-notice {'
    );

    expect(projectBoardSource).toContain(
      'const [hasCompletedInitialBoardLoad, setHasCompletedInitialBoardLoad] = useState(false);'
    );
    expect(projectBoardSource).toContain('if (mode === "initial") {');
    expect(projectBoardSource).toContain('setHasCompletedInitialBoardLoad(true);');
    expect(projectBoardSource).toContain(
      'activeSurfaceTab === "board" && loadState === "loading" && !hasCompletedInitialBoardLoad'
    );
    expect(projectBoardSource).toContain('className="project-board-loading-overlay"');
    expect(projectBoardSource).toContain('role="status"');
    expect(projectBoardSource).toContain('IconLoader2');
    expect(overlayStyleSource).toContain('position: relative;');
    expect(overlayStyleSource).toContain('.project-board-loading-overlay');
    expect(overlayStyleSource).toContain('position: absolute;');
    expect(overlayStyleSource).toContain('.project-board-loading-spinner');
    expect(overlayStyleSource).toContain('@keyframes project-board-loading-spin');
  });

  test('gives every Kanban dialog control one height and one text size', () => {
    /*
     * CDXC:ProjectBoardDialogRedesign 2026-08-24:
     * The dialogs used to opt individual classes into the Project Board control
     * height, so anything the list missed rendered at shadcn's own size: select
     * triggers came out taller than the buttons beside them and dropdowns
     * disagreed with each other on font size. One rule now covers every input
     * and select trigger in a dialog, and one rule pins the shared 14px text
     * scale across inputs, textareas, dropdowns, and buttons. The per-class
     * height opt-ins and every size="sm" control in a dialog row must stay gone.
     */
    const dialogControlSource = sourceBetweenIn(
      stylesSource,
      'CDXC:ProjectBoardDialogRedesign 2026-08-24:',
      'CDXC:ProjectBoardRoundness 2026-06-29-20:55:'
    );

    expect(dialogControlSource).toContain('.project-ticket-dialog [data-slot="input"]');
    expect(dialogControlSource).toContain('.project-ticket-dialog [data-slot="select-trigger"]');
    expect(dialogControlSource).toContain('.project-ticket-dialog [data-slot="textarea"]');
    expect(dialogControlSource).toContain('.project-ticket-dialog [data-slot="button"]');
    expect(dialogControlSource).toContain('height: var(--project-board-control-height);');
    expect(dialogControlSource).toContain('min-height: var(--project-board-control-height);');
    expect(dialogControlSource).toContain('font-size: 14px;');
    expect(dialogControlSource).toContain('font-weight: 400;');
    expect(stylesSource).not.toContain('.project-ticket-dialog-footer [data-slot="button"],');
    expect(stylesSource).not.toContain('.project-ticket-title-input,\n  .project-ticket-label-editor input {');
    expect(ticketDialogsSource).not.toContain('size="sm"');
    expect(ticketDetailSource).not.toContain('size="sm"');
    expect(boardColumnsDialogSource).not.toContain('size="sm"');
    // The automation dialog keeps one size="sm", on its pill Switch, which is
    // not a form-row control.
    expect(automationDialogSource).not.toContain('<SelectTrigger size="sm"');
    expect(automationDialogSource).not.toContain('<Button size="sm"');
  });

  test('keeps the Kanban dialogs on the raised panel surface without bold chrome', () => {
    /*
     * CDXC:ProjectBoardDialogRedesign 2026-08-24:
     * Dialogs sit on the board's #161616 panel token with the 12px section
     * radius, and their titles, labels, and section headers stay at regular or
     * 500 weight so no dialog text reads bolder than the page behind it.
     */
    const dialogSurfaceSource = sourceBetweenIn(
      stylesSource,
      '  .project-ticket-dialog {',
      '.project-ticket-dialog-body {'
    );

    expect(dialogSurfaceSource).toContain('background: var(--popover, #161616);');
    expect(dialogSurfaceSource).toContain('border-radius: var(--project-board-radius-section);');
    expect(dialogSurfaceSource).not.toContain('--app-modal-background');
    expect(stylesSource).not.toContain('font-weight: 650;');
    expect(stylesSource).not.toContain('font-weight: 700;');
    expect(ticketDialogsSource).toContain('<DialogTitle className="text-[15px] font-normal">');
    expect(automationDialogSource).toContain('<DialogTitle className="text-[15px] font-normal">');
    expect(boardColumnsDialogSource).toContain('<DialogTitle className="text-[15px] font-normal">');
  });

  test('uses brighter Kanban bead card surfaces than lane panels', () => {
    /*
     * CDXC:ProjectBoardCards 2026-06-19-09:14:
     * Kanban cards should stand out from the macOS Project board lanes before hover, so the card background token must stay visibly brighter than the lane panel token.
     */
    const variableSource = sourceBetweenIn(stylesSource, ':root {', '* { box-sizing: border-box; }');

    expect(variableSource).toContain('--project-board-panel: #161616;');
    expect(variableSource).toContain('--project-board-card: #1d1d1d;');
    expect(variableSource).toContain('--project-board-card-hover: #232323;');
  });

  test('prevents accidental text selection inside Kanban bead cards', () => {
    /*
     * CDXC:ProjectBoardCards 2026-06-13-13:55:
     * Kanban bead cards are draggable and right-clickable, so card text should not be user-selectable by accidental pointer movement.
     */
    const ticketCardSource = sourceBetweenIn(
      boardLaneCardSource,
      'function TicketCard(',
      'function ProjectBoardTicketContextMenu('
    );

    expect(ticketCardSource).toContain('select-none');
  });

  test('renders Kanban bead context-menu Start work and Delete actions', () => {
    /*
     * CDXC:ProjectBoard 2026-06-13-13:37:
     * Right-clicking a Kanban bead card should expose Start work and Delete in the card context menu while reusing the existing ticket start/delete handlers.
     */
    const projectBoardSource = sourceFrom(projectBoardAppSource, 'function ProjectBoardApp()');
    const ticketCardSource = sourceBetweenIn(
      boardLaneCardSource,
      'function TicketCard(',
      'function ProjectBoardTicketContextMenu('
    );
    const contextMenuSource = sourceFrom(boardLaneCardSource, 'function ProjectBoardTicketContextMenu(');

    expect(ticketCardSource).toContain('onContextMenu={(event) =>');
    expect(ticketCardSource).toContain('onOpenContextMenu(ticket');
    expect(projectBoardSource).toContain('void startTicketWork(contextMenuTicket)');
    expect(projectBoardSource).toContain('void deleteTicket(contextMenuTicket)');
    expect(projectBoardSource).toContain('"Start work"');
    expect(contextMenuSource).toContain('"Delete"');
  });

  test('shows the Beads creator distinctly from the assignee', () => {
    /*
     * CDXC:ProjectBoardCreator 2026-08-07-07:52:
     * Cards and Edit ticket show who created a bead next to who works it, so the two must read
     * differently: the creator is muted "by <name>" text with no person icon, the assignee keeps its
     * icon chip. Both resolve the creator through ticketCreatorName so it disappears when it is unset
     * or the same person as the assignee, and the assignee chip spells its role out in the tooltip.
     */
    const ticketCardSource = sourceBetweenIn(
      boardLaneCardSource,
      'function TicketCard(',
      'function ProjectBoardTicketContextMenu('
    );
    const metaFieldsSource = sourceBetweenIn(
      ticketDetailSource,
      'function TicketMetaFields(',
      'function DependencyPicker('
    );
    expect(ticketCardSource).toContain('ticketCreatorName(ticket.created_by, ticket.assignee)');
    expect(ticketCardSource).toContain('Created by {creator}');
    expect(ticketCardSource).toContain('title={`Assigned to ${ticket.assignee}`}');
    expect(metaFieldsSource).toContain('ticketCreatorName(createdBy, assignee)');
    expect(metaFieldsSource).toContain('{creator ? (');
    expect(metaFieldsSource).toContain('<span>Created by</span>');
    expect(metaFieldsSource).toContain('className="project-ticket-creator-value"');
    expect(ticketDialogsSource).toContain('createdBy={detail.ticket?.created_by}');
  });

  test('reports sanitized focus-owner events for native Kanban focus arbitration', () => {
    /*
     * CDXC:ProjectBoardFocus 2026-06-12-08:44:
     * Kanban typing focus must notify native with event categories only so focus arbitration can protect board input without recording user text, paths, URLs, ticket titles, or command content.
     */
    const focusOwnerSource = sourceBetweenIn(
      boardStateSource,
      'function postProjectBoardFocusOwnerChanged',
      'function createEmptyDetailDraft()'
    );
    const focusEffectSource = sourceBetweenIn(
      projectBoardAppSource,
      'useEffect(() => {\n    let lastPostedAt = 0;',
      '  const openNewTicket = useCallback'
    );

    expect(focusOwnerSource).toContain('action: "projectEditorFocusOwnerChanged"');
    expect(focusOwnerSource).toContain('event,');
    expect(focusOwnerSource).toContain('projectEditorId,');
    expect(focusOwnerSource).toContain('projectId,');
    expect(focusOwnerSource).not.toContain('projectPath');
    expect(focusOwnerSource).not.toContain('details');
    expect(focusOwnerSource).not.toContain('ticketTitle');
    expect(focusEffectSource).toContain('window.addEventListener("pointerdown", handlePointerDown, true)');
    expect(focusEffectSource).toContain('window.addEventListener("focusin", handleFocusIn, true)');
    expect(focusEffectSource).toContain('window.addEventListener("keydown", handleKeyDown, true)');
    expect(focusEffectSource).toContain('postFocusOwnerChanged("keydown", event.target)');
    expect(focusEffectSource).toContain('event !== "pointerdown" && !isProjectBoardEditableFocusTarget(target)');
  });

  test('logs Kanban title-generation diagnostics without raw prompt-agent output', () => {
    /*
     * CDXC:ProjectBoardDiagnostics 2026-06-21-03:56:
     * Empty-title Kanban ticket diagnostics should join webview and native
     * title-generation attempts by ids, counts, lengths, and failure classes,
     * not by persisting prompt text, command text, stdout, stderr, or raw error
     * output in the support bundle.
     */
    const titleGenerationSource = sourceBetweenIn(
      projectBoardAppSource,
      'const generateCreatedTicketTitle = async',
      'if (!createdIssue?.id)'
    );

    expect(titleGenerationSource).toContain('titleGenerationDebugDetails');
    expect(titleGenerationSource).toContain('defaultAgentKind');
    expect(titleGenerationSource).toContain('promptLength: prompt.length');
    expect(titleGenerationSource).toContain('promptAgentCommandLength');
    expect(titleGenerationSource).toContain('resolvedAgentKind');
    expect(titleGenerationSource).toContain('selectedAgentKind');
    expect(titleGenerationSource).toContain('issueId,');
    expect(titleGenerationSource).toContain('projectBoardTitleGenerationFailureDetails(error)');
    expect(titleGenerationSource).not.toContain('error: error instanceof Error ? error.message : String(error)');
    expect(boardStateSource).toContain('function projectBoardPromptAgentKind');
    expect(boardStateSource).toContain('function projectBoardTitleGenerationFailureDetails');
    expect(boardStateSource).toContain('function projectBoardTitleGenerationErrorClass');
  });

  test('keeps generated Kanban titles background-only after deterministic draft creation', () => {
    /*
     * CDXC:ProjectBoardTitleGeneration 2026-06-21-16:56:
     * Empty-title Kanban ticket creation should use a deterministic draft title immediately, schedule prompt-agent title generation as detached background work, and patch only that card when the generated title lands.
     */
    const draftTitleSource = sourceBetweenIn(
      boardStateSource,
      'function createProjectBoardDraftTitle',
      'function applyPendingBoardStatusMoves'
    );
    const createTicketSource = sourceBetweenIn(
      projectBoardAppSource,
      'const createTicket = async',
      'const deleteTicket = async'
    );
    const titleGenerationSource = sourceBetweenIn(
      projectBoardAppSource,
      'const generateCreatedTicketTitle = async',
      'if (!createdIssue?.id)'
    );

    expect(projectBoardAppSource).not.toContain('PROJECT_BOARD_GENERATING_TITLE');
    expect(boardStateSource).not.toContain('PROJECT_BOARD_GENERATING_TITLE');
    expect(draftTitleSource).toContain('PROJECT_BOARD_DRAFT_TITLE_MAX_LENGTH');
    expect(draftTitleSource).toContain('return "New ticket";');
    expect(createTicketSource).toContain(
      'const title = shouldGenerateTitle ? createProjectBoardDraftTitle(prompt) : requestedTitle;'
    );
    expect(createTicketSource).toContain('void reconcileCreatedTicket();');
    expect(createTicketSource).toContain('scheduleProjectBoardGeneratedTitle(() =>');
    expect(createTicketSource).not.toContain('void reconcileCreatedTicket().then');
    expect(titleGenerationSource).toContain('setLocalTicketTitle(issueId, generatedTitle);');
    expect(titleGenerationSource).not.toContain('loadTickets({ mode: "background" })');
    expect(titleGenerationSource).not.toContain('setErrorMessage(');
  });

  test('closes edit-ticket Save before background persistence finishes', () => {
    /*
     * CDXC:ProjectBoardLocalFirst 2026-06-27-18:02:
     * Edit-ticket Save should dismiss the Kanban modal immediately, update the visible card optimistically, and use a native error toast plus the saved draft if detached Beads persistence fails.
     */
    const saveSource = sourceBetweenIn(
      projectBoardAppSource,
      'const persistTicketDetail = async',
      'const createTicket = async'
    );
    const toastSource = sourceBetweenIn(
      projectBoardAppSource,
      'const showProjectBoardToast = useCallback',
      'const setLocalTicketStatus'
    );

    expect(toastSource).toContain('action: "showToast"');
    expect(saveSource).toContain('setDetail(createEmptyDetailDraft());');
    expect(saveSource).toContain('upsertLocalIssue(optimisticIssue);');
    expect(saveSource).toContain('void persistTicketDetail(draft).catch');
    expect(saveSource).toContain('showProjectBoardToast("error", "Ticket save failed", message);');
    expect(saveSource).toContain('detailSaveSerialRef.current === saveToken');
    expect(saveSource).toContain(
      'setDetail({\n          ...draft,\n          isDeleting: false,\n          isSaving: false,'
    );
    expect(saveSource).not.toContain('setDetail((current) => ({ ...current, isSaving: true }))');
    expect(saveSource).not.toContain('await loadTickets({ mode: "mutation" })');
  });

  test('starts ticket work with the agent the bead is assigned to', () => {
    /*
     * CDXC:ProjectBoardStartWork 2026-08-07-07:01:
     * Opening a ticket assigned to a configured agent should show that agent in the
     * Start work select and start that agent from the card context menu too, while
     * an agent the user picked for that ticket this session stays selected.
     */
    const projectBoardSource = sourceFrom(projectBoardAppSource, 'function ProjectBoardApp()');
    const startWorkSource = sourceBetweenIn(
      projectBoardAppSource,
      'const startTicketWork = async',
      'const selectTicketAgent ='
    );

    expect(projectBoardSource).toContain('const pickedAgentIdByBeadIdRef = useRef(new Map<string, string>());');
    expect(projectBoardSource).toContain(
      'pickedAgentIdByBeadIdRef.current.get(ticket.id) ??\n    resolveAssignedAgentId(ticket.assignee, conversationState.agents)'
    );
    expect(projectBoardSource).toContain('const nextAgentId = assignedAgentIdForTicket(ticket);');
    expect(projectBoardSource).toContain('pickedAgentIdByBeadIdRef.current.set(detail.ticket.id, agentId);');
    expect(projectBoardSource).toContain('onSelectedAgentChange={selectTicketAgent}');
    expect(startWorkSource).toContain(
      'assignedAgentIdForTicket(ticket) || selectedAgentId || conversationState.defaultAgentId'
    );
    expect(startWorkSource).toContain('agentId: startAgentId,');
  });

  test('sorts lanes in the board toolbar before the visible ticket limit', () => {
    /*
     * CDXC:ProjectBoardSort 2026-08-07:
     * The Kanban toolbar owns ticket order alongside the existing filters, and lane grouping must
     * sort before BoardLane slices to PROJECT_BOARD_MAX_VISIBLE_TICKETS_PER_COLUMN so the newest
     * closed beads stay visible in Done.
     */
    const projectBoardSource = sourceFrom(projectBoardAppSource, 'function ProjectBoardApp()');
    const filtersSource = sourceBetweenIn(
      projectBoardAppSource,
      'aria-label="Ticket filters"',
      '{activeSurfaceTab === "triage" ? ('
    );
    const laneSource = sourceBetweenIn(boardLaneCardSource, 'function BoardLane({', 'function TicketCard(');

    expect(projectBoardSource).toContain(
      'const [sortOption, setSortOption] = useState<BoardSortOption>(storedViewPreferences.sortOption);'
    );
    expect(projectBoardSource).toContain('result[column.key] = sortBoardTickets(');
    expect(projectBoardSource).toContain('sortOption,\n          column.key,');
    expect(filtersSource).toContain('aria-label="Sort tickets"');
    expect(filtersSource).toContain('PROJECT_BOARD_SORT_SELECT_ITEMS');
    expect(filtersSource).toContain('setSortOption(value as BoardSortOption)');
    expect(laneSource).toContain(
      'const visibleTickets = tickets.slice(0, PROJECT_BOARD_MAX_VISIBLE_TICKETS_PER_COLUMN);'
    );
  });

  test('restores and stores the board toolbar selections across projects', () => {
    /*
     * CDXC:ProjectBoardViewPreferences 2026-08-07:
     * Switching away from the Kanban tab unmounts the board surface, so the toolbar must seed its
     * priority, estimate, sort, and tag state from the stored preferences and write every later
     * selection back. The key and the write are project-independent so the selections follow the
     * user into every board. Search stays out of that payload.
     */
    const projectBoardSource = sourceFrom(projectBoardAppSource, 'function ProjectBoardApp()');

    expect(constantsSource).toContain('function readProjectBoardViewPreferences(): ProjectBoardViewPreferences {');
    expect(constantsSource).toContain(
      'JSON.parse(window.localStorage.getItem(PROJECT_BOARD_VIEW_PREFERENCES_STORAGE_KEY) || "null"),'
    );
    expect(constantsSource).toContain('return DEFAULT_PROJECT_BOARD_VIEW_PREFERENCES;');
    expect(projectBoardSource).toContain(
      'const storedViewPreferences = useMemo(() => readProjectBoardViewPreferences(), []);'
    );
    expect(projectBoardSource).toContain(
      'useState<BoardPriorityFilter>(\n    storedViewPreferences.priorityFilter,\n  );'
    );
    expect(projectBoardSource).toContain(
      'useState<BoardEstimateFilter>(\n    storedViewPreferences.estimateFilter,\n  );'
    );
    expect(projectBoardSource).toContain('useState<BoardSortOption>(storedViewPreferences.sortOption);');
    expect(projectBoardSource).toContain('useState<BoardTagFilter>(storedViewPreferences.tagFilter);');
    expect(projectBoardSource).toContain(
      'try {\n      window.localStorage.setItem(\n        PROJECT_BOARD_VIEW_PREFERENCES_STORAGE_KEY,\n        JSON.stringify({ estimateFilter, priorityFilter, sortOption, tagFilter }),\n      );\n    } catch {'
    );
    expect(projectBoardSource).toContain('}, [estimateFilter, priorityFilter, sortOption, tagFilter]);');
    expect(projectBoardSource).not.toContain('JSON.stringify({ estimateFilter, priorityFilter, searchQuery');
    expect(projectBoardSource).toContain('const [searchQuery, setSearchQuery] = useState("");');
  });

  test('snapshots form values before functional state updaters', () => {
    /*
     * CDXC:ProjectBoardForms 2026-06-09-15:36:
     * New automation and ticket text entry should keep the Kanban page mounted even when React defers functional state updaters.
     * Updater closures must use already-captured primitives instead of reading value or checked from the React event object.
     */
    const dialogSources = [
      sourceFrom(projectBoardAppSource, 'function ProjectBoardApp()'),
      ticketDialogsSource,
      automationDialogSource,
    ];
    const updaterCalls = dialogSources.flatMap((source) => [
      ...collectFunctionalUpdaterCalls(source, 'setAutomationDraft((current) =>'),
      ...collectFunctionalUpdaterCalls(source, 'setDetail((current) =>'),
      ...collectFunctionalUpdaterCalls(source, 'setNewTicket((current) =>'),
    ]);

    expect(updaterCalls).not.toHaveLength(0);
    expect(updaterCalls.filter((call) => /event\.(?:currentTarget|target)\.(?:checked|value)/u.test(call))).toEqual([]);
  });
});
