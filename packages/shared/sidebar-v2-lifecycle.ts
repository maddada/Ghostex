import type { SidebarV2ChangeRequestState, SidebarV2Session } from "./sidebar-v2-session";
import { firstValidTimestampMs } from "./sidebar-v2-status";

/*
CDXC:SidebarV2 2026-07-29-00:00:
Sidebar inbox lifecycle and settlement rules.
(`effectiveSettled`, `effectiveSnoozed`, `canSettle`, `canSnooze`,
`threadWokeAt`, `threadRaisedHandWhileSnoozed`, `threadLastActivityAt`).

Ghostex adaptations:
- A provider's "session running/starting" blocker maps to `activity: "working"`.
  `lifecycleState: "running"` is pane liveness, not work, and must not block a
  settle — every awake terminal would be unsettleable otherwise.
- A provider-side queued-turn grace window has no Ghostex twin: a Ghostex
  session has no "message sent but no turn adopted it" state, because the text
  goes straight into the terminal and gxserver flips `activity` to working. It
  is therefore dropped rather than emulated.
- A provider's latest completed-turn marker maps to
  gxserver's meaningful-activity clock `lastInteractionAt`, which advances when
  a run finishes and deliberately does not advance for short working blips.

Everything is pure: callers pass `nowMs`, matching the quantized clock the V2
render tree already ticks on.
*/

const DAY_MS = 24 * 60 * 60 * 1_000;

/**
 * gxserver capability flags. Older remote daemons publish no lifecycle state,
 * so the affordances hide and nothing auto-settles instead of the client
 * inventing lifecycle out of derived data.
 */
export type SidebarV2LifecycleCapabilities = {
  settle: boolean;
  snooze: boolean;
};

export const SIDEBAR_V2_LIFECYCLE_CAPABILITIES_ENABLED: SidebarV2LifecycleCapabilities = {
  settle: true,
  snooze: true,
};

export const SIDEBAR_V2_LIFECYCLE_CAPABILITIES_DISABLED: SidebarV2LifecycleCapabilities = {
  settle: false,
  snooze: false,
};

export type SidebarV2LifecycleSession = Pick<
  SidebarV2Session,
  | "activity"
  | "lastInteractionAt"
  | "lifecycleState"
  | "settledAt"
  | "settledOverride"
  | "snoozedAt"
  | "snoozedUntil"
  | "workingStartedAt"
>;

export type SidebarV2SettleOptions = {
  /** Days of inactivity before an unpinned session auto-settles. `null`
      disables inactivity auto-settle entirely. */
  autoSettleAfterDays: number | null;
  capabilities?: SidebarV2LifecycleCapabilities;
  /** P3: a merged/closed pull request auto-settles immediately. */
  changeRequestState?: SidebarV2ChangeRequestState | null;
  nowMs: number;
};

export type SidebarV2SnoozeOptions = {
  capabilities?: SidebarV2LifecycleCapabilities;
  nowMs: number;
};

/** The meaningful-activity clock a session's settle window counts from. */
export function sessionLastActivityAtMs(
  session: Pick<SidebarV2Session, "lastInteractionAt" | "workingStartedAt">,
): number | null {
  const lastInteractionAtMs = firstValidTimestampMs(session.lastInteractionAt);
  const workingStartedAtMs = firstValidTimestampMs(session.workingStartedAt);
  if (lastInteractionAtMs === null) {
    return workingStartedAtMs;
  }
  if (workingStartedAtMs === null) {
    return lastInteractionAtMs;
  }
  return Math.max(lastInteractionAtMs, workingStartedAtMs);
}

/** The agent is blocked on the user. Never hide this, whatever the override. */
export function isSidebarV2SessionBlockedOnUser(
  session: Pick<SidebarV2Session, "activity">,
): boolean {
  return session.activity === "attention";
}

/** The agent is in motion. Ghostex reads this from `activity`, never from
    `lifecycleState`, which only reports pane liveness. */
export function isSidebarV2SessionWorking(
  session: Pick<SidebarV2Session, "activity">,
): boolean {
  return session.activity === "working";
}

/**
 * A session may be settled only when none of `effectiveSettled`'s activity
 * blockers hold. Deliberately the same list: anything the partition refuses to
 * CLASSIFY as settled must also be refused as a settle TARGET.
 */
export function canSettleSidebarV2Session(
  session: Pick<SidebarV2Session, "activity">,
  options: { capabilities?: SidebarV2LifecycleCapabilities } = {},
): boolean {
  if (options.capabilities !== undefined && !options.capabilities.settle) {
    return false;
  }
  return !isSidebarV2SessionBlockedOnUser(session) && !isSidebarV2SessionWorking(session);
}

/**
 * A session may be snoozed unless the agent is blocked on the user: hiding a
 * pending approval or input request defeats the request. A working session IS
 * snoozable — snooze only affects visibility, never the agent.
 */
export function canSnoozeSidebarV2Session(
  session: Pick<SidebarV2Session, "activity">,
  options: { capabilities?: SidebarV2LifecycleCapabilities } = {},
): boolean {
  if (options.capabilities !== undefined && !options.capabilities.snooze) {
    return false;
  }
  return !isSidebarV2SessionBlockedOnUser(session);
}

/**
 * A snoozed session "raises its hand" when something outranks the user's
 * snooze: the agent is blocked on them, the session failed, or work completed
 * after the snooze was set. Raising a hand never clears the server-side snooze
 * fields; it only stops the session from CLASSIFYING as snoozed.
 */
export function sidebarV2SessionRaisedHandWhileSnoozed(
  session: SidebarV2LifecycleSession,
): boolean {
  if (isSidebarV2SessionBlockedOnUser(session)) {
    return true;
  }
  const snoozedAtMs = firstValidTimestampMs(session.snoozedAt);
  const lastActivityMs = sessionLastActivityAtMs(session);

  // Only a FRESH failure raises the hand: a session snoozed while already
  // failed stays snoozed — that snooze was the user saying "I saw it, not now".
  if (session.lifecycleState === "error") {
    if (snoozedAtMs === null) {
      return true;
    }
    if (lastActivityMs !== null && lastActivityMs > snoozedAtMs) {
      return true;
    }
    return false;
  }

  // Work that finished after the snooze is new information the user asked to
  // be told about ("something happened" wakes early).
  if (
    snoozedAtMs !== null &&
    session.activity === "idle" &&
    lastActivityMs !== null &&
    lastActivityMs > snoozedAtMs
  ) {
    return true;
  }
  return false;
}

/**
 * Snoozed resolution: hidden from the inbox while the wake time is in the
 * future and the session has not raised its hand. Timer wakes are derived — no
 * event fires when `snoozedUntil` passes; the stale fields simply stop
 * classifying as snoozed.
 */
export function effectiveSidebarV2Snoozed(
  session: SidebarV2LifecycleSession,
  options: SidebarV2SnoozeOptions,
): boolean {
  if (options.capabilities !== undefined && !options.capabilities.snooze) {
    return false;
  }
  const wakeAtMs = firstValidTimestampMs(session.snoozedUntil);
  // Missing or malformed data never hides a session.
  if (wakeAtMs === null) {
    return false;
  }
  if (wakeAtMs <= options.nowMs) {
    return false;
  }
  return !sidebarV2SessionRaisedHandWhileSnoozed(session);
}

/**
 * When a previously-snoozed session woke, or null if it never snoozed / is
 * still snoozed. The inbox sort is deliberately static, so a woken row
 * reappears in its original position and the wake signal has to carry the
 * weight. Compare against the client's last-visited stamp — visiting clears the
 * indicator like it clears unread.
 */
export function sidebarV2SessionWokeAtMs(
  session: SidebarV2LifecycleSession,
  options: SidebarV2SnoozeOptions,
): number | null {
  if (options.capabilities !== undefined && !options.capabilities.snooze) {
    return null;
  }
  const wakeAtMs = firstValidTimestampMs(session.snoozedUntil);
  if (wakeAtMs === null) {
    return null;
  }
  // An early hand-raise wake stays authoritative even after the scheduled wake
  // time passes: reporting `snoozedUntil` then would resurface a Woke indicator
  // the user already cleared by visiting.
  if (sidebarV2SessionRaisedHandWhileSnoozed(session)) {
    return sessionLastActivityAtMs(session) ?? firstValidTimestampMs(session.snoozedAt) ?? wakeAtMs;
  }
  return wakeAtMs <= options.nowMs ? wakeAtMs : null;
}

/**
 * Settled resolution over the server-backed settled lifecycle. Activity
 * blockers (attention, working) are checked first and hold a session active
 * regardless of any override. Past the blockers the explicit user override
 * wins in both directions; without one, a session auto-settles on a
 * merged/closed pull request immediately, or on inactivity past the window.
 */
export function effectiveSidebarV2Settled(
  session: SidebarV2LifecycleSession,
  options: SidebarV2SettleOptions,
): boolean {
  if (options.capabilities !== undefined && !options.capabilities.settle) {
    return false;
  }
  // Blocked or in-motion work must remain visible even when explicitly settled.
  if (isSidebarV2SessionBlockedOnUser(session) || isSidebarV2SessionWorking(session)) {
    return false;
  }
  if (session.settledOverride === "settled") {
    return true;
  }
  // "active" is the explicit keep-active pin: it suppresses auto-settle until
  // real activity clears it server-side.
  if (session.settledOverride === "active") {
    return false;
  }
  if (options.changeRequestState === "merged" || options.changeRequestState === "closed") {
    return true;
  }
  if (options.autoSettleAfterDays === null) {
    return false;
  }

  const lastActivityMs = sessionLastActivityAtMs(session);
  if (lastActivityMs === null) {
    return false;
  }
  return lastActivityMs < options.nowMs - options.autoSettleAfterDays * DAY_MS;
}

export type SidebarV2AutoSettleReason = "changeRequest" | "inactivity";

export type SidebarV2AutoSettleDecision = {
  reason: SidebarV2AutoSettleReason | null;
  shouldAutoSettle: boolean;
};

/**
 * Why a session would auto-settle right now. Split out from
 * `effectiveSidebarV2Settled` so the settled shelf can explain itself ("merged"
 * vs "idle for 3 days") without re-deriving the rule.
 */
export function resolveSidebarV2AutoSettleDecision(
  session: SidebarV2LifecycleSession,
  options: SidebarV2SettleOptions,
): SidebarV2AutoSettleDecision {
  if (options.capabilities !== undefined && !options.capabilities.settle) {
    return { reason: null, shouldAutoSettle: false };
  }
  if (!canSettleSidebarV2Session(session)) {
    return { reason: null, shouldAutoSettle: false };
  }
  if (session.settledOverride === "active") {
    return { reason: null, shouldAutoSettle: false };
  }
  if (options.changeRequestState === "merged" || options.changeRequestState === "closed") {
    return { reason: "changeRequest", shouldAutoSettle: true };
  }
  if (options.autoSettleAfterDays === null) {
    return { reason: null, shouldAutoSettle: false };
  }
  const lastActivityMs = sessionLastActivityAtMs(session);
  if (lastActivityMs === null) {
    return { reason: null, shouldAutoSettle: false };
  }
  const isInactive = lastActivityMs < options.nowMs - options.autoSettleAfterDays * DAY_MS;
  return {
    reason: isInactive ? "inactivity" : null,
    shouldAutoSettle: isInactive,
  };
}

/**
 * The next moment this session's classification can change on its own — the
 * exact-boundary wake timer the V2 shelf schedules instead of polling.
 */
export function resolveSidebarV2NextWakeAtMs(
  session: SidebarV2LifecycleSession,
  options: SidebarV2SnoozeOptions,
): number | null {
  if (!effectiveSidebarV2Snoozed(session, options)) {
    return null;
  }
  return firstValidTimestampMs(session.snoozedUntil);
}
