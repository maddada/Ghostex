import { describe, expect, test } from "vite-plus/test";
import { createStoryPreviousSession } from "./sidebar-story-fixture-helpers";
import {
  createSidebarSessionSearchResults,
  getNextSidebarSessionSearchSelection,
} from "./sidebar-session-search";

describe("createSidebarSessionSearchResults", () => {
  test("should flatten displayed current and previous sessions in render order", () => {
    expect(
      createSidebarSessionSearchResults({
        displayedWorkspaceGroupIds: ["workspace-group-1", "workspace-group-2"],
        displayedWorkspaceSessionIdsByGroup: {
          "workspace-group-1": ["session-1", "session-2"],
          "workspace-group-2": ["session-3"],
        },
        filteredPreviousSessions: [
          createStoryPreviousSession({
            alias: "archived 1",
            historyId: "history-1",
            sessionId: "archived-session-1",
            shortcutLabel: "⌘⌥7",
          }),
          createStoryPreviousSession({
            alias: "archived 2",
            historyId: "history-2",
            sessionId: "archived-session-2",
            shortcutLabel: "⌘⌥8",
          }),
        ],
      }),
    ).toEqual([
      {
        groupId: "workspace-group-1",
        kind: "session",
        sessionId: "session-1",
      },
      {
        groupId: "workspace-group-1",
        kind: "session",
        sessionId: "session-2",
      },
      {
        groupId: "workspace-group-2",
        kind: "session",
        sessionId: "session-3",
      },
      {
        historyId: "history-1",
        kind: "previous",
      },
      {
        historyId: "history-2",
        kind: "previous",
      },
    ]);
  });
});

describe("getNextSidebarSessionSearchSelection", () => {
  const results = [
    {
      groupId: "group-1",
      kind: "session" as const,
      sessionId: "session-1",
    },
    {
      groupId: "group-1",
      kind: "session" as const,
      sessionId: "session-2",
    },
    {
      historyId: "history-1",
      kind: "previous" as const,
    },
  ];

  test("should select the first result when moving down without a current selection", () => {
    expect(
      getNextSidebarSessionSearchSelection({
        direction: 1,
        results,
      }),
    ).toEqual({
      kind: "session",
      sessionId: "session-1",
    });
  });

  test("should select the last result when moving up without a current selection", () => {
    expect(
      getNextSidebarSessionSearchSelection({
        direction: -1,
        results,
      }),
    ).toEqual({
      historyId: "history-1",
      kind: "previous",
    });
  });

  test("should advance to the next result and wrap to the first one", () => {
    expect(
      getNextSidebarSessionSearchSelection({
        currentSelection: {
          kind: "session",
          sessionId: "session-2",
        },
        direction: 1,
        results,
      }),
    ).toEqual({
      historyId: "history-1",
      kind: "previous",
    });

    expect(
      getNextSidebarSessionSearchSelection({
        currentSelection: {
          historyId: "history-1",
          kind: "previous",
        },
        direction: 1,
        results,
      }),
    ).toEqual({
      kind: "session",
      sessionId: "session-1",
    });
  });

  test("should move to the previous result and wrap to the last one", () => {
    expect(
      getNextSidebarSessionSearchSelection({
        currentSelection: {
          kind: "session",
          sessionId: "session-1",
        },
        direction: -1,
        results,
      }),
    ).toEqual({
      historyId: "history-1",
      kind: "previous",
    });
  });
});
