import { describe, expect, test } from "vite-plus/test";
import {
  createSessionAlias,
  createSessionRecord,
  DEFAULT_TERMINAL_SESSION_TITLE,
  type SessionGridSnapshot,
} from "./session-grid-contract";
import {
  normalizeSessionGridSnapshot,
  removeSessionInSnapshot,
  renameSessionAliasInSnapshot,
  setSessionTitleInSnapshot,
  syncSessionOrderInSnapshot,
} from "./session-grid-state";

describe("syncSessionOrderInSnapshot", () => {
  test("should ignore invalid or incomplete order payloads", () => {
    const snapshot = {
      focusedSessionId: "session-1",
      sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1), createSessionRecord(3, 2)],
      viewMode: "grid" as const,
      visibleCount: 2 as const,
      visibleSessionIds: ["session-1", "session-2"],
    };
    const normalizedSnapshot = normalizeSessionGridSnapshot(snapshot);

    const duplicate = syncSessionOrderInSnapshot(snapshot, ["session-2", "session-2", "session-1"]);
    const missing = syncSessionOrderInSnapshot(snapshot, ["session-2", "session-1"]);

    expect(duplicate.changed).toBe(false);
    expect(duplicate.snapshot).toEqual(normalizedSnapshot);
    expect(missing.changed).toBe(false);
    expect(missing.snapshot).toEqual(normalizedSnapshot);
  });
});

describe("renameSessionAliasInSnapshot", () => {
  test("should update the session alias when the trimmed value is non-empty", () => {
    const result = renameSessionAliasInSnapshot(
      {
        focusedSessionId: "session-1",
        sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
        viewMode: "grid",
        visibleCount: 2,
        visibleSessionIds: ["session-1", "session-2"],
      },
      "session-2",
      "  Build Logs  ",
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.sessions.map((session) => session.alias)).toEqual([
      createSessionAlias(1, 0),
      "Build Logs",
    ]);
  });

  test("should ignore empty aliases", () => {
    const snapshot = {
      focusedSessionId: "session-1",
      sessions: [createSessionRecord(1, 0)],
      viewMode: "grid",
      visibleCount: 1 as const,
      visibleSessionIds: ["session-1"],
    } satisfies SessionGridSnapshot;
    const normalizedSnapshot = normalizeSessionGridSnapshot(snapshot);

    const result = renameSessionAliasInSnapshot(snapshot, "session-1", "   ");

    expect(result.changed).toBe(false);
    expect(result.snapshot).toEqual(normalizedSnapshot);
  });

  test("should ignore numeric-only aliases", () => {
    const snapshot = {
      focusedSessionId: "session-1",
      sessions: [createSessionRecord(1, 0)],
      viewMode: "grid",
      visibleCount: 1 as const,
      visibleSessionIds: ["session-1"],
    } satisfies SessionGridSnapshot;
    const normalizedSnapshot = normalizeSessionGridSnapshot(snapshot);

    const result = renameSessionAliasInSnapshot(snapshot, "session-1", " 123 ");

    expect(result.changed).toBe(false);
    expect(result.snapshot).toEqual(normalizedSnapshot);
  });
});

describe("setSessionTitleInSnapshot", () => {
  test("should update the primary session title while keeping the alias untouched", () => {
    const result = setSessionTitleInSnapshot(
      {
        focusedSessionId: "session-1",
        sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
        viewMode: "grid",
        visibleCount: 2,
        visibleSessionIds: ["session-1", "session-2"],
      },
      "session-2",
      "  Claude: Fix Sidebar  ",
    );

    expect(result.changed).toBe(true);
    /**
     * CDXC:SessionTitles 2026-05-12-09:43
     * Title mutation should update only the targeted primary title. Untouched
     * sessions keep the shared default terminal title instead of reverting to a
     * legacy alias-style title such as "Session 1".
     */
    expect(result.snapshot.sessions.map((session) => session.title)).toEqual([
      DEFAULT_TERMINAL_SESSION_TITLE,
      "Claude: Fix Sidebar",
    ]);
    expect(result.snapshot.sessions.map((session) => session.alias)).toEqual([
      createSessionAlias(1, 0),
      createSessionAlias(2, 1),
    ]);
  });
});

describe("removeSessionInSnapshot", () => {
  test("should remove the session from the grid and keep a valid focus target", () => {
    const result = removeSessionInSnapshot(
      {
        focusedSessionId: "session-2",
        sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1), createSessionRecord(3, 2)],
        viewMode: "grid",
        visibleCount: 2,
        visibleSessionIds: ["session-2", "session-3"],
      },
      "session-2",
    );

    expect(result.changed).toBe(true);
    expect(result.snapshot.sessions.map((session) => session.sessionId)).toEqual([
      "session-1",
      "session-3",
    ]);
    expect(result.snapshot.focusedSessionId).toBe("session-3");
    expect(result.snapshot.visibleSessionIds).toEqual(["session-3", "session-1"]);
  });
});
