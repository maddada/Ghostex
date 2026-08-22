import { describe, expect, test } from "vitest";
import {
  appendImageMarkdownToDescription,
  BOARD_SORT_OPTIONS,
  beadsStatusToBoardStatus,
  boardStatusBeadsValue,
  boardStatusLabel,
  boardTagFilterOptions,
  buildAgentWorkPrompt,
  buildBoardColumns,
  DEFAULT_PROJECT_BOARD_VIEW_PREFERENCES,
  extractDescriptionImagePreviews,
  extractDescriptionImageReferences,
  ensureIssuePrefix,
  filterBoardTickets,
  formatProjectBoardCommentText,
  normalizeProjectBoardViewPreferences,
  parseProjectBoardCommentText,
  priorityLabel,
  prioritySelectValue,
  projectBoardRawProjectIdFromUrlParam,
  PROJECT_BOARD_VIEW_PREFERENCES_STORAGE_KEY,
  removeDescriptionImageReference,
  resolveAssignedAgentId,
  resolveBoardTagFilter,
  sortBoardTickets,
  ticketCreatorName,
  type BoardTicket,
} from "./project-board-shared";

const pngDataUrl = "data:image/png;base64,abc123";
const cleanShotImagePath = "/Users/madda/Library/Application Support/CleanShot/media/media_x/2026-05-23_Ghostex_13-53-53@2x.png";
const savedImagePath = "~/.ghostex/i/260528082700.png";

describe("project board description image helpers", () => {
  test("keeps image references visible in the prompt text", () => {
    const description = `Before\n\n[Image #1](${cleanShotImagePath})\n\nAfter`;

    expect(extractDescriptionImagePreviews(description)).toEqual([cleanShotImagePath]);
  });

  test("inserts pasted image references at the caret", () => {
    const description = "Before after";
    const insertAt = "Before".length;

    expect(appendImageMarkdownToDescription(description, cleanShotImagePath, insertAt, insertAt)).toBe(
      `Before\n\n[Image #1](${cleanShotImagePath})\n\n after`,
    );
  });

  test("numbers pasted image references from existing visible image labels", () => {
    const existingImageMarkdown = `[Image #1](${cleanShotImagePath})`;

    expect(appendImageMarkdownToDescription(`Prompt\n\n${existingImageMarkdown}`, savedImagePath)).toBe(
      `Prompt\n\n${existingImageMarkdown}\n\n[Image #2](${savedImagePath})`,
    );
  });

  test("previews standalone pasted image paths entered in the text", () => {
    const description = `Prompt\n\n${cleanShotImagePath}\n\nNotes`;

    expect(extractDescriptionImagePreviews(description)).toEqual([cleanShotImagePath]);
  });

  test("prefers pasted paths over legacy data URI image Markdown for previews", () => {
    const description = `Prompt\n\n[Image #1](${cleanShotImagePath})\n\n![pasted-image](${pngDataUrl})`;

    expect(extractDescriptionImagePreviews(description)).toEqual([cleanShotImagePath]);
  });

  test("removes a selected thumbnail image from the persisted description", () => {
    const description = `Prompt\n\n[Image #1](${cleanShotImagePath})\n\n[Image #2](${savedImagePath})`;
    const [, secondImage] = extractDescriptionImageReferences(description);

    expect(secondImage).toBeDefined();
    expect(removeDescriptionImageReference(description, secondImage!.id)).toBe(
      `Prompt\n\n[Image #1](${cleanShotImagePath})`,
    );
  });

  test("keeps the preview source list compatible with existing callers", () => {
    const description = `Prompt\n\n![pasted-image](${pngDataUrl})`;

    expect(extractDescriptionImagePreviews(description)).toEqual([pngDataUrl]);
  });
});

describe("project board priority labels", () => {
  test("uses urgency words while preserving numeric bd values", () => {
    expect([0, 1, 2, 3].map((priority) => priorityLabel(priority))).toEqual([
      "Urgent",
      "High",
      "Medium",
      "Low",
    ]);
  });

  test("normalizes legacy P4 values into the visible Low tier", () => {
    expect(priorityLabel(4)).toBe("Low");
    expect(prioritySelectValue(4)).toBe("3");
  });
});

describe("project board creator", () => {
  test("shows the creator only when it differs from the assignee", () => {
    expect(ticketCreatorName("harry", "dobby")).toBe("harry");
    expect(ticketCreatorName("harry", "harry")).toBeUndefined();
    expect(ticketCreatorName("harry", undefined)).toBe("harry");
    expect(ticketCreatorName(undefined, "dobby")).toBeUndefined();
    expect(ticketCreatorName("", "dobby")).toBeUndefined();
  });
});

describe("project board filters", () => {
  const tickets: BoardTicket[] = [
    {
      boardStatus: "todo",
      displayId: "ZMX-1",
      estimate: 15,
      id: "urgent-xs",
      labels: ["docs", "needs:review"],
      priority: 0,
      status: "open",
      title: "Urgent XS task",
    },
    {
      boardStatus: "in_progress",
      displayId: "ZMX-2",
      estimate: null,
      id: "medium-none",
      labels: ["backend", "docs"],
      priority: 2,
      status: "in_progress",
      title: "Medium unestimated task",
    },
    {
      boardStatus: "review",
      displayId: "ZMX-3",
      estimate: 120,
      id: "legacy-low",
      priority: 4,
      status: "review",
      title: "Legacy low task",
    },
  ];

  test("filters by normalized priority and estimate without changing lane status", () => {
    expect(filterBoardTickets(tickets, "", "3", "all", "all").map((ticket) => ticket.id)).toEqual([
      "legacy-low",
    ]);
    expect(filterBoardTickets(tickets, "", "all", "none", "all").map((ticket) => ticket.id)).toEqual([
      "medium-none",
    ]);
    expect(filterBoardTickets(tickets, "", "0", "XS", "all").map((ticket) => ticket.id)).toEqual([
      "urgent-xs",
    ]);
  });

  test("filters by tag alongside the other toolbar selections", () => {
    /*
     * CDXC:ProjectBoardTagFilter 2026-08-21:
     * The tag control only ever includes, so a selected tag narrows the board to the tickets that
     * carry it and stacks with priority and estimate instead of replacing them. Tickets with no
     * labels are simply not in that set rather than being treated as a selectable state of their
     * own, because an untagged bead is untriaged rather than a kind of work.
     */
    expect(filterBoardTickets(tickets, "", "all", "all", "docs").map((ticket) => ticket.id)).toEqual([
      "urgent-xs",
      "medium-none",
    ]);
    expect(filterBoardTickets(tickets, "", "all", "all", "backend").map((ticket) => ticket.id)).toEqual([
      "medium-none",
    ]);
    expect(filterBoardTickets(tickets, "", "0", "all", "docs").map((ticket) => ticket.id)).toEqual([
      "urgent-xs",
    ]);
    expect(filterBoardTickets(tickets, "", "0", "all", "backend")).toEqual([]);
    expect(filterBoardTickets(tickets, "", "all", "all", "all").map((ticket) => ticket.id)).toEqual([
      "urgent-xs",
      "medium-none",
      "legacy-low",
    ]);
  });

  test("offers the loaded tickets' own labels, sorted, with all first", () => {
    expect(boardTagFilterOptions(tickets)).toEqual(["all", "backend", "docs", "needs:review"]);
    expect(boardTagFilterOptions([])).toEqual(["all"]);
  });

  test("resolves a tag the loaded board does not offer back to all", () => {
    /*
     * CDXC:ProjectBoardTagFilter 2026-08-21:
     * A stored tag outlives the board that produced it, so opening a project that never used it
     * must show the whole board rather than an empty one under a tag nothing carries.
     */
    expect(resolveBoardTagFilter("frontend", boardTagFilterOptions(tickets))).toBe("all");
    expect(resolveBoardTagFilter("docs", boardTagFilterOptions(tickets))).toBe("docs");
    expect(resolveBoardTagFilter("docs", boardTagFilterOptions([]))).toBe("all");
  });
});

describe("project board sorting", () => {
  const doneTickets: BoardTicket[] = [
    {
      boardStatus: "done",
      closed_at: "2026-06-01T10:00:00.000Z",
      created_at: "2026-01-01T10:00:00.000Z",
      displayId: "ZMX-1",
      id: "closed-june",
      priority: 3,
      status: "closed",
      title: "Closed in June",
      updated_at: "2026-06-01T10:00:00.000Z",
    },
    {
      boardStatus: "done",
      closed_at: "2026-08-05T10:00:00.000Z",
      created_at: "2026-07-01T10:00:00.000Z",
      displayId: "ZMX-2",
      id: "closed-august",
      priority: 2,
      status: "closed",
      title: "Closed in August",
      updated_at: "2026-08-05T10:00:00.000Z",
    },
    {
      boardStatus: "done",
      created_at: "2026-02-01T10:00:00.000Z",
      displayId: "ZMX-3",
      id: "closed-without-timestamp",
      priority: 0,
      status: "closed",
      title: "Closed before bd recorded closed_at",
      updated_at: "2026-07-04T10:00:00.000Z",
    },
  ];

  test("offers both directions for every sort key", () => {
    expect(BOARD_SORT_OPTIONS.map((option) => option.value)).toEqual([
      "default",
      "updated-desc",
      "updated-asc",
      "created-desc",
      "created-asc",
      "priority-asc",
      "priority-desc",
    ]);
    expect(BOARD_SORT_OPTIONS.map((option) => option.label)).toEqual([
      "Default order",
      "Last updated (newest first)",
      "Last updated (oldest first)",
      "Created (newest first)",
      "Created (oldest first)",
      "Priority (urgent first)",
      "Priority (low first)",
    ]);
  });

  test("puts the newest closed beads first in Done without a selected sort", () => {
    expect(sortBoardTickets(doneTickets, "default", "done").map((ticket) => ticket.id)).toEqual([
      "closed-august",
      "closed-without-timestamp",
      "closed-june",
    ]);
  });

  test("keeps the newest closed beads visible under the lane limit", () => {
    /*
     * CDXC:ProjectBoardSort 2026-08-07:
     * Lanes slice their ticket list before rendering, so Done ordering only helps if it happens
     * ahead of that cap.
     */
    expect(
      sortBoardTickets(doneTickets, "default", "done")
        .slice(0, 1)
        .map((ticket) => ticket.id),
    ).toEqual(["closed-august"]);
  });

  test("leaves other lanes in Beads order without a selected sort", () => {
    const todoTickets: BoardTicket[] = [
      {
        boardStatus: "todo",
        created_at: "2026-01-01T10:00:00.000Z",
        displayId: "ZMX-4",
        id: "older",
        priority: 2,
        status: "open",
        title: "Older",
        updated_at: "2026-01-02T10:00:00.000Z",
      },
      {
        boardStatus: "todo",
        created_at: "2026-05-01T10:00:00.000Z",
        displayId: "ZMX-5",
        id: "newer",
        priority: 2,
        status: "open",
        title: "Newer",
        updated_at: "2026-05-02T10:00:00.000Z",
      },
    ];

    expect(sortBoardTickets(todoTickets, "default", "todo")).toBe(todoTickets);
  });

  test("applies a selected sort to every lane in both directions", () => {
    expect(sortBoardTickets(doneTickets, "updated-desc", "done").map((ticket) => ticket.id)).toEqual([
      "closed-august",
      "closed-without-timestamp",
      "closed-june",
    ]);
    expect(sortBoardTickets(doneTickets, "updated-asc", "done").map((ticket) => ticket.id)).toEqual([
      "closed-june",
      "closed-without-timestamp",
      "closed-august",
    ]);
    expect(sortBoardTickets(doneTickets, "created-desc", "backlog").map((ticket) => ticket.id)).toEqual([
      "closed-august",
      "closed-without-timestamp",
      "closed-june",
    ]);
    expect(sortBoardTickets(doneTickets, "created-asc", "backlog").map((ticket) => ticket.id)).toEqual([
      "closed-june",
      "closed-without-timestamp",
      "closed-august",
    ]);
  });

  test("reverses the priority view, tie-break included, and keeps legacy P4 in the Low tier", () => {
    const tickets: BoardTicket[] = [
      {
        boardStatus: "todo",
        displayId: "ZMX-6",
        id: "legacy-low",
        priority: 4,
        status: "open",
        title: "Legacy low",
        updated_at: "2026-05-02T10:00:00.000Z",
      },
      {
        boardStatus: "todo",
        displayId: "ZMX-7",
        id: "low",
        priority: 3,
        status: "open",
        title: "Low",
        updated_at: "2026-05-01T10:00:00.000Z",
      },
      {
        boardStatus: "todo",
        displayId: "ZMX-8",
        id: "high",
        priority: 1,
        status: "open",
        title: "High",
        updated_at: "2026-04-01T10:00:00.000Z",
      },
    ];

    expect(sortBoardTickets(tickets, "priority-asc", "todo").map((ticket) => ticket.id)).toEqual([
      "high",
      "low",
      "legacy-low",
    ]);
    expect(sortBoardTickets(tickets, "priority-desc", "todo").map((ticket) => ticket.id)).toEqual([
      "legacy-low",
      "low",
      "high",
    ]);
  });

  test("sinks beads with no usable timestamps to the bottom in both directions", () => {
    const tickets: BoardTicket[] = [
      {
        boardStatus: "done",
        displayId: "ZMX-9",
        id: "undated-first",
        priority: 2,
        status: "closed",
        title: "Undated",
      },
      {
        boardStatus: "done",
        closed_at: "not-a-date",
        displayId: "ZMX-10",
        id: "undated-second",
        priority: 2,
        status: "closed",
        title: "Unparseable",
      },
      {
        boardStatus: "done",
        closed_at: "2026-03-01T10:00:00.000Z",
        displayId: "ZMX-11",
        id: "dated",
        priority: 2,
        status: "closed",
        title: "Dated",
        updated_at: "2026-03-01T10:00:00.000Z",
      },
    ];

    expect(sortBoardTickets(tickets, "default", "done").map((ticket) => ticket.id)).toEqual([
      "dated",
      "undated-first",
      "undated-second",
    ]);
    expect(sortBoardTickets(tickets, "updated-asc", "done").map((ticket) => ticket.id)).toEqual([
      "dated",
      "undated-first",
      "undated-second",
    ]);
  });
});

describe("buildAgentWorkPrompt", () => {
  const ticket: BoardTicket = {
    boardStatus: "todo",
    displayId: "ZMU-41",
    id: "zmux-zkr",
    priority: 2,
    status: "open",
    title: "Generating title...",
    description: "Document bead progress in comments after each agent turn.",
  };

  test("includes bead comment guidance and status workflow commands", () => {
    const prompt = buildAgentWorkPrompt(ticket);

    expect(prompt).toContain("Work on bead zmux-zkr (ZMU-41): Generating title...");
    expect(prompt).toContain("Document bead progress in comments after each agent turn.");
    expect(prompt).toContain('bd comment zmux-zkr "<summary>"');
    expect(prompt).toContain("user-facing requirements");
    expect(prompt).toContain("Do not list specific files or line numbers.");
    expect(prompt).toContain("Agent: <agent name>");
    expect(prompt).toContain("Session: <saved agent CLI session id>");
    expect(prompt).toContain("bd update zmux-zkr --status backlog");
    expect(prompt).toContain("bd update zmux-zkr --status in_progress");
    expect(prompt).toContain("bd update zmux-zkr --status test");
    expect(prompt).toContain("bd update zmux-zkr --status review");
    expect(prompt).toContain("bd close zmux-zkr");
  });
});

describe("project board comment metadata", () => {
  test("formats agent and session attribution as a bd-compatible footer", () => {
    expect(
      formatProjectBoardCommentText("Delivered the nicer ticket comments.", {
        agentName: "Cursor CLI",
        sessionId: "019e95a9-58aa-7850-ab86-5c109fe456fc",
      }),
    ).toBe(
      "Delivered the nicer ticket comments.\n\n---\nAgent: Cursor CLI\nSession: 019e95a9-58aa-7850-ab86-5c109fe456fc",
    );
  });

  test("parses agent and session metadata without showing footer lines in the body", () => {
    expect(
      parseProjectBoardCommentText(
        "Implemented the comment polish.\n\n---\nAgent: Codex\nSession: 019e95a9-58aa-7850-ab86-5c109fe456fc",
      ),
    ).toEqual({
      agentName: "Codex",
      body: "Implemented the comment polish.",
      sessionId: "019e95a9-58aa-7850-ab86-5c109fe456fc",
    });
  });

  test("leaves legacy comments unchanged", () => {
    expect(parseProjectBoardCommentText("Legacy note without metadata.")).toEqual({
      body: "Legacy note without metadata.",
    });
  });
});

describe("project board statuses", () => {
  const builtinColumns = buildBoardColumns("");

  test("places Backlog before Todo and persists it as a Beads custom status", () => {
    expect(builtinColumns.map((column) => column.key)).toEqual([
      "backlog",
      "todo",
      "in_progress",
      "test",
      "review",
      "done",
    ]);
    expect(beadsStatusToBoardStatus("backlog", builtinColumns)).toBe("backlog");
    expect(boardStatusBeadsValue("backlog", builtinColumns)).toBe("backlog");
  });

  test("adds the board's own extra statuses as columns after the built-in lanes", () => {
    const columns = buildBoardColumns("backlog,review,test,needs_input:wip");

    expect(columns.map((column) => column.key)).toEqual([
      "backlog",
      "todo",
      "in_progress",
      "test",
      "review",
      "done",
      "needs_input",
    ]);
    expect(columns[columns.length - 1]).toEqual({
      key: "needs_input",
      label: "Needs Input",
      beadsStatus: "needs_input",
      tone: "muted",
    });
  });

  test("shows a bead parked in an extra status in that status's own lane", () => {
    const columns = buildBoardColumns("backlog,review,test,needs_input:wip");

    expect(beadsStatusToBoardStatus("needs_input", columns)).toBe("needs_input");
    expect(boardStatusLabel("needs_input", columns)).toBe("Needs Input");
    expect(boardStatusBeadsValue("needs_input", columns)).toBe("needs_input");
  });

  test("keeps a bead whose status has no lane visible in Todo", () => {
    expect(beadsStatusToBoardStatus("needs_input", builtinColumns)).toBe("todo");
    expect(boardStatusLabel("needs_input", builtinColumns)).toBe("Todo");
    expect(boardStatusBeadsValue("needs_input", builtinColumns)).toBe("open");
  });
});

describe("project board view preferences", () => {
  test("stores toolbar selections under one app-wide key", () => {
    /*
     * CDXC:ProjectBoardViewPreferences 2026-08-07:
     * Priority, estimate, and sort describe how the user reads a board rather than anything about
     * a project, so the key carries no project id and every board restores the same selections.
     */
    expect(PROJECT_BOARD_VIEW_PREFERENCES_STORAGE_KEY).toBe("ghostex-project-board-view");
  });

  test("restores stored priority, estimate, sort, and tag selections", () => {
    expect(
      normalizeProjectBoardViewPreferences({
        estimateFilter: "M",
        priorityFilter: "1",
        sortOption: "created-asc",
        tagFilter: "docs",
      }),
    ).toEqual({
      estimateFilter: "M",
      priorityFilter: "1",
      sortOption: "created-asc",
      tagFilter: "docs",
    });
    expect(normalizeProjectBoardViewPreferences({ estimateFilter: "none" }).estimateFilter).toBe(
      "none",
    );
  });

  test("falls back to defaults for missing or unusable stored values", () => {
    /*
     * CDXC:ProjectBoardViewPreferences 2026-08-07:
     * Stored preferences outlive the option lists that produced them, and localStorage is editable
     * from outside the board, so anything that is not a current option must land on its default
     * instead of leaving the toolbar on a value the board cannot filter or sort by.
     */
    expect(normalizeProjectBoardViewPreferences(null)).toEqual(DEFAULT_PROJECT_BOARD_VIEW_PREFERENCES);
    expect(normalizeProjectBoardViewPreferences("all")).toEqual(DEFAULT_PROJECT_BOARD_VIEW_PREFERENCES);
    expect(normalizeProjectBoardViewPreferences({})).toEqual(DEFAULT_PROJECT_BOARD_VIEW_PREFERENCES);
    expect(
      normalizeProjectBoardViewPreferences({
        estimateFilter: "XXL",
        priorityFilter: 1,
        sortOption: "closed-desc",
        tagFilter: 7,
      }),
    ).toEqual(DEFAULT_PROJECT_BOARD_VIEW_PREFERENCES);
  });

  test("keeps a stored tag the current board may no longer offer", () => {
    /*
     * CDXC:ProjectBoardTagFilter 2026-08-21:
     * Tag options are the labels the loaded tickets carry, so nothing at storage-read time can say
     * whether a tag is still real. Normalisation therefore rejects only values that could never be
     * a tag and leaves a stale-looking one intact, so a board that still has it restores the
     * selection instead of the first tagless project erasing it. resolveBoardTagFilter is what
     * lands an unavailable tag on "all" once a board has actually loaded.
     */
    expect(normalizeProjectBoardViewPreferences({ tagFilter: "docs" }).tagFilter).toBe("docs");
    expect(normalizeProjectBoardViewPreferences({ tagFilter: "retired-lane-tag" }).tagFilter).toBe(
      "retired-lane-tag",
    );
    expect(normalizeProjectBoardViewPreferences({ tagFilter: "   " }).tagFilter).toBe("all");
    expect(normalizeProjectBoardViewPreferences({ tagFilter: ["docs"] }).tagFilter).toBe("all");
  });

  test("keeps every offered toolbar option restorable", () => {
    for (const option of BOARD_SORT_OPTIONS) {
      expect(normalizeProjectBoardViewPreferences({ sortOption: option.value }).sortOption).toBe(
        option.value,
      );
    }
  });
});

describe("project board routing", () => {
  test("normalizes old editor ids to raw project ids", () => {
    /*
     * CDXC:ProjectBoardRouting 2026-06-04-23:51:
     * Open Project panes from older builds can keep `project-editor:<projectId>:tasks` in the URL. The board must strip the editor wrapper before calling gxserver so stale paths do not decide the Beads project.
     */
    expect(projectBoardRawProjectIdFromUrlParam("project-editor:P3lv0:tasks")).toBe("P3lv0");
    expect(projectBoardRawProjectIdFromUrlParam("project-editor:remote%3Amachine%3AP9:tasks")).toBe(
      "remote:machine:P9",
    );
    expect(projectBoardRawProjectIdFromUrlParam("P3lv0")).toBe("P3lv0");
  });
});

describe("project board issue prefix", () => {
  test("updates Beads issue_prefix when it differs from the project prefix", async () => {
    /*
     * CDXC:ProjectBoardBeads 2026-06-10-20:27:
     * The Project board's visible ticket key is separate from Beads' durable issue_prefix, so prefix reconciliation must write the real project prefix before ticket creation when stale config still says gxserver.
     */
    const requests: Array<{ action: string; value?: string }> = [];
    await ensureIssuePrefix(async (request) => {
      requests.push({ action: request.action, value: request.value });
      return request.action === "configGetIssuePrefix" ? { value: "gxserver" } : {};
    }, "zmux");

    expect(requests).toEqual([
      { action: "configGetIssuePrefix", value: undefined },
      { action: "renamePrefix", value: "zmux-" },
    ]);
  });

  test("does not rewrite already matching normalized issue_prefix values", async () => {
    const requests: Array<{ action: string; value?: string }> = [];
    await ensureIssuePrefix(async (request) => {
      requests.push({ action: request.action, value: request.value });
      return { value: "zmux" };
    }, "ZMUX");

    expect(requests).toEqual([{ action: "configGetIssuePrefix", value: undefined }]);
  });

  test("accepts bare Beads config string payloads for issue_prefix", async () => {
    const requests: Array<{ action: string; value?: string }> = [];
    await ensureIssuePrefix(async (request) => {
      requests.push({ action: request.action, value: request.value });
      return "zmux";
    }, "zmux");

    expect(requests).toEqual([{ action: "configGetIssuePrefix", value: undefined }]);
  });

  test("keeps an established issue_prefix that differs from the project prefix", async () => {
    /*
     * CDXC:ProjectBoardBeads 2026-07-31:
     * A shared beadsDirectory serves several projects; an established prefix is durable data, not
     * stale bootstrap config, so focusing a differently named project must not rename the board.
     */
    const requests: Array<{ action: string; value?: string }> = [];
    await ensureIssuePrefix(async (request) => {
      requests.push({ action: request.action, value: request.value });
      return { value: "agent-bo" };
    }, "email-cleaner");

    expect(requests).toEqual([{ action: "configGetIssuePrefix", value: undefined }]);
  });

  test("adopts the project prefix when issue_prefix is unset", async () => {
    const requests: Array<{ action: string; value?: string }> = [];
    await ensureIssuePrefix(async (request) => {
      requests.push({ action: request.action, value: request.value });
      return request.action === "configGetIssuePrefix" ? { value: "" } : {};
    }, "email-cleaner");

    expect(requests).toEqual([
      { action: "configGetIssuePrefix", value: undefined },
      { action: "renamePrefix", value: "email-cl-" },
    ]);
  });
});
describe("project board assigned agent resolution", () => {
  /*
   * CDXC:ProjectBoardStartWork 2026-08-07-07:01:
   * Start work should open with the agent the bead is assigned to, matched by the
   * configured agent label or agent id, and fall back to the board default when
   * the assignee is empty or names someone who is not a configured agent.
   */
  const agents = [
    { agentId: "claude", label: "Claude Code" },
    { agentId: "custom-mf1k2j-9ab3de", label: "Dobby" },
  ];

  test("matches a custom agent by its configured name", () => {
    expect(resolveAssignedAgentId("dobby", agents)).toBe("custom-mf1k2j-9ab3de");
  });

  test("matches a built-in agent by its agent id", () => {
    expect(resolveAssignedAgentId("CLAUDE", agents)).toBe("claude");
  });

  test("ignores surrounding whitespace on the assignee", () => {
    expect(resolveAssignedAgentId("  Dobby  ", agents)).toBe("custom-mf1k2j-9ab3de");
  });

  test("keeps the existing default for empty or unknown assignees", () => {
    expect(resolveAssignedAgentId(undefined, agents)).toBeUndefined();
    expect(resolveAssignedAgentId("   ", agents)).toBeUndefined();
    expect(resolveAssignedAgentId("madda", agents)).toBeUndefined();
  });

  test("matches a tool-suffixed assignee against the bare agent id", () => {
    expect(resolveAssignedAgentId("claude-code", agents)).toBe("claude");
    expect(resolveAssignedAgentId("Gemini-CLI", [{ agentId: "gemini", label: "Gemini" }])).toBe(
      "gemini",
    );
  });

  test("prefers an exactly named agent over the tool-suffix fallback", () => {
    expect(
      resolveAssignedAgentId("claude-code", [
        { agentId: "claude", label: "Claude Code" },
        { agentId: "custom-tsr7hq-4c1f80", label: "claude-code" },
      ]),
    ).toBe("custom-tsr7hq-4c1f80");
  });

  test("does not invent a match when the stripped name is not configured", () => {
    expect(resolveAssignedAgentId("harry-cli", agents)).toBeUndefined();
    expect(resolveAssignedAgentId("-cli", agents)).toBeUndefined();
    expect(resolveAssignedAgentId("-code", agents)).toBeUndefined();
  });

  test("returns the first configured agent that matches", () => {
    expect(
      resolveAssignedAgentId("codex", [
        { agentId: "codex", label: "Codex" },
        { agentId: "custom-mf1k2j-77c1aa", label: "codex" },
      ]),
    ).toBe("codex");
  });
});
