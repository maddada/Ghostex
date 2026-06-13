import { describe, expect, test } from "vite-plus/test";
import {
  DEFAULT_MAIN_GROUP_ID,
  createDefaultGroupedSessionWorkspaceSnapshot,
  createSessionRecord,
  createTimestampedSessionId,
  formatSessionDisplayId,
  type GroupedSessionWorkspaceSnapshot,
  type SessionPaneLayoutNode,
} from "./session-grid-contract";
import {
  createGroupInSimpleWorkspace,
  createGroupFromSessionInSimpleWorkspace,
  createSessionInSimpleWorkspace,
  ensureAllSessionsInFocusedPaneTabGroupInSimpleWorkspace,
  focusSessionExclusivelyInSimpleWorkspace,
  focusSidebarSessionInSimpleWorkspace,
  focusGroupInSimpleWorkspace,
  focusSessionInSimpleWorkspace,
  focusVisibleDirectionInSimpleWorkspace,
  mergeAllTabsInPaneLayoutInSimpleWorkspace,
  moveSessionInPaneLayoutInSimpleWorkspace,
  moveSessionToGroupInSimpleWorkspace,
  normalizeSimpleGroupedSessionWorkspaceSnapshot,
  removeSessionInSimpleWorkspace,
  reorderSessionInPaneTabGroupInSimpleWorkspace,
  rotatePaneLayoutClockwiseInSimpleWorkspace,
  setSessionFavoriteInSimpleWorkspace,
  setSessionPinnedInSimpleWorkspace,
  setGroupSleepingInSimpleWorkspace,
  setSessionSleepingInSimpleWorkspace,
  setT3SessionMetadataInSimpleWorkspace,
  setTerminalSessionLastActivityAtInSimpleWorkspace,
  setVisibleCountInSimpleWorkspace,
  selectPaneTabInSimpleWorkspace,
  swapVisibleSessionsInSimpleWorkspace,
  syncSessionOrderAcrossSimpleWorkspaceGroups,
  syncSessionOrderInSimpleWorkspace,
  toggleFullscreenSessionInSimpleWorkspace,
  wakePaneTabSessionInSimpleWorkspace,
} from "./simple-grouped-session-workspace-state";

describe("normalizeSimpleGroupedSessionWorkspaceSnapshot", () => {
  test("should backfill boundThreadId for legacy T3 sessions", () => {
    const snapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: "session-1",
            fullscreenRestoreVisibleCount: undefined,
            sessions: [
              {
                ...createSessionRecord(1, 0, {
                  kind: "t3",
                  t3: {
                    projectId: "project-1",
                    serverOrigin: "http://127.0.0.1:3774",
                    threadId: "thread-1",
                    workspaceRoot: "/workspace",
                  },
                  title: "T3 Code",
                }),
                t3: {
                  projectId: "project-1",
                  serverOrigin: "http://127.0.0.1:3774",
                  threadId: "thread-1",
                  workspaceRoot: "/workspace",
                },
              },
            ],
            viewMode: "grid",
            visibleCount: 1,
            visibleSessionIds: ["session-1"],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 1,
      nextSessionNumber: 2,
    });

    expect(snapshot.groups[0]?.snapshot.sessions[0]).toEqual(
      expect.objectContaining({
        t3: {
          boundThreadId: "thread-1",
          projectId: "project-1",
          serverOrigin: "http://127.0.0.1:3774",
          threadId: "thread-1",
          workspaceRoot: "/workspace",
        },
      }),
    );
  });

  test("should preserve browser sessions as workspace panes", () => {
    const snapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: "session-1",
            fullscreenRestoreVisibleCount: undefined,
            sessions: [
              createSessionRecord(1, 0, {
                browser: { url: "https://example.com" },
                kind: "browser",
                title: "Browser",
              }),
            ],
            viewMode: "grid",
            visibleCount: 2,
            visibleSessionIds: ["session-1"],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 1,
      nextSessionNumber: 2,
    });

    expect(snapshot.groups).toHaveLength(1);
    expect(snapshot.groups[0]?.snapshot.sessions).toEqual([
      expect.objectContaining({
        browser: { url: "https://example.com" },
        kind: "browser",
        title: "Browser",
      }),
    ]);
    expect(snapshot.groups[0]?.snapshot.focusedSessionId).toBe("session-1");
    expect(snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual(["session-1"]);
  });

  test("should repair duplicate generated display ids", () => {
    const snapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: "session-2",
            fullscreenRestoreVisibleCount: undefined,
            sessions: [
              createSessionRecord(1, 0, { displayId: "52" }),
              createSessionRecord(2, 1, { displayId: "52" }),
            ],
            viewMode: "grid",
            visibleCount: 2,
            visibleSessionIds: ["session-1", "session-2"],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 0,
      nextSessionNumber: 3,
    });

    const sessions = snapshot.groups[0]?.snapshot.sessions ?? [];
    expect(sessions.map((session) => session.displayId)).toEqual(["52", "00"]);
    expect(sessions.map((session) => session.alias)).toEqual(["52", "00"]);
    expect(sessions.map((session) => session.sessionId)).toEqual(["session-1", "session-2"]);
  });
});

const sessionIdForDisplay = (displayId: number | string): string => {
  const numericDisplayId = Number.parseInt(formatSessionDisplayId(displayId), 10);
  return `session-${numericDisplayId + 1}`;
};

describe("createTimestampedSessionId", () => {
  test("should use a compact ghostex month-day creation timestamp", () => {
    const sessionId = createTimestampedSessionId([], new Date(2026, 3, 26, 20, 54, 12), () => 0.5);

    expect(sessionId).toBe("g-0426-205412");
  });

  test("should avoid active or archived session ids while preserving the compact shape", () => {
    const sessionId = createTimestampedSessionId(
      ["g-0426-205412"],
      new Date(2026, 3, 26, 20, 54, 12),
      () => 0,
    );

    expect(sessionId).toBe("g-0426-205413");
  });
});

describe("focusVisibleDirectionInSimpleWorkspace", () => {
  test("should move focus between visible pane sessions without changing visible session tabs", () => {
    const leftSessionId = sessionIdForDisplay(0);
    const rightSessionId = sessionIdForDisplay(1);
    const paneLayout: SessionPaneLayoutNode = {
      children: [
        { kind: "leaf", sessionId: leftSessionId },
        { kind: "leaf", sessionId: rightSessionId },
      ],
      direction: "horizontal",
      kind: "split",
    };
    const result = focusVisibleDirectionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: leftSessionId,
              paneLayout,
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [leftSessionId, rightSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      "right",
    );

    const group = result.snapshot.groups[0]?.snapshot;
    expect(result.changed).toBe(true);
    expect(group?.focusedSessionId).toBe(rightSessionId);
    expect(group?.paneLayout).toEqual(paneLayout);
    expect(group?.visibleSessionIds).toEqual([leftSessionId, rightSessionId]);
  });

  test("should not reveal hidden sessions when directional focus leaves the visible pane set", () => {
    const leftSessionId = sessionIdForDisplay(0);
    const focusedSessionId = sessionIdForDisplay(1);
    const hiddenRightSessionId = sessionIdForDisplay(2);
    const paneLayout: SessionPaneLayoutNode = {
      children: [
        { kind: "leaf", sessionId: leftSessionId },
        { kind: "leaf", sessionId: focusedSessionId },
      ],
      direction: "horizontal",
      kind: "split",
    };
    const result = focusVisibleDirectionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId,
              paneLayout,
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [leftSessionId, focusedSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      "right",
    );

    const group = result.snapshot.groups[0]?.snapshot;
    expect(result.changed).toBe(false);
    expect(group?.focusedSessionId).toBe(focusedSessionId);
    expect(group?.paneLayout).toEqual(paneLayout);
    expect(group?.visibleSessionIds).toEqual([leftSessionId, focusedSessionId]);
    expect(group?.visibleSessionIds).not.toContain(hiddenRightSessionId);
  });

  test("should use visible tab group layout instead of stored session grid slots", () => {
    const rightSessionId = sessionIdForDisplay(0);
    const leftSessionId = sessionIdForDisplay(1);
    const hiddenLeftTabSessionId = sessionIdForDisplay(2);
    const hiddenRightTabSessionId = sessionIdForDisplay(3);
    const paneLayout: SessionPaneLayoutNode = {
      children: [
        {
          activeSessionId: leftSessionId,
          kind: "tabs",
          sessionIds: [leftSessionId, hiddenLeftTabSessionId],
        },
        {
          activeSessionId: rightSessionId,
          kind: "tabs",
          sessionIds: [hiddenRightTabSessionId, rightSessionId],
        },
      ],
      direction: "horizontal",
      kind: "split",
    };
    const result = focusVisibleDirectionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: leftSessionId,
              paneLayout,
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
                createSessionRecord(4, 3),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [rightSessionId, leftSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 4,
        nextSessionNumber: 5,
      }),
      "right",
    );

    const group = result.snapshot.groups[0]?.snapshot;
    expect(result.changed).toBe(true);
    expect(group?.focusedSessionId).toBe(rightSessionId);
    expect(group?.paneLayout).toEqual(paneLayout);
    expect(group?.visibleSessionIds).toEqual([rightSessionId, leftSessionId]);
  });

  test("should prefer the directly lower pane over a taller right pane", () => {
    const topLeftSessionId = sessionIdForDisplay(0);
    const bottomLeftSessionId = sessionIdForDisplay(1);
    const rightSessionId = sessionIdForDisplay(2);
    const paneLayout: SessionPaneLayoutNode = {
      children: [
        {
          children: [
            { kind: "leaf", sessionId: topLeftSessionId },
            { kind: "leaf", sessionId: bottomLeftSessionId },
          ],
          direction: "vertical",
          kind: "split",
        },
        { kind: "leaf", sessionId: rightSessionId },
      ],
      direction: "horizontal",
      kind: "split",
    };
    const result = focusVisibleDirectionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: topLeftSessionId,
              paneLayout,
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 3,
              visibleSessionIds: [topLeftSessionId, bottomLeftSessionId, rightSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      "down",
    );

    const group = result.snapshot.groups[0]?.snapshot;
    expect(result.changed).toBe(true);
    expect(group?.focusedSessionId).toBe(bottomLeftSessionId);
    expect(group?.visibleSessionIds).toEqual([
      topLeftSessionId,
      bottomLeftSessionId,
      rightSessionId,
    ]);
  });

  test("should move right from top-left to top-right in a four-way split", () => {
    const topLeftSessionId = sessionIdForDisplay(0);
    const bottomLeftSessionId = sessionIdForDisplay(1);
    const bottomRightSessionId = sessionIdForDisplay(2);
    const topRightSessionId = sessionIdForDisplay(3);
    const paneLayout: SessionPaneLayoutNode = {
      children: [
        {
          children: [
            { kind: "leaf", sessionId: topLeftSessionId },
            { kind: "leaf", sessionId: bottomLeftSessionId },
          ],
          direction: "vertical",
          kind: "split",
        },
        {
          children: [
            { kind: "leaf", sessionId: topRightSessionId },
            { kind: "leaf", sessionId: bottomRightSessionId },
          ],
          direction: "vertical",
          kind: "split",
        },
      ],
      direction: "horizontal",
      kind: "split",
    };
    const result = focusVisibleDirectionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: topLeftSessionId,
              paneLayout,
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
                createSessionRecord(4, 3),
              ],
              viewMode: "grid",
              visibleCount: 4,
              visibleSessionIds: [
                topLeftSessionId,
                bottomLeftSessionId,
                bottomRightSessionId,
                topRightSessionId,
              ],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 4,
        nextSessionNumber: 5,
      }),
      "right",
    );

    const group = result.snapshot.groups[0]?.snapshot;
    expect(result.changed).toBe(true);
    expect(group?.focusedSessionId).toBe(topRightSessionId);
    expect(group?.visibleSessionIds).toEqual([
      topLeftSessionId,
      bottomLeftSessionId,
      bottomRightSessionId,
      topRightSessionId,
    ]);
  });

  test("should ignore hidden panes while keeping visible right-column ordering", () => {
    const topLeftSessionId = sessionIdForDisplay(0);
    const bottomLeftSessionId = sessionIdForDisplay(1);
    const topRightSessionId = sessionIdForDisplay(2);
    const bottomRightSessionId = sessionIdForDisplay(3);
    const hiddenTopRightTabSessionId = sessionIdForDisplay(4);
    const hiddenFarRightSessionId = sessionIdForDisplay(5);
    const paneLayout: SessionPaneLayoutNode = {
      children: [
        {
          children: [
            { kind: "leaf", sessionId: topLeftSessionId },
            { kind: "leaf", sessionId: bottomLeftSessionId },
          ],
          direction: "vertical",
          kind: "split",
        },
        {
          children: [
            {
              activeSessionId: hiddenTopRightTabSessionId,
              kind: "tabs",
              sessionIds: [hiddenTopRightTabSessionId],
            },
            { kind: "leaf", sessionId: topRightSessionId },
            { kind: "leaf", sessionId: bottomRightSessionId },
          ],
          direction: "vertical",
          kind: "split",
        },
        { kind: "leaf", sessionId: hiddenFarRightSessionId },
      ],
      direction: "horizontal",
      kind: "split",
    };
    const result = focusVisibleDirectionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: topLeftSessionId,
              paneLayout,
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
                createSessionRecord(4, 3),
                { ...createSessionRecord(5, 4), isSleeping: true },
                { ...createSessionRecord(6, 5), isSleeping: true },
              ],
              viewMode: "grid",
              visibleCount: 4,
              visibleSessionIds: [
                topLeftSessionId,
                bottomLeftSessionId,
                topRightSessionId,
                bottomRightSessionId,
              ],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 6,
        nextSessionNumber: 7,
      }),
      "right",
    );

    const group = result.snapshot.groups[0]?.snapshot;
    expect(result.changed).toBe(true);
    expect(group?.focusedSessionId).toBe(topRightSessionId);
    expect(group?.visibleSessionIds).toEqual([
      topLeftSessionId,
      bottomLeftSessionId,
      topRightSessionId,
      bottomRightSessionId,
    ]);
  });

  test("should not use normalized visibleCount padding as directional focus targets", () => {
    const topLeftSessionId = sessionIdForDisplay(0);
    const bottomLeftSessionId = sessionIdForDisplay(1);
    const topRightSessionId = sessionIdForDisplay(2);
    const bottomRightSessionId = sessionIdForDisplay(3);
    const hiddenTopRightTabSessionId = sessionIdForDisplay(4);
    const hiddenFarRightSessionId = sessionIdForDisplay(5);
    const paneLayout: SessionPaneLayoutNode = {
      children: [
        {
          children: [
            { kind: "leaf", sessionId: topLeftSessionId },
            { kind: "leaf", sessionId: bottomLeftSessionId },
          ],
          direction: "vertical",
          kind: "split",
        },
        {
          children: [
            {
              activeSessionId: hiddenTopRightTabSessionId,
              kind: "tabs",
              sessionIds: [hiddenTopRightTabSessionId],
            },
            { kind: "leaf", sessionId: topRightSessionId },
            { kind: "leaf", sessionId: bottomRightSessionId },
          ],
          direction: "vertical",
          kind: "split",
        },
        { kind: "leaf", sessionId: hiddenFarRightSessionId },
      ],
      direction: "horizontal",
      kind: "split",
    };
    const result = focusVisibleDirectionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: bottomRightSessionId,
              paneLayout,
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
                createSessionRecord(4, 3),
                { ...createSessionRecord(5, 4), isSleeping: true },
                { ...createSessionRecord(6, 5), isSleeping: true },
              ],
              viewMode: "grid",
              visibleCount: 6,
              visibleSessionIds: [
                topLeftSessionId,
                bottomLeftSessionId,
                topRightSessionId,
                bottomRightSessionId,
              ],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 6,
        nextSessionNumber: 7,
      }),
      "right",
    );

    const group = result.snapshot.groups[0]?.snapshot;
    expect(result.changed).toBe(false);
    expect(group?.focusedSessionId).toBe(bottomRightSessionId);
    expect(group?.visibleSessionIds).toEqual([
      topLeftSessionId,
      bottomLeftSessionId,
      topRightSessionId,
      bottomRightSessionId,
    ]);
    expect(group?.visibleSessionIds).not.toContain(hiddenFarRightSessionId);
    expect(group?.visibleSessionIds).not.toContain(hiddenTopRightTabSessionId);
  });
});

describe("focusSessionInSimpleWorkspace", () => {
  test("should exit focus mode before focusing a session outside the focused tab group", () => {
    const focusedTabSessionId = sessionIdForDisplay(0);
    const focusedSiblingSessionId = sessionIdForDisplay(1);
    const rightSessionId = sessionIdForDisplay(2);
    const result = focusSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: focusedTabSessionId,
              fullscreenRestoreVisibleCount: 4,
              paneLayout: {
                children: [
                  {
                    activeSessionId: focusedTabSessionId,
                    kind: "tabs",
                    sessionIds: [focusedTabSessionId, focusedSiblingSessionId],
                  },
                  { kind: "leaf", sessionId: rightSessionId },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [focusedTabSessionId],
            },
            title: "Main",
          },
        ],
      }),
      rightSessionId,
    );

    const group = result.snapshot.groups[0]?.snapshot;
    expect(group?.focusedSessionId).toBe(rightSessionId);
    expect(group?.fullscreenRestoreVisibleCount).toBeUndefined();
    expect(group?.visibleCount).toBe(4);
    expect(group?.paneLayout).toEqual({
      children: [
        {
          activeSessionId: focusedTabSessionId,
          kind: "tabs",
          sessionIds: [focusedTabSessionId, focusedSiblingSessionId],
        },
        { kind: "leaf", sessionId: rightSessionId },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should keep focus mode when selecting another tab inside the focused tab group", () => {
    const focusedTabSessionId = sessionIdForDisplay(0);
    const focusedSiblingSessionId = sessionIdForDisplay(1);
    const result = focusSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: focusedTabSessionId,
              fullscreenRestoreVisibleCount: 4,
              paneLayout: {
                activeSessionId: focusedTabSessionId,
                kind: "tabs",
                sessionIds: [focusedTabSessionId, focusedSiblingSessionId],
              },
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [focusedTabSessionId],
            },
            title: "Main",
          },
        ],
      }),
      focusedSiblingSessionId,
    );

    const group = result.snapshot.groups[0]?.snapshot;
    expect(group?.focusedSessionId).toBe(focusedSiblingSessionId);
    expect(group?.fullscreenRestoreVisibleCount).toBe(4);
    expect(group?.visibleCount).toBe(1);
    expect(group?.paneLayout).toEqual({
      activeSessionId: focusedSiblingSessionId,
      kind: "tabs",
      sessionIds: [focusedTabSessionId, focusedSiblingSessionId],
    });
  });

  test("should focus a session exclusively while preserving its pane tab group", () => {
    const leftSessionId = sessionIdForDisplay(0);
    const rightSessionId = sessionIdForDisplay(1);
    const rightSiblingSessionId = sessionIdForDisplay(2);
    const result = focusSessionExclusivelyInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: leftSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: leftSessionId },
                  {
                    activeSessionId: rightSessionId,
                    kind: "tabs",
                    sessionIds: [rightSessionId, rightSiblingSessionId],
                  },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 4,
              visibleSessionIds: [leftSessionId, rightSessionId, rightSiblingSessionId],
            },
            title: "Main",
          },
        ],
      }),
      rightSiblingSessionId,
    );

    const group = result.snapshot.groups[0]?.snapshot;
    expect(group?.focusedSessionId).toBe(rightSiblingSessionId);
    expect(group?.fullscreenRestoreVisibleCount).toBe(4);
    expect(group?.visibleCount).toBe(1);
    expect(group?.visibleSessionIds).toEqual([rightSiblingSessionId]);
    /*
     * CDXC:SessionFocusMode 2026-05-23-09:28:
     * Exclusive focus must not rewrite split/tab topology. Native layout later
     * scopes rendering to the focused tab group, so the persisted paneLayout
     * can restore the original split when focus mode exits.
     */
    expect(group?.paneLayout).toEqual({
      children: [
        { kind: "leaf", sessionId: leftSessionId },
        {
          activeSessionId: rightSiblingSessionId,
          kind: "tabs",
          sessionIds: [rightSessionId, rightSiblingSessionId],
        },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should preserve the split layout when virtual tab materialization runs during focus mode", () => {
    const leftSessionId = sessionIdForDisplay(0);
    const rightSessionId = sessionIdForDisplay(1);
    const rightSiblingSessionId = sessionIdForDisplay(2);
    const originalPaneLayout: SessionPaneLayoutNode = {
      children: [
        { kind: "leaf", sessionId: leftSessionId },
        {
          activeSessionId: rightSessionId,
          kind: "tabs",
          sessionIds: [rightSessionId, rightSiblingSessionId],
        },
      ],
      direction: "horizontal",
      kind: "split",
    };
    const focused = focusSessionExclusivelyInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: leftSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: originalPaneLayout,
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 4,
              visibleSessionIds: [leftSessionId, rightSessionId, rightSiblingSessionId],
            },
            title: "Main",
          },
        ],
      }),
      rightSiblingSessionId,
    ).snapshot;

    const materialized = ensureAllSessionsInFocusedPaneTabGroupInSimpleWorkspace(
      focused,
      DEFAULT_MAIN_GROUP_ID,
    ).snapshot;
    const restored = toggleFullscreenSessionInSimpleWorkspace(materialized);

    /*
     * CDXC:SessionFocusMode 2026-06-02-18:45:
     * publish() calls virtual tab materialization before native layout sync.
     * While Focus mode is active that materialization must be a no-op for paneLayout, otherwise Exit focus can restore the saved visible count but has no preserved split tree left to render.
     */
    expect(materialized.groups[0]?.snapshot.visibleCount).toBe(1);
    expect(materialized.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        { kind: "leaf", sessionId: leftSessionId },
        {
          activeSessionId: rightSiblingSessionId,
          kind: "tabs",
          sessionIds: [rightSessionId, rightSiblingSessionId],
        },
      ],
      direction: "horizontal",
      kind: "split",
    });
    expect(restored.groups[0]?.snapshot.visibleCount).toBe(4);
    expect(restored.groups[0]?.snapshot.fullscreenRestoreVisibleCount).toBeUndefined();
    expect(restored.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        { kind: "leaf", sessionId: leftSessionId },
        {
          activeSessionId: rightSiblingSessionId,
          kind: "tabs",
          sessionIds: [rightSessionId, rightSiblingSessionId],
        },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should preserve the split layout when selecting a focused-pane tab during focus mode", () => {
    const leftSessionId = sessionIdForDisplay(0);
    const leftSiblingSessionId = sessionIdForDisplay(1);
    const rightSessionId = sessionIdForDisplay(2);
    const rightSiblingSessionId = sessionIdForDisplay(3);
    const rightBackgroundSessionId = sessionIdForDisplay(4);
    const originalPaneLayout: SessionPaneLayoutNode = {
      children: [
        {
          activeSessionId: leftSiblingSessionId,
          kind: "tabs",
          sessionIds: [leftSessionId, leftSiblingSessionId],
        },
        {
          activeSessionId: rightSessionId,
          kind: "tabs",
          sessionIds: [rightSessionId, rightSiblingSessionId, rightBackgroundSessionId],
        },
      ],
      direction: "horizontal",
      kind: "split",
    };
    const focused = focusSessionExclusivelyInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: leftSiblingSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: originalPaneLayout,
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
                createSessionRecord(4, 3),
                createSessionRecord(5, 4),
              ],
              viewMode: "grid",
              visibleCount: 4,
              visibleSessionIds: [leftSiblingSessionId, rightSessionId],
            },
            title: "Main",
          },
        ],
      }),
      rightSessionId,
    ).snapshot;

    const selected = selectPaneTabInSimpleWorkspace(
      focused,
      DEFAULT_MAIN_GROUP_ID,
      rightSiblingSessionId,
    ).snapshot;
    const restored = toggleFullscreenSessionInSimpleWorkspace(selected);

    /*
     * CDXC:SessionFocusMode 2026-06-04-20:37:
     * Native tab selection can run while Focus mode is active, and that path
     * must not materialize hidden split branches into the focused tab group.
     * Keep the left and right pane tab groups separate so Exit focus restores
     * the original split with all tabs still in their owning panes.
     */
    expect(selected.groups[0]?.snapshot.visibleCount).toBe(1);
    expect(selected.groups[0]?.snapshot.visibleSessionIds).toEqual([rightSiblingSessionId]);
    expect(selected.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: leftSiblingSessionId,
          kind: "tabs",
          sessionIds: [leftSessionId, leftSiblingSessionId],
        },
        {
          activeSessionId: rightSiblingSessionId,
          kind: "tabs",
          sessionIds: [rightSessionId, rightSiblingSessionId, rightBackgroundSessionId],
        },
      ],
      direction: "horizontal",
      kind: "split",
    });
    expect(restored.groups[0]?.snapshot.visibleCount).toBe(4);
    expect(restored.groups[0]?.snapshot.fullscreenRestoreVisibleCount).toBeUndefined();
    expect(restored.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: leftSiblingSessionId,
          kind: "tabs",
          sessionIds: [leftSessionId, leftSiblingSessionId],
        },
        {
          activeSessionId: rightSiblingSessionId,
          kind: "tabs",
          sessionIds: [rightSessionId, rightSiblingSessionId, rightBackgroundSessionId],
        },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should not enter focus mode for a single pane tab group", () => {
    const firstSessionId = sessionIdForDisplay(0);
    const secondSessionId = sessionIdForDisplay(1);
    const result = focusSessionExclusivelyInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: firstSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                activeSessionId: firstSessionId,
                kind: "tabs",
                sessionIds: [firstSessionId, secondSessionId],
              },
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [firstSessionId, secondSessionId],
            },
            title: "Main",
          },
        ],
      }),
      secondSessionId,
    );

    const group = result.snapshot.groups[0]?.snapshot;
    expect(group?.focusedSessionId).toBe(secondSessionId);
    expect(group?.fullscreenRestoreVisibleCount).toBeUndefined();
    expect(group?.visibleCount).toBe(2);
    expect(group?.visibleSessionIds).toEqual([firstSessionId, secondSessionId]);
    expect(group?.paneLayout).toEqual({
      activeSessionId: secondSessionId,
      kind: "tabs",
      sessionIds: [firstSessionId, secondSessionId],
    });
  });

  test("should not enter focus mode when the only other split pane is sleeping", () => {
    const awakeSessionId = sessionIdForDisplay(0);
    const sleepingSessionId = sessionIdForDisplay(1);
    const result = focusSessionExclusivelyInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: awakeSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: awakeSessionId },
                  { kind: "leaf", sessionId: sleepingSessionId },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                { ...createSessionRecord(2, 1), isSleeping: true },
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [awakeSessionId, sleepingSessionId],
            },
            title: "Main",
          },
        ],
      }),
      awakeSessionId,
    );

    const group = result.snapshot.groups[0]?.snapshot;
    expect(group?.focusedSessionId).toBe(awakeSessionId);
    expect(group?.fullscreenRestoreVisibleCount).toBeUndefined();
    expect(group?.visibleCount).toBe(2);
    expect(group?.paneLayout).toEqual({
      children: [
        { kind: "leaf", sessionId: awakeSessionId },
        { kind: "leaf", sessionId: sleepingSessionId },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should replace the focused visible session when selecting a hidden session in split 2", () => {
    const result = focusSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      sessionIdForDisplay(2),
    );

    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(sessionIdForDisplay(2));
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(2),
      sessionIdForDisplay(1),
    ]);
  });

  test("should replace the focused pane layout slot when selecting a hidden sidebar session", () => {
    const result = focusSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: sessionIdForDisplay(0) },
                  { kind: "leaf", sessionId: sessionIdForDisplay(1) },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      sessionIdForDisplay(2),
    );

    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(2),
      sessionIdForDisplay(1),
    ]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        { kind: "leaf", sessionId: sessionIdForDisplay(2) },
        { kind: "leaf", sessionId: sessionIdForDisplay(1) },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should preserve an existing split pane when native focus sees stale visible ids", () => {
    const tabOwnerSessionId = sessionIdForDisplay(0);
    const hiddenTabSessionId = sessionIdForDisplay(1);
    const clickedPaneSessionId = sessionIdForDisplay(2);
    const backgroundTabSessionId = sessionIdForDisplay(3);

    const result = focusSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: tabOwnerSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  {
                    activeSessionId: tabOwnerSessionId,
                    kind: "tabs",
                    sessionIds: [hiddenTabSessionId, tabOwnerSessionId, backgroundTabSessionId],
                  },
                  { kind: "leaf", sessionId: clickedPaneSessionId },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
                createSessionRecord(4, 3),
              ],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [tabOwnerSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 4,
        nextSessionNumber: 5,
      }),
      clickedPaneSessionId,
    );

    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(clickedPaneSessionId);
    /*
     * CDXC:PaneFocus 2026-06-12-13:13:
     * Native terminalFocused can target a real split pane even when legacy
     * visibleSessionIds only names the previous pane owner. Because paneLayout
     * already owns the clicked pane, focus must select it in place instead of
     * moving that session into the previous pane's tab group and flattening the
     * split.
     */
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: tabOwnerSessionId,
          kind: "tabs",
          sessionIds: [hiddenTabSessionId, tabOwnerSessionId, backgroundTabSessionId],
        },
        { kind: "leaf", sessionId: clickedPaneSessionId },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should keep hidden tab-group members when selecting a hidden sidebar session", () => {
    const sleepingSession = {
      ...createSessionRecord(2, 1),
      isSleeping: true,
    };
    const result = focusSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  {
                    activeSessionId: sessionIdForDisplay(0),
                    kind: "tabs",
                    sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
                  },
                  { kind: "leaf", sessionId: sessionIdForDisplay(2) },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                sleepingSession,
                createSessionRecord(3, 2),
                createSessionRecord(4, 3),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(2)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 4,
        nextSessionNumber: 5,
      }),
      sessionIdForDisplay(3),
    );

    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(3),
      sessionIdForDisplay(2),
    ]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: sessionIdForDisplay(3),
          kind: "tabs",
          sessionIds: [sessionIdForDisplay(3), sessionIdForDisplay(1)],
        },
        { kind: "leaf", sessionId: sessionIdForDisplay(2) },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should preserve visible slot order when focusing an already visible session", () => {
    const result = focusSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(1), sessionIdForDisplay(0)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      sessionIdForDisplay(1),
    );

    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(sessionIdForDisplay(1));
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(1),
      sessionIdForDisplay(0),
    ]);
  });
});

describe("focusSidebarSessionInSimpleWorkspace", () => {
  test("should select an unmounted sidebar session in its existing split tab group", () => {
    const focusedLeftSessionId = sessionIdForDisplay(0);
    const activeRightSessionId = sessionIdForDisplay(1);
    const unmountedRightSessionId = sessionIdForDisplay(2);
    const backgroundSessionId = sessionIdForDisplay(3);

    const result = focusSidebarSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: focusedLeftSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: focusedLeftSessionId },
                  {
                    activeSessionId: activeRightSessionId,
                    kind: "tabs",
                    sessionIds: [unmountedRightSessionId, activeRightSessionId],
                  },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
                createSessionRecord(4, 3),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [focusedLeftSessionId, activeRightSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 4,
        nextSessionNumber: 5,
      }),
      unmountedRightSessionId,
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(unmountedRightSessionId);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      focusedLeftSessionId,
      activeRightSessionId,
      unmountedRightSessionId,
    ]);
    /**
     * CDXC:SidebarSessionFocus 2026-05-29-09:47:
     * Clicking a sidebar row for an unmounted tab is a selection/restore intent
     * for the tab's current pane, not a request to move it into the focused
     * pane. Preserve the source tab-group membership and ordering exactly while
     * making that tab active.
     */
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: focusedLeftSessionId,
          kind: "tabs",
          sessionIds: [focusedLeftSessionId, backgroundSessionId],
        },
        {
          activeSessionId: unmountedRightSessionId,
          kind: "tabs",
          sessionIds: [unmountedRightSessionId, activeRightSessionId],
        },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });
});

describe("focusGroupInSimpleWorkspace", () => {
  test("should restore each group's own visible sessions when switching groups", () => {
    const snapshot = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: sessionIdForDisplay(0),
            fullscreenRestoreVisibleCount: undefined,
            sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
            viewMode: "grid",
            visibleCount: 2,
            visibleSessionIds: [sessionIdForDisplay(1), sessionIdForDisplay(0)],
          },
          title: "Main",
        },
        {
          groupId: "group-2",
          snapshot: {
            focusedSessionId: sessionIdForDisplay(2),
            fullscreenRestoreVisibleCount: undefined,
            sessions: [createSessionRecord(3, 0), createSessionRecord(4, 1)],
            viewMode: "grid",
            visibleCount: 2,
            visibleSessionIds: [sessionIdForDisplay(2), sessionIdForDisplay(3)],
          },
          title: "Design",
        },
      ],
      nextGroupNumber: 3,
      nextSessionDisplayId: 4,
      nextSessionNumber: 5,
    });

    const result = focusGroupInSimpleWorkspace(snapshot, "group-2");

    expect(result.snapshot.activeGroupId).toBe("group-2");
    expect(result.snapshot.groups[1]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(2),
      sessionIdForDisplay(3),
    ]);
    expect(result.snapshot.groups[1]?.snapshot.focusedSessionId).toBe(sessionIdForDisplay(2));
  });
});

describe("moveSessionToGroupInSimpleWorkspace", () => {
  test("should move the session, activate the target group, and focus the moved session", () => {
    const result = moveSessionToGroupInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            title: "Main",
          },
          {
            groupId: "group-2",
            snapshot: {
              focusedSessionId: sessionIdForDisplay(2),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(3, 0)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [sessionIdForDisplay(2)],
            },
            title: "Infra",
          },
        ],
        nextGroupNumber: 3,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      sessionIdForDisplay(1),
      "group-2",
    );

    expect(result.snapshot.activeGroupId).toBe("group-2");
    expect(result.snapshot.groups[1]?.snapshot.focusedSessionId).toBe(sessionIdForDisplay(1));
    expect(result.snapshot.groups[1]?.snapshot.visibleSessionIds).toEqual([sessionIdForDisplay(1)]);
  });
});

describe("removeSessionInSimpleWorkspace", () => {
  test("should select the next tab to the right when closing the active split-pane tab", () => {
    const parkedLeftSessionId = sessionIdForDisplay(0);
    const closingT3SessionId = sessionIdForDisplay(1);
    const nextRightSessionId = sessionIdForDisplay(2);
    const rightPaneSessionId = sessionIdForDisplay(3);

    const result = removeSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: closingT3SessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  {
                    activeSessionId: closingT3SessionId,
                    kind: "tabs",
                    sessionIds: [
                      parkedLeftSessionId,
                      closingT3SessionId,
                      nextRightSessionId,
                    ],
                  },
                  { kind: "leaf", sessionId: rightPaneSessionId },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                { ...createSessionRecord(1, 0), isSleeping: true },
                createSessionRecord(2, 1, {
                  kind: "t3",
                  t3: {
                    projectId: "project-1",
                    serverOrigin: "http://127.0.0.1:3774",
                    threadId: "thread-1",
                    workspaceRoot: "/workspace",
                  },
                }),
                { ...createSessionRecord(3, 2), isSleeping: true },
                createSessionRecord(4, 3),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [closingT3SessionId, rightPaneSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 4,
        nextSessionNumber: 5,
      }),
      closingT3SessionId,
    );

    expect(result.changed).toBe(true);
    const groupSnapshot = result.snapshot.groups[0]?.snapshot;
    expect(groupSnapshot?.focusedSessionId).toBe(nextRightSessionId);
    expect(groupSnapshot?.visibleCount).toBe(2);
    expect(groupSnapshot?.visibleSessionIds).toEqual([nextRightSessionId, rightPaneSessionId]);
    expect(groupSnapshot?.sessions.find((session) => session.sessionId === nextRightSessionId)?.isSleeping).toBe(false);
    /*
     * CDXC:PaneTabs 2026-06-06-04:32:
     * Closing an active T3/web tab in a split pane must promote the next tab in that pane before sidebar publish materializes virtual tabs.
     * This keeps a concrete owner in the split branch, so the materializer preserves the split instead of collapsing every project tab under the other pane.
     */
    expect(groupSnapshot?.paneLayout).toEqual({
      children: [
        {
          activeSessionId: nextRightSessionId,
          kind: "tabs",
          sessionIds: [parkedLeftSessionId, nextRightSessionId],
        },
        { kind: "leaf", sessionId: rightPaneSessionId },
      ],
      direction: "horizontal",
      kind: "split",
    });

    const materialized = ensureAllSessionsInFocusedPaneTabGroupInSimpleWorkspace(
      result.snapshot,
      DEFAULT_MAIN_GROUP_ID,
    );
    expect(materialized.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: nextRightSessionId,
          kind: "tabs",
          sessionIds: [parkedLeftSessionId, nextRightSessionId],
        },
        { kind: "leaf", sessionId: rightPaneSessionId },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should select the left sibling when closing the rightmost active tab", () => {
    const leftSiblingSessionId = sessionIdForDisplay(0);
    const closingSessionId = sessionIdForDisplay(1);

    const result = removeSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: closingSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                activeSessionId: closingSessionId,
                kind: "tabs",
                sessionIds: [leftSiblingSessionId, closingSessionId],
              },
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [closingSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      closingSessionId,
    );

    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(leftSiblingSessionId);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([leftSiblingSessionId]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      kind: "leaf",
      sessionId: leftSiblingSessionId,
    });
  });

  test("should collapse the split branch when closing the pane's last tab", () => {
    const closingOnlyTabSessionId = sessionIdForDisplay(0);
    const remainingActiveSessionId = sessionIdForDisplay(1);
    const remainingSiblingSessionId = sessionIdForDisplay(2);

    const result = removeSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: closingOnlyTabSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: closingOnlyTabSessionId },
                  {
                    activeSessionId: remainingActiveSessionId,
                    kind: "tabs",
                    sessionIds: [remainingActiveSessionId, remainingSiblingSessionId],
                  },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [closingOnlyTabSessionId, remainingActiveSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      closingOnlyTabSessionId,
    );

    /*
     * CDXC:PaneTabs 2026-06-06-04:32:
     * A split pane with no remaining tab owner should close as a pane, not pull a background session into that branch.
     * After a two-pane split loses that branch, the remaining pane owns the visible workspace and its tabs stay together.
     */
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(remainingActiveSessionId);
    expect(result.snapshot.groups[0]?.snapshot.visibleCount).toBe(1);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      remainingActiveSessionId,
    ]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      activeSessionId: remainingActiveSessionId,
      kind: "tabs",
      sessionIds: [remainingActiveSessionId, remainingSiblingSessionId],
    });
  });

  test("should switch to the previous non-empty group when closing the active group's last session", () => {
    const result = removeSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: "group-2",
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(1), sessionIdForDisplay(0)],
            },
            title: "Main",
          },
          {
            groupId: "group-2",
            snapshot: {
              focusedSessionId: sessionIdForDisplay(2),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(3, 0)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [sessionIdForDisplay(2)],
            },
            title: "Focused",
          },
          {
            groupId: "group-3",
            snapshot: {
              focusedSessionId: sessionIdForDisplay(3),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(4, 0)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [sessionIdForDisplay(3)],
            },
            title: "Later",
          },
        ],
        nextGroupNumber: 4,
        nextSessionDisplayId: 4,
        nextSessionNumber: 5,
      }),
      sessionIdForDisplay(2),
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.activeGroupId).toBe(DEFAULT_MAIN_GROUP_ID);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(1),
      sessionIdForDisplay(0),
    ]);
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(sessionIdForDisplay(0));
    expect(result.snapshot.groups[1]?.snapshot.sessions).toEqual([]);
  });

  test("should skip empty groups and switch to the next populated group", () => {
    const result = removeSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: "group-2",
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: undefined,
              fullscreenRestoreVisibleCount: undefined,
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
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(1, 0)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [sessionIdForDisplay(0)],
            },
            title: "Focused",
          },
          {
            groupId: "group-3",
            snapshot: {
              focusedSessionId: sessionIdForDisplay(1),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(2, 0), createSessionRecord(3, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(2), sessionIdForDisplay(1)],
            },
            title: "Next",
          },
        ],
        nextGroupNumber: 4,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      sessionIdForDisplay(0),
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.activeGroupId).toBe("group-3");
    expect(result.snapshot.groups[2]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(2),
      sessionIdForDisplay(1),
    ]);
    expect(result.snapshot.groups[2]?.snapshot.focusedSessionId).toBe(sessionIdForDisplay(1));
    expect(result.snapshot.groups[1]?.snapshot.sessions).toEqual([]);
  });

  test("should remove a closed tab without splitting the remaining tab group", () => {
    const result = removeSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  {
                    activeSessionId: sessionIdForDisplay(0),
                    kind: "tabs",
                    sessionIds: [
                      sessionIdForDisplay(0),
                      sessionIdForDisplay(1),
                      sessionIdForDisplay(2),
                    ],
                  },
                  { kind: "leaf", sessionId: sessionIdForDisplay(3) },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
                createSessionRecord(4, 3),
              ],
              viewMode: "grid",
              visibleCount: 4,
              visibleSessionIds: [
                sessionIdForDisplay(0),
                sessionIdForDisplay(1),
                sessionIdForDisplay(2),
                sessionIdForDisplay(3),
              ],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 4,
        nextSessionNumber: 5,
      }),
      sessionIdForDisplay(1),
    );

    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: sessionIdForDisplay(0),
          kind: "tabs",
          sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(2)],
        },
        { kind: "leaf", sessionId: sessionIdForDisplay(3) },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should keep three split panes stable when closing a tab in the third pane", () => {
    const firstPaneOwnerId = sessionIdForDisplay(3);
    const firstPaneSiblingId = sessionIdForDisplay(4);
    const middlePaneOwnerId = sessionIdForDisplay(0);
    const thirdPaneLeftId = sessionIdForDisplay(1);
    const closingThirdPaneTabId = sessionIdForDisplay(5);
    const thirdPaneRightId = sessionIdForDisplay(2);

    const result = removeSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: closingThirdPaneTabId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  {
                    activeSessionId: firstPaneOwnerId,
                    kind: "tabs",
                    sessionIds: [firstPaneOwnerId, firstPaneSiblingId],
                  },
                  {
                    children: [
                      { kind: "leaf", sessionId: middlePaneOwnerId },
                      {
                        activeSessionId: closingThirdPaneTabId,
                        kind: "tabs",
                        sessionIds: [thirdPaneLeftId, closingThirdPaneTabId, thirdPaneRightId],
                      },
                    ],
                    direction: "vertical",
                    kind: "split",
                  },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
                createSessionRecord(4, 3),
                createSessionRecord(5, 4),
                createSessionRecord(6, 5),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [middlePaneOwnerId, closingThirdPaneTabId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 6,
        nextSessionNumber: 7,
      }),
      closingThirdPaneTabId,
    );

    const materialized = ensureAllSessionsInFocusedPaneTabGroupInSimpleWorkspace(
      result.snapshot,
      DEFAULT_MAIN_GROUP_ID,
    );

    /*
     * CDXC:PaneTabs 2026-06-12-06:35:
     * Three-way split panes must remain stable when a tab opens or closes in the third pane.
     * Virtual-tab materialization may append true background sessions, but it must not relocate another live split pane's tab group into the focused pane just because legacy visibleSessionIds omitted that pane owner.
     */
    expect(materialized.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: firstPaneOwnerId,
          kind: "tabs",
          sessionIds: [firstPaneOwnerId, firstPaneSiblingId],
        },
        {
          children: [
            { kind: "leaf", sessionId: middlePaneOwnerId },
            {
              activeSessionId: thirdPaneRightId,
              kind: "tabs",
              sessionIds: [thirdPaneLeftId, thirdPaneRightId],
            },
          ],
          direction: "vertical",
          kind: "split",
        },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });
});

describe("syncSessionOrderInSimpleWorkspace", () => {
  test("should reorder sessions within the same group", () => {
    const result = syncSessionOrderInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      DEFAULT_MAIN_GROUP_ID,
      [sessionIdForDisplay(1), sessionIdForDisplay(0), sessionIdForDisplay(2)],
    );

    expect(result.changed).toBe(true);
    expect(
      result.snapshot.groups[0]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual([sessionIdForDisplay(1), sessionIdForDisplay(0), sessionIdForDisplay(2)]);
    expect(
      result.snapshot.groups[0]?.snapshot.sessions.map((session) => session.slotIndex),
    ).toEqual([0, 1, 2]);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(1),
      sessionIdForDisplay(0),
    ]);
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(sessionIdForDisplay(0));
  });

  test("should preserve every session when reordering a group with more than nine sessions", () => {
    const sessions = Array.from({ length: 10 }, (_, index) =>
      createSessionRecord(index + 1, index),
    );
    const result = syncSessionOrderInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(2),
              fullscreenRestoreVisibleCount: undefined,
              sessions,
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [sessionIdForDisplay(2)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 10,
        nextSessionNumber: 11,
      }),
      DEFAULT_MAIN_GROUP_ID,
      [
        sessionIdForDisplay(0),
        sessionIdForDisplay(2),
        sessionIdForDisplay(3),
        sessionIdForDisplay(1),
        sessionIdForDisplay(4),
        sessionIdForDisplay(5),
        sessionIdForDisplay(6),
        sessionIdForDisplay(7),
        sessionIdForDisplay(8),
        sessionIdForDisplay(9),
      ],
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.sessions).toHaveLength(10);
    expect(
      result.snapshot.groups[0]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual([
      sessionIdForDisplay(0),
      sessionIdForDisplay(2),
      sessionIdForDisplay(3),
      sessionIdForDisplay(1),
      sessionIdForDisplay(4),
      sessionIdForDisplay(5),
      sessionIdForDisplay(6),
      sessionIdForDisplay(7),
      sessionIdForDisplay(8),
      sessionIdForDisplay(9),
    ]);
  });

  test("should reorder browser panes with terminal panes", () => {
    const terminalSession = createSessionRecord(1, 0);
    const browserSession = createSessionRecord(2, 1, {
      browser: { url: "https://example.com" },
      kind: "browser",
      title: "example.com",
    });
    const result = syncSessionOrderInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: terminalSession.sessionId,
              fullscreenRestoreVisibleCount: undefined,
              sessions: [terminalSession, browserSession],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [terminalSession.sessionId, browserSession.sessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      DEFAULT_MAIN_GROUP_ID,
      [browserSession.sessionId, terminalSession.sessionId],
    );

    expect(result.changed).toBe(true);
    expect(
      result.snapshot.groups[0]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual([browserSession.sessionId, terminalSession.sessionId]);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      browserSession.sessionId,
      terminalSession.sessionId,
    ]);
  });
});

describe("swapVisibleSessionsInSimpleWorkspace", () => {
  test("should swap surfaced pane placement without surfacing hidden sessions", () => {
    const hiddenT3Session = createSessionRecord(1, 0, {
      kind: "t3",
      t3: {
        projectId: "project",
        serverOrigin: "http://127.0.0.1:3000",
        threadId: "thread-hidden",
        workspaceRoot: "/workspace",
      },
      title: "T3 Code",
    });
    const firstVisibleSession = createSessionRecord(2, 1);
    const secondVisibleSession = createSessionRecord(3, 2);
    const thirdVisibleSession = createSessionRecord(4, 3);
    const result = swapVisibleSessionsInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: firstVisibleSession.sessionId,
              fullscreenRestoreVisibleCount: undefined,
              sessions: [
                hiddenT3Session,
                firstVisibleSession,
                secondVisibleSession,
                thirdVisibleSession,
              ],
              viewMode: "grid",
              visibleCount: 3,
              visibleSessionIds: [
                firstVisibleSession.sessionId,
                secondVisibleSession.sessionId,
                thirdVisibleSession.sessionId,
              ],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 4,
        nextSessionNumber: 5,
      }),
      DEFAULT_MAIN_GROUP_ID,
      firstVisibleSession.sessionId,
      secondVisibleSession.sessionId,
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      secondVisibleSession.sessionId,
      firstVisibleSession.sessionId,
      thirdVisibleSession.sessionId,
    ]);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).not.toContain(
      hiddenT3Session.sessionId,
    );
    expect(
      result.snapshot.groups[0]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual([
      hiddenT3Session.sessionId,
      secondVisibleSession.sessionId,
      firstVisibleSession.sessionId,
      thirdVisibleSession.sessionId,
    ]);
  });

  test("should ignore drops involving hidden sessions", () => {
    const hiddenT3Session = createSessionRecord(1, 0, {
      kind: "t3",
      t3: {
        projectId: "project",
        serverOrigin: "http://127.0.0.1:3000",
        threadId: "thread-hidden",
        workspaceRoot: "/workspace",
      },
      title: "T3 Code",
    });
    const firstVisibleSession = createSessionRecord(2, 1);
    const secondVisibleSession = createSessionRecord(3, 2);
    const snapshot = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: firstVisibleSession.sessionId,
            fullscreenRestoreVisibleCount: undefined,
            sessions: [hiddenT3Session, firstVisibleSession, secondVisibleSession],
            viewMode: "grid",
            visibleCount: 2,
            visibleSessionIds: [firstVisibleSession.sessionId, secondVisibleSession.sessionId],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 3,
      nextSessionNumber: 4,
    });

    const result = swapVisibleSessionsInSimpleWorkspace(
      snapshot,
      DEFAULT_MAIN_GROUP_ID,
      firstVisibleSession.sessionId,
      hiddenT3Session.sessionId,
    );

    expect(result.changed).toBe(false);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      firstVisibleSession.sessionId,
      secondVisibleSession.sessionId,
    ]);
  });

  test("should not surface a hidden focused session while swapping visible panes", () => {
    const hiddenT3Session = createSessionRecord(1, 0, {
      kind: "t3",
      t3: {
        projectId: "project",
        serverOrigin: "http://127.0.0.1:3000",
        threadId: "thread-hidden",
        workspaceRoot: "/workspace",
      },
      title: "T3 Code",
    });
    const firstVisibleSession = createSessionRecord(2, 1);
    const secondVisibleSession = createSessionRecord(3, 2);
    const thirdVisibleSession = createSessionRecord(4, 3);
    const snapshot = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: firstVisibleSession.sessionId,
            fullscreenRestoreVisibleCount: undefined,
            sessions: [
              hiddenT3Session,
              firstVisibleSession,
              secondVisibleSession,
              thirdVisibleSession,
            ],
            viewMode: "grid",
            visibleCount: 3,
            visibleSessionIds: [
              firstVisibleSession.sessionId,
              secondVisibleSession.sessionId,
              thirdVisibleSession.sessionId,
            ],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 4,
      nextSessionNumber: 5,
    });
    const snapshotWithHiddenFocus = {
      ...snapshot,
      groups: snapshot.groups.map((group) =>
        group.groupId === DEFAULT_MAIN_GROUP_ID
          ? {
              ...group,
              snapshot: {
                ...group.snapshot,
                focusedSessionId: hiddenT3Session.sessionId,
              },
            }
          : group,
      ),
    };

    const result = swapVisibleSessionsInSimpleWorkspace(
      snapshotWithHiddenFocus,
      DEFAULT_MAIN_GROUP_ID,
      firstVisibleSession.sessionId,
      secondVisibleSession.sessionId,
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      secondVisibleSession.sessionId,
      firstVisibleSession.sessionId,
      thirdVisibleSession.sessionId,
    ]);
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(
      secondVisibleSession.sessionId,
    );
  });
});

describe("syncSessionOrderAcrossSimpleWorkspaceGroups", () => {
  test("should apply a flattened project order to each owning group", () => {
    const result = syncSessionOrderAcrossSimpleWorkspaceGroups(
      createWorkspaceSnapshot({
        activeGroupId: "group-1",
        groups: [
          {
            groupId: "group-1",
            snapshot: {
              focusedSessionId: "session-a",
              fullscreenRestoreVisibleCount: undefined,
              sessions: [
                createSessionRecord(1, 0, { displayId: "session-a", sessionId: "session-a" }),
                createSessionRecord(2, 1, { displayId: "session-b", sessionId: "session-b" }),
                createSessionRecord(3, 2, { displayId: "session-c", sessionId: "session-c" }),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: ["session-a", "session-b"],
            },
            title: "Main",
          },
          {
            groupId: "group-2",
            snapshot: {
              focusedSessionId: "session-d",
              fullscreenRestoreVisibleCount: undefined,
              sessions: [
                createSessionRecord(4, 0, { displayId: "session-d", sessionId: "session-d" }),
                createSessionRecord(5, 1, { displayId: "session-e", sessionId: "session-e" }),
              ],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: ["session-d"],
            },
            title: "Aux",
          },
        ],
        nextGroupNumber: 3,
        nextSessionDisplayId: 5,
        nextSessionNumber: 6,
      }),
      ["session-b", "session-a", "session-e", "session-d", "session-c"],
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.sessions.map((session) => session.sessionId)).toEqual(
      ["session-b", "session-a", "session-c"],
    );
    expect(result.snapshot.groups[1]?.snapshot.sessions.map((session) => session.sessionId)).toEqual(
      ["session-e", "session-d"],
    );
  });
});

describe("createSessionInSimpleWorkspace", () => {
  test("should preserve an explicit session id when daemon-created sessions are placed locally", () => {
    const result = createSessionInSimpleWorkspace(
      createDefaultGroupedSessionWorkspaceSnapshot(),
      {
        sessionId: "G1abc",
        title: "Daemon Session",
      },
    );

    expect(result.session?.sessionId).toBe("G1abc");
    expect(result.session?.displayId).toBe("00");
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe("G1abc");
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual(["G1abc"]);
  });

  test("should keep split mode and surface the new session when adding a session", () => {
    let snapshot = createDefaultGroupedSessionWorkspaceSnapshot();
    const firstResult = createSessionInSimpleWorkspace(snapshot);
    snapshot = setVisibleCountInSimpleWorkspace(firstResult.snapshot, 2);
    const secondResult = createSessionInSimpleWorkspace(snapshot);
    const firstSessionId = firstResult.session?.sessionId;
    const secondSessionId = secondResult.session?.sessionId;

    expect(firstSessionId).toMatch(/^g-\d{4}-\d{6}$/);
    expect(secondSessionId).toMatch(/^g-\d{4}-\d{6}$/);
    expect(secondResult.snapshot.groups[0]?.snapshot.visibleCount).toBe(2);
    expect(secondResult.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      firstSessionId,
      secondSessionId,
    ]);
    expect(secondResult.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(secondSessionId);
  });

  test("should add default-created sessions as tabs instead of split panes", () => {
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: sessionIdForDisplay(0) },
                  { kind: "leaf", sessionId: sessionIdForDisplay(1) },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 3,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
    );

    /**
     * CDXC:SplitIntent 2026-05-19-08:29:
     * Plain session creation has no split intent. It may surface and focus the
     * new session, but it must attach to the focused pane as a tab instead of
     * increasing split leaf count.
     */
    expect(countSplitLeafNodes(result.snapshot.groups[0]?.snapshot.paneLayout)).toBe(1);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: result.session?.sessionId,
          kind: "tabs",
          sessionIds: [sessionIdForDisplay(0), result.session?.sessionId],
        },
        { kind: "leaf", sessionId: sessionIdForDisplay(1) },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should seed missing pane layout as tabs when creation has no split intent", () => {
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(1),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 3,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
    );

    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      activeSessionId: result.session?.sessionId,
      kind: "tabs",
      sessionIds: [
        sessionIdForDisplay(0),
        sessionIdForDisplay(1),
        result.session?.sessionId,
      ],
    });
  });

  test("should use one timestamped opaque id for session id, display id, and alias", () => {
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay("02"),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [
                createSessionRecord(1, 0, { displayId: "00" }),
                createSessionRecord(2, 1, { displayId: "02" }),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay("00"), sessionIdForDisplay("02")],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 0,
        nextSessionNumber: 3,
      }),
    );

    expect(result.session?.sessionId).toMatch(/^g-\d{4}-\d{6}$/);
    expect(result.session?.displayId).toBe(result.session?.sessionId);
    expect(result.session?.alias).toBe(result.session?.sessionId);
  });

  test("should keep the current focus and visible slots when creating a background session", () => {
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay("00"),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [
                createSessionRecord(1, 0, { displayId: "00" }),
                createSessionRecord(2, 1, { displayId: "01" }),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay("00"), sessionIdForDisplay("01")],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      {
        initialPresentation: "background",
        title: "Build",
      },
    );

    expect(result.session?.sessionId).toMatch(/^g-\d{4}-\d{6}$/);
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(sessionIdForDisplay("00"));
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay("00"),
      sessionIdForDisplay("01"),
    ]);
  });

  test("should insert split-created sessions after the targeted visible pane with exact count", () => {
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(1),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
                createSessionRecord(4, 3),
              ],
              viewMode: "grid",
              visibleCount: 4,
              visibleSessionIds: [
                sessionIdForDisplay(0),
                sessionIdForDisplay(1),
                sessionIdForDisplay(2),
                sessionIdForDisplay(3),
              ],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 4,
        nextSessionNumber: 5,
      }),
      undefined,
      {
        /**
         * CDXC:NativeSplits 2026-05-10-18:30
         * Cmd+D/title-bar split creates a real session and surfaces it next to
         * the target pane, so four visible panes become exactly five panes.
         */
        visiblePlacement: { kind: "insertAfter", targetSessionId: sessionIdForDisplay(1) },
      },
    );

    expect(result.snapshot.groups[0]?.snapshot.visibleCount).toBe(5);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(0),
      sessionIdForDisplay(1),
      result.session?.sessionId,
      sessionIdForDisplay(2),
      sessionIdForDisplay(3),
    ]);
    expect(countSplitLeafNodes(result.snapshot.groups[0]?.snapshot.paneLayout)).toBe(1);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: sessionIdForDisplay(1),
          kind: "tabs",
          sessionIds: [
            sessionIdForDisplay(0),
            sessionIdForDisplay(1),
            sessionIdForDisplay(2),
            sessionIdForDisplay(3),
          ],
        },
        { kind: "leaf", sessionId: result.session?.sessionId },
      ],
      direction: "horizontal",
      kind: "split",
    });
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(result.session?.sessionId);
  });

  test("should preserve hidden tab members when split-creating beside a tab group", () => {
    const sleepingSession = {
      ...createSessionRecord(2, 1),
      isSleeping: true,
    };
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  {
                    activeSessionId: sessionIdForDisplay(0),
                    kind: "tabs",
                    sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
                  },
                  { kind: "leaf", sessionId: sessionIdForDisplay(2) },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [createSessionRecord(1, 0), sleepingSession, createSessionRecord(3, 2)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(2)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      undefined,
      {
        /**
         * CDXC:PaneTabs 2026-05-11-18:48
         * Split creation must preserve the entire target pane tab group,
         * including hidden/sleeping members. Losing those ids is what makes
         * native sync rebuild grouped tabs as separate one-tab panes.
         */
        visiblePlacement: { kind: "insertAfter", targetSessionId: sessionIdForDisplay(0) },
      },
    );

    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(0),
      result.session?.sessionId,
      sessionIdForDisplay(2),
    ]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: sessionIdForDisplay(0),
          kind: "tabs",
          sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
        },
        { kind: "leaf", sessionId: result.session?.sessionId },
        { kind: "leaf", sessionId: sessionIdForDisplay(2) },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should replace the targeted visible pane when replace placement targets it", () => {
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 3,
              visibleSessionIds: [
                sessionIdForDisplay(0),
                sessionIdForDisplay(1),
                sessionIdForDisplay(2),
              ],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      undefined,
      {
        visiblePlacement: { kind: "replace", targetSessionId: sessionIdForDisplay(1) },
      },
    );

    expect(result.snapshot.groups[0]?.snapshot.visibleCount).toBe(3);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(0),
      result.session?.sessionId,
      sessionIdForDisplay(2),
    ]);
  });

  test("should preserve hidden tab members when replacing a visible pane", () => {
    const sleepingSession = {
      ...createSessionRecord(2, 1),
      isSleeping: true,
    };
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  {
                    activeSessionId: sessionIdForDisplay(0),
                    kind: "tabs",
                    sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
                  },
                  { kind: "leaf", sessionId: sessionIdForDisplay(2) },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [createSessionRecord(1, 0), sleepingSession, createSessionRecord(3, 2)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(2)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      undefined,
      {
        /**
         * CDXC:PaneTabs 2026-05-11-18:48
         * Replacement creation swaps only the targeted pane member. Other tab
         * members stay parked in the same paneLayout tabs node.
         */
        visiblePlacement: { kind: "replace", targetSessionId: sessionIdForDisplay(0) },
      },
    );

    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      result.session?.sessionId,
      sessionIdForDisplay(2),
    ]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: result.session?.sessionId,
          kind: "tabs",
          sessionIds: [result.session?.sessionId, sessionIdForDisplay(1)],
        },
        { kind: "leaf", sessionId: sessionIdForDisplay(2) },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should append a title-bar terminal action to the clicked pane tab group", () => {
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(1),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  {
                    activeSessionId: sessionIdForDisplay(1),
                    kind: "tabs",
                    sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
                  },
                  { kind: "leaf", sessionId: sessionIdForDisplay(2) },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 3,
              visibleSessionIds: [
                sessionIdForDisplay(0),
                sessionIdForDisplay(1),
                sessionIdForDisplay(2),
              ],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      undefined,
      {
        /**
         * CDXC:PaneTabs 2026-05-11-16:16
         * Title-bar New Terminal must add a selected tab to the clicked pane's
         * existing tab group. The target session remains in that group instead
         * of being replaced and later reappearing as a separate split.
         */
        visiblePlacement: { kind: "appendToTabGroup", targetSessionId: sessionIdForDisplay(1) },
      },
    );

    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(0),
      sessionIdForDisplay(1),
      result.session?.sessionId,
      sessionIdForDisplay(2),
    ]);
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(result.session?.sessionId);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: result.session?.sessionId,
          kind: "tabs",
          sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1), result.session?.sessionId],
        },
        { kind: "leaf", sessionId: sessionIdForDisplay(2) },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should preserve hidden tab members when appending to a tab group", () => {
    const sleepingSession = {
      ...createSessionRecord(2, 1),
      isSleeping: true,
    };
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  {
                    activeSessionId: sessionIdForDisplay(0),
                    kind: "tabs",
                    sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
                  },
                  { kind: "leaf", sessionId: sessionIdForDisplay(2) },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [createSessionRecord(1, 0), sleepingSession, createSessionRecord(3, 2)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(2)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      undefined,
      {
        /**
         * CDXC:PaneTabs 2026-05-11-18:48
         * Appending a terminal/browser tab to a pane keeps every existing tab
         * member in that pane, even when a member is sleeping and absent from
         * visibleSessionIds.
         */
        visiblePlacement: { kind: "appendToTabGroup", targetSessionId: sessionIdForDisplay(0) },
      },
    );

    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: result.session?.sessionId,
          kind: "tabs",
          sessionIds: [
            sessionIdForDisplay(0),
            sessionIdForDisplay(1),
            result.session?.sessionId,
          ],
        },
        { kind: "leaf", sessionId: sessionIdForDisplay(2) },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should insert a new focused tab immediately after the target tab when requested", () => {
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                activeSessionId: sessionIdForDisplay(0),
                kind: "tabs",
                sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
              },
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      undefined,
      {
        /**
         * CDXC:PaneTabs 2026-06-06-04:36:
         * Cmd+T and Cmd+N create the new terminal/browser tab immediately after the focused tab in the current split pane, so keyboard-created tabs appear beside the user's current work instead of at the end of a long tab group.
         */
        visiblePlacement: {
          kind: "appendToTabGroup",
          position: "after",
          targetSessionId: sessionIdForDisplay(0),
        },
      },
    );

    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      activeSessionId: result.session?.sessionId,
      kind: "tabs",
      sessionIds: [
        sessionIdForDisplay(0),
        result.session?.sessionId,
        sessionIdForDisplay(1),
      ],
    });
  });

  test("should preserve split ratios when adding a terminal to the focused tab group", () => {
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(1),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  {
                    activeSessionId: sessionIdForDisplay(1),
                    kind: "tabs",
                    sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
                  },
                  { kind: "leaf", sessionId: sessionIdForDisplay(2) },
                ],
                direction: "vertical",
                kind: "split",
                ratio: 0.7,
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 3,
              visibleSessionIds: [
                sessionIdForDisplay(0),
                sessionIdForDisplay(1),
                sessionIdForDisplay(2),
              ],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      undefined,
      {
        /**
         * CDXC:PaneTabs 2026-05-11-11:51
         * Sidebar double-click and project-header terminal creation must add a
         * focused tab to the active session's tab group without changing the
         * surrounding split tree or user-adjusted pane ratio.
         */
        visiblePlacement: { kind: "appendToTabGroup", targetSessionId: sessionIdForDisplay(1) },
      },
    );

    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(result.session?.sessionId);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: result.session?.sessionId,
          kind: "tabs",
          sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1), result.session?.sessionId],
        },
        { kind: "leaf", sessionId: sessionIdForDisplay(2) },
      ],
      direction: "vertical",
      kind: "split",
      ratio: 0.7,
    });
  });

  test("should preserve tab groups when creating a terminal without explicit placement", () => {
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(1),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  {
                    activeSessionId: sessionIdForDisplay(1),
                    kind: "tabs",
                    sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
                  },
                  { kind: "leaf", sessionId: sessionIdForDisplay(2) },
                ],
                direction: "vertical",
                kind: "split",
                ratio: 0.7,
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 3,
              visibleSessionIds: [
                sessionIdForDisplay(0),
                sessionIdForDisplay(1),
                sessionIdForDisplay(2),
              ],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
    );

    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: result.session?.sessionId,
          kind: "tabs",
          sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1), result.session?.sessionId],
        },
        { kind: "leaf", sessionId: sessionIdForDisplay(2) },
      ],
      direction: "vertical",
      kind: "split",
      ratio: 0.7,
    });
  });

  test("should keep the tenth title-bar terminal action inside the clicked pane tab group", () => {
    const existingSessionRecords = Array.from({ length: 9 }, (_, index) =>
      createSessionRecord(index + 1, index),
    );
    const existingSessionIds = existingSessionRecords.map((session) => session.sessionId);
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(8),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                activeSessionId: sessionIdForDisplay(8),
                kind: "tabs",
                sessionIds: existingSessionIds,
              },
              sessions: existingSessionRecords,
              viewMode: "grid",
              visibleCount: 9,
              visibleSessionIds: existingSessionIds,
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 9,
        nextSessionNumber: 10,
      }),
      undefined,
      {
        /**
         * CDXC:PaneTabs 2026-05-11-17:04
         * The old fixed workspace pane cap is gone. Adding the tenth title-bar tab
         * must preserve all tab ids in paneLayout and visibleSessionIds so
         * native sync does not surface any trimmed id as a new split.
         */
        visiblePlacement: { kind: "appendToTabGroup", targetSessionId: sessionIdForDisplay(8) },
      },
    );

    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      ...existingSessionIds,
      result.session?.sessionId,
    ]);
    expect(result.snapshot.groups[0]?.snapshot.visibleCount).toBe(10);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      activeSessionId: result.session?.sessionId,
      kind: "tabs",
      sessionIds: [...existingSessionIds, result.session?.sessionId],
    });
  });

  test("should convert a single clicked pane into a tab group when adding a browser", () => {
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: { kind: "leaf", sessionId: sessionIdForDisplay(0) },
              sessions: [createSessionRecord(1, 0)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [sessionIdForDisplay(0)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 1,
        nextSessionNumber: 2,
      }),
      {
        browser: { url: "https://example.com" },
        kind: "browser",
        title: "Browser",
      },
      {
        /**
         * CDXC:PaneTabs 2026-05-11-16:16
         * Title-bar Open Browser uses the same clicked-pane tab placement as
         * New Terminal. A one-session pane becomes a two-tab pane and the new
         * browser tab becomes active.
         */
        visiblePlacement: { kind: "appendToTabGroup", targetSessionId: sessionIdForDisplay(0) },
      },
    );

    expect(result.session).toEqual(expect.objectContaining({ kind: "browser" }));
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(0),
      result.session?.sessionId,
    ]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      activeSessionId: result.session?.sessionId,
      kind: "tabs",
      sessionIds: [sessionIdForDisplay(0), result.session?.sessionId],
    });
  });

  test("should preserve the previously focused pane when global new session replaces a visible pane", () => {
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      undefined,
      {
        visiblePlacement: {
          kind: "replaceNonFocused",
          preserveSessionId: sessionIdForDisplay(0),
        },
      },
    );

    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(0),
      result.session?.sessionId,
    ]);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).not.toContain(
      sessionIdForDisplay(1),
    );
  });

  test("should persist a vertical split tree when Cmd+Shift+D creates a pane", () => {
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: sessionIdForDisplay(0) },
                  { kind: "leaf", sessionId: sessionIdForDisplay(1) },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      undefined,
      {
        visiblePlacement: {
          kind: "insertAfter",
          splitDirection: "vertical",
          targetSessionId: sessionIdForDisplay(0),
        },
      },
    );

    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          children: [
            { kind: "leaf", sessionId: sessionIdForDisplay(0) },
            { kind: "leaf", sessionId: result.session?.sessionId },
          ],
          direction: "vertical",
          kind: "split",
        },
        { kind: "leaf", sessionId: sessionIdForDisplay(1) },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should append full-width terminal panes as a bottom row", () => {
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: sessionIdForDisplay(0) },
                  { kind: "leaf", sessionId: sessionIdForDisplay(1) },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      undefined,
      {
        /**
         * CDXC:WorkspacePanes 2026-05-11-02:51
         * The Settings-row secondary terminal action appends a full-width row,
         * not a split inside the currently focused pane.
         */
        visiblePlacement: { kind: "appendFullWidth" },
      },
    );

    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(0),
      sessionIdForDisplay(1),
      result.session?.sessionId,
    ]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          children: [
            { kind: "leaf", sessionId: sessionIdForDisplay(0) },
            { kind: "leaf", sessionId: sessionIdForDisplay(1) },
          ],
          direction: "horizontal",
          kind: "split",
        },
        { kind: "leaf", sessionId: result.session?.sessionId },
      ],
      direction: "vertical",
      kind: "split",
      ratio: 0.85,
    });
  });

  test("should group a dragged pane into the target pane tab group", () => {
    const workspace = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: sessionIdForDisplay(0),
            fullscreenRestoreVisibleCount: undefined,
            paneLayout: {
              children: [
                { kind: "leaf", sessionId: sessionIdForDisplay(0) },
                { kind: "leaf", sessionId: sessionIdForDisplay(1) },
              ],
              direction: "horizontal",
              kind: "split",
            },
            sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
            viewMode: "grid",
            visibleCount: 2,
            visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 2,
      nextSessionNumber: 3,
    });

    const result = moveSessionInPaneLayoutInSimpleWorkspace(
      workspace,
      DEFAULT_MAIN_GROUP_ID,
      sessionIdForDisplay(1),
      sessionIdForDisplay(0),
      "center",
    );

    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      activeSessionId: sessionIdForDisplay(1),
      kind: "tabs",
      sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
    });
  });

  test("should allow dragging an awake tab that was appended outside visibleSessionIds", () => {
    const workspace = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: sessionIdForDisplay(0),
            fullscreenRestoreVisibleCount: undefined,
            paneLayout: { kind: "leaf", sessionId: sessionIdForDisplay(0) },
            sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
            viewMode: "grid",
            visibleCount: 1,
            visibleSessionIds: [sessionIdForDisplay(0)],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 2,
      nextSessionNumber: 3,
    });

    const result = moveSessionInPaneLayoutInSimpleWorkspace(
      workspace,
      DEFAULT_MAIN_GROUP_ID,
      sessionIdForDisplay(1),
      sessionIdForDisplay(0),
      "center",
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(0),
      sessionIdForDisplay(1),
    ]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      activeSessionId: sessionIdForDisplay(1),
      kind: "tabs",
      sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
    });
  });

  test("should wake a sleeping tab when a committed pane drop splits it beside another pane", () => {
    const sleepingSession = {
      ...createSessionRecord(2, 1),
      isSleeping: true,
    };
    const workspace = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: sessionIdForDisplay(0),
            fullscreenRestoreVisibleCount: undefined,
            paneLayout: {
              children: [
                {
                  activeSessionId: sessionIdForDisplay(0),
                  kind: "tabs",
                  sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
                },
                { kind: "leaf", sessionId: sessionIdForDisplay(2) },
              ],
              direction: "horizontal",
              kind: "split",
            },
            sessions: [createSessionRecord(1, 0), sleepingSession, createSessionRecord(3, 2)],
            viewMode: "grid",
            visibleCount: 2,
            visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(2)],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 3,
      nextSessionNumber: 4,
    });

    const result = moveSessionInPaneLayoutInSimpleWorkspace(
      workspace,
      DEFAULT_MAIN_GROUP_ID,
      sessionIdForDisplay(1),
      sessionIdForDisplay(2),
      "right",
      { wakeSourceSession: true },
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.sessions[1]?.isSleeping).toBe(false);
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(sessionIdForDisplay(1));
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(0),
      sessionIdForDisplay(2),
      sessionIdForDisplay(1),
    ]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        { kind: "leaf", sessionId: sessionIdForDisplay(0) },
        { kind: "leaf", sessionId: sessionIdForDisplay(2) },
        { kind: "leaf", sessionId: sessionIdForDisplay(1) },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should select the active session in an existing tab group", () => {
    const workspace = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: sessionIdForDisplay(0),
            fullscreenRestoreVisibleCount: undefined,
            paneLayout: {
              activeSessionId: sessionIdForDisplay(0),
              kind: "tabs",
              sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
            viewMode: "grid",
            visibleCount: 2,
            visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 2,
      nextSessionNumber: 3,
    });

    const result = selectPaneTabInSimpleWorkspace(
      workspace,
      DEFAULT_MAIN_GROUP_ID,
      sessionIdForDisplay(1),
    );

    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(sessionIdForDisplay(1));
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      activeSessionId: sessionIdForDisplay(1),
      kind: "tabs",
      sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
    });
  });

  test("should select a hidden sibling tab while staying in focus mode", () => {
    const focusedSessionId = sessionIdForDisplay(0);
    const siblingSessionId = sessionIdForDisplay(1);
    const workspace = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId,
            fullscreenRestoreVisibleCount: 4,
            paneLayout: {
              activeSessionId: focusedSessionId,
              kind: "tabs",
              sessionIds: [focusedSessionId, siblingSessionId],
            },
            sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
            viewMode: "grid",
            visibleCount: 1,
            visibleSessionIds: [focusedSessionId],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 2,
      nextSessionNumber: 3,
    });

    const result = selectPaneTabInSimpleWorkspace(
      workspace,
      DEFAULT_MAIN_GROUP_ID,
      siblingSessionId,
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(siblingSessionId);
    expect(result.snapshot.groups[0]?.snapshot.fullscreenRestoreVisibleCount).toBe(4);
    expect(result.snapshot.groups[0]?.snapshot.visibleCount).toBe(1);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([siblingSessionId]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      activeSessionId: siblingSessionId,
      kind: "tabs",
      sessionIds: [focusedSessionId, siblingSessionId],
    });
  });

  test("should reorder sessions inside an existing pane tab group", () => {
    const workspace = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: sessionIdForDisplay(1),
            fullscreenRestoreVisibleCount: undefined,
            paneLayout: {
              activeSessionId: sessionIdForDisplay(1),
              kind: "tabs",
              sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1), sessionIdForDisplay(2)],
            },
            sessions: [
              createSessionRecord(1, 0),
              createSessionRecord(2, 1),
              createSessionRecord(3, 2),
            ],
            viewMode: "grid",
            visibleCount: 3,
            visibleSessionIds: [
              sessionIdForDisplay(0),
              sessionIdForDisplay(1),
              sessionIdForDisplay(2),
            ],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 3,
      nextSessionNumber: 4,
    });

    const result = reorderSessionInPaneTabGroupInSimpleWorkspace(
      workspace,
      DEFAULT_MAIN_GROUP_ID,
      sessionIdForDisplay(2),
      sessionIdForDisplay(0),
      "before",
    );

    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      activeSessionId: sessionIdForDisplay(1),
      kind: "tabs",
      sessionIds: [sessionIdForDisplay(2), sessionIdForDisplay(0), sessionIdForDisplay(1)],
    });
  });

  test("should split the active tab out of its own multi-tab pane", () => {
    const workspace = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: sessionIdForDisplay(1),
            fullscreenRestoreVisibleCount: undefined,
            paneLayout: {
              activeSessionId: sessionIdForDisplay(1),
              kind: "tabs",
              sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1), sessionIdForDisplay(2)],
            },
            sessions: [
              createSessionRecord(1, 0),
              createSessionRecord(2, 1),
              createSessionRecord(3, 2),
            ],
            viewMode: "grid",
            visibleCount: 3,
            visibleSessionIds: [
              sessionIdForDisplay(0),
              sessionIdForDisplay(1),
              sessionIdForDisplay(2),
            ],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 3,
      nextSessionNumber: 4,
    });

    const result = moveSessionInPaneLayoutInSimpleWorkspace(
      workspace,
      DEFAULT_MAIN_GROUP_ID,
      sessionIdForDisplay(1),
      sessionIdForDisplay(1),
      "right",
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(0),
      sessionIdForDisplay(2),
      sessionIdForDisplay(1),
    ]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: sessionIdForDisplay(0),
          kind: "tabs",
          sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(2)],
        },
        { kind: "leaf", sessionId: sessionIdForDisplay(1) },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should not split a single-tab pane onto itself", () => {
    const workspace = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: sessionIdForDisplay(0),
            fullscreenRestoreVisibleCount: undefined,
            paneLayout: { kind: "leaf", sessionId: sessionIdForDisplay(0) },
            sessions: [createSessionRecord(1, 0)],
            viewMode: "grid",
            visibleCount: 1,
            visibleSessionIds: [sessionIdForDisplay(0)],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 1,
      nextSessionNumber: 2,
    });

    const result = moveSessionInPaneLayoutInSimpleWorkspace(
      workspace,
      DEFAULT_MAIN_GROUP_ID,
      sessionIdForDisplay(0),
      sessionIdForDisplay(0),
      "right",
    );

    expect(result.changed).toBe(false);
    expect(result.snapshot).toBe(workspace);
  });

  test("should preserve hidden tab members when appending a full-width pane", () => {
    const sleepingSession = {
      ...createSessionRecord(2, 1),
      isSleeping: true,
    };
    const result = createSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  {
                    activeSessionId: sessionIdForDisplay(0),
                    kind: "tabs",
                    sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
                  },
                  { kind: "leaf", sessionId: sessionIdForDisplay(2) },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [createSessionRecord(1, 0), sleepingSession, createSessionRecord(3, 2)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(2)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      undefined,
      {
        /**
         * CDXC:WorkspacePanes 2026-05-11-18:48
         * Full-width pane creation wraps the current pane tree. It must preserve
         * hidden members in existing tab groups before adding the bottom row.
         */
        visiblePlacement: { kind: "appendFullWidth" },
      },
    );

    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          children: [
            {
              activeSessionId: sessionIdForDisplay(0),
              kind: "tabs",
              sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            { kind: "leaf", sessionId: sessionIdForDisplay(2) },
          ],
          direction: "horizontal",
          kind: "split",
        },
        { kind: "leaf", sessionId: result.session?.sessionId },
      ],
      direction: "vertical",
      kind: "split",
      ratio: 0.85,
    });
  });
});

describe("setT3SessionMetadataInSimpleWorkspace", () => {
  test("should update the stored T3 metadata without changing the session identity", () => {
    const placeholderSession = createSessionRecord(1, 0, {
      kind: "t3",
      t3: {
        projectId: "pending-project",
        serverOrigin: "http://127.0.0.1:3773",
        threadId: "pending-thread",
        workspaceRoot: "/tmp/project",
      },
      title: "T3 Code",
    });
    const snapshot = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: placeholderSession.sessionId,
            fullscreenRestoreVisibleCount: undefined,
            sessions: [placeholderSession],
            viewMode: "grid",
            visibleCount: 1,
            visibleSessionIds: [placeholderSession.sessionId],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 1,
      nextSessionNumber: 2,
    });
    const normalizedSessionId = snapshot.groups[0]?.snapshot.sessions[0]?.sessionId;

    const result = setT3SessionMetadataInSimpleWorkspace(snapshot, normalizedSessionId ?? "", {
      projectId: "project-123",
      serverOrigin: "http://127.0.0.1:3773",
      threadId: "thread-456",
      workspaceRoot: "/tmp/project",
    });

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.sessions[0]).toEqual(
      expect.objectContaining({
        sessionId: normalizedSessionId,
        t3: {
          boundThreadId: "thread-456",
          projectId: "project-123",
          serverOrigin: "http://127.0.0.1:3773",
          threadId: "thread-456",
          workspaceRoot: "/tmp/project",
        },
      }),
    );
  });
});

describe("createGroupFromSessionInSimpleWorkspace", () => {
  test("should move the dragged session into a new active group", () => {
    const result = createGroupFromSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      sessionIdForDisplay(1),
    );

    expect(result.groupId).toBe("group-2");
    expect(result.snapshot.activeGroupId).toBe("group-2");
    expect(result.snapshot.groups).toHaveLength(2);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([sessionIdForDisplay(0)]);
    expect(
      result.snapshot.groups[0]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual([sessionIdForDisplay(0)]);
    expect(result.snapshot.groups[1]?.snapshot.visibleSessionIds).toEqual([sessionIdForDisplay(1)]);
    expect(
      result.snapshot.groups[1]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual([sessionIdForDisplay(1)]);
  });

  test("should remove the canonicalized dragged session from the source group", () => {
    const draggedSession = {
      ...createSessionRecord(5, 1, { displayId: "04" }),
      sessionId: sessionIdForDisplay("00"),
    };
    const result = createGroupFromSessionInSimpleWorkspace(
      {
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay("04"),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(4, 0, { displayId: "03" }), draggedSession],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay("03"), sessionIdForDisplay("04")],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 5,
        nextSessionNumber: 6,
      },
      sessionIdForDisplay("04"),
    );

    expect(result.groupId).toBe("group-2");
    expect(result.snapshot.groups).toHaveLength(2);
    expect(
      result.snapshot.groups[0]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual([sessionIdForDisplay("03")]);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay("03"),
    ]);
    expect(
      result.snapshot.groups[1]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual([sessionIdForDisplay("04")]);
    expect(result.snapshot.groups[1]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay("04"),
    ]);
  });
});

describe("moveSessionToGroupInSimpleWorkspace", () => {
  test("should reorder within the same group while keeping the moved session focused and visible", () => {
    const result = moveSessionToGroupInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(2),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(1), sessionIdForDisplay(2)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      sessionIdForDisplay(2),
      DEFAULT_MAIN_GROUP_ID,
      1,
    );

    expect(result.changed).toBe(true);
    expect(
      result.snapshot.groups[0]?.snapshot.sessions.map((session) => session.sessionId),
    ).toEqual([sessionIdForDisplay(0), sessionIdForDisplay(2), sessionIdForDisplay(1)]);
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(sessionIdForDisplay(2));
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      sessionIdForDisplay(1),
      sessionIdForDisplay(2),
    ]);
  });
});

describe("createGroupInSimpleWorkspace", () => {
  test("should append an empty active group", () => {
    const result = createGroupInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(1, 0)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [sessionIdForDisplay(0)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 1,
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

describe("setSessionSleepingInSimpleWorkspace", () => {
  test("should switch focus to another awake session in the same group", () => {
    const result = setSessionSleepingInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(1),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      sessionIdForDisplay(1),
      true,
    );

    expect(result.snapshot.groups[0]?.snapshot.sessions[1]?.isSleeping).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(sessionIdForDisplay(0));
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([sessionIdForDisplay(0)]);
  });

  test("should preserve pane layout position while sleeping and wake into focused tab group", () => {
    const sleepingSessionId = sessionIdForDisplay(1);
    const awakeSessionId = sessionIdForDisplay(0);
    const slept = setSessionSleepingInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sleepingSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: awakeSessionId },
                  { kind: "leaf", sessionId: sleepingSessionId },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [awakeSessionId, sleepingSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      sleepingSessionId,
      true,
    );

    expect(slept.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([awakeSessionId]);
    expect(slept.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        { kind: "leaf", sessionId: awakeSessionId },
        { kind: "leaf", sessionId: sleepingSessionId },
      ],
      direction: "horizontal",
      kind: "split",
    });

    const woke = setSessionSleepingInSimpleWorkspace(slept.snapshot, sleepingSessionId, false);
    expect(woke.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(sleepingSessionId);
    expect(woke.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      awakeSessionId,
      sleepingSessionId,
    ]);
    /**
     * CDXC:SessionSleep 2026-05-18-15:47:
     * Direct wake uses the same pane-tab restore rule as session focus. A
     * sleeping split leaf becomes a tab in the currently focused pane instead
     * of reappearing as a separate split when Agents mode is surfaced later.
     */
    expect(woke.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      activeSessionId: sleepingSessionId,
      kind: "tabs",
      sessionIds: [awakeSessionId, sleepingSessionId],
    });
  });

  test("should preserve focus and active tab when a background session sleeps", () => {
    const focusedSessionId = sessionIdForDisplay(2);
    const backgroundSessionId = sessionIdForDisplay(0);
    const otherAwakeSessionId = sessionIdForDisplay(1);
    const slept = setSessionSleepingInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                activeSessionId: focusedSessionId,
                kind: "tabs",
                sessionIds: [backgroundSessionId, otherAwakeSessionId, focusedSessionId],
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 3,
              visibleSessionIds: [backgroundSessionId, otherAwakeSessionId, focusedSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      backgroundSessionId,
      true,
    );

    /*
     * CDXC:SessionSleep 2026-06-06-22:52:
     * Auto-sleeping a background pane tab must park only that tab. The focused
     * tab remains the active pane owner so native/sidebar sync does not redirect
     * keyboard input to the next project-list session.
     */
    expect(slept.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(focusedSessionId);
    expect(slept.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      otherAwakeSessionId,
      focusedSessionId,
    ]);
    expect(slept.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      activeSessionId: focusedSessionId,
      kind: "tabs",
      sessionIds: [backgroundSessionId, otherAwakeSessionId, focusedSessionId],
    });
  });

  test("should restore a sleeping session into the focused pane tab group instead of its old pane", () => {
    const sleepingSession = {
      ...createSessionRecord(2, 1),
      isSleeping: true,
    };
    const awakeSessionId = sessionIdForDisplay(0);
    const sleepingSessionId = sessionIdForDisplay(1);
    const result = focusSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: awakeSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: awakeSessionId },
                  { kind: "leaf", sessionId: sleepingSessionId },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [createSessionRecord(1, 0), sleepingSession],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [awakeSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      sleepingSessionId,
    );

    expect(result.snapshot.groups[0]?.snapshot.sessions[1]?.isSleeping).toBe(false);
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(sleepingSessionId);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      awakeSessionId,
      sleepingSessionId,
    ]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      activeSessionId: sleepingSessionId,
      kind: "tabs",
      sessionIds: [awakeSessionId, sleepingSessionId],
    });
  });

  test("should wake a sleeping session into the currently active split pane tab group", () => {
    const leftSessionId = sessionIdForDisplay(0);
    const activeSessionId = sessionIdForDisplay(1);
    const sleepingSessionId = sessionIdForDisplay(2);
    const activeSiblingSessionId = sessionIdForDisplay(3);
    const sleepingSession = {
      ...createSessionRecord(3, 2),
      isSleeping: true,
    };

    const result = focusSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: activeSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: leftSessionId },
                  {
                    activeSessionId,
                    kind: "tabs",
                    sessionIds: [activeSessionId, activeSiblingSessionId],
                  },
                  { kind: "leaf", sessionId: sleepingSessionId },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                sleepingSession,
                createSessionRecord(4, 3),
              ],
              viewMode: "grid",
              visibleCount: 3,
              visibleSessionIds: [leftSessionId, activeSessionId, activeSiblingSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 4,
        nextSessionNumber: 5,
      }),
      sleepingSessionId,
    );

    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(sleepingSessionId);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      leftSessionId,
      activeSessionId,
      sleepingSessionId,
      activeSiblingSessionId,
    ]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        { kind: "leaf", sessionId: leftSessionId },
        {
          activeSessionId: sleepingSessionId,
          kind: "tabs",
          sessionIds: [activeSessionId, activeSiblingSessionId, sleepingSessionId],
        },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should wake a sleeping native pane tab in its existing split tab group", () => {
    const leftSessionId = sessionIdForDisplay(0);
    const focusedLeftSessionId = sessionIdForDisplay(1);
    const sleepingRightSessionId = sessionIdForDisplay(2);
    const activeRightSessionId = sessionIdForDisplay(3);
    const woke = wakePaneTabSessionInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: focusedLeftSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  {
                    activeSessionId: focusedLeftSessionId,
                    kind: "tabs",
                    sessionIds: [leftSessionId, focusedLeftSessionId],
                  },
                  {
                    activeSessionId: activeRightSessionId,
                    kind: "tabs",
                    sessionIds: [sleepingRightSessionId, activeRightSessionId],
                  },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                { ...createSessionRecord(3, 2), isSleeping: true },
                createSessionRecord(4, 3),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [focusedLeftSessionId, activeRightSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 4,
        nextSessionNumber: 5,
      }),
      DEFAULT_MAIN_GROUP_ID,
      sleepingRightSessionId,
    );

    const selected = selectPaneTabInSimpleWorkspace(
      woke.snapshot,
      DEFAULT_MAIN_GROUP_ID,
      sleepingRightSessionId,
    );

    expect(selected.snapshot.groups[0]?.snapshot.sessions[2]?.isSleeping).toBe(false);
    expect(selected.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(sleepingRightSessionId);
    expect(selected.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      focusedLeftSessionId,
      activeRightSessionId,
      sleepingRightSessionId,
    ]);
    /*
     * CDXC:PaneTabs 2026-05-23-09:08:
     * Native pane-tab wake must select the clicked tab where it already lives.
     * The right split stays a right split instead of moving the restored tab
     * into the previously focused left group.
     */
    expect(selected.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: focusedLeftSessionId,
          kind: "tabs",
          sessionIds: [leftSessionId, focusedLeftSessionId],
        },
        {
          activeSessionId: sleepingRightSessionId,
          kind: "tabs",
          sessionIds: [sleepingRightSessionId, activeRightSessionId],
        },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should select a sleeping native pane tab without waking it", () => {
    const leftSessionId = sessionIdForDisplay(0);
    const focusedLeftSessionId = sessionIdForDisplay(1);
    const sleepingRightSessionId = sessionIdForDisplay(2);
    const activeRightSessionId = sessionIdForDisplay(3);

    const selected = selectPaneTabInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: focusedLeftSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  {
                    activeSessionId: focusedLeftSessionId,
                    kind: "tabs",
                    sessionIds: [leftSessionId, focusedLeftSessionId],
                  },
                  {
                    activeSessionId: activeRightSessionId,
                    kind: "tabs",
                    sessionIds: [sleepingRightSessionId, activeRightSessionId],
                  },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                { ...createSessionRecord(3, 2), isSleeping: true },
                createSessionRecord(4, 3),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [focusedLeftSessionId, activeRightSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 4,
        nextSessionNumber: 5,
      }),
      DEFAULT_MAIN_GROUP_ID,
      sleepingRightSessionId,
    );

    /*
     * CDXC:SleepingPanePlaceholders 2026-06-13-01:44:
     * Click-to-wake tab selection is a layout operation, not a wake operation.
     * The selected sleeping tab stays cold while its original right split
     * remains selected in paneLayout so native can render a black placeholder
     * in that slot. Keep runtime focus and visibleSessionIds on awake sessions
     * until the user clicks the placeholder body to wake the terminal.
     */
    expect(selected.snapshot.groups[0]?.snapshot.sessions[2]?.isSleeping).toBe(true);
    expect(selected.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(focusedLeftSessionId);
    expect(selected.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      focusedLeftSessionId,
      activeRightSessionId,
    ]);
    expect(selected.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: focusedLeftSessionId,
          kind: "tabs",
          sessionIds: [leftSessionId, focusedLeftSessionId],
        },
        {
          activeSessionId: sleepingRightSessionId,
          kind: "tabs",
          sessionIds: [sleepingRightSessionId, activeRightSessionId],
        },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should materialize virtual tabs in the focused split tab group", () => {
    const leftSessionId = sessionIdForDisplay(0);
    const focusedSessionId = sessionIdForDisplay(1);
    const focusedSiblingSessionId = sessionIdForDisplay(2);
    const sleepingSessionId = sessionIdForDisplay(3);
    const missingProviderSessionId = sessionIdForDisplay(4);

    const result = ensureAllSessionsInFocusedPaneTabGroupInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: leftSessionId },
                  {
                    activeSessionId: focusedSessionId,
                    kind: "tabs",
                    sessionIds: [focusedSessionId, focusedSiblingSessionId],
                  },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
                { ...createSessionRecord(4, 3), isSleeping: true },
                { ...createSessionRecord(5, 4), isSleeping: true },
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [leftSessionId, focusedSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 5,
        nextSessionNumber: 6,
      }),
      DEFAULT_MAIN_GROUP_ID,
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      leftSessionId,
      focusedSessionId,
    ]);
    /**
     * CDXC:PaneTabs 2026-05-29-09:04:
     * Virtual native tabs for sleeping, unmounted, or missing-provider sessions
     * belong to the focused pane group so AppKit tab chrome and persisted
     * paneLayout agree before the user clicks or drags one of those tabs.
     */
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        { kind: "leaf", sessionId: leftSessionId },
        {
          activeSessionId: focusedSessionId,
          kind: "tabs",
          sessionIds: [
            focusedSessionId,
            focusedSiblingSessionId,
            sleepingSessionId,
            missingProviderSessionId,
          ],
        },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should keep passive virtual tab materialization from merging split panes", () => {
    const leftOwnerSessionId = sessionIdForDisplay(0);
    const leftSiblingSessionId = sessionIdForDisplay(1);
    const rightOwnerSessionId = sessionIdForDisplay(2);
    const rightSiblingSessionId = sessionIdForDisplay(3);
    const missingVirtualTabSessionId = sessionIdForDisplay(4);

    const result = ensureAllSessionsInFocusedPaneTabGroupInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: rightOwnerSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  {
                    activeSessionId: leftOwnerSessionId,
                    kind: "tabs",
                    sessionIds: [leftSiblingSessionId, leftOwnerSessionId],
                  },
                  {
                    activeSessionId: rightOwnerSessionId,
                    kind: "tabs",
                    sessionIds: [rightSiblingSessionId, rightOwnerSessionId],
                  },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                { ...createSessionRecord(1, 0), isSleeping: true },
                { ...createSessionRecord(2, 1), isSleeping: true },
                createSessionRecord(3, 2),
                createSessionRecord(4, 3),
                { ...createSessionRecord(5, 4), isSleeping: true },
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [rightOwnerSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 5,
        nextSessionNumber: 6,
      }),
      DEFAULT_MAIN_GROUP_ID,
      { intent: "passiveSync" },
    );

    expect(result.changed).toBe(true);
    /*
     * CDXC:PaneTabs 2026-06-12-09:18:
     * Passive publish can materialize missing virtual tabs, but it must not
     * turn an existing two-pane split into one tab group when visible/awake
     * owner state is stale. Only explicit pane close, Focus, or Merge actions
     * are allowed to reduce the split topology.
     */
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: leftOwnerSessionId,
          kind: "tabs",
          sessionIds: [leftSiblingSessionId, leftOwnerSessionId],
        },
        {
          activeSessionId: rightOwnerSessionId,
          kind: "tabs",
          sessionIds: [
            rightSiblingSessionId,
            rightOwnerSessionId,
            missingVirtualTabSessionId,
          ],
        },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should relocate offscreen layout sessions into the focused split tab group", () => {
    const leftSessionId = sessionIdForDisplay(0);
    const focusedSessionId = sessionIdForDisplay(1);
    const focusedVisibleSiblingSessionId = sessionIdForDisplay(2);
    const oldSleepingTabSessionId = sessionIdForDisplay(3);
    const oldSleepingTabSiblingSessionId = sessionIdForDisplay(4);
    const oldSleepingLeafSessionId = sessionIdForDisplay(5);
    const oldSleepingSplitLeafSessionId = sessionIdForDisplay(6);

    const result = ensureAllSessionsInFocusedPaneTabGroupInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: leftSessionId },
                  {
                    activeSessionId: oldSleepingTabSessionId,
                    kind: "tabs",
                    sessionIds: [oldSleepingTabSessionId, oldSleepingTabSiblingSessionId],
                  },
                  {
                    activeSessionId: focusedSessionId,
                    kind: "tabs",
                    sessionIds: [focusedSessionId, focusedVisibleSiblingSessionId],
                  },
                  {
                    children: [
                      { kind: "leaf", sessionId: oldSleepingLeafSessionId },
                      { kind: "leaf", sessionId: oldSleepingSplitLeafSessionId },
                    ],
                    direction: "vertical",
                    kind: "split",
                  },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
                { ...createSessionRecord(4, 3), isSleeping: true },
                { ...createSessionRecord(5, 4), isSleeping: true },
                { ...createSessionRecord(6, 5), isSleeping: true },
                { ...createSessionRecord(7, 6), isSleeping: true },
              ],
              viewMode: "grid",
              visibleCount: 3,
              visibleSessionIds: [
                leftSessionId,
                focusedSessionId,
                focusedVisibleSiblingSessionId,
              ],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 7,
        nextSessionNumber: 8,
      }),
      DEFAULT_MAIN_GROUP_ID,
    );

    expect(result.changed).toBe(true);
    /**
     * CDXC:PaneTabs 2026-05-29-09:26:
     * Runtime layout normalization must repair saved layouts where sidebar
     * sessions already exist in sleeping-only branches. Those branches have no
     * native pane owner, so preserving them would still hide their tabs after
     * restart even though paneLayout technically contains the session ids.
     */
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        { kind: "leaf", sessionId: leftSessionId },
        {
          activeSessionId: focusedSessionId,
          kind: "tabs",
          sessionIds: [
            focusedSessionId,
            focusedVisibleSiblingSessionId,
            oldSleepingTabSessionId,
            oldSleepingTabSiblingSessionId,
            oldSleepingLeafSessionId,
            oldSleepingSplitLeafSessionId,
          ],
        },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should drag a virtual sleeping tab to another pane and wake it there", () => {
    const leftSessionId = sessionIdForDisplay(0);
    const focusedSessionId = sessionIdForDisplay(1);
    const sleepingSessionId = sessionIdForDisplay(2);
    const workspace = ensureAllSessionsInFocusedPaneTabGroupInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: leftSessionId },
                  { kind: "leaf", sessionId: focusedSessionId },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                { ...createSessionRecord(3, 2), isSleeping: true },
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [leftSessionId, focusedSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      DEFAULT_MAIN_GROUP_ID,
    ).snapshot;

    const result = moveSessionInPaneLayoutInSimpleWorkspace(
      workspace,
      DEFAULT_MAIN_GROUP_ID,
      sleepingSessionId,
      leftSessionId,
      "center",
      { wakeSourceSession: true },
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.sessions[2]?.isSleeping).toBe(false);
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(sleepingSessionId);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        {
          activeSessionId: sleepingSessionId,
          kind: "tabs",
          sessionIds: [leftSessionId, sleepingSessionId],
        },
        { kind: "leaf", sessionId: focusedSessionId },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should select a virtual unmounted tab that is outside legacy visible ids", () => {
    const leftSessionId = sessionIdForDisplay(0);
    const focusedSessionId = sessionIdForDisplay(1);
    const unmountedSessionId = sessionIdForDisplay(2);
    const workspace = ensureAllSessionsInFocusedPaneTabGroupInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: leftSessionId },
                  { kind: "leaf", sessionId: focusedSessionId },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                createSessionRecord(2, 1),
                createSessionRecord(3, 2),
              ],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [leftSessionId, focusedSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      DEFAULT_MAIN_GROUP_ID,
    ).snapshot;

    const result = selectPaneTabInSimpleWorkspace(
      workspace,
      DEFAULT_MAIN_GROUP_ID,
      unmountedSessionId,
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.focusedSessionId).toBe(unmountedSessionId);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      leftSessionId,
      focusedSessionId,
      unmountedSessionId,
    ]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        { kind: "leaf", sessionId: leftSessionId },
        {
          activeSessionId: unmountedSessionId,
          kind: "tabs",
          sessionIds: [focusedSessionId, unmountedSessionId],
        },
      ],
      direction: "horizontal",
      kind: "split",
    });
  });

  test("should fall back to another group when the active group loses its last awake session", () => {
    const result = setSessionSleepingInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: "group-2",
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(1, 0)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [sessionIdForDisplay(0)],
            },
            title: "Main",
          },
          {
            groupId: "group-2",
            snapshot: {
              focusedSessionId: sessionIdForDisplay(1),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(2, 0)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [sessionIdForDisplay(1)],
            },
            title: "Focused",
          },
        ],
        nextGroupNumber: 3,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      sessionIdForDisplay(1),
      true,
    );

    expect(result.snapshot.activeGroupId).toBe(DEFAULT_MAIN_GROUP_ID);
    expect(result.snapshot.groups[1]?.snapshot.focusedSessionId).toBeUndefined();
    expect(result.snapshot.groups[1]?.snapshot.visibleSessionIds).toEqual([]);
  });
});

describe("setSessionFavoriteInSimpleWorkspace", () => {
  test("should persist the favorite flag on the target session", () => {
    const snapshot = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: sessionIdForDisplay(0),
            fullscreenRestoreVisibleCount: undefined,
            sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
            viewMode: "grid",
            visibleCount: 2,
            visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 2,
      nextSessionNumber: 3,
    });

    const result = setSessionFavoriteInSimpleWorkspace(snapshot, sessionIdForDisplay(1), true);

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.sessions[1]?.isFavorite).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.sessions[0]?.isFavorite).toBeUndefined();
  });
});

describe("setSessionPinnedInSimpleWorkspace", () => {
  test("should persist the pinned flag on the target session without changing favorite state", () => {
    const snapshot = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: sessionIdForDisplay(0),
            fullscreenRestoreVisibleCount: undefined,
            sessions: [
              createSessionRecord(1, 0),
              { ...createSessionRecord(2, 1), isFavorite: true },
            ],
            viewMode: "grid",
            visibleCount: 2,
            visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 2,
      nextSessionNumber: 3,
    });

    const result = setSessionPinnedInSimpleWorkspace(snapshot, sessionIdForDisplay(1), true);

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.sessions[1]?.isPinned).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.sessions[1]?.isFavorite).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.sessions[0]?.isPinned).toBeUndefined();
  });
});

describe("setTerminalSessionLastActivityAtInSimpleWorkspace", () => {
  test("should persist valid last activity timestamps on terminal sessions", () => {
    const snapshot = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: sessionIdForDisplay(0),
            fullscreenRestoreVisibleCount: undefined,
            sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
            viewMode: "grid",
            visibleCount: 2,
            visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 2,
      nextSessionNumber: 3,
    });

    const result = setTerminalSessionLastActivityAtInSimpleWorkspace(
      snapshot,
      sessionIdForDisplay(1),
      "2026-05-17T02:45:00.000Z",
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.sessions[1]).toEqual(
      expect.objectContaining({
        lastActivityAt: "2026-05-17T02:45:00.000Z",
      }),
    );
    const untouchedSession = result.snapshot.groups[0]?.snapshot.sessions[0];
    expect(
      untouchedSession?.kind === "terminal" ? untouchedSession.lastActivityAt : undefined,
    ).toBeUndefined();
  });

  test("should clear invalid last activity timestamps", () => {
    const session = {
      ...createSessionRecord(1, 0),
      lastActivityAt: "2026-05-17T02:45:00.000Z",
    };
    const snapshot = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: session.sessionId,
            fullscreenRestoreVisibleCount: undefined,
            sessions: [session],
            viewMode: "grid",
            visibleCount: 1,
            visibleSessionIds: [session.sessionId],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 1,
      nextSessionNumber: 2,
    });

    const result = setTerminalSessionLastActivityAtInSimpleWorkspace(
      snapshot,
      session.sessionId,
      "not-a-date",
    );

    expect(result.changed).toBe(true);
    const normalizedSession = result.snapshot.groups[0]?.snapshot.sessions[0];
    expect(
      normalizedSession?.kind === "terminal" ? normalizedSession.lastActivityAt : undefined,
    ).toBeUndefined();
  });
});

describe("setGroupSleepingInSimpleWorkspace", () => {
  test("should sleep every session in the group and switch away when needed", () => {
    const result = setGroupSleepingInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: "group-2",
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(1, 0)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [sessionIdForDisplay(0)],
            },
            title: "Main",
          },
          {
            groupId: "group-2",
            snapshot: {
              focusedSessionId: sessionIdForDisplay(1),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(2, 0), createSessionRecord(3, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(1), sessionIdForDisplay(2)],
            },
            title: "Focused",
          },
        ],
        nextGroupNumber: 3,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      "group-2",
      true,
    );

    expect(result.snapshot.activeGroupId).toBe(DEFAULT_MAIN_GROUP_ID);
    expect(
      result.snapshot.groups[1]?.snapshot.sessions.every((session) => session.isSleeping),
    ).toBe(true);
    expect(result.snapshot.groups[1]?.snapshot.visibleSessionIds).toEqual([]);
  });

  test("should only sleep the targeted sessions in the group", () => {
    const result = setGroupSleepingInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: "group-2",
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(1, 0)],
              viewMode: "grid",
              visibleCount: 1,
              visibleSessionIds: [sessionIdForDisplay(0)],
            },
            title: "Main",
          },
          {
            groupId: "group-2",
            snapshot: {
              focusedSessionId: sessionIdForDisplay(1),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(2, 0), createSessionRecord(3, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(1), sessionIdForDisplay(2)],
            },
            title: "Focused",
          },
        ],
        nextGroupNumber: 3,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      "group-2",
      true,
      [sessionIdForDisplay(1)],
    );

    expect(result.snapshot.activeGroupId).toBe("group-2");
    expect(result.snapshot.groups[1]?.snapshot.sessions[0]?.isSleeping).toBe(true);
    expect(result.snapshot.groups[1]?.snapshot.sessions[1]?.isSleeping).toBeUndefined();
    expect(result.snapshot.groups[1]?.snapshot.visibleSessionIds).toEqual([sessionIdForDisplay(2)]);
  });

  test("should wake targeted group sessions into one focused pane tab group", () => {
    const focusedSessionId = sessionIdForDisplay(0);
    const sleepingSessionId = sessionIdForDisplay(1);
    const secondSleepingSessionId = sessionIdForDisplay(2);
    const result = setGroupSleepingInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId,
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: focusedSessionId },
                  { kind: "leaf", sessionId: sleepingSessionId },
                  { kind: "leaf", sessionId: secondSleepingSessionId },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [
                createSessionRecord(1, 0),
                { ...createSessionRecord(2, 1), isSleeping: true },
                { ...createSessionRecord(3, 2), isSleeping: true },
              ],
              viewMode: "grid",
              visibleCount: 3,
              visibleSessionIds: [focusedSessionId],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 3,
        nextSessionNumber: 4,
      }),
      DEFAULT_MAIN_GROUP_ID,
      false,
      [sleepingSessionId, secondSleepingSessionId],
    );

    expect(result.snapshot.groups[0]?.snapshot.sessions.map((session) => session.isSleeping)).toEqual([
      undefined,
      false,
      false,
    ]);
    expect(result.snapshot.groups[0]?.snapshot.visibleSessionIds).toEqual([
      focusedSessionId,
      sleepingSessionId,
      secondSleepingSessionId,
    ]);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      activeSessionId: secondSleepingSessionId,
      kind: "tabs",
      sessionIds: [focusedSessionId, sleepingSessionId, secondSleepingSessionId],
    });
  });
});

describe("rotatePaneLayoutClockwiseInSimpleWorkspace", () => {
  test("should merge split pane tab groups into one pane in the owning workspace group", () => {
    const workspace = createWorkspaceSnapshot({
      activeGroupId: DEFAULT_MAIN_GROUP_ID,
      groups: [
        {
          groupId: DEFAULT_MAIN_GROUP_ID,
          snapshot: {
            focusedSessionId: sessionIdForDisplay(1),
            fullscreenRestoreVisibleCount: undefined,
            paneLayout: {
              children: [
                {
                  activeSessionId: sessionIdForDisplay(1),
                  kind: "tabs",
                  sessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
                },
                {
                  activeSessionId: sessionIdForDisplay(3),
                  kind: "tabs",
                  sessionIds: [sessionIdForDisplay(2), sessionIdForDisplay(3)],
                },
              ],
              direction: "horizontal",
              kind: "split",
            },
            sessions: [
              createSessionRecord(1, 0),
              createSessionRecord(2, 1),
              createSessionRecord(3, 2),
              createSessionRecord(4, 3),
            ],
            viewMode: "grid",
            visibleCount: 4,
            visibleSessionIds: [
              sessionIdForDisplay(0),
              sessionIdForDisplay(1),
              sessionIdForDisplay(2),
              sessionIdForDisplay(3),
            ],
          },
          title: "Main",
        },
      ],
      nextGroupNumber: 2,
      nextSessionDisplayId: 4,
      nextSessionNumber: 5,
    });

    const result = mergeAllTabsInPaneLayoutInSimpleWorkspace(
      workspace,
      DEFAULT_MAIN_GROUP_ID,
      sessionIdForDisplay(2),
    );

    expect(result.changed).toBe(true);
    /**
     * CDXC:PaneTabs 2026-05-15-13:35
     * Merge All Tabs is a workspace-group operation: every tab from the
     * group's split pane tree becomes one tab group, and the clicked tab stays
     * active. Command Terminal tabs live outside this workspace snapshot and
     * therefore cannot be merged by this mutation.
     */
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      activeSessionId: sessionIdForDisplay(2),
      kind: "tabs",
      sessionIds: [
        sessionIdForDisplay(0),
        sessionIdForDisplay(1),
        sessionIdForDisplay(2),
        sessionIdForDisplay(3),
      ],
    });
  });

  test("should not synthesize a split tree just to rotate legacy visible panes", () => {
    const result = rotatePaneLayoutClockwiseInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      DEFAULT_MAIN_GROUP_ID,
    );

    expect(result.changed).toBe(false);
  });

  test("should rotate two columns into two rows", () => {
    const result = rotatePaneLayoutClockwiseInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: sessionIdForDisplay(0) },
                  { kind: "leaf", sessionId: sessionIdForDisplay(1) },
                ],
                direction: "horizontal",
                kind: "split",
              },
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      DEFAULT_MAIN_GROUP_ID,
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        { kind: "leaf", sessionId: sessionIdForDisplay(0) },
        { kind: "leaf", sessionId: sessionIdForDisplay(1) },
      ],
      direction: "vertical",
      kind: "split",
    });
  });

  test("should rotate stacked panes clockwise into reversed columns", () => {
    const result = rotatePaneLayoutClockwiseInSimpleWorkspace(
      createWorkspaceSnapshot({
        activeGroupId: DEFAULT_MAIN_GROUP_ID,
        groups: [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: {
              focusedSessionId: sessionIdForDisplay(0),
              fullscreenRestoreVisibleCount: undefined,
              paneLayout: {
                children: [
                  { kind: "leaf", sessionId: sessionIdForDisplay(0) },
                  { kind: "leaf", sessionId: sessionIdForDisplay(1) },
                ],
                direction: "vertical",
                kind: "split",
                ratio: 0.25,
              },
              sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
              viewMode: "grid",
              visibleCount: 2,
              visibleSessionIds: [sessionIdForDisplay(0), sessionIdForDisplay(1)],
            },
            title: "Main",
          },
        ],
        nextGroupNumber: 2,
        nextSessionDisplayId: 2,
        nextSessionNumber: 3,
      }),
      DEFAULT_MAIN_GROUP_ID,
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.groups[0]?.snapshot.paneLayout).toEqual({
      children: [
        { kind: "leaf", sessionId: sessionIdForDisplay(1) },
        { kind: "leaf", sessionId: sessionIdForDisplay(0) },
      ],
      direction: "horizontal",
      kind: "split",
      ratio: 0.75,
    });
  });
});

function createWorkspaceSnapshot(
  snapshot: GroupedSessionWorkspaceSnapshot,
): GroupedSessionWorkspaceSnapshot {
  return normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
}

function countSplitLeafNodes(layout: SessionPaneLayoutNode | undefined): number {
  if (!layout) {
    return 0;
  }
  switch (layout.kind) {
    case "leaf":
      return 1;
    case "tabs":
      return 0;
    case "split":
      return layout.children.reduce((count, child) => count + countSplitLeafNodes(child), 0);
  }
}
