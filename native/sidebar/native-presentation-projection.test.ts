import { describe, expect, test } from "vitest";
import {
  NATIVE_PRESENTATION_CHATS_GROUP_ID,
  createNativePresentationProjectionSessionKey,
  createNativePresentationSidebarGroups,
  type NativePresentationProjectProjection,
} from "./native-presentation-projection";
import { createCombinedProjectGroupId, createCombinedProjectSessionId } from "./combined-sidebar-mode";
import { createDefaultSidebarProjectDiffStats } from "../../shared/project-diff-stats";
import type {
  GxserverPresentationGroup,
  GxserverPresentationProject,
  GxserverPresentationSession,
  GxserverPresentationSnapshot,
  GxserverProjectId,
  GxserverSessionId,
  GxserverZmxSessionName,
} from "../../shared/gxserver-protocol";
import type { SidebarSessionItem } from "../../shared/session-grid-contract";

describe("Native Presentation Projection", () => {
  test("joins delayed-send timers onto gxserver presentation rows", () => {
    const projectId = projectIdValue("P1abc");
    const sessionId = sessionIdValue("G1abc");
    const groups = createNativePresentationSidebarGroups({
      activeProjectId: projectId,
      focusedSessionId: sessionId,
      localProjects: [localProject({ projectId })],
      presentation: snapshot({
        groups: [presentationGroup({ projectId, sessionIds: [sessionId] })],
        projects: [presentationProject({ projectId })],
        sessions: [presentationSession({ projectId, sessionId })],
      }),
      resolveAgentIcon: () => "codex",
      resolveDelayedSend: () => ({
        deadlineAt: "2026-06-12T10:00:00.000Z",
        remainingLabel: "2m",
        remainingMs: 120_000,
      }),
      resolveSessionRoutingId: () => "S1a-P1abc-G1abc",
      visibleSessionIds: new Set([sessionId]),
    });

    const session = groups[1]?.sessions[0];
    expect(session).toMatchObject({
      delayedSendDeadlineAt: "2026-06-12T10:00:00.000Z",
      delayedSendRemainingLabel: "2m",
      delayedSendRemainingMs: 120_000,
      isFocused: true,
      sessionId: createCombinedProjectSessionId(projectId, sessionId),
      sessionRoutingId: "S1a-P1abc-G1abc",
    });
  });

  test("prefers gxserver provider identity over local terminal identity", () => {
    const projectId = projectIdValue("P1abc");
    const sessionId = sessionIdValue("G1abc");
    const groups = createNativePresentationSidebarGroups({
      activeProjectId: projectId,
      localProjects: [
        localProject({
          localSidebarSessions: [
            localSidebarSession({
              agentSessionId: "local-provider-session",
              sessionId,
              sessionKind: "terminal",
            }),
          ],
          projectId,
        }),
      ],
      presentation: snapshot({
        groups: [presentationGroup({ projectId, sessionIds: [sessionId] })],
        projects: [presentationProject({ projectId })],
        sessions: [
          presentationSession({
            agentSessionId: "gxserver-provider-session",
            projectId,
            sessionId,
          }),
        ],
      }),
      resolveAgentIcon: () => "codex",
      resolveDelayedSend: () => undefined,
      resolveSessionRoutingId: () => undefined,
      visibleSessionIds: new Set([sessionId]),
    });

    expect(groups[1]?.sessions[0]?.agentSessionId).toBe("gxserver-provider-session");
  });

  test("keeps native-only T3 and browser panes visible in gxserver-owned project groups", () => {
    const projectId = projectIdValue("P1abc");
    const terminalId = sessionIdValue("G1abc");
    const t3Id = "local-t3";
    const browserId = "local-browser";
    const groups = createNativePresentationSidebarGroups({
      activeProjectId: projectId,
      hiddenSessionKeys: new Set([
        createNativePresentationProjectionSessionKey(projectId, browserId),
      ]),
      localProjects: [
        localProject({
          localSidebarSessions: [
            localSidebarSession({ sessionId: terminalId, sessionKind: "terminal" }),
            localSidebarSession({ sessionId: t3Id, sessionKind: "t3" }),
            localSidebarSession({ sessionId: browserId, sessionKind: "browser" }),
          ],
          projectId,
        }),
      ],
      presentation: snapshot({
        groups: [presentationGroup({ projectId, sessionIds: [terminalId] })],
        projects: [presentationProject({ projectId })],
        sessions: [presentationSession({ projectId, sessionId: terminalId })],
      }),
      resolveAgentIcon: () => "codex",
      resolveDelayedSend: () => undefined,
      resolveSessionRoutingId: () => undefined,
      visibleSessionIds: new Set([terminalId]),
    });

    expect(groups[1]?.sessions.map((session) => session.sessionId)).toEqual([
      createCombinedProjectSessionId(projectId, terminalId),
      createCombinedProjectSessionId(projectId, t3Id),
    ]);
  });

  test("merges Quick local rows and presentation rows into the Chats group", () => {
    const projectId = projectIdValue("P2abc");
    const terminalId = sessionIdValue("G2abc");
    const browserId = "quick-browser";
    const groups = createNativePresentationSidebarGroups({
      activeProjectId: projectId,
      chatProjectIds: new Set([projectId]),
      localProjects: [
        localProject({
          isQuickProject: true,
          localSidebarSessions: [
            localSidebarSession({ sessionId: terminalId, sessionKind: "terminal" }),
            localSidebarSession({ sessionId: browserId, sessionKind: "browser" }),
          ],
          projectId,
          title: "Quick",
        }),
      ],
      presentation: snapshot({
        groups: [presentationGroup({ projectId, sessionIds: [terminalId] })],
        projects: [presentationProject({ path: "/Users/test/.ghostex/chats/one", projectId, title: "Quick" })],
        sessions: [presentationSession({ projectId, sessionId: terminalId })],
      }),
      resolveAgentIcon: () => "codex",
      resolveDelayedSend: () => undefined,
      resolveSessionRoutingId: () => undefined,
      visibleSessionIds: new Set([terminalId]),
    });

    expect(groups[0]?.groupId).toBe(NATIVE_PRESENTATION_CHATS_GROUP_ID);
    expect(groups[0]?.isActive).toBe(true);
    expect(groups[0]?.sessions.map((session) => session.sessionId)).toEqual([
      createCombinedProjectSessionId(projectId, terminalId),
      browserId,
    ]);
    expect(groups).toHaveLength(1);
  });

  test("returns empty project groups for visible gxserver projects without sessions", () => {
    const projectId = projectIdValue("P3abc");
    const groups = createNativePresentationSidebarGroups({
      localProjects: [localProject({ projectId })],
      presentation: snapshot({
        groups: [presentationGroup({ projectId, sessionIds: [] })],
        projects: [presentationProject({ projectId, title: "Empty Project" })],
        sessions: [],
      }),
      resolveAgentIcon: () => "codex",
      resolveDelayedSend: () => undefined,
      resolveSessionRoutingId: () => undefined,
    });

    expect(groups[1]).toMatchObject({
      groupId: createCombinedProjectGroupId(projectId),
      sessions: [],
      title: "Empty Project",
      visibleCount: 1,
    });
  });
});

function localProject({
  isQuickProject = false,
  localSidebarSessions = [],
  projectId = projectIdValue("P1abc"),
  title = "Project",
}: {
  isQuickProject?: boolean;
  localSidebarSessions?: readonly SidebarSessionItem[];
  projectId?: GxserverProjectId;
  title?: string;
} = {}): NativePresentationProjectProjection {
  return {
    editor: {
      diffStats: createDefaultSidebarProjectDiffStats(),
      isOpen: false,
      isSleeping: false,
      projectId,
      status: "idle",
    },
    isQuickProject,
    localSidebarSessions,
    orderIndex: 0,
    path: `/repo/${projectId}`,
    projectId,
    title,
  };
}

function localSidebarSession({
  agentSessionId,
  sessionId,
  sessionKind,
}: {
  agentSessionId?: string;
  sessionId: string;
  sessionKind: NonNullable<SidebarSessionItem["sessionKind"]>;
}): SidebarSessionItem {
  return {
    activity: "idle",
    agentSessionId,
    alias: sessionId,
    column: 0,
    isFocused: false,
    isRunning: true,
    isVisible: true,
    row: 0,
    sessionId,
    sessionKind,
    shortcutLabel: "",
  };
}

function snapshot({
  groups,
  projects,
  sessions,
}: {
  groups: readonly GxserverPresentationGroup[];
  projects: readonly GxserverPresentationProject[];
  sessions: readonly GxserverPresentationSession[];
}): GxserverPresentationSnapshot {
  return {
    generatedAt: "2026-06-12T00:00:00.000Z",
    groups,
    projects,
    revision: 1 as GxserverPresentationSnapshot["revision"],
    sessions,
  };
}

function presentationProject({
  path = "/repo/project",
  projectId,
  title = "Project",
}: {
  path?: string;
  projectId: GxserverProjectId;
  title?: string;
}): GxserverPresentationProject {
  return {
    createdAt: "2026-06-12T00:00:00.000Z",
    groupIds: [`${projectId}:active`],
    isFavorite: false,
    isPinned: false,
    path,
    projectId,
    sortKey: `2:${title.toLocaleLowerCase()}:${projectId}`,
    title,
    updatedAt: "2026-06-12T00:00:00.000Z",
  };
}

function presentationGroup({
  projectId,
  sessionIds,
}: {
  projectId: GxserverProjectId;
  sessionIds: readonly GxserverSessionId[];
}): GxserverPresentationGroup {
  return {
    groupId: `${projectId}:active`,
    projectId,
    sessionIds,
    sortKey: `2:${projectId}:active`,
    title: "Active",
  };
}

function presentationSession({
  agentSessionId,
  projectId,
  sessionId,
}: {
  agentSessionId?: string;
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
}): GxserverPresentationSession {
  return {
    actions: {
      acknowledgeAttention: false,
      attach: true,
      focus: true,
      kill: true,
      readText: true,
      sendMessage: true,
      sendText: true,
      sleep: true,
      wake: true,
    },
    activity: "idle",
    agentSessionId,
    createdAt: "2026-06-12T00:00:00.000Z",
    groupId: `${projectId}:active`,
    isFavorite: false,
    isGeneratingFirstPromptTitle: false,
    isPinned: false,
    isPrimaryTitleTerminalTitle: false,
    isTemporaryTitle: false,
    kind: "agent",
    lifecycleState: "running",
    projectId,
    sessionId,
    sortKey: `0:${sessionId}`,
    surface: "workspace",
    title: "Agent Session",
    titleSource: "placeholder",
    updatedAt: "2026-06-12T00:00:00.000Z",
    visibleInSidebarByDefault: true,
    zmxName: `S1a-${projectId}-${sessionId}` as GxserverZmxSessionName,
  };
}

function projectIdValue(value: string): GxserverProjectId {
  return value as GxserverProjectId;
}

function sessionIdValue(value: string): GxserverSessionId {
  return value as GxserverSessionId;
}
