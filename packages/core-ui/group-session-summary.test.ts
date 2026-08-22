import { describe, expect, test } from "vitest";
import type { SidebarSessionItem } from "../shared/session-grid-contract";
import { getGroupSessionSummary } from "./group-session-summary";

describe("getGroupSessionSummary", () => {
  test("should count attention and working sessions while preserving the attention priority indicator", () => {
    expect(
      getGroupSessionSummary([
        createSession("session-1", { activity: "working", lifecycleState: "running" }),
        createSession("session-2", { activity: "attention", lifecycleState: "done" }),
        createSession("session-3", { activity: "attention", lifecycleState: "done" }),
      ]),
    ).toEqual({
      attentionCount: 2,
      indicatorActivity: "attention",
      workingCount: 1,
    });
  });

  test("should show orange only when there are working sessions and no attention sessions", () => {
    expect(
      getGroupSessionSummary([
        createSession("session-1", { activity: "idle", lifecycleState: "running" }),
        createSession("session-2", { activity: "idle", lifecycleState: "done" }),
        createSession("session-3", { activity: "working", lifecycleState: "done" }),
      ]),
    ).toEqual({
      attentionCount: 0,
      indicatorActivity: "working",
      workingCount: 1,
    });
  });

  test("should ignore idle, sleeping, and error sessions", () => {
    expect(
      getGroupSessionSummary([
        createSession("session-1", {
          activity: "idle",
          lifecycleState: "sleeping",
          isRunning: true,
          isSleeping: true,
        }),
        createSession("session-2", {
          activity: "idle",
          lifecycleState: "sleeping",
          isRunning: false,
          isSleeping: true,
        }),
        createSession("session-3", { activity: "idle", lifecycleState: "done", isRunning: false }),
        createSession("session-4", { activity: "idle", lifecycleState: "error", isRunning: false }),
      ]),
    ).toEqual({
      attentionCount: 0,
      indicatorActivity: undefined,
      workingCount: 0,
    });
  });
});

function createSession(
  sessionId: string,
  overrides: Partial<SidebarSessionItem>,
): SidebarSessionItem {
  return {
    activity: "idle",
    activityLabel: undefined,
    alias: sessionId,
    column: 0,
    isFocused: false,
    lifecycleState: "running",
    isRunning: true,
    isVisible: false,
    primaryTitle: sessionId,
    row: 0,
    sessionId,
    shortcutLabel: "⌘⌥1",
    ...overrides,
  };
}
