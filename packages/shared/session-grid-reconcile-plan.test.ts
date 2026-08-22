import { describe, expect, test } from "vite-plus/test";
import {
  createSessionRecord,
  type SessionGridSnapshot,
  type TerminalViewMode,
  type VisibleSessionCount,
} from "./session-grid-contract";
import { createVisibleSessionReconcilePlan } from "./session-grid-reconcile-plan";

describe("createVisibleSessionReconcilePlan", () => {
  test("should rebuild when there is no current visible layout", () => {
    const nextSnapshot = createSnapshot([1, 2], ["session-1", "session-2"], "vertical");
    const plan = createVisibleSessionReconcilePlan(undefined, nextSnapshot);

    expect(plan).toEqual({
      currentVisibleSessionIds: [],
      nextVisibleSessionIds: ["session-1", "session-2"],
      reason: "missing-current-layout",
      strategy: "rebuild",
    });
  });

  test("should preserve unchanged slots when focusing a hidden session replaces one visible slot", () => {
    const currentSnapshot = createSnapshot([1, 2, 3], ["session-1", "session-2"], "vertical");
    const nextSnapshot = createSnapshot([1, 2, 3], ["session-1", "session-3"], "vertical");
    const plan = createVisibleSessionReconcilePlan(currentSnapshot, nextSnapshot);

    expect(plan.strategy).toBe("incremental");
    if (plan.strategy !== "incremental") {
      throw new Error("Expected incremental plan");
    }

    expect(plan.unchangedSlots).toEqual([{ sessionId: "session-1", slotIndex: 0 }]);
    expect(plan.changedSlots).toEqual([
      {
        currentSessionId: "session-2",
        nextSessionId: "session-3",
        slotIndex: 1,
      },
    ]);
    expect(plan.incomingSlots).toEqual([{ sessionId: "session-3", slotIndex: 1 }]);
    expect(plan.outgoingSlots).toEqual([{ sessionId: "session-2", slotIndex: 1 }]);
    expect(plan.movedSessions).toEqual([]);
  });

  test("should capture the minimal changed slots when a killed visible session is backfilled", () => {
    const currentSnapshot = createSnapshot(
      [1, 2, 3, 4],
      ["session-1", "session-2", "session-3"],
      "horizontal",
    );
    const nextSnapshot = createSnapshot(
      [1, 3, 4],
      ["session-1", "session-3", "session-4"],
      "horizontal",
    );
    const plan = createVisibleSessionReconcilePlan(currentSnapshot, nextSnapshot);

    expect(plan.strategy).toBe("incremental");
    if (plan.strategy !== "incremental") {
      throw new Error("Expected incremental plan");
    }

    expect(plan.unchangedSlots).toEqual([{ sessionId: "session-1", slotIndex: 0 }]);
    expect(plan.changedSlots).toEqual([
      {
        currentSessionId: "session-2",
        nextSessionId: "session-3",
        slotIndex: 1,
      },
      {
        currentSessionId: "session-3",
        nextSessionId: "session-4",
        slotIndex: 2,
      },
    ]);
    expect(plan.incomingSlots).toEqual([
      { sessionId: "session-3", slotIndex: 1 },
      { sessionId: "session-4", slotIndex: 2 },
    ]);
    expect(plan.outgoingSlots).toEqual([
      { sessionId: "session-2", slotIndex: 1 },
      { sessionId: "session-3", slotIndex: 2 },
    ]);
    expect(plan.movedSessions).toEqual([
      {
        fromSlotIndex: 2,
        sessionId: "session-3",
        toSlotIndex: 1,
      },
    ]);
  });

  test("should keep unaffected slots during sidebar reorder changes", () => {
    const currentSnapshot = createSnapshot(
      [1, 2, 3, 4],
      ["session-1", "session-2", "session-3", "session-4"],
      "grid",
    );
    const nextSnapshot = createSnapshot(
      [1, 3, 2, 4],
      ["session-1", "session-3", "session-2", "session-4"],
      "grid",
    );
    const plan = createVisibleSessionReconcilePlan(currentSnapshot, nextSnapshot);

    expect(plan.strategy).toBe("incremental");
    if (plan.strategy !== "incremental") {
      throw new Error("Expected incremental plan");
    }

    expect(plan.unchangedSlots).toEqual([
      { sessionId: "session-1", slotIndex: 0 },
      { sessionId: "session-4", slotIndex: 3 },
    ]);
    expect(plan.incomingSlots).toEqual([
      { sessionId: "session-3", slotIndex: 1 },
      { sessionId: "session-2", slotIndex: 2 },
    ]);
    expect(plan.outgoingSlots).toEqual([
      { sessionId: "session-2", slotIndex: 1 },
      { sessionId: "session-3", slotIndex: 2 },
    ]);
    expect(plan.movedSessions).toEqual([
      {
        fromSlotIndex: 2,
        sessionId: "session-3",
        toSlotIndex: 1,
      },
      {
        fromSlotIndex: 1,
        sessionId: "session-2",
        toSlotIndex: 2,
      },
    ]);
  });

  test("should return an incremental no-op when the visible slots already match", () => {
    const snapshot = createSnapshot([1, 2], ["session-1", "session-2"], "vertical");
    const plan = createVisibleSessionReconcilePlan(snapshot, snapshot);

    expect(plan.strategy).toBe("incremental");
    if (plan.strategy !== "incremental") {
      throw new Error("Expected incremental plan");
    }

    expect(plan.hasChanges).toBe(false);
    expect(plan.changedSlots).toEqual([]);
    expect(plan.incomingSlots).toEqual([]);
    expect(plan.outgoingSlots).toEqual([]);
    expect(plan.unchangedSlots).toEqual([
      { sessionId: "session-1", slotIndex: 0 },
      { sessionId: "session-2", slotIndex: 1 },
    ]);
  });

  test("should rebuild when the requested layout shape changes", () => {
    const currentSnapshot = createSnapshot([1, 2], ["session-1", "session-2"], "vertical");
    const nextSnapshot = createSnapshot(
      [1, 2, 3, 4],
      ["session-1", "session-2", "session-3", "session-4"],
      "grid",
    );
    const plan = createVisibleSessionReconcilePlan(currentSnapshot, nextSnapshot);

    expect(plan).toEqual({
      currentVisibleSessionIds: ["session-1", "session-2"],
      nextVisibleSessionIds: ["session-1", "session-2", "session-3", "session-4"],
      reason: "layout-shape-changed",
      strategy: "rebuild",
    });
  });

  test("should rebuild when the view mode changes even if the visible count stays the same", () => {
    const currentSnapshot = createSnapshot(
      [1, 2, 3],
      ["session-1", "session-2", "session-3"],
      "horizontal",
    );
    const nextSnapshot = createSnapshot([1, 2, 3], ["session-1", "session-2", "session-3"], "grid");
    const plan = createVisibleSessionReconcilePlan(currentSnapshot, nextSnapshot);

    expect(plan).toEqual({
      currentVisibleSessionIds: ["session-1", "session-2", "session-3"],
      nextVisibleSessionIds: ["session-1", "session-2", "session-3"],
      reason: "layout-shape-changed",
      strategy: "rebuild",
    });
  });
});

function createSnapshot(
  orderedSessionNumbers: readonly number[],
  visibleSessionIds: readonly string[],
  viewMode: TerminalViewMode,
): SessionGridSnapshot {
  return {
    focusedSessionId: visibleSessionIds.at(-1),
    fullscreenRestoreVisibleCount: undefined,
    sessions: orderedSessionNumbers.map((sessionNumber, slotIndex) =>
      createSessionRecord(sessionNumber, slotIndex),
    ),
    viewMode,
    visibleCount: toVisibleSessionCount(
      visibleSessionIds.length === 0 ? 1 : visibleSessionIds.length,
    ),
    visibleSessionIds: [...visibleSessionIds],
  };
}

function toVisibleSessionCount(value: number): VisibleSessionCount {
  switch (value) {
    case 1:
    case 2:
    case 3:
    case 4:
    case 5:
    case 6:
    case 7:
    case 8:
    case 9:
      return value;
    default:
      throw new Error(`Invalid visible session count: ${value}`);
  }
}
