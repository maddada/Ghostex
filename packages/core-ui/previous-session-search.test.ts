import { describe, expect, test } from "vite-plus/test";
import type { SidebarPreviousSessionItem } from "../shared/session-grid-contract";
import {
  filterPreviousSessions,
  filterSidebarSessionItems,
  filterPreviousSessionsModalItems,
  getNextPreviousSessionsModalSelection,
  groupPreviousSessionsByDay,
  removePreviousSessionByHistoryId,
} from "./previous-session-search";

describe("filterPreviousSessions", () => {
  test("should fuzzy match aliases and secondary session text", () => {
    const previousSessions = [
      createPreviousSession({
        alias: "Adding prev sessions",
        detail: "Codex CLI",
        historyId: "history-1",
      }),
      createPreviousSession({
        alias: "Publish release prep",
        detail: "Claude Code",
        historyId: "history-2",
      }),
    ];

    expect(filterPreviousSessions(previousSessions, "ad pvs")).toMatchObject([
      { historyId: "history-1" },
    ]);
    expect(filterPreviousSessions(previousSessions, "cld")).toMatchObject([
      { historyId: "history-2" },
    ]);
  });

  test("should match the same session words across spaces, hyphens, and camel case", () => {
    const previousSessions = [
      createPreviousSession({
        alias: "My Session Title",
        historyId: "history-1",
      }),
      createPreviousSession({
        alias: "my-session-title",
        historyId: "history-2",
      }),
      createPreviousSession({
        alias: "MySessionTitle",
        historyId: "history-3",
      }),
    ];

    expect(filterPreviousSessions(previousSessions, "my session title")).toMatchObject([
      { historyId: "history-1" },
      { historyId: "history-2" },
      { historyId: "history-3" },
    ]);
    expect(filterPreviousSessions(previousSessions, "my-session-title")).toMatchObject([
      { historyId: "history-1" },
      { historyId: "history-2" },
      { historyId: "history-3" },
    ]);
  });

  test("should keep long session search terms tighter than scattered-letter fuzzy matches", () => {
    const sessions = [
      createPreviousSession({
        alias: "Sidebar Divider Shift",
        historyId: "history-sidebar",
        sessionId: "session-sidebar",
      }),
      createPreviousSession({
        alias: "Side Bar Resize",
        historyId: "history-side-bar",
        sessionId: "session-side-bar",
      }),
      createPreviousSession({
        alias: "Status Bar Cleanup",
        historyId: "history-status-bar",
        sessionId: "session-status-bar",
      }),
      createPreviousSession({
        alias: "Session Border Cleanup",
        historyId: "history-session-border",
        sessionId: "session-session-border",
      }),
    ];

    /*
     * CDXC:SidebarSearch 2026-06-28-06:29:
     * Typing a concrete long term like "sidebar" should show exact and joined
     * word matches in live projects and Previous Sessions, but it should not
     * pull in rows whose only relationship is scattered letters such as
     * "Status Bar".
     */
    expect(filterSidebarSessionItems(sessions, "sidebar").map((session) => session.sessionId)).toEqual([
      "session-sidebar",
      "session-side-bar",
    ]);
    expect(filterSidebarSessionItems(sessions, "sidebr").map((session) => session.sessionId)).toEqual([
      "session-sidebar",
      "session-side-bar",
    ]);
    expect(filterSidebarSessionItems(sessions, "sbar")).toEqual([]);
  });

  test("should exclude default agent session names from searched sessions", () => {
    const sessions = [
      createPreviousSession({
        alias: "Pi Agent Session",
        historyId: "history-default-pi",
        sessionId: "session-default-pi",
      }),
      createPreviousSession({
        alias: "Codex Agent Session",
        historyId: "history-default-codex",
        sessionId: "session-default-codex",
      }),
      createPreviousSession({
        alias: "Review Pi agent behavior",
        historyId: "history-real-title",
        sessionId: "session-real-title",
      }),
    ];

    /*
     * CDXC:SessionSearch 2026-06-18-00:01:
     * Default agent CLI names are creation placeholders, so active sidebar
     * search and previous-session search must omit them while preserving real
     * user/agent titles that mention the same agent.
     */
    expect(filterSidebarSessionItems(sessions, "agent").map((session) => session.sessionId)).toEqual([
      "session-real-title",
    ]);
    expect(filterPreviousSessions(sessions, "agent").map((session) => session.historyId)).toEqual([
      "history-real-title",
    ]);
  });

  test("should optionally restrict results to selected session tags before searching", () => {
    const previousSessions = [
      createPreviousSession({
        alias: "Favorite release prep",
        historyId: "history-1",
        isFavorite: true,
      }),
      createPreviousSession({
        alias: "Todo release prep",
        historyId: "history-2",
        sessionTag: "todo",
      }),
      createPreviousSession({
        alias: "Normal release prep",
        historyId: "history-3",
        isFavorite: false,
      }),
    ];

    expect(filterPreviousSessions(previousSessions, "", { sessionTags: ["favorite"] })).toMatchObject([
      { historyId: "history-1" },
    ]);
    expect(filterPreviousSessions(previousSessions, "", { sessionTags: ["favorite", "todo"] })).toMatchObject([
      { historyId: "history-1" },
      { historyId: "history-2" },
    ]);
    expect(filterPreviousSessions(previousSessions, "normal", { sessionTags: ["favorite"] })).toEqual([]);
    expect(filterPreviousSessions(previousSessions, "", { sessionTags: ["untagged"] })).toMatchObject([
      { historyId: "history-3" },
    ]);
    expect(
      filterPreviousSessions(previousSessions, "", { sessionTags: ["untagged", "todo"] }),
    ).toMatchObject([{ historyId: "history-2" }, { historyId: "history-3" }]);
  });

  test("should keep only the latest session for the same project and title", () => {
    const previousSessions = [
      createPreviousSession({
        alias: "Duplicate title",
        closedAt: "2026-03-24T10:00:00.000Z",
        historyId: "history-old",
        lastInteractionAt: "2026-03-25T12:00:00.000Z",
        projectName: "ghostex",
        projectPath: "/Users/madda/dev/_active/ghostex",
      }),
      createPreviousSession({
        alias: "Other project duplicate title",
        closedAt: "2026-03-24T11:00:00.000Z",
        historyId: "history-other-project",
        primaryTitle: "Duplicate title",
        projectName: "other",
        projectPath: "/Users/madda/dev/_active/other",
      }),
      createPreviousSession({
        alias: "Duplicate title",
        closedAt: "2026-03-24T12:00:00.000Z",
        historyId: "history-new",
        projectName: "ghostex",
        projectPath: "/Users/madda/dev/_active/ghostex",
      }),
    ];

    expect(filterPreviousSessions(previousSessions, "")).toMatchObject([
      { historyId: "history-other-project" },
      { historyId: "history-new" },
    ]);
  });
});

describe("groupPreviousSessionsByDay", () => {
  test("should order groups and rows by closed time instead of last active time", () => {
    const previousSessions = [
      createPreviousSession({
        closedAt: "2026-06-01T12:00:00.000Z",
        historyId: "history-old-activity",
        lastInteractionAt: "2026-06-03T12:00:00.000Z",
      }),
      createPreviousSession({
        closedAt: "2026-06-02T10:00:00.000Z",
        historyId: "history-newer-close",
        lastInteractionAt: "2026-06-01T10:00:00.000Z",
      }),
      createPreviousSession({
        closedAt: "2026-06-02T14:00:00.000Z",
        historyId: "history-newest-close",
        lastInteractionAt: "2026-06-01T09:00:00.000Z",
      }),
    ];

    const groups = groupPreviousSessionsByDay(previousSessions);

    expect(groups).toHaveLength(2);
    expect(groups.flatMap((group) => group.sessions.map((session) => session.historyId))).toEqual([
      "history-newest-close",
      "history-newer-close",
      "history-old-activity",
    ]);
  });
});

describe("filterPreviousSessionsModalItems", () => {
  test("should hide browser page history from the previous sessions modal", () => {
    const previousSessions = [
      createPreviousSession({
        alias: "Agent plan",
        historyId: "history-agent",
        sessionKind: "terminal",
      }),
      createPreviousSession({
        agentIcon: "browser",
        alias: "Example Domain",
        historyId: "history-browser-icon",
      }),
      createPreviousSession({
        alias: "Browser pane",
        historyId: "history-browser-kind",
        sessionKind: "browser",
      }),
      createPreviousSession({
        alias: "Stored browser pane",
        historyId: "history-browser-record",
        sessionRecord: {
          alias: "Stored browser pane",
          browser: { url: "https://example.com" },
          column: 0,
          createdAt: "2026-03-24T09:00:00.000Z",
          displayId: "B1",
          kind: "browser",
          row: 0,
          sessionId: "browser-record",
          slotIndex: 0,
          title: "Example Domain",
        },
      }),
    ];

    expect(filterPreviousSessionsModalItems(previousSessions)).toMatchObject([
      { historyId: "history-agent" },
    ]);
  });
});

describe("removePreviousSessionByHistoryId", () => {
  test("should remove the clicked row from the modal result page", () => {
    const previousSessions = [
      createPreviousSession({ historyId: "history-1" }),
      createPreviousSession({ historyId: "history-2" }),
      createPreviousSession({ historyId: "history-3" }),
    ];

    expect(removePreviousSessionByHistoryId(previousSessions, "history-2")).toMatchObject([
      { historyId: "history-1" },
      { historyId: "history-3" },
    ]);
  });
});

describe("getNextPreviousSessionsModalSelection", () => {
  test("should select the first row when arrowing down without an active row", () => {
    const previousSessions = [
      createPreviousSession({ historyId: "history-1" }),
      createPreviousSession({ historyId: "history-2" }),
    ];

    expect(
      getNextPreviousSessionsModalSelection({
        currentHistoryId: undefined,
        direction: 1,
        sessions: previousSessions,
      }),
    ).toBe("history-1");
  });

  test("should select the last row when arrowing up without an active row", () => {
    const previousSessions = [
      createPreviousSession({ historyId: "history-1" }),
      createPreviousSession({ historyId: "history-2" }),
    ];

    expect(
      getNextPreviousSessionsModalSelection({
        currentHistoryId: undefined,
        direction: -1,
        sessions: previousSessions,
      }),
    ).toBe("history-2");
  });

  test("should wrap between visible modal rows", () => {
    const previousSessions = [
      createPreviousSession({ historyId: "history-1" }),
      createPreviousSession({ historyId: "history-2" }),
      createPreviousSession({ historyId: "history-3" }),
    ];

    expect(
      getNextPreviousSessionsModalSelection({
        currentHistoryId: "history-3",
        direction: 1,
        sessions: previousSessions,
      }),
    ).toBe("history-1");
    expect(
      getNextPreviousSessionsModalSelection({
        currentHistoryId: "history-1",
        direction: -1,
        sessions: previousSessions,
      }),
    ).toBe("history-3");
  });
});

function createPreviousSession(
  overrides: Partial<SidebarPreviousSessionItem>,
): SidebarPreviousSessionItem {
  return {
    activity: "idle",
    alias: "Atlas",
    closedAt: "2026-03-24T10:00:00.000Z",
    column: 0,
    historyId: "history",
    isFocused: false,
    isGeneratedName: false,
    isRestorable: true,
    isRunning: false,
    isVisible: false,
    row: 0,
    sessionId: "session-1",
    shortcutLabel: "⌘⌥1",
    ...overrides,
  };
}
