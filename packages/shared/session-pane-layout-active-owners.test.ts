import { describe, expect, test } from "vitest";
import type { SessionPaneLayoutNode } from "./session-grid-contract-core";
import { collectActivePaneOwnerSessionIds } from "./session-pane-layout-active-owners";

describe("collectActivePaneOwnerSessionIds", () => {
  test("collects one selected owner from each split pane", () => {
    /*
     * CDXC:AutoSleep 2026-06-09-20:33:
     * Split protection is based on persisted pane ownership, not only current
     * AppKit visibility, so hidden Focus Mode branches still keep their active
     * terminal owner awake.
     */
    const layout: SessionPaneLayoutNode = {
      children: [
        { kind: "leaf", sessionId: "G-left" },
        {
          activeSessionId: "G-right-active",
          kind: "tabs",
          sessionIds: ["G-right-parked", "G-right-active"],
        },
      ],
      direction: "horizontal",
      kind: "split",
    };

    expect(collectActivePaneOwnerSessionIds(layout)).toEqual(["G-left", "G-right-active"]);
  });

  test("falls back to the first valid tab when active tab state is stale", () => {
    const layout: SessionPaneLayoutNode = {
      activeSessionId: "G-missing",
      kind: "tabs",
      sessionIds: ["G-stale", "G-valid"],
    };

    expect(
      collectActivePaneOwnerSessionIds(layout, {
        validSessionIds: new Set(["G-valid"]),
      }),
    ).toEqual(["G-valid"]);
  });
});
