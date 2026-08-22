import {
  isNumericSessionAlias,
  type SessionGridSnapshot,
  type SessionRecord,
  type TerminalViewMode,
  type VisibleSessionCount,
} from "./session-grid-contract";
import {
  reindexSessionsInOrder,
  restoreLayoutVisibleCountInSnapshot,
} from "./session-grid-state-helpers";
import { normalizeSessionGridSnapshot } from "./session-grid-state-normalize";

export function setVisibleCountInSnapshot(
  snapshot: SessionGridSnapshot,
  visibleCount: VisibleSessionCount,
): SessionGridSnapshot {
  return normalizeSessionGridSnapshot({
    ...normalizeSessionGridSnapshot(snapshot),
    fullscreenRestoreVisibleCount: undefined,
    visibleCount,
  });
}

export function toggleFullscreenSessionInSnapshot(
  snapshot: SessionGridSnapshot,
): SessionGridSnapshot {
  const normalizedSnapshot = normalizeSessionGridSnapshot(snapshot);
  if (normalizedSnapshot.visibleCount === 1 && normalizedSnapshot.fullscreenRestoreVisibleCount) {
    return restoreLayoutVisibleCountInSnapshot(normalizedSnapshot, normalizeSessionGridSnapshot);
  }

  return normalizeSessionGridSnapshot({
    ...normalizedSnapshot,
    fullscreenRestoreVisibleCount:
      normalizedSnapshot.visibleCount > 1 ? normalizedSnapshot.visibleCount : undefined,
    visibleCount: 1,
  });
}

export function setViewModeInSnapshot(
  snapshot: SessionGridSnapshot,
  viewMode: TerminalViewMode,
): SessionGridSnapshot {
  return normalizeSessionGridSnapshot({
    ...restoreLayoutVisibleCountInSnapshot(snapshot, normalizeSessionGridSnapshot),
    viewMode,
  });
}

export function syncSessionOrderInSnapshot(
  snapshot: SessionGridSnapshot,
  sessionIds: readonly string[],
): { changed: boolean; snapshot: SessionGridSnapshot } {
  const normalizedSnapshot = normalizeSessionGridSnapshot(snapshot);
  const currentSessionIds = normalizedSnapshot.sessions
    .slice()
    .sort((left: SessionRecord, right: SessionRecord) => left.slotIndex - right.slotIndex)
    .map((session) => session.sessionId);
  if (sessionIds.length !== currentSessionIds.length) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  if (currentSessionIds.every((sessionId, index) => sessionId === sessionIds[index])) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const currentSessionIdSet = new Set(currentSessionIds);
  const incomingSessionIdSet = new Set(sessionIds);
  if (
    incomingSessionIdSet.size !== sessionIds.length ||
    currentSessionIds.some((sessionId) => !incomingSessionIdSet.has(sessionId)) ||
    sessionIds.some((sessionId) => !currentSessionIdSet.has(sessionId))
  ) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const sessionById = new Map(
    normalizedSnapshot.sessions.map((session): [string, SessionRecord] => [
      session.sessionId,
      session,
    ]),
  );
  const sessions = reindexSessionsInOrder(
    sessionIds.map((sessionId) => {
      const session = sessionById.get(sessionId);
      if (!session) {
        throw new Error(`Missing session for reorder: ${sessionId}`);
      }
      return session;
    }),
  );
  const visibleSessionIds = sessionIds.slice(
    0,
    Math.min(normalizedSnapshot.visibleCount, sessions.length),
  );

  return {
    changed: true,
    snapshot: normalizeSessionGridSnapshot({
      ...normalizedSnapshot,
      focusedSessionId:
        normalizedSnapshot.focusedSessionId &&
        visibleSessionIds.includes(normalizedSnapshot.focusedSessionId)
          ? normalizedSnapshot.focusedSessionId
          : visibleSessionIds[0],
      sessions,
      visibleSessionIds,
    }),
  };
}

export function renameSessionAliasInSnapshot(
  snapshot: SessionGridSnapshot,
  sessionId: string,
  alias: string,
): { changed: boolean; snapshot: SessionGridSnapshot } {
  const normalizedAlias = alias.trim();
  if (!normalizedAlias || isNumericSessionAlias(normalizedAlias)) {
    return { changed: false, snapshot: normalizeSessionGridSnapshot(snapshot) };
  }

  return updateSession(snapshot, sessionId, normalizedAlias, "alias");
}

export function setSessionTitleInSnapshot(
  snapshot: SessionGridSnapshot,
  sessionId: string,
  title: string,
): { changed: boolean; snapshot: SessionGridSnapshot } {
  return updateSession(snapshot, sessionId, title.trim(), "title");
}

export function setBrowserSessionMetadataInSnapshot(
  snapshot: SessionGridSnapshot,
  sessionId: string,
  title: string,
  url: string,
): { changed: boolean; snapshot: SessionGridSnapshot } {
  const normalizedSnapshot = normalizeSessionGridSnapshot(snapshot);
  const normalizedTitle = title.trim();
  const normalizedUrl = url.trim();
  if (!normalizedTitle || !normalizedUrl) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  let changed = false;
  const sessions = normalizedSnapshot.sessions.map((session) => {
    if (session.sessionId !== sessionId || session.kind !== "browser") {
      return session;
    }
    if (session.title === normalizedTitle && session.browser.url === normalizedUrl) {
      return session;
    }

    changed = true;
    return {
      ...session,
      browser: { ...session.browser, url: normalizedUrl },
      title: normalizedTitle,
    };
  });

  return {
    changed,
    snapshot: changed
      ? normalizeSessionGridSnapshot({ ...normalizedSnapshot, sessions })
      : normalizedSnapshot,
  };
}

export function removeSessionInSnapshot(
  snapshot: SessionGridSnapshot,
  sessionId: string,
): { changed: boolean; snapshot: SessionGridSnapshot } {
  const normalizedSnapshot = normalizeSessionGridSnapshot(snapshot);
  if (
    !normalizedSnapshot.sessions.some((session: SessionRecord) => session.sessionId === sessionId)
  ) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const sessions = normalizedSnapshot.sessions.filter(
    (session: SessionRecord) => session.sessionId !== sessionId,
  );
  const visibleSessionIds = normalizedSnapshot.visibleSessionIds.filter(
    (visibleSessionId) => visibleSessionId !== sessionId,
  );

  return {
    changed: true,
    snapshot: normalizeSessionGridSnapshot({
      ...normalizedSnapshot,
      focusedSessionId:
        normalizedSnapshot.focusedSessionId === sessionId
          ? (visibleSessionIds[0] ?? sessions[0]?.sessionId)
          : normalizedSnapshot.focusedSessionId,
      sessions,
      visibleSessionIds,
    }),
  };
}

function updateSession(
  snapshot: SessionGridSnapshot,
  sessionId: string,
  value: string,
  key: "alias" | "title",
): { changed: boolean; snapshot: SessionGridSnapshot } {
  const normalizedSnapshot = normalizeSessionGridSnapshot(snapshot);
  if (!value) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  let changed = false;
  const sessions = normalizedSnapshot.sessions.map((session) => {
    if (session.sessionId !== sessionId || session[key] === value) {
      return session;
    }

    changed = true;
    return { ...session, [key]: value };
  });

  return {
    changed,
    snapshot: changed
      ? normalizeSessionGridSnapshot({ ...normalizedSnapshot, sessions })
      : normalizedSnapshot,
  };
}
