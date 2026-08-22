import { describe, expect, test } from "vitest";
import { PROJECT_SESSION_LIST_COLLAPSED_COUNT } from "./project-session-list-toggle";
import {
  createRenderedSidebarSessionSlotIds,
  createRenderedSidebarSessionSlots,
  createVisibleSidebarSessionSlotIds,
  resolveAdjacentRenderedSidebarSessionSlotId,
  resolveRenderedSidebarSessionAdditiveSelection,
  resolveRenderedSidebarSessionRangeSelection,
  resolveVisibleSidebarSessionSlotId,
  type RenderedSidebarSessionSlotElement,
} from "./sidebar-visible-session-slots";

function renderedSlotElement({
  dataVisible = true,
  hidden = false,
  sleeping = false,
  sessionId,
}: {
  dataVisible?: boolean;
  hidden?: boolean;
  sleeping?: boolean;
  sessionId: string;
}): RenderedSidebarSessionSlotElement {
  return {
    closest: () => (hidden ? ({} as Element) : null),
    getAttribute: (name) => {
      if (name === "data-sidebar-session-id") {
        return sessionId;
      }
      if (name === "data-sleeping") {
        return String(sleeping);
      }
      if (name === "data-visible") {
        return dataVisible ? "true" : "false";
      }
      return null;
    },
  };
}

describe("createVisibleSidebarSessionSlotIds", () => {
  test("flattens sessions in the same order as visible sidebar rows", () => {
    const longProjectSessions = Array.from(
      { length: PROJECT_SESSION_LIST_COLLAPSED_COUNT + 2 },
      (_, index) => `project-session-${index + 1}`,
    );

    expect(
      createVisibleSidebarSessionSlotIds({
        collapsedGroupsById: {
          "collapsed-project": true,
        },
        displayedWorkspaceGroupIds: [
          "quick",
          "project",
          "collapsed-project",
          "remote-project",
        ],
        displayedWorkspaceSessionIdsByGroup: {
          "collapsed-project": ["hidden-session"],
          project: longProjectSessions,
          quick: ["quick-session"],
          "remote-project": ["remote-session"],
        },
        enableProjectSessionListToggle: true,
        groupsById: {
          "collapsed-project": { projectContext: { editor: { projectId: "collapsed-project-id" } } },
          project: { projectContext: { editor: { projectId: "project-id" } } },
          quick: { isChatCollection: true },
          "remote-project": {
            projectContext: { editor: { projectId: "remote-project-id" } },
            remoteMachineContext: { machineId: "machine-1" },
          },
        },
        isReferenceChatsCollapsed: false,
        isReferenceProjectsCollapsed: false,
        projectSessionListCollapsedCount: PROJECT_SESSION_LIST_COLLAPSED_COUNT,
        projectSessionListCollapsedState: {
          "project-id": true,
        },
        remoteMachineIds: ["machine-1"],
      }),
    ).toEqual([
      "quick-session",
      ...longProjectSessions.slice(0, PROJECT_SESSION_LIST_COLLAPSED_COUNT),
      "remote-session",
    ]);
  });
});

describe("createRenderedSidebarSessionSlotIds", () => {
  test("keeps DOM row order while skipping hidden collapsed rows", () => {
    expect(
      createRenderedSidebarSessionSlotIds([
        renderedSlotElement({ sessionId: "visible-session-1" }),
        renderedSlotElement({ hidden: true, sessionId: "collapsed-session" }),
        renderedSlotElement({ dataVisible: false, sessionId: "filtered-session" }),
        renderedSlotElement({ sessionId: "visible-session-2" }),
      ]),
    ).toEqual(["visible-session-1", "visible-session-2"]);
  });

  test("keeps pane-hidden rendered rows for selection readers", () => {
    /*
     * CDXC:SidebarMultiSelect 2026-07-02-08:12:
     * data-visible mirrors surfaced workspace panes. Shift/cmd selection reads
     * every rendered row, so pane-hidden rows must stay in rendered order while
     * collapsed rows remain excluded.
     */
    expect(
      createRenderedSidebarSessionSlotIds(
        [
          renderedSlotElement({ sessionId: "visible-session-1" }),
          renderedSlotElement({ hidden: true, sessionId: "collapsed-session" }),
          renderedSlotElement({ dataVisible: false, sessionId: "pane-hidden-session" }),
          renderedSlotElement({ sessionId: "visible-session-2" }),
        ],
        { skipPaneHiddenRows: false },
      ),
    ).toEqual(["visible-session-1", "pane-hidden-session", "visible-session-2"]);
  });
});

describe("createRenderedSidebarSessionSlots", () => {
  test("reads sleeping state from rendered session rows", () => {
    expect(
      createRenderedSidebarSessionSlots([
        renderedSlotElement({ sessionId: "awake-session" }),
        renderedSlotElement({ sessionId: "sleeping-session", sleeping: true }),
      ]),
    ).toEqual([
      { isSleeping: false, sessionId: "awake-session" },
      { isSleeping: true, sessionId: "sleeping-session" },
    ]);
  });
});

describe("resolveAdjacentRenderedSidebarSessionSlotId", () => {
  test("walks rendered order while skipping sleeping sessions", () => {
    const slots = [
      { isSleeping: false, sessionId: "session-1" },
      { isSleeping: true, sessionId: "sleeping-session-2" },
      { isSleeping: false, sessionId: "session-3" },
      { isSleeping: true, sessionId: "sleeping-session-4" },
    ];

    expect(
      resolveAdjacentRenderedSidebarSessionSlotId({
        direction: 1,
        focusedSessionId: "session-1",
        slots,
      }),
    ).toBe("session-3");
    expect(
      resolveAdjacentRenderedSidebarSessionSlotId({
        direction: -1,
        focusedSessionId: "session-3",
        slots,
      }),
    ).toBe("session-1");
    expect(
      resolveAdjacentRenderedSidebarSessionSlotId({
        direction: 1,
        focusedSessionId: "sleeping-session-4",
        slots,
      }),
    ).toBe("session-1");
  });
});

describe("resolveRenderedSidebarSessionRangeSelection", () => {
  test("selects the inclusive rendered range between active and clicked sessions", () => {
    /*
     * CDXC:SidebarMultiSelect 2026-07-01-18:33:
     * Shift-click range selection must follow rendered sidebar order, not raw
     * group inventory order, because collapsed projects and filters can hide rows.
     */
    expect(
      resolveRenderedSidebarSessionRangeSelection({
        activeSessionId: "session-2",
        clickedSessionId: "session-5",
        visibleSessionIds: ["session-1", "session-2", "session-3", "session-4", "session-5"],
      }),
    ).toEqual(["session-2", "session-3", "session-4", "session-5"]);
  });

  test("falls back to the clicked row when the active session is not rendered", () => {
    expect(
      resolveRenderedSidebarSessionRangeSelection({
        activeSessionId: "collapsed-session",
        clickedSessionId: "session-3",
        visibleSessionIds: ["session-1", "session-2", "session-3"],
      }),
    ).toEqual(["session-3"]);
  });
});

describe("resolveRenderedSidebarSessionAdditiveSelection", () => {
  test("selects only the clicked session when starting a cmd-click selection", () => {
    /*
     * CDXC:SidebarMultiSelect 2026-07-02-08:25:
     * Cmd-click must not pull the currently active session into a fresh
     * selection; the active row is selected only when it is the clicked row.
     */
    expect(
      resolveRenderedSidebarSessionAdditiveSelection({
        clickedSessionId: "session-4",
        currentSelection: [],
        visibleSessionIds: ["session-1", "session-2", "session-3", "session-4"],
      }),
    ).toEqual(["session-4"]);
  });

  test("adds the clicked row while dropping stale hidden selections", () => {
    expect(
      resolveRenderedSidebarSessionAdditiveSelection({
        clickedSessionId: "session-4",
        currentSelection: ["session-2", "hidden-session"],
        visibleSessionIds: ["session-1", "session-2", "session-3", "session-4"],
      }),
    ).toEqual(["session-2", "session-4"]);
  });
});

describe("resolveVisibleSidebarSessionSlotId", () => {
  test("uses one-based number slots and wraps previous/next over visible sessions", () => {
    const visibleSessionIds = ["session-1", "session-2", "session-3"];

    expect(resolveVisibleSidebarSessionSlotId({ slotNumber: 2, visibleSessionIds })).toBe(
      "session-2",
    );
    expect(
      resolveVisibleSidebarSessionSlotId({
        focusedSessionId: "session-3",
        slotNumber: 0,
        visibleSessionIds,
      }),
    ).toBe("session-1");
    expect(
      resolveVisibleSidebarSessionSlotId({
        focusedSessionId: "session-1",
        slotNumber: -1,
        visibleSessionIds,
      }),
    ).toBe("session-3");
    expect(resolveVisibleSidebarSessionSlotId({ slotNumber: 0, visibleSessionIds })).toBe(
      "session-1",
    );
  });
});
