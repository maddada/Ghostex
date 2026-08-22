import { describe, expect, test } from "vite-plus/test";
import { getSessionHistoryCardTitle } from "./session-history-card-title";

describe("getSessionHistoryCardTitle", () => {
  test("should prefer the stored primary title", () => {
    expect(
      getSessionHistoryCardTitle({
        alias: "02",
        primaryTitle: "Bug Fix",
        terminalTitle: "Claude Code / bugfix",
      }),
    ).toBe("Bug Fix");
  });

  test("should fall back to the terminal title for archived auto-named sessions", () => {
    expect(
      getSessionHistoryCardTitle({
        alias: "02",
        primaryTitle: undefined,
        terminalTitle: "Codex / sidebar polish",
      }),
    ).toBe("Codex / sidebar polish");
  });

  test("should fall back to the alias when no titles are available", () => {
    expect(
      getSessionHistoryCardTitle({
        alias: "02",
        primaryTitle: undefined,
        terminalTitle: undefined,
      }),
    ).toBe("02");
  });
});
