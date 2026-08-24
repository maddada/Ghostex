import {
  createDefaultSessionGridSnapshot,
  formatSessionDisplayId,
  getOrderedSessions,
  type SessionGridSnapshot,
  type SessionRecord,
} from './session-grid-contract';
import { reindexSessionsInOrder } from './session-grid-state-helpers';

export type ReorderGroupSessionsResult = {
  changed: boolean;
  snapshot: SessionGridSnapshot;
};

export function reorderGroupSessions(
  snapshot: SessionGridSnapshot,
  sessionIds: readonly string[]
): ReorderGroupSessionsResult {
  const currentSessions = getOrderedSessions({
    ...createDefaultSessionGridSnapshot(),
    ...snapshot,
    /**
     * CDXC:NativePaneReorder 2026-05-03-02:50
     * Native header drag-to-reorder works across every visible pane type:
     * terminal and browser sessions. Keep browser records in the reorder set so a
     * drop involving a WKWebView pane mutates the same session order that
     * native layout renders.
     */
    sessions: snapshot.sessions,
  });
  const currentExactSessionIds = currentSessions.map((session) => session.sessionId);
  const currentCanonicalSessionIds = currentSessions.map((session) => createCanonicalSessionId(session.displayId));
  if (sessionIds.length !== currentExactSessionIds.length) {
    return { changed: false, snapshot };
  }

  const incomingSessionIdSet = new Set(sessionIds);
  if (incomingSessionIdSet.size !== sessionIds.length) {
    return { changed: false, snapshot };
  }
  const matchesExactIds = currentExactSessionIds.every((sessionId) => incomingSessionIdSet.has(sessionId));
  const matchesCanonicalIds = currentCanonicalSessionIds.every((sessionId) => incomingSessionIdSet.has(sessionId));
  if (!matchesExactIds && !matchesCanonicalIds) {
    return { changed: false, snapshot };
  }

  const currentComparableSessionIds = matchesExactIds ? currentExactSessionIds : currentCanonicalSessionIds;
  if (currentComparableSessionIds.every((sessionId, index) => sessionId === sessionIds[index])) {
    return { changed: false, snapshot };
  }

  const sessionByComparableId = new Map<string, SessionRecord>();
  for (const session of currentSessions) {
    sessionByComparableId.set(session.sessionId, session);
    sessionByComparableId.set(createCanonicalSessionId(session.displayId), session);
  }

  const seenSessions = new Set<SessionRecord>();
  const reorderedSessions: SessionRecord[] = [];
  for (const sessionId of sessionIds) {
    const session = sessionByComparableId.get(sessionId);
    if (!session || seenSessions.has(session)) {
      return { changed: false, snapshot };
    }

    seenSessions.add(session);
    reorderedSessions.push(session);
  }

  const reindexedSessions = reindexSessionsInOrder(reorderedSessions);
  const nextVisibleSessionIds = sessionIds.slice(0, Math.min(snapshot.visibleCount, sessionIds.length));
  const focusedSession = snapshot.focusedSessionId ? sessionByComparableId.get(snapshot.focusedSessionId) : undefined;
  const focusedComparableSessionId = focusedSession
    ? matchesExactIds
      ? focusedSession.sessionId
      : createCanonicalSessionId(focusedSession.displayId)
    : undefined;

  return {
    changed: true,
    snapshot: {
      ...snapshot,
      focusedSessionId:
        focusedComparableSessionId && nextVisibleSessionIds.includes(focusedComparableSessionId)
          ? focusedComparableSessionId
          : nextVisibleSessionIds[0],
      sessions: reindexedSessions,
      visibleSessionIds: nextVisibleSessionIds,
    },
  };
}

function createCanonicalSessionId(displayId: string | number | undefined): string {
  if (typeof displayId === 'string' && /^(?:g|s)-[a-z0-9-]+$/i.test(displayId.trim())) {
    return displayId.trim();
  }

  return `session-${formatSessionDisplayId(displayId ?? 0)}`;
}
