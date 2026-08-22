import { describe, expect, test } from "vitest";
import type {
  GxserverPresentationGroup,
  GxserverPresentationProject,
  GxserverPresentationRevision,
  GxserverPresentationSession,
  GxserverPresentationSnapshot,
  GxserverProjectDomainState,
  GxserverProjectId,
  GxserverSessionId,
} from "./gxserver-protocol";
import {
  reduceGxserverPresentationDelta,
  reduceGxserverProjectCacheForPresentationDelta,
  reorderPresentationProjectSessions,
} from "./gxserver-presentation-cache";

function snapshot(
  overrides: Partial<GxserverPresentationSnapshot> = {},
): GxserverPresentationSnapshot {
  return {
    generatedAt: "2026-06-02T07:18:00.000Z",
    groups: [group()],
    projects: [project()],
    revision: 1 as GxserverPresentationRevision,
    sessions: [],
    ...overrides,
  };
}

function project(
  overrides: Partial<GxserverPresentationProject> = {},
): GxserverPresentationProject {
  return {
    createdAt: "2026-06-02T07:18:00.000Z",
    groupIds: ["Pmain:active"],
    isFavorite: false,
    isPinned: false,
    path: "/repo",
    projectId: "Pmain" as GxserverProjectId,
    sortKey: "2:repo:Pmain",
    title: "Repo",
    updatedAt: "2026-06-02T07:18:00.000Z",
    ...overrides,
  };
}

function group(overrides: Partial<GxserverPresentationGroup> = {}): GxserverPresentationGroup {
  return {
    groupId: "Pmain:active",
    projectId: "Pmain" as GxserverProjectId,
    sessionIds: [],
    sortKey: "2:repo:Pmain:active",
    title: "Active",
    ...overrides,
  };
}

function session(
  sessionId: string,
  overrides: Partial<GxserverPresentationSession> = {},
): GxserverPresentationSession {
  return {
    activity: "idle",
    createdAt: "2026-06-02T07:18:00.000Z",
    groupId: "Pmain:active",
    isFavorite: false,
    isGeneratingFirstPromptTitle: false,
    isPinned: false,
    isPrimaryTitleTerminalTitle: false,
    isTemporaryTitle: false,
    kind: "terminal",
    lifecycleState: "running",
    projectId: "Pmain" as GxserverProjectId,
    sessionId: sessionId as GxserverSessionId,
    sortKey: `2:${sessionId}`,
    surface: "workspace",
    title: sessionId,
    titleSource: "user",
    updatedAt: "2026-06-02T07:18:00.000Z",
    visibleInSidebarByDefault: true,
    zmxName: sessionId as GxserverSessionId,
    ...overrides,
  };
}

function domainProject(
  overrides: Partial<GxserverProjectDomainState> = {},
): GxserverProjectDomainState {
  return {
    attentionRules: {},
    completionRules: {},
    createdAt: "2026-06-02T07:18:00.000Z",
    customAgentOrder: [],
    customAgents: [],
    customCommandOrder: [],
    customCommands: [],
    deletedDefaultCommandIds: [],
    gitConfig: {},
    isFavorite: false,
    isPinned: false,
    isRecentProject: false,
    launchSettings: {},
    name: "Repo",
    notificationRules: {},
    path: "/repo",
    previousSessionHistory: [],
    projectBoardConfig: {},
    projectId: "Pmain" as GxserverProjectId,
    runtimeSettings: {},
    updatedAt: "2026-06-02T07:18:00.000Z",
    ...overrides,
  };
}

describe("gxserver presentation cache reducer", () => {
  test("updates gxserver-owned group membership from session deltas", () => {
    /*
    CDXC:GxserverPresentationGroups 2026-06-02-11:18:
    Native must not rebuild shared presentation group membership independently. Session deltas update the gxserver presentation cache and then reconcile group sessionIds from gxserver session rows.
    */
    const first = reduceGxserverPresentationDelta(
      snapshot(),
      { session: session("G2bbb", { sortKey: "2:bbb" }), type: "sessionAdded" },
      2,
    );
    const second = reduceGxserverPresentationDelta(
      first,
      { session: session("G1aaa", { sortKey: "1:aaa" }), type: "sessionAdded" },
      3,
    );

    expect(second.groups[0]?.sessionIds).toEqual(["G1aaa", "G2bbb"]);
    expect(second.sessions.map((item) => item.sessionId)).toEqual(["G1aaa", "G2bbb"]);
  });

  test("applies group deltas instead of dropping them", () => {
    const next = reduceGxserverPresentationDelta(
      snapshot(),
      {
        group: group({
          sessionIds: ["G1aaa" as GxserverSessionId],
          sortKey: "0:pinned",
          title: "Pinned",
        }),
        type: "groupUpdated",
      },
      2,
    );

    expect(next.groups[0]).toMatchObject({
      sessionIds: ["G1aaa"],
      sortKey: "0:pinned",
      title: "Pinned",
    });
  });

  test("removes sessions that belonged only to a removed presentation group", () => {
    const next = reduceGxserverPresentationDelta(
      snapshot({
        groups: [group({ sessionIds: ["G1aaa" as GxserverSessionId] })],
        sessions: [session("G1aaa")],
      }),
      {
        groupId: "Pmain:active",
        projectId: "Pmain" as GxserverProjectId,
        type: "groupRemoved",
      },
      2,
    );

    expect(next.groups).toEqual([]);
    expect(next.sessions).toEqual([]);
  });

  test("updates the domain project cache only from project deltas", () => {
    const nextProjects = reduceGxserverProjectCacheForPresentationDelta(
      [domainProject({ name: "Old" })],
      {
        domainProject: domainProject({ name: "New" }),
        project: project({ title: "New" }),
        type: "projectUpdated",
      },
    );

    expect(nextProjects[0]?.name).toBe("New");
  });

  test("reorders project presentation sessions for local-first pinned drag", () => {
    const next = reorderPresentationProjectSessions(
      snapshot({
        groups: [
          group({
            sessionIds: ["G1aaa" as GxserverSessionId, "G2bbb" as GxserverSessionId],
          }),
        ],
        sessions: [
          session("G1aaa", { isPinned: true, sortKey: "0:0:z:2026-06-02T07:18:00.000Z:G1aaa" }),
          session("G2bbb", { isPinned: true, sortKey: "0:0:z:2026-06-02T07:18:00.000Z:G2bbb" }),
        ],
      }),
      "Pmain" as GxserverProjectId,
      ["G2bbb" as GxserverSessionId, "G1aaa" as GxserverSessionId],
    );

    expect(next.groups[0]?.sessionIds).toEqual(["G2bbb", "G1aaa"]);
    expect(next.sessions.map((item) => [item.sessionId, item.sidebarOrder])).toEqual([
      ["G2bbb", 1000],
      ["G1aaa", 2000],
    ]);
  });

  test("keeps unsnapshotted project sessions after saved manual order", () => {
    const next = reorderPresentationProjectSessions(
      snapshot({
        groups: [
          group({
            sessionIds: [
              "G1aaa" as GxserverSessionId,
              "G2bbb" as GxserverSessionId,
              "G3ccc" as GxserverSessionId,
            ],
          }),
        ],
        sessions: [
          session("G1aaa", { sortKey: "0:2:z:2026-06-02T07:18:00.000Z:G1aaa" }),
          session("G2bbb", { sortKey: "0:2:z:2026-06-02T07:18:00.000Z:G2bbb" }),
          session("G3ccc", { sortKey: "z:0:2:2026-06-02T07:18:00.000Z:G3ccc" }),
        ],
      }),
      "Pmain" as GxserverProjectId,
      ["G2bbb" as GxserverSessionId, "G1aaa" as GxserverSessionId],
    );

    expect(next.groups[0]?.sessionIds).toEqual(["G2bbb", "G1aaa", "G3ccc"]);
  });
});
