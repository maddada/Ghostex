import { describe, expect, test } from "vitest";
import {
  appendImageMarkdownToDescription,
  BOARD_COLUMNS,
  beadsStatusToBoardStatus,
  boardStatusBeadsValue,
  buildAgentWorkPrompt,
  extractDescriptionImagePreviews,
  extractDescriptionImageReferences,
  ensureIssuePrefix,
  filterBoardTickets,
  formatProjectBoardCommentText,
  parseProjectBoardCommentText,
  priorityLabel,
  prioritySelectValue,
  projectBoardRawProjectIdFromUrlParam,
  removeDescriptionImageReference,
  resolveAssignedAgentId,
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

describe("project board filters", () => {
  const tickets: BoardTicket[] = [
    {
      boardStatus: "todo",
      displayId: "ZMX-1",
      estimate: 15,
      id: "urgent-xs",
      priority: 0,
      status: "open",
      title: "Urgent XS task",
    },
    {
      boardStatus: "in_progress",
      displayId: "ZMX-2",
      estimate: null,
      id: "medium-none",
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
    expect(filterBoardTickets(tickets, "", "3", "all").map((ticket) => ticket.id)).toEqual([
      "legacy-low",
    ]);
    expect(filterBoardTickets(tickets, "", "all", "none").map((ticket) => ticket.id)).toEqual([
      "medium-none",
    ]);
    expect(filterBoardTickets(tickets, "", "0", "XS").map((ticket) => ticket.id)).toEqual([
      "urgent-xs",
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
    expect(prompt).toContain('gx bd comment zmux-zkr "<summary>"');
    expect(prompt).toContain("user-facing requirements");
    expect(prompt).toContain("Do not list specific files or line numbers.");
    expect(prompt).toContain("Agent: <agent name>");
    expect(prompt).toContain("Session: <saved agent CLI session id>");
    expect(prompt).toContain("gx bd update zmux-zkr --status backlog");
    expect(prompt).toContain("gx bd update zmux-zkr --status in_progress");
    expect(prompt).toContain("gx bd update zmux-zkr --status test");
    expect(prompt).toContain("gx bd update zmux-zkr --status review");
    expect(prompt).toContain("gx bd close zmux-zkr");
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
  test("places Backlog before Todo and persists it as a Beads custom status", () => {
    expect(BOARD_COLUMNS.map((column) => column.key)).toEqual([
      "backlog",
      "todo",
      "in_progress",
      "test",
      "review",
      "done",
    ]);
    expect(beadsStatusToBoardStatus("backlog")).toBe("backlog");
    expect(boardStatusBeadsValue("backlog")).toBe("backlog");
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

  test("returns the first configured agent that matches", () => {
    expect(
      resolveAssignedAgentId("codex", [
        { agentId: "codex", label: "Codex" },
        { agentId: "custom-mf1k2j-77c1aa", label: "codex" },
      ]),
    ).toBe("codex");
  });
});

