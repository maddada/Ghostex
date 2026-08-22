import { describe, expect, test } from "vite-plus/test";
import {
  DEFAULT_MAIN_GROUP_ID,
  createDefaultGroupedSessionWorkspaceSnapshot,
  createSessionRecord,
  type GroupedSessionWorkspaceSnapshot,
} from "./session-grid-contract";
import {
  createGroupInWorkspace,
  createGroupFromSessionInWorkspace,
  createSessionInWorkspace,
  focusGroupByIndexInWorkspace,
  focusSessionInWorkspace,
  moveSessionToGroupInWorkspace,
  normalizeGroupedSessionWorkspaceSnapshot,
  renameGroupInWorkspace,
  removeGroupInWorkspace,
  setViewModeInWorkspace,
  setVisibleCountInWorkspace,
  syncGroupOrderInWorkspace,
  syncSessionOrderInWorkspace,
  toggleFullscreenSessionInWorkspace,
} from "./grouped-session-workspace-state";

describe("normalizeGroupedSessionWorkspaceSnapshot", () => {
  test("should fall back to a clean grouped default for invalid stored state", () => {
    const snapshot = normalizeGroupedSessionWorkspaceSnapshot(undefined);

    expect(snapshot.activeGroupId).toBe(DEFAULT_MAIN_GROUP_ID);
    expect(snapshot.groups).toHaveLength(1);
    expect(snapshot.groups[0]?.title).toBe("Main");
    expect(snapshot.nextGroupNumber).toBe(2);
    expect(snapshot.nextSessionNumber).toBe(1);
  });

  test("should preserve existing two-digit session display ids", () => {
    const firstSession = createSessionRecord(1, 0, { displayId: "00" });
    const secondSession = createSessionRecord(2, 1, { displayId: "01" });

    const snapshot = normalizeGroupedSessionWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: firstSession.sessionId,
            sessions: [firstSession, secondSession],
            viewMode: "grid",
            visibleCount: 2,
            visibleSessionIds: [firstSession.sessionId, secondSession.sessionId],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 2,
      nextSessionNumber: 3,
    });

    expect(snapshot.groups[0]?.snapshot.sessions.map((session) => session.displayId)).toEqual([
      "00",
      "01",
    ]);
  });
});

describe("createSessionInWorkspace", () => {
  test("should create sessions in the active group and increment the workspace session counter", () => {
    let snapshot = createDefaultGroupedSessionWorkspaceSnapshot();

    const first = createSessionInWorkspace(snapshot);
    snapshot = first.snapshot;
    const second = createSessionInWorkspace(snapshot);

    expect(first.session?.sessionId).toBe("session-1");
    expect(second.session?.sessionId).toBe("session-2");
    expect(
      second.snapshot.groups[0]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual(["session-1", "session-2"]);
    expect(second.snapshot.nextSessionNumber).toBe(3);
  });
});

describe("createGroupFromSessionInWorkspace", () => {
  test("should move the dragged session into a newly created active group", () => {
    const result = createGroupFromSessionInWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: "session-1",
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: ["session-1", "session-2"],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionNumber: 3,
      }),
      "session-2",
    );

    expect(result.changed).toBe(true);
    expect(result.groupId).toBe("group-2");
    expect(result.snapshot.activeGroupId).toBe("group-2");
    expect(result.snapshot.groups).toHaveLength(2);
    expect(
      result.snapshot.groups[0]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual(["session-1"]);
    expect(result.snapshot.groups[1]?.title).toBe("Group 2");
    expect(
      result.snapshot.groups[1]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual(["session-2"]);
  });
});

describe("createGroupInWorkspace", () => {
  test("should append an empty active group", () => {
    const result = createGroupInWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: "session-1",
              sessions: [createSessionRecord(1, 0)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: ["session-1"],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionNumber: 2,
      }),
    );

    expect(result.changed).toBe(true);
    expect(result.groupId).toBe("group-2");
    expect(result.snapshot.activeGroupId).toBe("group-2");
    expect(result.snapshot.groups).toHaveLength(2);
    expect(result.snapshot.groups[1]).toMatchObject({
      groupId: "group-2",
      title: "Group 2",
    });
    expect(result.snapshot.groups[1]?.snapshot.sessions).toEqual([]);
    expect(result.snapshot.nextGroupNumber).toBe(3);
  });
});

describe("moveSessionToGroupInWorkspace", () => {
  test("should append the moved session to the target group and activate it", () => {
    const result = moveSessionToGroupInWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: "session-1",
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: ["session-1", "session-2"],
            },
            title: "Main",
          },
          {
            groupId: "group-2",
            snapshot: {
              focusedSessionId: undefined,
              sessions: [createSessionRecord(3, 0)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: ["session-3"],
            },
            title: "Backend",
          },
        ],
        nextGroupNumber: 3,
        nextSessionNumber: 4,
      }),
      "session-2",
      "group-2",
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.activeGroupId).toBe("group-2");
    expect(
      result.snapshot.groups[0]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual(["session-1"]);
    expect(
      result.snapshot.groups[1]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual(["session-3", "session-2"]);
    expect(result.snapshot.groups[1]?.snapshot.focusedSessionId).toBe("session-2");
  });

  test("should insert the moved session at the requested index in the target group", () => {
    const result = moveSessionToGroupInWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: "session-1",
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: ["session-1", "session-2"],
            },
            title: "Main",
          },
          {
            groupId: "group-2",
            snapshot: {
              focusedSessionId: "session-3",
              sessions: [createSessionRecord(3, 0), createSessionRecord(4, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: ["session-3", "session-4"],
            },
            title: "Backend",
          },
        ],
        nextGroupNumber: 3,
        nextSessionNumber: 5,
      }),
      "session-2",
      "group-2",
      1,
    );

    expect(result.changed).toBe(true);
    expect(
      result.snapshot.groups[0]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual(["session-1"]);
    expect(
      result.snapshot.groups[1]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual(["session-3", "session-2", "session-4"]);
    expect(result.snapshot.groups[1]?.snapshot.focusedSessionId).toBe("session-2");
  });
});

describe("syncSessionOrderInWorkspace", () => {
  test("should reorder sessions within the same group", () => {
    const sessions = Array.from({ length: 4 }, (_, index) => createSessionRecord(index + 1, index));
    const result = syncSessionOrderInWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: "session-3",
              sessions,
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: ["session-3"],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionNumber: 5,
      }),
      DEFAULT_MAIN_GROUP_ID,
      ["session-1", "session-3", "session-4", "session-2"],
    );

    expect(result.changed).toBe(true);
    expect(
      result.snapshot.groups[0]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual(["session-1", "session-3", "session-4", "session-2"]);
  });
});

describe("focusSessionInWorkspace", () => {
  test("should activate the owning group when focusing a session outside the current group", () => {
    const result = focusSessionInWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: "session-1",
              sessions: [createSessionRecord(1, 0)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: ["session-1"],
            },
            title: "Main",
          },
          {
            groupId: "group-2",
            snapshot: {
              focusedSessionId: "session-2",
              sessions: [createSessionRecord(2, 0), createSessionRecord(3, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: ["session-2", "session-3"],
            },
            title: "Infra",
          },
        ],
        nextGroupNumber: 3,
        nextSessionNumber: 4,
      }),
      "session-3",
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.activeGroupId).toBe("group-2");
    expect(result.snapshot.groups[1]?.snapshot.focusedSessionId).toBe("session-3");
  });
});

describe("active-group preferences", () => {
  test("should keep view mode, visible count, and fullscreen state scoped to the active group", () => {
    let snapshot = createDefaultGroupedSessionWorkspaceSnapshot();
    snapshot = createSessionInWorkspace(snapshot).snapshot;
    snapshot = createSessionInWorkspace(snapshot).snapshot;
    snapshot = createGroupFromSessionInWorkspace(snapshot, "session-2").snapshot;
    snapshot = setVisibleCountInWorkspace(snapshot, 1);
    snapshot = setViewModeInWorkspace(snapshot, "vertical");
    snapshot = toggleFullscreenSessionInWorkspace(snapshot);
    snapshot = focusGroupByIndexInWorkspace(snapshot, 1).snapshot;

    expect(snapshot.groups[0]?.snapshot.viewMode).toBe("grid");
    expect(snapshot.groups[0]?.snapshot.visibleCount).toBe(1);
    expect(snapshot.groups[1]?.snapshot.viewMode).toBe("vertical");
    expect(snapshot.groups[1]?.snapshot.visibleCount).toBe(1);
    expect(snapshot.groups[1]?.snapshot.fullscreenRestoreVisibleCount).toBeUndefined();
  });

  test("should rename the main group", () => {
    const result = renameGroupInWorkspace(
      createDefaultGroupedSessionWorkspaceSnapshot(),
      "group-1",
      "Client A",
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.title).toBe("Client A");
  });

  test("should allow removing the active main group when another group remains", () => {
    const result = removeGroupInWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: "session-1",
              sessions: [createSessionRecord(1, 0)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: ["session-1"],
            },
            title: "Main",
          },
          {
            groupId: "group-2",
            snapshot: {
              focusedSessionId: "session-2",
              sessions: [createSessionRecord(2, 0)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: ["session-2"],
            },
            title: "Client B",
          },
        ],
        nextGroupNumber: 3,
        nextSessionNumber: 3,
      }),
      DEFAULT_MAIN_GROUP_ID,
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.activeGroupId).toBe("group-2");
    expect(result.snapshot.groups.map((group) => group.groupId)).toEqual(["group-2"]);
  });

  test("should reorder groups when the incoming order matches the current set", () => {
    const result = syncGroupOrderInWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: undefined,
              sessions: [],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [],
            },
            title: "Main",
          },
          {
            groupId: "group-2",
            snapshot: {
              focusedSessionId: undefined,
              sessions: [],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [],
            },
            title: "Client B",
          },
          {
            groupId: "group-3",
            snapshot: {
              focusedSessionId: undefined,
              sessions: [],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [],
            },
            title: "Client C",
          },
        ],
        nextGroupNumber: 4,
        nextSessionNumber: 1,
      }),
      ["group-3", DEFAULT_MAIN_GROUP_ID, "group-2"],
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups.map((group) => group.groupId)).toEqual([
      "group-3",
      DEFAULT_MAIN_GROUP_ID,
      "group-2",
    ]);
    expect(result.snapshot.activeGroupId).toBe(DEFAULT_MAIN_GROUP_ID);
  });
});

type TestWorkspaceSnapshotInput = Omit<GroupedSessionWorkspaceSnapshot, "nextSessionDisplayId"> & {
  nextSessionDisplayId?: number;
};

function createWorkspaceSnapshot(
  input: TestWorkspaceSnapshotInput,
): GroupedSessionWorkspaceSnapshot {
  return {
    ...createDefaultGroupedSessionWorkspaceSnapshot(),
    ...input,
    nextSessionDisplayId: input.nextSessionDisplayId ?? 0,
  };
}
