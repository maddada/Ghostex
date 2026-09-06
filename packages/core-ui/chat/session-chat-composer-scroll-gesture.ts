export interface SessionChatComposerScrollGesture {
  accumulatedDeltaPx: number;
  collapseSuppressed: boolean;
  lastEventAt: number;
}

export function createSessionChatComposerScrollGesture(): SessionChatComposerScrollGesture {
  return { accumulatedDeltaPx: 0, collapseSuppressed: false, lastEventAt: -Infinity };
}

export function resetSessionChatComposerScrollGesture(gesture: SessionChatComposerScrollGesture): void {
  gesture.accumulatedDeltaPx = 0;
  gesture.collapseSuppressed = false;
  gesture.lastEventAt = -Infinity;
}

export function suppressSessionChatComposerScrollGesture(
  gesture: SessionChatComposerScrollGesture,
  now: number,
  gestureResetMs: number
): void {
  if (now - gesture.lastEventAt <= gestureResetMs) gesture.collapseSuppressed = true;
}

export function recordSessionChatComposerScrollGesture(
  gesture: SessionChatComposerScrollGesture,
  input: {
    now: number;
    deltaPx: number;
    collapseThresholdPx: number;
    collapseEligible: boolean;
    canScrollInGestureDirection: boolean;
    scrollsTowardLogicalEnd: boolean;
  }
): boolean {
  gesture.lastEventAt = input.now;
  if (
    gesture.collapseSuppressed ||
    !input.collapseEligible ||
    !input.canScrollInGestureDirection ||
    input.scrollsTowardLogicalEnd
  ) {
    gesture.accumulatedDeltaPx = 0;
    return false;
  }
  gesture.accumulatedDeltaPx += input.deltaPx;
  if (gesture.accumulatedDeltaPx < input.collapseThresholdPx) return false;
  gesture.accumulatedDeltaPx = 0;
  return true;
}
