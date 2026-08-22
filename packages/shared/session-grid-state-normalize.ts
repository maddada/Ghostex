import {
  type SessionGridSnapshot,
  clampTerminalViewMode,
  clampVisibleSessionCount,
  createDefaultSessionGridSnapshot,
  getOrderedSessions,
} from "./session-grid-contract";
import {
  normalizeFullscreenRestoreVisibleCount,
  normalizeSessionRecord,
  normalizeVisibleSessionIds,
} from "./session-grid-state-helpers";

export function normalizeSessionGridSnapshot(
  snapshot: SessionGridSnapshot | undefined,
): SessionGridSnapshot {
  const normalizedSnapshot = snapshot ?? createDefaultSessionGridSnapshot();
  const orderedSessions = getOrderedSessions({
    ...normalizedSnapshot,
    sessions: normalizedSnapshot.sessions.map((session) => normalizeSessionRecord(session)),
  });
  const sessionIds = new Set(orderedSessions.map((session) => session.sessionId));
  const visibleCount = clampVisibleSessionCount(normalizedSnapshot.visibleCount);
  const viewMode = clampTerminalViewMode(normalizedSnapshot.viewMode);

  const focusedSessionId =
    normalizedSnapshot.focusedSessionId && sessionIds.has(normalizedSnapshot.focusedSessionId)
      ? normalizedSnapshot.focusedSessionId
      : orderedSessions[0]?.sessionId;
  const normalizedVisibleIds = normalizeVisibleSessionIds(
    orderedSessions,
    normalizedSnapshot.visibleSessionIds,
    Math.min(visibleCount, orderedSessions.length),
    focusedSessionId,
  );

  return {
    focusedSessionId,
    fullscreenRestoreVisibleCount: normalizeFullscreenRestoreVisibleCount(
      normalizedSnapshot.fullscreenRestoreVisibleCount,
      visibleCount,
    ),
    /**
     * CDXC:PaneFocus 2026-05-15-13:31:
     * Generic snapshot normalization must preserve the persisted paneLayout tree.
     * Directional focus hotkeys and other legacy snapshot helpers normalize before focusing; dropping paneLayout there makes native sync rebuild grouped tabs as separate leaves.
     */
    ...(normalizedSnapshot.paneLayout ? { paneLayout: normalizedSnapshot.paneLayout } : {}),
    sessions: orderedSessions,
    visibleCount,
    visibleSessionIds: normalizedVisibleIds,
    viewMode,
  };
}
