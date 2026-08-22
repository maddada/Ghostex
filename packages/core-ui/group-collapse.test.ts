import { describe, expect, test } from "vitest";
import {
  getAutoCollapseGroupIds,
  reconcileCollapsedGroupsById,
} from "./group-collapse";

describe("getAutoCollapseGroupIds", () => {
  test("keeps empty project groups out of auto-collapse", () => {
    /**
     * CDXC:ProjectGroups 2026-05-21-11:07:
     * Global browser groups are no longer rendered in the reference sidebar.
     * Auto-collapse applies only to visible non-project workspace sections.
     */
    expect(
      getAutoCollapseGroupIds({
        groupsById: {
          "combined-chats": {},
          "project-ghostex": { projectContext: {} },
        },
        workspaceGroupIds: ["combined-chats", "project-ghostex"],
      }),
    ).toEqual(["combined-chats"]);
  });
});

describe("reconcileCollapsedGroupsById", () => {
  test("drops collapse state for removed groups", () => {
    expect(
      reconcileCollapsedGroupsById({
        groupIds: ["group-1"],
        previousSessionCountsByGroup: {},
        previousCollapsedGroupsById: {
          "group-1": true,
          "group-2": true,
        },
        sessionIdsByGroup: {
          "group-1": ["session-1"],
        },
      }),
    ).toEqual({
      "group-1": true,
    });
  });

  test("preserves collapse state for temporarily missing startup groups", () => {
    /**
     * CDXC:SidebarGroups 2026-06-02-22:18:
     * The gxserver-unavailable startup placeholder is not a real project list.
     * Reconcile must keep unknown collapsed project IDs until the authoritative
     * project hydrate arrives, otherwise restart rewrites them as expanded.
     */
    expect(
      reconcileCollapsedGroupsById({
        groupIds: ["combined-chats", "gxserver-unavailable"],
        preserveUnknownCollapsedGroups: true,
        previousSessionCountsByGroup: {},
        previousCollapsedGroupsById: {
          "combined-project-zmux": true,
          "combined-project-other": true,
        },
        sessionIdsByGroup: {
          "combined-chats": [],
          "gxserver-unavailable": [],
        },
      }),
    ).toEqual({
      "combined-project-zmux": true,
      "combined-project-other": true,
    });
  });

  test("auto-collapses empty non-project sections", () => {
    expect(
      reconcileCollapsedGroupsById({
        autoCollapseGroupIds: ["combined-chats"],
        groupIds: ["combined-chats", "project-ghostex"],
        previousSessionCountsByGroup: {},
        previousCollapsedGroupsById: {},
        sessionIdsByGroup: {
          "combined-chats": [],
          "project-ghostex": [],
        },
      }),
    ).toEqual({
      "combined-chats": true,
    });
  });

  test("expands a collapsed auto-collapse group when a session is created", () => {
    expect(
      reconcileCollapsedGroupsById({
        autoCollapseGroupIds: ["combined-chats"],
        groupIds: ["combined-chats", "project-ghostex"],
        previousSessionCountsByGroup: {
          "combined-chats": 0,
          "project-ghostex": 0,
        },
        previousCollapsedGroupsById: {
          "combined-chats": true,
          "project-ghostex": true,
        },
        sessionIdsByGroup: {
          "combined-chats": ["chat-session-1"],
          "project-ghostex": ["session-1"],
        },
      }),
    ).toEqual({
      "project-ghostex": true,
    });
  });

  test("keeps collapsed project groups collapsed while seeding the startup baseline", () => {
    /**
     * CDXC:SidebarGroups 2026-05-20-12:00:
     * Restart hydration replays restored session counts from zero. The first
     * post-hydrate reconcile must not treat that replay as a newly created
     * session.
     */
    expect(
      reconcileCollapsedGroupsById({
        expandOnSessionCountIncreaseGroupIds: ["project-ghostex"],
        groupIds: ["project-ghostex"],
        previousSessionCountsByGroup: {
          "project-ghostex": 0,
        },
        previousCollapsedGroupsById: {
          "project-ghostex": true,
        },
        sessionIdsByGroup: {
          "project-ghostex": ["session-1", "session-2"],
        },
        skipExpandOnSessionCountIncrease: true,
      }),
    ).toEqual({
      "project-ghostex": true,
    });
  });

  test("expands a collapsed project group when a session is created without auto-collapsing empty projects", () => {
    /**
     * CDXC:SidebarGroups 2026-05-08-11:09:
     * Project groups do not use empty auto-collapse, but a newly added
     * terminal/browser/agent session inside a collapsed project must still
     * expand that project.
     */
    expect(
      reconcileCollapsedGroupsById({
        autoCollapseGroupIds: ["combined-chats"],
        expandOnSessionCountIncreaseGroupIds: ["combined-chats", "project-ghostex"],
        groupIds: ["combined-chats", "project-ghostex"],
        previousSessionCountsByGroup: {
          "combined-chats": 1,
          "project-ghostex": 1,
        },
        previousCollapsedGroupsById: {
          "combined-chats": true,
          "project-ghostex": true,
        },
        sessionIdsByGroup: {
          "combined-chats": ["chat-session-1"],
          "project-ghostex": ["session-1", "session-2"],
        },
      }),
    ).toEqual({
      "combined-chats": true,
    });
  });

  test("defers auto-collapse while a section create action is still settling", () => {
    expect(
      reconcileCollapsedGroupsById({
        autoCollapseGroupIds: ["combined-chats"],
        collapseBlockedGroupIds: ["combined-chats"],
        groupIds: ["combined-chats", "group-1"],
        previousSessionCountsByGroup: {
          "combined-chats": 1,
        },
        previousCollapsedGroupsById: {},
        sessionIdsByGroup: {
          "combined-chats": [],
          "group-1": ["session-1"],
        },
      }),
    ).toEqual({});
  });

  test("preserves the same object when counts stay flat", () => {
    const collapsedGroupsById = {
      "combined-chats": true,
      "group-1": true,
    } satisfies Record<string, true>;

    expect(
      reconcileCollapsedGroupsById({
        autoCollapseGroupIds: ["combined-chats"],
        groupIds: ["combined-chats", "group-1"],
        previousSessionCountsByGroup: {
          "combined-chats": 1,
        },
        previousCollapsedGroupsById: collapsedGroupsById,
        sessionIdsByGroup: {
          "combined-chats": ["chat-session-1"],
          "group-1": ["session-1"],
        },
      }),
    ).toBe(collapsedGroupsById);
  });
});
