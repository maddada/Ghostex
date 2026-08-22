import { createEditorLayoutPlan } from "./editor-layout";
import type { SessionGridSnapshot } from "./session-grid-contract";

export type VisibleSessionSlotRef = {
  sessionId: string;
  slotIndex: number;
};

export type VisibleSessionSlotChange = {
  currentSessionId?: string;
  nextSessionId?: string;
  slotIndex: number;
};

export type VisibleSessionReconcilePlan =
  | {
      currentVisibleSessionIds: string[];
      nextVisibleSessionIds: string[];
      reason: "layout-shape-changed" | "missing-current-layout";
      strategy: "rebuild";
    }
  | {
      changedSlots: VisibleSessionSlotChange[];
      currentVisibleSessionIds: string[];
      hasChanges: boolean;
      incomingSlots: VisibleSessionSlotRef[];
      movedSessions: Array<{
        fromSlotIndex: number;
        sessionId: string;
        toSlotIndex: number;
      }>;
      nextVisibleSessionIds: string[];
      outgoingSlots: VisibleSessionSlotRef[];
      strategy: "incremental";
      unchangedSlots: VisibleSessionSlotRef[];
    };

export function createVisibleSessionReconcilePlan(
  currentSnapshot: SessionGridSnapshot | undefined,
  nextSnapshot: SessionGridSnapshot,
): VisibleSessionReconcilePlan {
  const currentVisibleSessionIds = currentSnapshot?.visibleSessionIds ?? [];
  const nextVisibleSessionIds = nextSnapshot.visibleSessionIds;

  if (!currentSnapshot) {
    return {
      currentVisibleSessionIds,
      nextVisibleSessionIds,
      reason: "missing-current-layout",
      strategy: "rebuild",
    };
  }

  if (hasLayoutShapeChanged(currentSnapshot, nextSnapshot)) {
    return {
      currentVisibleSessionIds,
      nextVisibleSessionIds,
      reason: "layout-shape-changed",
      strategy: "rebuild",
    };
  }

  const changedSlots: VisibleSessionSlotChange[] = [];
  const unchangedSlots: VisibleSessionSlotRef[] = [];
  const slotCount = Math.max(currentVisibleSessionIds.length, nextVisibleSessionIds.length);

  for (let slotIndex = 0; slotIndex < slotCount; slotIndex += 1) {
    const currentSessionId = currentVisibleSessionIds[slotIndex];
    const nextSessionId = nextVisibleSessionIds[slotIndex];

    if (currentSessionId && nextSessionId && currentSessionId === nextSessionId) {
      unchangedSlots.push({ sessionId: currentSessionId, slotIndex });
      continue;
    }

    changedSlots.push({
      currentSessionId,
      nextSessionId,
      slotIndex,
    });
  }

  const currentIndexBySessionId = new Map(
    currentVisibleSessionIds.map((sessionId, slotIndex) => [sessionId, slotIndex]),
  );
  const incomingSlots = nextVisibleSessionIds
    .map((sessionId, slotIndex) => ({ sessionId, slotIndex }))
    .filter(({ sessionId, slotIndex }) => currentVisibleSessionIds[slotIndex] !== sessionId);
  const outgoingSlots = currentVisibleSessionIds
    .map((sessionId, slotIndex) => ({ sessionId, slotIndex }))
    .filter(({ sessionId, slotIndex }) => nextVisibleSessionIds[slotIndex] !== sessionId);
  const movedSessions = nextVisibleSessionIds
    .map((sessionId, toSlotIndex) => {
      const fromSlotIndex = currentIndexBySessionId.get(sessionId);
      return fromSlotIndex !== undefined && fromSlotIndex !== toSlotIndex
        ? {
            fromSlotIndex,
            sessionId,
            toSlotIndex,
          }
        : undefined;
    })
    .filter((move): move is NonNullable<typeof move> => move !== undefined);

  return {
    changedSlots,
    currentVisibleSessionIds,
    hasChanges: changedSlots.length > 0,
    incomingSlots,
    movedSessions,
    nextVisibleSessionIds,
    outgoingSlots,
    strategy: "incremental",
    unchangedSlots,
  };
}

function hasLayoutShapeChanged(
  currentSnapshot: SessionGridSnapshot,
  nextSnapshot: SessionGridSnapshot,
): boolean {
  if (currentSnapshot.viewMode !== nextSnapshot.viewMode) {
    return true;
  }

  const currentLayoutPlan = createEditorLayoutPlan(
    currentSnapshot.visibleSessionIds.length,
    currentSnapshot.viewMode,
  );
  const nextLayoutPlan = createEditorLayoutPlan(
    nextSnapshot.visibleSessionIds.length,
    nextSnapshot.viewMode,
  );

  return !haveSameNumbers(currentLayoutPlan.rowLengths, nextLayoutPlan.rowLengths);
}

function haveSameNumbers(left: readonly number[], right: readonly number[]): boolean {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}
