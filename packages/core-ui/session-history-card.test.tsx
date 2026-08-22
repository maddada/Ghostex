import { readFileSync } from "node:fs";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, test } from "vitest";
import type { SidebarPreviousSessionItem } from "../shared/session-grid-contract";
import { SessionHistoryCard } from "./session-history-card";

function createPreviousSession(
  overrides: Partial<SidebarPreviousSessionItem> = {},
): SidebarPreviousSessionItem {
  return {
    activity: "idle",
    agentIcon: "codex",
    alias: "Previous Codex session",
    closedAt: "2026-06-05T10:00:00.000Z",
    detail: "OpenAI Codex",
    historyId: "history-1",
    isFavorite: false,
    isFocused: false,
    isGeneratedName: false,
    isRestorable: true,
    isRunning: false,
    isVisible: false,
    primaryTitle: "Previous Codex session",
    sessionId: "session-1",
    terminalTitle: undefined,
    ...overrides,
  };
}

describe("SessionHistoryCard", () => {
  test("renders the floating identity icon inside the restore button", () => {
    const markup = renderToStaticMarkup(
      createElement(SessionHistoryCard, {
        onDelete: () => {},
        onRestore: () => {},
        session: createPreviousSession(),
        showDebugSessionNumbers: false,
      }),
    );

    const articleIndex = markup.indexOf('class="session session-history-card"');
    const iconIndex = markup.indexOf('class="session-floating-agent-icon"');
    const titleIndex = markup.indexOf('class="session-head"');
    const articleCloseIndex = markup.indexOf("</article>");

    expect(articleIndex).toBeGreaterThanOrEqual(0);
    expect(iconIndex).toBeGreaterThan(articleIndex);
    expect(iconIndex).toBeLessThan(titleIndex);
    expect(iconIndex).toBeLessThan(articleCloseIndex);
  });

  test("marks selected search result rows like project session cards", () => {
    const markup = renderToStaticMarkup(
      createElement(SessionHistoryCard, {
        isSearchSelected: true,
        onDelete: () => {},
        onRestore: () => {},
        session: createPreviousSession(),
        showDebugSessionNumbers: false,
      }),
    );

    expect(markup).toContain('data-search-selected="true"');
  });

  test("marks tagged history rows like sidebar cards and keeps the agent icon available for hover", () => {
    const markup = renderToStaticMarkup(
      createElement(SessionHistoryCard, {
        onDelete: () => {},
        onRestore: () => {},
        session: createPreviousSession({
          agentIcon: "codex",
          isFavorite: false,
          sessionTag: "testing",
        }),
        showDebugSessionNumbers: false,
      }),
    );

    expect(markup).toContain('data-tagged="true"');
    expect(markup).toContain("session-tag-agent-icon");
    expect(markup).toContain('data-session-tag="testing"');
    expect(markup).toContain('class="session-floating-agent-icon"');
    expect(markup).toContain('data-agent-icon="codex"');
  });

  test("counts tag-only history rows as having a leading identity icon", () => {
    const markup = renderToStaticMarkup(
      createElement(SessionHistoryCard, {
        onDelete: () => {},
        onRestore: () => {},
        session: createPreviousSession({
          agentIcon: undefined,
          isFavorite: false,
          sessionTag: "todo",
        }),
        showDebugSessionNumbers: false,
      }),
    );

    expect(markup).toContain('data-has-agent-icon="true"');
    expect(markup).toContain('data-session-tag="todo"');
  });

  test("keeps reference-sidebar history title padding aligned with project rows", () => {
    const css = readFileSync(new URL("./styles/session-cards.css", import.meta.url), "utf8");
    const referenceHistoryRule = css.match(
      /\.sidebar-reference-layout\[data-reference-sidebar="true"\] \.session\.session-history-card\s*\{[^}]+padding-left:[^}]+\}/u,
    )?.[0];

    expect(referenceHistoryRule).toContain("var(--reference-session-title-inset)");
    expect(referenceHistoryRule).toContain("var(--reference-sidebar-scroll-row-bleed-left");
    expect(referenceHistoryRule).toContain("var(--reference-session-full-width-left-padding)");
  });

  test("hides the underlying history agent decoration at rest for tagged rows", () => {
    const css = readFileSync(new URL("./styles/session-cards.css", import.meta.url), "utf8");

    expect(css).toContain(
      '.session-history-frame:is([data-tagged="true"], [data-pinned="true"])',
    );
    expect(css).toContain(".session-floating-agent-tabler-icon[data-agent-icon]");
    expect(css).toContain("CDXC:PreviousSessions 2026-06-09-09:41");
  });
});
