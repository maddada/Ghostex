import { describe, expect, test } from "vite-plus/test";
import { createSessionRecord } from "./session-grid-contract";
import {
  focusDirectionInSnapshot,
  focusSessionInSnapshot,
  setViewModeInSnapshot,
  setVisibleCountInSnapshot,
  syncSessionOrderInSnapshot,
  toggleFullscreenSessionInSnapshot,
} from "./session-grid-state";

describe("focusSessionInSnapshot", () => {
  test("should reveal an offscreen session while preserving the visible set order", () => {
    const result = focusSessionInSnapshot(
      {
        focusedSessionId: "session-1",
        sessions: [
          createSessionRecord(1, 0),
          createSessionRecord(2, 1),
          createSessionRecord(3, 2),
          createSessionRecord(4, 3),
        ],
        viewMode: "grid",
        visibleCount: 2,
        visibleSessionIds: ["session-1", "session-2"],
      },
      "session-3",
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.focusedSessionId).toBe("session-3");
    expect(result.snapshot.visibleSessionIds).toEqual(["session-3", "session-2"]);
  });

  test("should preserve pane layout while focusing a visible session", () => {
    const paneLayout = {
      activeSessionId: "session-1",
      kind: "tabs" as const,
      sessionIds: ["session-1", "session-2"],
    };
    const result = focusSessionInSnapshot(
      {
        focusedSessionId: "session-1",
        paneLayout,
        sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
        viewMode: "grid",
        visibleCount: 2,
        visibleSessionIds: ["session-1", "session-2"],
      },
      "session-2",
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.paneLayout).toEqual(paneLayout);
  });
});

describe("focusDirectionInSnapshot", () => {
  test("should move to the nearest directional neighbor on the logical grid", () => {
    const snapshot = {
      focusedSessionId: "session-1",
      sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1), createSessionRecord(3, 3)],
      viewMode: "grid" as const,
      visibleCount: 2 as const,
      visibleSessionIds: ["session-1", "session-2"],
    };

    const right = focusDirectionInSnapshot(snapshot, "right");
    expect(right.changed).toBe(true);
    expect(right.snapshot.focusedSessionId).toBe("session-2");

    const down = focusDirectionInSnapshot(snapshot, "down");
    expect(down.changed).toBe(true);
    expect(down.snapshot.focusedSessionId).toBe("session-3");
  });

  test("should preserve pane layout when moving focus with direction keys", () => {
    const paneLayout = {
      activeSessionId: "session-1",
      kind: "tabs" as const,
      sessionIds: ["session-1", "session-2", "session-3"],
    };
    const result = focusDirectionInSnapshot(
      {
        focusedSessionId: "session-1",
        paneLayout,
        sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1), createSessionRecord(3, 2)],
        viewMode: "grid",
        visibleCount: 3,
        visibleSessionIds: ["session-1", "session-2", "session-3"],
      },
      "right",
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.focusedSessionId).toBe("session-2");
    expect(result.snapshot.paneLayout).toEqual(paneLayout);
  });
});

describe("setVisibleCountInSnapshot", () => {
  test("should preserve the requested visible list order", () => {
    const snapshot = setVisibleCountInSnapshot(
      {
        focusedSessionId: "session-2",
        sessions: [
          createSessionRecord(1, 0),
          createSessionRecord(2, 1),
          createSessionRecord(3, 2),
          createSessionRecord(4, 3),
        ],
        viewMode: "grid",
        visibleCount: 4,
        visibleSessionIds: ["session-2", "session-1", "session-3", "session-4"],
      },
      6,
    );

    expect(snapshot.visibleCount).toBe(6);
    expect(snapshot.visibleSessionIds).toEqual([
      "session-2",
      "session-1",
      "session-3",
      "session-4",
    ]);
  });

  test("should keep focus visible without scrambling slot order when reducing the count", () => {
    const snapshot = setVisibleCountInSnapshot(
      {
        focusedSessionId: "session-4",
        sessions: [
          createSessionRecord(1, 0),
          createSessionRecord(2, 1),
          createSessionRecord(3, 2),
          createSessionRecord(4, 3),
        ],
        viewMode: "grid",
        visibleCount: 4,
        visibleSessionIds: ["session-1", "session-2", "session-3", "session-4"],
      },
      3,
    );

    expect(snapshot.visibleCount).toBe(3);
    expect(snapshot.focusedSessionId).toBe("session-4");
    expect(snapshot.visibleSessionIds).toEqual(["session-2", "session-3", "session-4"]);
  });

  test("should clear fullscreen restore state on an explicit visible-count change", () => {
    const snapshot = setVisibleCountInSnapshot(
      {
        focusedSessionId: "session-1",
        fullscreenRestoreVisibleCount: 4,
        sessions: [
          createSessionRecord(1, 0),
          createSessionRecord(2, 1),
          createSessionRecord(3, 2),
          createSessionRecord(4, 3),
        ],
        viewMode: "grid",
        visibleCount: 1,
        visibleSessionIds: ["session-1"],
      },
      3,
    );

    expect(snapshot.fullscreenRestoreVisibleCount).toBeUndefined();
    expect(snapshot.visibleCount).toBe(3);
  });
});

describe("toggleFullscreenSessionInSnapshot", () => {
  test("should enter fullscreen and remember the previous visible count", () => {
    const snapshot = toggleFullscreenSessionInSnapshot({
      focusedSessionId: "session-3",
      sessions: [
        createSessionRecord(1, 0),
        createSessionRecord(2, 1),
        createSessionRecord(3, 2),
        createSessionRecord(4, 3),
      ],
      viewMode: "grid",
      visibleCount: 4,
      visibleSessionIds: ["session-1", "session-2", "session-3", "session-4"],
    });

    expect(snapshot.visibleCount).toBe(1);
    expect(snapshot.fullscreenRestoreVisibleCount).toBe(4);
    expect(snapshot.focusedSessionId).toBe("session-3");
    expect(snapshot.visibleSessionIds).toEqual(["session-3"]);
  });

  test("should restore the previous visible count on the next toggle", () => {
    const snapshot = toggleFullscreenSessionInSnapshot({
      focusedSessionId: "session-3",
      fullscreenRestoreVisibleCount: 4,
      sessions: [
        createSessionRecord(1, 0),
        createSessionRecord(2, 1),
        createSessionRecord(3, 2),
        createSessionRecord(4, 3),
      ],
      viewMode: "grid",
      visibleCount: 1,
      visibleSessionIds: ["session-3"],
    });

    expect(snapshot.visibleCount).toBe(4);
    expect(snapshot.fullscreenRestoreVisibleCount).toBeUndefined();
    expect(snapshot.visibleSessionIds).toEqual([
      "session-3",
      "session-1",
      "session-2",
      "session-4",
    ]);
  });
});

describe("setViewModeInSnapshot", () => {
  test("should update the persisted view mode while keeping the visible set stable", () => {
    const snapshot = setViewModeInSnapshot(
      {
        focusedSessionId: "session-2",
        sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1), createSessionRecord(3, 2)],
        viewMode: "grid",
        visibleCount: 6,
        visibleSessionIds: ["session-2", "session-1", "session-3"],
      },
      "horizontal",
    );

    expect(snapshot.viewMode).toBe("horizontal");
    expect(snapshot.visibleCount).toBe(6);
    expect(snapshot.visibleSessionIds).toEqual(["session-2", "session-1", "session-3"]);
  });

  test("should exit focus mode and restore the previous visible count before switching view mode", () => {
    const snapshot = setViewModeInSnapshot(
      {
        focusedSessionId: "session-3",
        fullscreenRestoreVisibleCount: 4,
        sessions: [
          createSessionRecord(1, 0),
          createSessionRecord(2, 1),
          createSessionRecord(3, 2),
          createSessionRecord(4, 3),
        ],
        viewMode: "grid",
        visibleCount: 1,
        visibleSessionIds: ["session-3"],
      },
      "horizontal",
    );

    expect(snapshot.fullscreenRestoreVisibleCount).toBeUndefined();
    expect(snapshot.viewMode).toBe("horizontal");
    expect(snapshot.visibleCount).toBe(4);
    expect(snapshot.visibleSessionIds).toEqual([
      "session-3",
      "session-1",
      "session-2",
      "session-4",
    ]);
  });
});

describe("syncSessionOrderInSnapshot", () => {
  test("should reassign slot positions and visible terminals from the committed order", () => {
    const result = syncSessionOrderInSnapshot(
      {
        focusedSessionId: "session-4",
        sessions: [
          createSessionRecord(1, 0),
          createSessionRecord(2, 1),
          createSessionRecord(3, 2),
          createSessionRecord(4, 3),
        ],
        viewMode: "grid",
        visibleCount: 2,
        visibleSessionIds: ["session-4", "session-2"],
      },
      ["session-3", "session-1", "session-4", "session-2"],
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.sessions.map((session) => session.sessionId)).toEqual([
      "session-3",
      "session-1",
      "session-4",
      "session-2",
    ]);
    expect(result.snapshot.sessions.map((session) => session.slotIndex)).toEqual([0, 1, 2, 3]);
    expect(result.snapshot.visibleSessionIds).toEqual(["session-3", "session-1"]);
    expect(result.snapshot.focusedSessionId).toBe("session-3");
  });

  test("should preserve focus when the focused session remains visible after sync", () => {
    const result = syncSessionOrderInSnapshot(
      {
        focusedSessionId: "session-2",
        sessions: [
          createSessionRecord(1, 0),
          createSessionRecord(2, 1),
          createSessionRecord(3, 2),
          createSessionRecord(4, 3),
        ],
        viewMode: "grid",
        visibleCount: 3,
        visibleSessionIds: ["session-1", "session-2", "session-3"],
      },
      ["session-2", "session-4", "session-1", "session-3"],
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.visibleSessionIds).toEqual(["session-2", "session-4", "session-1"]);
    expect(result.snapshot.focusedSessionId).toBe("session-2");
  });
});
