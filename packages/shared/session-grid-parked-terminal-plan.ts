import type { SessionGridSnapshot } from "./session-grid-contract";
import {
  createVisibleSessionReconcilePlan,
  type VisibleSessionSlotRef,
} from "./session-grid-reconcile-plan";

export type ParkedTerminalTransferStep =
  | {
      sessionId: string;
      slotIndex: number;
      type: "promote";
    }
  | {
      sessionId: string;
      slotIndex: number;
      type: "demote";
    };

export type ParkedTerminalReconcilePlan =
  | {
      currentVisibleSessionIds: string[];
      nextVisibleSessionIds: string[];
      reason: "layout-shape-changed" | "missing-current-layout" | "unsupported-transfer";
      strategy: "rebuild";
    }
  | {
      currentVisibleSessionIds: string[];
      demoteSteps: Extract<ParkedTerminalTransferStep, { type: "demote" }>[];
      hasChanges: boolean;
      nextVisibleSessionIds: string[];
      promoteSteps: Extract<ParkedTerminalTransferStep, { type: "promote" }>[];
      steps: ParkedTerminalTransferStep[];
      strategy: "transfer";
      unchangedSlots: VisibleSessionSlotRef[];
    };

export function createParkedTerminalReconcilePlan(
  currentSnapshot: SessionGridSnapshot | undefined,
  nextSnapshot: SessionGridSnapshot,
): ParkedTerminalReconcilePlan {
  const basePlan = createVisibleSessionReconcilePlan(currentSnapshot, nextSnapshot);
  if (basePlan.strategy === "rebuild") {
    return basePlan;
  }

  const steps = createTransferSteps(
    basePlan.currentVisibleSessionIds,
    basePlan.nextVisibleSessionIds,
  );
  const promoteSteps = steps.filter(
    (step): step is Extract<ParkedTerminalTransferStep, { type: "promote" }> =>
      step.type === "promote",
  );
  const demoteSteps = steps.filter(
    (step): step is Extract<ParkedTerminalTransferStep, { type: "demote" }> =>
      step.type === "demote",
  );

  return {
    currentVisibleSessionIds: basePlan.currentVisibleSessionIds,
    demoteSteps,
    hasChanges: steps.length > 0,
    nextVisibleSessionIds: basePlan.nextVisibleSessionIds,
    promoteSteps,
    steps,
    strategy: "transfer",
    unchangedSlots: basePlan.unchangedSlots,
  };
}

function createTransferSteps(
  currentVisibleSessionIds: readonly string[],
  nextVisibleSessionIds: readonly string[],
): ParkedTerminalTransferStep[] {
  const steps: ParkedTerminalTransferStep[] = [];
  const currentSlots: Array<string | undefined> = [...currentVisibleSessionIds];
  const panelSessionIds = new Set(
    nextVisibleSessionIds.filter((sessionId) => !currentVisibleSessionIds.includes(sessionId)),
  );

  while (!haveSameSessionIds(currentSlots, nextVisibleSessionIds)) {
    let progressed = false;

    for (let slotIndex = 0; slotIndex < nextVisibleSessionIds.length; slotIndex += 1) {
      const targetSessionId = nextVisibleSessionIds[slotIndex];
      const currentSessionId = currentSlots[slotIndex];

      if (
        !targetSessionId ||
        currentSessionId === targetSessionId ||
        !panelSessionIds.has(targetSessionId)
      ) {
        continue;
      }

      steps.push({
        sessionId: targetSessionId,
        slotIndex,
        type: "promote",
      });
      panelSessionIds.delete(targetSessionId);

      if (currentSessionId) {
        steps.push({
          sessionId: currentSessionId,
          slotIndex,
          type: "demote",
        });
        panelSessionIds.add(currentSessionId);
      }

      currentSlots[slotIndex] = targetSessionId;
      progressed = true;
    }

    if (progressed) {
      continue;
    }

    const cycleBreakSlotIndex = findCycleBreakSlotIndex(currentSlots, nextVisibleSessionIds);
    if (cycleBreakSlotIndex < 0) {
      break;
    }

    const cycleBreakSessionId = currentSlots[cycleBreakSlotIndex];
    if (!cycleBreakSessionId) {
      break;
    }

    steps.push({
      sessionId: cycleBreakSessionId,
      slotIndex: cycleBreakSlotIndex,
      type: "demote",
    });
    panelSessionIds.add(cycleBreakSessionId);
    currentSlots[cycleBreakSlotIndex] = undefined;
  }

  return steps;
}

function findCycleBreakSlotIndex(
  currentVisibleSessionIds: readonly (string | undefined)[],
  nextVisibleSessionIds: readonly string[],
): number {
  for (let slotIndex = currentVisibleSessionIds.length - 1; slotIndex >= 0; slotIndex -= 1) {
    if (currentVisibleSessionIds[slotIndex] !== nextVisibleSessionIds[slotIndex]) {
      return slotIndex;
    }
  }

  return -1;
}

function haveSameSessionIds(
  left: readonly (string | undefined)[],
  right: readonly string[],
): boolean {
  return (
    left.length === right.length && left.every((sessionId, index) => sessionId === right[index])
  );
}
