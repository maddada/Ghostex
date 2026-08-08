import {
  effectiveSidebarV2Settled,
  effectiveSidebarV2Snoozed,
  type SidebarV2LifecycleCapabilities,
} from "./sidebar-v2-lifecycle";
import type { SidebarV2ChangeRequestState, SidebarV2Session } from "./sidebar-v2-session";
import { firstValidTimestampMs, parseTimestampMs } from "./sidebar-v2-status";

/*
CDXC:SidebarV2 2026-07-29-00:00:
Ported from t3code `Sidebar.logic.ts` (`sortThreadsForSidebarV2`,
`sortSettledThreadsForSidebarV2`, `resolveSettledTimestamp`,
`resolveAdjacentThreadId`, `orderItemsByPreferredIds`).

The inbox sort is static creation order, newest on top. Activity NEVER reorders
the list — a row holds its position from creation until it settles, so the
screen only moves at lifecycle transitions. Status is carried by each card's
edge strip, not by position. Pinned rows float above the rest, matching V1's
pin semantics.

Ghostex caveat: `SidebarSessionItem` does not carry a creation timestamp yet
(gxserver's `GxserverPresentationSession.createdAt` exists but is not projected,
and `sortKey` is unusable here because it embeds `lastActiveAt` and therefore
moves on every activity change). Until the projection carries `createdAt`,
callers pass a first-seen ranking built with `reconcileSidebarV2CreationOrder`,
which is position-stable by construction.
*/

export type SidebarV2SortSession = Pick<
  SidebarV2Session,
  "createdAt" | "isPinned" | "sessionId"
>;

export type SidebarV2SortOptions = {
  /**
   * Explicit creation ranking (higher = newer). When supplied it REPLACES
   * `createdAt` for every row, so a partially-populated map cannot mix two
   * incomparable scales; unknown ids sort to the bottom.
   */
  creationRankById?: ReadonlyMap<string, number>;
};

function resolveCreationRank(
  session: SidebarV2SortSession,
  options: SidebarV2SortOptions,
): number {
  if (options.creationRankById) {
    return options.creationRankById.get(session.sessionId) ?? Number.NEGATIVE_INFINITY;
  }
  return parseTimestampMs(session.createdAt);
}

function compareSidebarV2Inbox(
  left: SidebarV2SortSession,
  right: SidebarV2SortSession,
  options: SidebarV2SortOptions,
): number {
  // Compared instead of subtracted so an unranked row (-Infinity) cannot
  // produce NaN and silently scramble the whole comparator.
  const leftRank = resolveCreationRank(left, options);
  const rightRank = resolveCreationRank(right, options);
  if (leftRank !== rightRank) {
    return rightRank > leftRank ? 1 : -1;
  }
  return left.sessionId.localeCompare(right.sessionId);
}

/**
 * Position-stable inbox order: pinned rows first, then newest-created first.
 * Pinned rows keep the host's persisted per-project order so the user can
 * rearrange that partition; unpinned rows remain creation-sorted and therefore
 * never jump in response to activity.
 */
export function sortSessionsForSidebarV2<T extends SidebarV2SortSession>(
  sessions: readonly T[],
  options: SidebarV2SortOptions = {},
): T[] {
  const pinned: T[] = [];
  const rest: T[] = [];
  for (const session of sessions) {
    (session.isPinned === true ? pinned : rest).push(session);
  }
  const byCreation = (left: T, right: T) => compareSidebarV2Inbox(left, right, options);
  return [...pinned, ...rest.sort(byCreation)];
}

/**
 * Rebuilds the newest-first first-seen order from the previous tick's order and
 * the ids present now. New sessions enter at the TOP (preserving their relative
 * incoming order), known sessions keep their exact slot, and removed sessions
 * drop out. This is the interim stand-in for a real `createdAt` and is pure:
 * the caller owns the carried `knownOrder`.
 */
export function reconcileSidebarV2CreationOrder(input: {
  knownOrder: readonly string[];
  sessionIds: readonly string[];
}): string[] {
  const presentIds = new Set(input.sessionIds);
  const knownIds = new Set(input.knownOrder);
  const newIds = input.sessionIds.filter((sessionId) => !knownIds.has(sessionId));
  const retainedIds = input.knownOrder.filter((sessionId) => presentIds.has(sessionId));
  return [...newIds, ...retainedIds];
}

/** Turns a newest-first id list into the rank map `sortSessionsForSidebarV2`
    consumes (higher rank = newer). */
export function createSidebarV2CreationRankMap(
  newestFirstSessionIds: readonly string[],
): Map<string, number> {
  const ranks = new Map<string, number>();
  const total = newestFirstSessionIds.length;
  for (const [index, sessionId] of newestFirstSessionIds.entries()) {
    ranks.set(sessionId, total - index);
  }
  return ranks;
}

export type SidebarV2SettledSortSession = Pick<
  SidebarV2Session,
  "lastInteractionAt" | "sessionId" | "settledAt" | "workingStartedAt"
>;

/**
 * The timestamp a settled row sorts and labels by: `settledAt` when stamped
 * (explicit settles), otherwise the meaningful-activity clock — the same
 * candidate the auto-settle window reads, so a session settled by inactivity
 * does not sort by an unrelated stamp.
 */
export function resolveSidebarV2SettledTimestampMs(
  session: SidebarV2SettledSortSession,
): number | null {
  return firstValidTimestampMs(
    session.settledAt,
    session.lastInteractionAt,
    session.workingStartedAt,
  );
}

/** Settled rows are history: they order by when the work ENDED, not by when the
    session was created or last touched. */
export function sortSettledSessionsForSidebarV2<T extends SidebarV2SettledSortSession>(
  sessions: readonly T[],
): T[] {
  return [...sessions].sort((left, right) => {
    const delta =
      (resolveSidebarV2SettledTimestampMs(right) ?? 0) -
      (resolveSidebarV2SettledTimestampMs(left) ?? 0);
    return delta || left.sessionId.localeCompare(right.sessionId);
  });
}

export type SidebarV2SnoozedSortSession = Pick<SidebarV2Session, "sessionId" | "snoozedUntil">;

/** The snoozed shelf reads as a schedule: soonest wake first. Rows with no
    usable wake time sink to the bottom instead of jumping to the top. */
export function sortSnoozedSessionsForSidebarV2<T extends SidebarV2SnoozedSortSession>(
  sessions: readonly T[],
): T[] {
  const wakeAtMs = (session: T) =>
    firstValidTimestampMs(session.snoozedUntil) ?? Number.POSITIVE_INFINITY;
  return [...sessions].sort((left, right) => {
    const leftWake = wakeAtMs(left);
    const rightWake = wakeAtMs(right);
    if (leftWake !== rightWake) {
      return leftWake < rightWake ? -1 : 1;
    }
    return left.sessionId.localeCompare(right.sessionId);
  });
}

export type SidebarV2Partition<T> = {
  active: T[];
  settled: T[];
  snoozed: T[];
};

export type SidebarV2PartitionOptions = {
  autoSettleAfterDays: number | null;
  capabilities?: SidebarV2LifecycleCapabilities;
  /** P3 pull-request state per session id; a merged/closed request auto-settles. */
  changeRequestStateBySessionId?: ReadonlyMap<string, SidebarV2ChangeRequestState | null>;
  creationRankById?: ReadonlyMap<string, number>;
  nowMs: number;
};

/**
 * Splits the inbox into its three shelves and sorts each with its own rule.
 * Snooze is checked before settle: a snoozed session is still "active" in the
 * data model, just suppressed until it wakes.
 */
export function partitionSidebarV2Sessions<T extends SidebarV2Session>(
  sessions: readonly T[],
  options: SidebarV2PartitionOptions,
): SidebarV2Partition<T> {
  const active: T[] = [];
  const settled: T[] = [];
  const snoozed: T[] = [];

  for (const session of sessions) {
    if (
      effectiveSidebarV2Snoozed(session, {
        capabilities: options.capabilities,
        nowMs: options.nowMs,
      })
    ) {
      snoozed.push(session);
      continue;
    }
    const isSettled = effectiveSidebarV2Settled(session, {
      autoSettleAfterDays: options.autoSettleAfterDays,
      capabilities: options.capabilities,
      changeRequestState: options.changeRequestStateBySessionId?.get(session.sessionId) ?? null,
      nowMs: options.nowMs,
    });
    (isSettled ? settled : active).push(session);
  }

  return {
    active: sortSessionsForSidebarV2(active, { creationRankById: options.creationRankById }),
    settled: sortSettledSessionsForSidebarV2(settled),
    snoozed: sortSnoozedSessionsForSidebarV2(snoozed),
  };
}

export type SidebarV2TraversalDirection = "next" | "previous";

/**
 * Keyboard traversal over the rendered row order. A current id that is not in
 * the list yields null rather than guessing a neighbor.
 */
export function resolveAdjacentSidebarV2SessionId<T>(input: {
  currentSessionId: T | null;
  direction: SidebarV2TraversalDirection;
  sessionIds: readonly T[];
}): T | null {
  const { currentSessionId, direction, sessionIds } = input;
  if (sessionIds.length === 0) {
    return null;
  }
  if (currentSessionId === null) {
    return direction === "previous" ? (sessionIds.at(-1) ?? null) : (sessionIds[0] ?? null);
  }

  const currentIndex = sessionIds.indexOf(currentSessionId);
  if (currentIndex === -1) {
    return null;
  }
  if (direction === "previous") {
    return currentIndex > 0 ? (sessionIds[currentIndex - 1] ?? null) : null;
  }
  return currentIndex < sessionIds.length - 1 ? (sessionIds[currentIndex + 1] ?? null) : null;
}

/**
 * Floats items matching `preferredIds` to the front in the requested order,
 * keeping every other item in its existing relative position. Used for scope
 * filters and collection ordering.
 */
export function orderItemsByPreferredIds<TItem, TId>(input: {
  getId: (item: TItem) => TId;
  getPreferenceIds?: (item: TItem) => readonly TId[];
  items: readonly TItem[];
  preferredIds: readonly TId[];
}): TItem[] {
  const { getId, getPreferenceIds, items, preferredIds } = input;
  if (preferredIds.length === 0) {
    return [...items];
  }

  const indexesByPreferenceId = new Map<TId, number[]>();
  for (const [index, item] of items.entries()) {
    const preferenceIds = getPreferenceIds?.(item) ?? [getId(item)];
    for (const preferenceId of new Set(preferenceIds)) {
      const indexes = indexesByPreferenceId.get(preferenceId);
      if (indexes) {
        indexes.push(index);
      } else {
        indexesByPreferenceId.set(preferenceId, [index]);
      }
    }
  }

  const emittedIndexes = new Set<number>();
  const ordered = preferredIds.flatMap((id) => {
    const index = indexesByPreferenceId.get(id)?.find((candidate) => !emittedIndexes.has(candidate));
    if (index === undefined) {
      return [];
    }
    emittedIndexes.add(index);
    return [items[index]!];
  });
  const remaining = items.filter((_, index) => !emittedIndexes.has(index));
  return [...ordered, ...remaining];
}
