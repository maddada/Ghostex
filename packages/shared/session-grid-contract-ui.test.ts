import { describe, expect, test } from "vitest";
import {
  createSessionRecord,
  getPreferredSessionTitle,
  getSessionCardPrimaryTitle,
  getVisiblePrimaryTitle,
  getVisibleTerminalTitle,
  getCodexSessionIdFromTitle,
  isGhostPlaceholderSessionTitle,
} from "./session-grid-contract-session";
import { createSidebarSessionItems } from "./session-grid-contract-ui";

describe("createSidebarSessionItems", () => {
  test("should expose browser favicon data and browser fallback icon through sidebar session items", () => {
    const faviconDataUrl = "data:image/png;base64,ZmF2aWNvbg==";
    const items = createSidebarSessionItems({
      focusedSessionId: "session-1",
      sessions: [
        createSessionRecord(1, 0, {
          browser: { faviconDataUrl, url: "https://example.com" },
          kind: "browser",
          title: "Example",
        }),
      ],
      viewMode: "grid",
      visibleCount: 1,
      visibleSessionIds: ["session-1"],
    });

    expect(items[0]?.agentIcon).toBe("browser");
    expect(items[0]?.faviconDataUrl).toBe(faviconDataUrl);
    expect(items[0]?.kind).toBe("browser");
    expect(items[0]?.sessionKind).toBe("browser");
  });

  test("should expose favorite state through sidebar session items", () => {
    const favoriteSession = {
      ...createSessionRecord(1, 0, { title: "Favorite plan" }),
      isFavorite: true,
    };
    const normalSession = createSessionRecord(2, 1, { title: "Normal plan" });

    const items = createSidebarSessionItems({
      focusedSessionId: "session-1",
      sessions: [favoriteSession, normalSession],
      viewMode: "grid",
      visibleCount: 2,
      visibleSessionIds: ["session-1", "session-2"],
    });

    expect(items[0]?.isFavorite).toBe(true);
    expect(items[1]?.isFavorite).toBe(false);
  });

  test("should expose pinned state through sidebar session items", () => {
    const pinnedSession = {
      ...createSessionRecord(1, 0, { title: "Pinned plan" }),
      isPinned: true,
    };
    const normalSession = createSessionRecord(2, 1, { title: "Normal plan" });

    const items = createSidebarSessionItems({
      focusedSessionId: "session-1",
      sessions: [pinnedSession, normalSession],
      viewMode: "grid",
      visibleCount: 2,
      visibleSessionIds: ["session-1", "session-2"],
    });

    expect(items[0]?.isPinned).toBe(true);
    expect(items[1]?.isPinned).toBe(false);
  });

  test("should expose captured agent session ids through sidebar session items", () => {
    const items = createSidebarSessionItems({
      focusedSessionId: "session-1",
      sessions: [
        createSessionRecord(1, 0, {
          agentName: "codex",
          agentSessionId: "codex-session-123",
          title: "Fix resume",
        }),
      ],
      viewMode: "grid",
      visibleCount: 1,
      visibleSessionIds: ["session-1"],
    });

    expect(items[0]?.agentSessionId).toBe("codex-session-123");
  });

  test("should treat Ghostty ghost titles as placeholders instead of persisted session names", () => {
    /**
     * CDXC:SessionTitleSync 2026-05-07-17:27
     * Reconnected zmx panes may report `👻` while the real title is still known
     * by the session record. The shared card/title contract must reject that
     * placeholder before it can replace the restored name or render as
     * `* Terminal Session` in the sidebar.
     */
    expect(isGhostPlaceholderSessionTitle("👻")).toBe(true);
    expect(isGhostPlaceholderSessionTitle("👻 Terminal Session")).toBe(true);
    expect(getVisibleTerminalTitle("👻")).toBeUndefined();
    expect(getVisibleTerminalTitle("👻 Terminal Session")).toBeUndefined();
    expect(getVisiblePrimaryTitle("👻")).toBeUndefined();
    expect(getPreferredSessionTitle("Persisted Codex Name", "👻")).toBe("Persisted Codex Name");
    expect(getSessionCardPrimaryTitle({ agentName: "codex", title: "👻" })).toBe("Codex Session");
  });

  test("should treat Codex UUID terminal titles as session identity instead of display names", () => {
    /**
     * CDXC:CodexAgent 2026-05-11-07:35
     * Codex can expose the durable conversation UUID as the terminal title. The
     * UUID should be captured for restore metadata, not displayed as the card
     * title or treated as a human session name.
     */
    const codexSessionId = "019dc83d-dd29-7b03-af60-389172520a68";

    expect(getCodexSessionIdFromTitle(codexSessionId)).toBe(codexSessionId);
    expect(getVisibleTerminalTitle(codexSessionId)).toBeUndefined();
    expect(getVisiblePrimaryTitle(codexSessionId)).toBeUndefined();
    expect(getSessionCardPrimaryTitle({ agentName: "codex", title: codexSessionId })).toBe(
      "Codex Session",
    );
    expect(getPreferredSessionTitle("Codex Session", codexSessionId)).toBeUndefined();
  });

  test("should treat Cursor Agent boot titles as placeholders instead of persisted names", () => {
    expect(getSessionCardPrimaryTitle({ agentName: "cursor", title: "Cursor Agent" })).toBe(
      "Cursor CLI Session",
    );
    expect(getPreferredSessionTitle("Cursor CLI Session", "Cursor Agent - ✅ Ready")).toBeUndefined();
  });
});
