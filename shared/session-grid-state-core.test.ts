import { describe, expect, test } from "vite-plus/test";
import {
  clampAgentManagerZoomPercent,
  clampTerminalViewMode,
  createDefaultSessionGridSnapshot,
  createAgentSessionDefaultTitle,
  createSidebarHudState,
  createSidebarSessionItems,
  createSessionAlias,
  createSessionRecord,
  DEFAULT_TERMINAL_SESSION_TITLE,
  getPreferredSessionTitle,
  getTerminalSessionSurfaceTitle,
  getSessionShortcutLabel,
  getVisiblePrimaryTitle,
  getVisibleTerminalTitle,
  isDefaultSessionSearchTitle,
  normalizeSessionRenameTitle,
  normalizeTerminalTitle,
} from "./session-grid-contract";
import { createSessionInSnapshot, normalizeSessionGridSnapshot } from "./session-grid-state";
import { normalizeSessionRecord } from "./session-grid-state-helpers";

describe("createSessionInSnapshot", () => {
  test("should allocate the lowest free slot in row-major order", () => {
    let snapshot = createDefaultSessionGridSnapshot();

    const first = createSessionInSnapshot(snapshot, 1);
    snapshot = first.snapshot;
    const second = createSessionInSnapshot(snapshot, 2);
    snapshot = second.snapshot;
    const third = createSessionInSnapshot(snapshot, 3);

    expect(first.session?.slotIndex).toBe(0);
    expect(second.session?.slotIndex).toBe(1);
    expect(third.session?.slotIndex).toBe(2);
    expect(third.snapshot.sessions.map((session) => session.title)).toEqual([
      DEFAULT_TERMINAL_SESSION_TITLE,
      DEFAULT_TERMINAL_SESSION_TITLE,
      DEFAULT_TERMINAL_SESSION_TITLE,
    ]);
    expect(third.snapshot.sessions.map((session) => session.alias)).toEqual([
      createSessionAlias(1, 0),
      createSessionAlias(2, 1),
      createSessionAlias(3, 2),
    ]);
  });

  test("should append a new session to the end of the current ordered list", () => {
    const result = createSessionInSnapshot(
      {
        focusedSessionId: "session-2",
        sessions: [createSessionRecord(1, 0), createSessionRecord(2, 2)],
        viewMode: "grid",
        visibleCount: 3,
        visibleSessionIds: ["session-1", "session-2"],
      },
      3,
    );

    expect(result.session?.sessionId).toBe("session-3");
    expect(result.session?.slotIndex).toBe(2);
    expect(result.snapshot.sessions.map((session) => session.sessionId)).toEqual([
      "session-1",
      "session-2",
      "session-3",
    ]);
    expect(result.snapshot.sessions.map((session) => session.slotIndex)).toEqual([0, 1, 2]);
    expect(result.snapshot.focusedSessionId).toBe("session-3");
    expect(result.snapshot.visibleSessionIds).toEqual(["session-1", "session-2", "session-3"]);
  });

  test("should create native Ghostty terminal sessions", () => {
    const result = createSessionInSnapshot(createDefaultSessionGridSnapshot(), 1, {
      terminalEngine: "ghostty-native",
      title: "Session 1",
    });

    expect(result.session?.kind).toBe("terminal");
    expect(
      result.session && "terminalEngine" in result.session
        ? result.session.terminalEngine
        : undefined,
    ).toBe("ghostty-native");
  });

  test("should create and normalize sessions beyond the old fixed pane cap", () => {
    let snapshot = createDefaultSessionGridSnapshot();
    for (let index = 1; index <= 12; index += 1) {
      const result = createSessionInSnapshot(snapshot, index);
      snapshot = result.snapshot;
    }

    expect(snapshot.sessions).toHaveLength(12);
    expect(snapshot.sessions.at(-1)?.slotIndex).toBe(11);
    expect(normalizeSessionGridSnapshot(snapshot).sessions).toHaveLength(12);
  });
});

describe("normalizeSessionGridSnapshot", () => {
  test("should preserve unbounded visible count and keep the focused session visible", () => {
    const snapshot = normalizeSessionGridSnapshot({
      focusedSessionId: "session-3",
      fullscreenRestoreVisibleCount: 4,
      sessions: [
        createSessionRecord(1, 0),
        createSessionRecord(2, 1),
        createSessionRecord(3, 2),
        createSessionRecord(4, 3),
      ],
      viewMode: "vertical",
      visibleCount: 99,
      visibleSessionIds: ["session-1", "session-2"],
    });

    expect(snapshot.viewMode).toBe("vertical");
    expect(snapshot.visibleCount).toBe(99);
    expect(snapshot.visibleSessionIds).toEqual([
      "session-1",
      "session-2",
      "session-3",
      "session-4",
    ]);
    expect(snapshot.fullscreenRestoreVisibleCount).toBeUndefined();
  });

  test("should fall back to grid mode for legacy snapshots missing or carrying invalid view modes", () => {
    const missingViewMode = normalizeSessionGridSnapshot({
      focusedSessionId: "session-1",
      sessions: [createSessionRecord(1, 0)],
      visibleCount: 1,
      visibleSessionIds: ["session-1"],
    } as unknown as ReturnType<typeof createDefaultSessionGridSnapshot>);

    const invalidViewMode = normalizeSessionGridSnapshot({
      focusedSessionId: "session-1",
      sessions: [createSessionRecord(1, 0)],
      viewMode: "diagonal",
      visibleCount: 6,
      visibleSessionIds: ["session-1"],
    } as unknown as ReturnType<typeof createDefaultSessionGridSnapshot>);

    expect(missingViewMode.viewMode).toBe("grid");
    expect(invalidViewMode.viewMode).toBe("grid");
    expect(invalidViewMode.visibleCount).toBe(6);
  });

  test("should backfill aliases for legacy snapshots that only stored the primary title", () => {
    const snapshot = normalizeSessionGridSnapshot({
      focusedSessionId: "session-1",
      sessions: [
        { ...createSessionRecord(1, 0), alias: undefined as unknown as string },
        { ...createSessionRecord(2, 1), alias: "" },
      ],
      viewMode: "grid",
      visibleCount: 2,
      visibleSessionIds: ["session-1", "session-2"],
    });

    expect(snapshot.sessions.map((session) => session.alias)).toEqual([
      createSessionAlias(1, 0),
      createSessionAlias(2, 1),
    ]);
  });

  test("should default missing terminal engines to native Ghostty", () => {
    const normalized = normalizeSessionRecord({
      ...createSessionRecord(1, 0),
      terminalEngine: undefined as unknown as "ghostty-native",
    });

    expect(normalized.kind).toBe("terminal");
    expect(normalized.terminalEngine).toBe("ghostty-native");
  });

  test("should preserve valid terminal last activity timestamps", () => {
    const normalized = normalizeSessionRecord({
      ...createSessionRecord(1, 0),
      lastActivityAt: "2026-05-17T02:45:00.000Z",
    });

    expect(normalized.kind).toBe("terminal");
    expect(normalized.kind === "terminal" ? normalized.lastActivityAt : undefined).toBe(
      "2026-05-17T02:45:00.000Z",
    );
  });

  test("should drop invalid terminal last activity timestamps", () => {
    const normalized = normalizeSessionRecord({
      ...createSessionRecord(1, 0),
      lastActivityAt: "not-a-date",
    });

    expect(normalized.kind).toBe("terminal");
    expect(normalized.kind === "terminal" ? normalized.lastActivityAt : undefined).toBeUndefined();
  });

  test("should preserve startup terminal restore activity wake hints", () => {
    const normalized = normalizeSessionRecord({
      ...createSessionRecord(1, 0),
      restoreActivity: "attention",
    });

    expect(normalized.kind).toBe("terminal");
    expect(normalized.kind === "terminal" ? normalized.restoreActivity : undefined).toBe(
      "attention",
    );
  });

  test("should drop invalid startup terminal restore activity wake hints", () => {
    const normalized = normalizeSessionRecord({
      ...createSessionRecord(1, 0),
      restoreActivity: "idle" as "attention",
    });

    expect(normalized.kind).toBe("terminal");
    expect(normalized.kind === "terminal" ? normalized.restoreActivity : undefined).toBeUndefined();
  });

  test("should preserve fullscreen restore count only while the snapshot is in fullscreen mode", () => {
    const snapshot = normalizeSessionGridSnapshot({
      focusedSessionId: "session-1",
      fullscreenRestoreVisibleCount: 4,
      sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
      viewMode: "grid",
      visibleCount: 1,
      visibleSessionIds: ["session-1"],
    });

    expect(snapshot.fullscreenRestoreVisibleCount).toBe(4);
  });
});

describe("session shortcut labels", () => {
  test("should generate numeric default aliases from the display id", () => {
    expect(createSessionAlias(1, 0)).toBe("00");
    expect(createSessionAlias(2, 1)).toBe("01");
    expect(createSessionAlias(3, 2)).toBe("02");
    expect(createSessionAlias(101, 0)).toBe("00");
  });

  test("should format per-session shortcut text from the slot index", () => {
    expect(getSessionShortcutLabel(0, "mac")).toBe("⌘⌥1");
    expect(getSessionShortcutLabel(5, "mac")).toBe("⌘⌥6");
    expect(getSessionShortcutLabel(8, "default")).toBe("⌃⌥9");
  });

  test("should expose shortcut labels through sidebar session items", () => {
    const items = createSidebarSessionItems(
      {
        focusedSessionId: "session-1",
        sessions: [createSessionRecord(1, 0), createSessionRecord(2, 2)],
        viewMode: "grid",
        visibleCount: 2,
        visibleSessionIds: ["session-1", "session-2"],
      },
      "mac",
    );

    expect(items.map((item) => item.shortcutLabel)).toEqual(["⌘⌥1", "⌘⌥3"]);
  });

  test("should expose two-digit display ids as session numbers", () => {
    const items = createSidebarSessionItems({
      focusedSessionId: "session-18",
      sessions: [createSessionRecord(18, 0, { displayId: "99" })],
      viewMode: "grid",
      visibleCount: 1,
      visibleSessionIds: ["session-18"],
    });

    expect(items[0]?.sessionNumber).toBe("99");
  });

  test("should expose sleeping state through sidebar session items", () => {
    const items = createSidebarSessionItems({
      focusedSessionId: undefined,
      sessions: [{ ...createSessionRecord(1, 0), isSleeping: true }],
      viewMode: "grid",
      visibleCount: 1,
      visibleSessionIds: [],
    });

    expect(items[0]?.isSleeping).toBe(true);
    expect(items[0]?.isRunning).toBe(false);
  });

});

describe("session surface titles", () => {
  test("should collapse autogenerated aliases to only the display id", () => {
    const terminalSession = createSessionRecord(18, 0, { displayId: "99" });

    expect(getTerminalSessionSurfaceTitle(terminalSession)).toBe("99");
  });

  test("should prefix renamed aliases with the display id", () => {
    const terminalSession = {
      ...createSessionRecord(18, 0, { displayId: "99" }),
      alias: "Backend",
    };
    expect(getTerminalSessionSurfaceTitle(terminalSession)).toBe("99 Backend");
  });

  test("should prefer a custom title for terminal surface titles", () => {
    const terminalSession = {
      ...createSessionRecord(18, 0, { displayId: "99" }),
      alias: "Backend",
      title: "Bug Fixing",
    };
    expect(getTerminalSessionSurfaceTitle(terminalSession)).toBe("99. Bug Fixing");
  });
});

describe("sidebar HUD state", () => {
  test("should expose the session card chrome settings", () => {
    const hud = createSidebarHudState(
      createDefaultSessionGridSnapshot(),
      "dark-green",
      95,
      true,
      false,
      true,
      "glass",
    );

    expect(hud.completionBellEnabled).toBe(true);
    expect(hud.completionSound).toBe("glass");
    expect(hud.completionSoundLabel).toBe("Glass");
    expect(hud.agentManagerZoomPercent).toBe(95);
    expect(hud.createSessionOnSidebarDoubleClick).toBe(false);
    expect(hud.renameSessionOnDoubleClick).toBe(false);
    expect(hud.showCloseButtonOnSessionCards).toBe(true);
    expect(hud.activeSessionsSortMode).toBe("lastActivity");
    expect(hud.isFocusModeActive).toBe(false);
  });

  test("should expose the empty-sidebar double click session setting", () => {
    const hud = createSidebarHudState(
      createDefaultSessionGridSnapshot(),
      "dark-green",
      100,
      false,
      false,
      false,
      "ping",
      undefined,
      undefined,
      undefined,
      undefined,
      "manual",
      true,
    );

    expect(hud.createSessionOnSidebarDoubleClick).toBe(true);
  });

  test("should expose the session-card rename double click setting", () => {
    const hud = createSidebarHudState(
      createDefaultSessionGridSnapshot(),
      "dark-green",
      100,
      false,
      false,
      false,
      "ping",
      undefined,
      undefined,
      undefined,
      undefined,
      "manual",
      false,
      true,
    );

    expect(hud.renameSessionOnDoubleClick).toBe(true);
  });

  test("should expose when reversible focus mode is active", () => {
    const hud = createSidebarHudState(
      {
        focusedSessionId: "session-2",
        fullscreenRestoreVisibleCount: 4,
        sessions: [createSessionRecord(1, 0), createSessionRecord(2, 1)],
        viewMode: "grid",
        visibleCount: 1,
        visibleSessionIds: ["session-2"],
      },
      "dark-green",
      100,
      false,
      true,
      false,
      "ping",
    );

    expect(hud.isFocusModeActive).toBe(true);
  });
});

describe("agent manager zoom settings", () => {
  test("should clamp invalid zoom values to the supported range", () => {
    expect(clampAgentManagerZoomPercent(undefined)).toBe(100);
    expect(clampAgentManagerZoomPercent(49)).toBe(50);
    expect(clampAgentManagerZoomPercent(90.4)).toBe(90);
    expect(clampAgentManagerZoomPercent(95.6)).toBe(96);
    expect(clampAgentManagerZoomPercent(240)).toBe(200);
  });
});

describe("visible primary titles", () => {
  test("should normalize direct rename modal submissions into plain session titles", () => {
    expect(normalizeSessionRenameTitle(" \n • Build   ∗ logs\t#1!!! \n ")).toBe(
      "Build logs #1!!!",
    );
    expect(normalizeSessionRenameTitle("  Fix: API_retry (v2) / auth + tests  ")).toBe(
      "Fix: API_retry (v2) / auth + tests",
    );
    expect(normalizeSessionRenameTitle(" • ∗ \n ")).toBeUndefined();
  });

  test("should strip leading progress markers from terminal titles for supported agents", () => {
    expect(normalizeTerminalTitle("  ⠸ OpenAI Codex  ")).toBe("OpenAI Codex");
    expect(normalizeTerminalTitle("✳ Claude Code")).toBe("Claude Code");
    expect(normalizeTerminalTitle("✻ check-implementation-status")).toBe(
      "check-implementation-status",
    );
    expect(normalizeTerminalTitle("✦ demo-project")).toBe("demo-project");
    expect(normalizeTerminalTitle("◇ demo-project")).toBe("demo-project");
    expect(normalizeTerminalTitle("🤖 Copilot fix")).toBe("Copilot fix");
    expect(normalizeTerminalTitle("🔔 Copilot fix")).toBe("Copilot fix");
    expect(normalizeTerminalTitle("OC | Review repository")).toBe("Review repository");
    expect(normalizeTerminalTitle("⠸ OC | Review repository")).toBe("Review repository");
    expect(normalizeTerminalTitle("π - Implement Pi restore - ghostex")).toBe(
      "Implement Pi restore",
    );
    expect(normalizeTerminalTitle("⠸ π - Implement Pi restore - ghostex")).toBe(
      "Implement Pi restore",
    );
    expect(normalizeTerminalTitle("  π >   Delete marketplace skills   ")).toBe(
      "Delete marketplace skills",
    );
    expect(normalizeTerminalTitle("  π ⠧   Delete marketplace skills   ")).toBe(
      "Delete marketplace skills",
    );
  });

  test("should strip Antigravity CLI attention titles down to agy", () => {
    expect(normalizeTerminalTitle("🔔 agy")).toBe("agy");
    expect(normalizeTerminalTitle("agy")).toBe("agy");
    expect(getVisibleTerminalTitle("🔔 agy")).toBeUndefined();
    expect(getVisibleTerminalTitle("agy")).toBeUndefined();
  });

  test("should strip Cursor CLI working and ready status suffixes from terminal titles", () => {
    expect(normalizeTerminalTitle("Cursor Agent - ✅ Ready")).toBeUndefined();
    expect(normalizeTerminalTitle("Cursor Agent")).toBeUndefined();
    expect(getVisibleTerminalTitle("Cursor Agent - ✅ Ready")).toBeUndefined();
    expect(getVisibleTerminalTitle("Cursor Agent")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Cursor Agent - ✅ Ready")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Cursor Agent")).toBeUndefined();
    expect(normalizeTerminalTitle("My Task - ⏳ Working ···")).toBe("My Task");
    expect(normalizeTerminalTitle("My Task - ⏳ Working .··")).toBe("My Task");
    expect(normalizeTerminalTitle("My Task - ⏳ Working ..·")).toBe("My Task");
    expect(normalizeTerminalTitle("My Task - ✅ Ready")).toBe("My Task");
    expect(normalizeTerminalTitle("- ⏳ Working ..·")).toBeUndefined();
    expect(getVisibleTerminalTitle("My Task - ⏳ Working ···")).toBe("My Task");
    expect(getVisibleTerminalTitle("- ⏳ Working ..·")).toBeUndefined();
  });

  test("should hide generated Session N placeholder titles in sidebar items", () => {
    expect(getVisiblePrimaryTitle("Session 1")).toBeUndefined();
    expect(getVisiblePrimaryTitle(" Session 12 ")).toBeUndefined();
    expect(getVisiblePrimaryTitle(DEFAULT_TERMINAL_SESSION_TITLE)).toBeUndefined();
    expect(getVisiblePrimaryTitle("Codex Session")).toBeUndefined();
    expect(getVisiblePrimaryTitle("…/dev/_active/agent-tiler")).toBeUndefined();
    expect(getVisiblePrimaryTitle("Claude Code")).toBe("Claude Code");
  });

  test("should hide bare agent status word terminal titles", () => {
    expect(getVisibleTerminalTitle("Working")).toBeUndefined();
    expect(getVisibleTerminalTitle("Fix attention title filtering")).toBe(
      "Fix attention title filtering",
    );
  });

  test("should prefer the terminal title when choosing a visible session title", () => {
    expect(getPreferredSessionTitle("Session 1", "Claude Code / repo sweep")).toBe(
      "Claude Code / repo sweep",
    );
  });

  test("should prefer Claude generated rename titles over numeric session card titles", () => {
    expect(getPreferredSessionTitle("00", "✳ check-implementation-status")).toBe(
      "check-implementation-status",
    );
  });

  test("should fall back to the custom session title when no terminal title exists", () => {
    expect(getPreferredSessionTitle("Bug Fix", undefined)).toBe("Bug Fix");
  });

  test("should ignore generic ghostex terminal titles when choosing a visible session title", () => {
    expect(getPreferredSessionTitle("Session 1", "ghostex")).toBeUndefined();
  });

  test("should ignore placeholder and path-like terminal titles when choosing persisted titles", () => {
    expect(getPreferredSessionTitle(DEFAULT_TERMINAL_SESSION_TITLE, undefined)).toBeUndefined();
    expect(getPreferredSessionTitle("Codex Session", "…/dev/_active/agent-tiler")).toBeUndefined();
    expect(getPreferredSessionTitle("Bug Fix", "…/dev/_active/agent-tiler")).toBe("Bug Fix");
  });

  test("should ignore bare agent names when choosing a visible session title", () => {
    expect(getPreferredSessionTitle("Session 1", "Codex")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Codex CLI")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Droid")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Factory Droid")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Grok")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Grok Build")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Amp")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Amp CLI")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Antigravity")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Antigravity CLI")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Cursor")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Cursor Agent")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Cursor CLI")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "OpenAI Codex")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "π - ghostex")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Claude")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "Claude Code")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "  ⠸ Codex  ")).toBeUndefined();
    expect(getPreferredSessionTitle("Session 1", "  ✳ Claude Code  ")).toBeUndefined();
  });

  test("should ignore the default Windows PowerShell executable title", () => {
    expect(
      getPreferredSessionTitle(
        "Session 1",
        "C:\\WINDOWS\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
      ),
    ).toBeUndefined();
  });

  test("should expose placeholder titles through sidebar session items without making them persisted titles", () => {
    expect(createAgentSessionDefaultTitle("codex")).toBe("Codex Session");
    expect(createAgentSessionDefaultTitle("droid")).toBe("Factory Droid Session");
    expect(createAgentSessionDefaultTitle("grok")).toBe("Grok Build Session");
    expect(createAgentSessionDefaultTitle("cursor")).toBe("Cursor CLI Session");
    expect(createAgentSessionDefaultTitle("antigravity")).toBe("Antigravity CLI Session");
    expect(createAgentSessionDefaultTitle("amp")).toBe("Amp CLI Session");
    expect(createAgentSessionDefaultTitle("pi")).toBe("Pi Session");
    const items = createSidebarSessionItems(
      {
        focusedSessionId: "session-1",
        sessions: [
          createSessionRecord(1, 0),
          { ...createSessionRecord(2, 1), title: "Claude Code" },
        ],
        viewMode: "grid",
        visibleCount: 2,
        visibleSessionIds: ["session-1", "session-2"],
      },
      "mac",
    );

    expect(items[0]?.alias).toBe(createSessionAlias(1, 0));
    expect(items[0]?.primaryTitle).toBe(DEFAULT_TERMINAL_SESSION_TITLE);
    expect(items[1]?.alias).toBe(createSessionAlias(2, 1));
    expect(items[1]?.primaryTitle).toBe(DEFAULT_TERMINAL_SESSION_TITLE);
  });

  test("should classify supported default agent session titles as search placeholders", () => {
    expect(isDefaultSessionSearchTitle("Pi Agent Session")).toBe(true);
    expect(isDefaultSessionSearchTitle("Codex Agent Session")).toBe(true);
    expect(isDefaultSessionSearchTitle("Cursor Agent Session")).toBe(true);
    expect(isDefaultSessionSearchTitle("Claude Code Agent Session")).toBe(true);
    expect(isDefaultSessionSearchTitle("Review Pi agent behavior")).toBe(false);
  });
});

describe("clampTerminalViewMode", () => {
  test("should default invalid modes to grid", () => {
    expect(clampTerminalViewMode("horizontal")).toBe("horizontal");
    expect(clampTerminalViewMode("vertical")).toBe("vertical");
    expect(clampTerminalViewMode("grid")).toBe("grid");
    expect(clampTerminalViewMode("diagonal")).toBe("grid");
    expect(clampTerminalViewMode(undefined)).toBe("grid");
  });
});
