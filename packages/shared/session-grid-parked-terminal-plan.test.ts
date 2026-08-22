import { describe, expect, test } from "vite-plus/test";
import {
  createSessionRecord,
  type SessionGridSnapshot,
  type TerminalViewMode,
  type VisibleSessionCount,
} from "./session-grid-contract";
import { createParkedTerminalReconcilePlan } from "./session-grid-parked-terminal-plan";

describe("createParkedTerminalReconcilePlan", () => {
  test("should transfer hidden-session replacement plans into the occupied slot", () => {
    const currentSnapshot = createSnapshot([1, 2], ["session-1"], "grid");
    const nextSnapshot = createSnapshot([1, 2], ["session-2"], "grid");
    const plan = createParkedTerminalReconcilePlan(currentSnapshot, nextSnapshot);

    expect(plan.strategy).toBe("transfer");
    if (plan.strategy !== "transfer") {
      throw new Error("Expected transfer plan");
    }

    expect(plan.steps).toEqual([
      {
        sessionId: "session-2",
        slotIndex: 0,
        type: "promote",
      },
      {
        sessionId: "session-1",
        slotIndex: 0,
        type: "demote",
      },
    ]);
  });

  test("should return a transfer no-op when the visible sessions already match", () => {
    const snapshot = createSnapshot([1, 2], ["session-1", "session-2"], "grid");
    const plan = createParkedTerminalReconcilePlan(snapshot, snapshot);

    expect(plan.strategy).toBe("transfer");
    if (plan.strategy !== "transfer") {
      throw new Error("Expected transfer plan");
    }

    expect(plan.hasChanges).toBe(false);
    expect(plan.promoteSteps).toEqual([]);
    expect(plan.demoteSteps).toEqual([]);
    expect(plan.steps).toEqual([]);
  });

  test("should rebuild when there is no current visible layout", () => {
    const nextSnapshot = createSnapshot([1, 2], ["session-1", "session-2"], "horizontal");
    const plan = createParkedTerminalReconcilePlan(undefined, nextSnapshot);

    expect(plan).toEqual({
      currentVisibleSessionIds: [],
      nextVisibleSessionIds: ["session-1", "session-2"],
      reason: "missing-current-layout",
      strategy: "rebuild",
    });
  });

  test("should rebuild when the requested layout shape changes", () => {
    const currentSnapshot = createSnapshot([1, 2], ["session-1", "session-2"], "vertical");
    const nextSnapshot = createSnapshot(
      [1, 2, 3, 4],
      ["session-1", "session-2", "session-3", "session-4"],
      "grid",
    );
    const plan = createParkedTerminalReconcilePlan(currentSnapshot, nextSnapshot);

    expect(plan).toEqual({
      currentVisibleSessionIds: ["session-1", "session-2"],
      nextVisibleSessionIds: ["session-1", "session-2", "session-3", "session-4"],
      reason: "layout-shape-changed",
      strategy: "rebuild",
    });
  });

  test("should transfer backfill plans by reusing outgoing slots first", () => {
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
    const plan = createParkedTerminalReconcilePlan(currentSnapshot, nextSnapshot);

    expect(plan.strategy).toBe("transfer");
    if (plan.strategy !== "transfer") {
      throw new Error("Expected transfer plan");
    }

    expect(plan.steps).toEqual([
      {
        sessionId: "session-4",
        slotIndex: 2,
        type: "promote",
      },
      {
        sessionId: "session-3",
        slotIndex: 2,
        type: "demote",
      },
      {
        sessionId: "session-3",
        slotIndex: 1,
        type: "promote",
      },
      {
        sessionId: "session-2",
        slotIndex: 1,
        type: "demote",
      },
    ]);
  });

  test("should transfer visible-session reorder plans without a full rebuild", () => {
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
    const plan = createParkedTerminalReconcilePlan(currentSnapshot, nextSnapshot);

    expect(plan.strategy).toBe("transfer");
    if (plan.strategy !== "transfer") {
      throw new Error("Expected transfer plan");
    }

    expect(plan.steps).toEqual([
      {
        sessionId: "session-3",
        slotIndex: 2,
        type: "demote",
      },
      {
        sessionId: "session-3",
        slotIndex: 1,
        type: "promote",
      },
      {
        sessionId: "session-2",
        slotIndex: 1,
        type: "demote",
      },
      {
        sessionId: "session-2",
        slotIndex: 2,
        type: "promote",
      },
    ]);
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
