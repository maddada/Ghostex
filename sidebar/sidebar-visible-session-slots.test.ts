import { describe, expect, test } from "vitest";
import { PROJECT_SESSION_LIST_COLLAPSED_COUNT } from "./project-session-list-toggle";
import {
  createRenderedSidebarSessionSlotIds,
  createRenderedSidebarSessionSlots,
  createVisibleSidebarSessionSlotIds,
  resolveAdjacentRenderedSidebarSessionSlotId,
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
