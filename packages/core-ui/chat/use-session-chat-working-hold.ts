// The transcript's settle debounce. The live working status genuinely flaps
// around turn boundaries (hook activity drops between a turn ending and its
// Stop hooks / follow-up turn starting), and every false blip used to settle
// the transcript instantly — folding the newest turn into "Worked for Xs",
// then unfolding it when the signal came back, repeatedly. Settling is a
// destructive layout change (it collapses the turn and moves the viewport),
// so it only happens after the session has been CONTINUOUSLY non-working for
// the hold period. Going working again is applied instantly.

import { useEffect, useState } from 'react';

/** How long the session must stay non-working before the transcript settles. */
export const SESSION_CHAT_SETTLE_HOLD_MS = 8_000;

export function useSessionChatWorkingHold(working: boolean, holdMs: number = SESSION_CHAT_SETTLE_HOLD_MS): boolean {
  // Seeded with the mount value: a session opened already-settled folds
  // immediately instead of rendering expanded for the hold period and then
  // collapsing under the reader.
  const [heldWorking, setHeldWorking] = useState(working);
  useEffect(() => {
    if (working) {
      setHeldWorking(true);
      return undefined;
    }
    const timer = setTimeout(() => setHeldWorking(false), holdMs);
    return () => clearTimeout(timer);
  }, [holdMs, working]);
  return working || heldWorking;
}
