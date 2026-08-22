import { describe, expect, test } from "vitest";
import {
  createGxserverPresentationSidebarGroup,
  createGxserverPresentationSidebarSession,
  gxserverPresentationSidebarAutoSettleAfterDays,
  gxserverPresentationSidebarLifecycleCapabilities,
} from "./gxserver-presentation-sidebar-projection";
import type {
  GxserverPresentationProject,
  GxserverPresentationSession,
  GxserverPresentationSnapshot,
  GxserverProjectId,
  GxserverSessionId,
  GxserverZmxSessionName,
} from "./gxserver-protocol";

describe("gxserver presentation sidebar projection", () => {
  test("carries title, lifecycle, activity, tag, and zmx metadata into sidebar rows", () => {
    const projectId = "P3a91" as GxserverProjectId;
    const sessionId = "G8v20" as GxserverSessionId;
    const session = createPresentationSession({
      displayTitle: "ghostex@remote: ~/ghostex",
      displayTitleTooltip: "ghostex@remote: ~/ghostex",
      lifecycleState: "running",
      primaryTitle: "Remote Ghostex",
      providerSessionState: "exists",
      sessionId,
      sessionPersistenceProvider: "zmx",
      sessionTag: "blocked",
      terminalTitle: "ghostex@remote: ~/ghostex",
      zmxName: "S7k-P3a91-G8v20" as GxserverZmxSessionName,
    });

    const row = createGxserverPresentationSidebarSession({
      createProjectSessionId: (projectId, sessionId) =>
        `remote:machine-1:session:${projectId}:${sessionId}`,
      focusedSessionId: sessionId,
      index: 0,
      isActiveProject: true,
      presentation: session,
      projectId,
      resolveAgentIcon: () => "codex",
    });

    expect(row).toMatchObject({
      activity: "idle",
      agentIcon: "codex",
      displayTitle: "ghostex@remote: ~/ghostex",
      displayTitleTooltip: "ghostex@remote: ~/ghostex",
      isFocused: true,
      isLive: true,
      isRunning: true,
      lifecycleState: "running",
      primaryTitle: "Remote Ghostex",
      providerSessionState: "exists",
      sessionId: "remote:machine-1:session:P3a91:G8v20",
      sessionPersistenceName: "S7k-P3a91-G8v20",
      sessionPersistenceProvider: "zmx",
      sessionTag: "blocked",
      terminalTitle: "ghostex@remote: ~/ghostex",
    });
  });

  /*
  CDXC:SidebarV2Lifecycle 2026-07-29:
  Settle/snooze is server-owned state that the sidebar only renders. If the
  projection drops a field, the V2 shelves silently go empty and the bug looks
  like a UI regression, so the copy-through is pinned here.
  */
  test("carries the settle/snooze lifecycle fields into sidebar rows", () => {
    const row = createGxserverPresentationSidebarSession({
      index: 0,
      isActiveProject: false,
      presentation: createPresentationSession({
        settledAt: "2026-07-28T09:00:00.000Z",
        settledOverride: "settled",
        snoozedAt: "2026-07-29T08:00:00.000Z",
        snoozedUntil: "2026-07-29T18:00:00.000Z",
      }),
      projectId: "P3a91" as GxserverProjectId,
      resolveAgentIcon: () => "codex",
    });

    expect(row).toMatchObject({
      settledAt: "2026-07-28T09:00:00.000Z",
      settledOverride: "settled",
      snoozedAt: "2026-07-29T08:00:00.000Z",
      snoozedUntil: "2026-07-29T18:00:00.000Z",
    });
  });

  test("leaves the lifecycle fields undefined for a daemon that publishes none", () => {
    const row = createGxserverPresentationSidebarSession({
      index: 0,
      isActiveProject: false,
      presentation: createPresentationSession(),
      projectId: "P3a91" as GxserverProjectId,
      resolveAgentIcon: () => "codex",
    });

    expect(row.settledAt).toBeUndefined();
    expect(row.settledOverride).toBeUndefined();
    expect(row.snoozedAt).toBeUndefined();
    expect(row.snoozedUntil).toBeUndefined();
  });

  /*
  CDXC:SidebarV2Git 2026-07-29:
  Only gxserver can run git in a session's cwd, so a dropped field here would
  silently empty the card's branch/PR line with no other symptom. The
  copy-through is pinned, including the null branch a detached HEAD reports.
  */
  test("carries the git/PR status object into sidebar rows", () => {
    const row = createGxserverPresentationSidebarSession({
      index: 0,
      isActiveProject: false,
      presentation: createPresentationSession({
        gitStatus: {
          additions: 412,
          branch: "ghostex/sidebar-v2",
          deletions: 87,
          prNumber: 128,
          prState: "open",
          prUrl: "https://github.com/ghostex/ghostex/pull/128",
          updatedAt: "2026-07-29T10:00:00.000Z",
        },
      }),
      projectId: "P3a91" as GxserverProjectId,
      resolveAgentIcon: () => "codex",
    });

    expect(row.gitStatus).toEqual({
      additions: 412,
      branch: "ghostex/sidebar-v2",
      deletions: 87,
      prNumber: 128,
      prState: "open",
      prUrl: "https://github.com/ghostex/ghostex/pull/128",
      updatedAt: "2026-07-29T10:00:00.000Z",
    });
  });

  test("leaves gitStatus undefined for a session the daemon never probed", () => {
    const row = createGxserverPresentationSidebarSession({
      index: 0,
      isActiveProject: false,
      presentation: createPresentationSession(),
      projectId: "P3a91" as GxserverProjectId,
      resolveAgentIcon: () => "codex",
    });

    expect(row.gitStatus).toBeUndefined();
  });
});

describe("gxserverPresentationSidebarLifecycleCapabilities", () => {
  test("normalizes a published capability block", () => {
    expect(
      gxserverPresentationSidebarLifecycleCapabilities({
        capabilities: { sessionSettlement: true, sessionSnooze: false },
      } as GxserverPresentationSnapshot),
    ).toEqual({
      sessionGitStatus: false,
      sessionSettlement: true,
      sessionSnooze: false,
      worktreeSessions: false,
    });
  });

  /*
  CDXC:SidebarV2Git 2026-07-29:
  A daemon can publish settle/snooze and still predate the git probe, so the
  missing key must normalize to false rather than undefined — the sidebar reads
  "no git data from this machine" and renders plain cards.
  */
  test("reports the git capability separately from the lifecycle ones", () => {
    expect(
      gxserverPresentationSidebarLifecycleCapabilities({
        capabilities: {
          sessionGitStatus: true,
          sessionSettlement: true,
          sessionSnooze: true,
        },
      } as GxserverPresentationSnapshot),
    ).toEqual({
      sessionGitStatus: true,
      sessionSettlement: true,
      sessionSnooze: true,
      worktreeSessions: false,
    });
  });

  /*
  CDXC:SidebarV2Worktree 2026-07-29:
  The worktree flow is its own capability step: a P3 daemon publishes git state
  and still cannot cut worktrees, and V2's split "+" must collapse for it.
  */
  test("reports the worktree capability separately", () => {
    expect(
      gxserverPresentationSidebarLifecycleCapabilities({
        capabilities: {
          sessionGitStatus: true,
          sessionSettlement: true,
          sessionSnooze: true,
          worktreeSessions: true,
        },
      } as GxserverPresentationSnapshot),
    ).toEqual({
      sessionGitStatus: true,
      sessionSettlement: true,
      sessionSnooze: true,
      worktreeSessions: true,
    });
  });

  /* An older gxserver publishes no `capabilities` at all. Reporting `undefined`
     (rather than a pair of falses) lets the sidebar tell "unsupported" apart
     from "supported but off", which is what hides the affordances. */
  test("reports undefined when the snapshot predates session lifecycle", () => {
    expect(gxserverPresentationSidebarLifecycleCapabilities({} as GxserverPresentationSnapshot)).toBeUndefined();
    expect(gxserverPresentationSidebarLifecycleCapabilities(undefined)).toBeUndefined();
  });
});

function createPresentationSession(
  overrides: Partial<GxserverPresentationSession> = {},
): GxserverPresentationSession {
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
    createdAt: "2026-06-30T00:00:00.000Z",
    groupId: "P3a91:active",
    isFavorite: false,
    isGeneratingFirstPromptTitle: false,
    isPinned: false,
    isPrimaryTitleTerminalTitle: true,
    isTemporaryTitle: false,
    kind: "terminal",
    lifecycleState: "running",
    projectId: "P3a91" as GxserverProjectId,
    providerSessionState: "exists",
    sessionId: "G8v20" as GxserverSessionId,
    sortKey: "0:G8v20",
    surface: "workspace",
    title: "Remote Session",
    titleSource: "terminal-auto",
    updatedAt: "2026-06-30T00:00:00.000Z",
    visibleInSidebarByDefault: true,
    zmxName: "S7k-P3a91-G8v20" as GxserverZmxSessionName,
    ...overrides,
  };
}

/*
CDXC:SidebarV2LogicalProjects 2026-07-29:
The two P5 wire fields. Both carry a three-state meaning (absent / null /
value), and both would be silently broken by a projection that collapsed absent
into null — the sidebar's fallback rules read the difference.
*/
describe("gxserverPresentationSidebarAutoSettleAfterDays", () => {
  test("carries a published window through", () => {
    expect(
      gxserverPresentationSidebarAutoSettleAfterDays({ autoSettleAfterDays: 14 }),
    ).toBe(14);
  });

  test("keeps an explicit null (this daemon does not inactivity-settle)", () => {
    expect(
      gxserverPresentationSidebarAutoSettleAfterDays({ autoSettleAfterDays: null }),
    ).toBeNull();
  });

  test("keeps an unstated window UNDEFINED, never null", () => {
    expect(gxserverPresentationSidebarAutoSettleAfterDays({})).toBeUndefined();
    expect(gxserverPresentationSidebarAutoSettleAfterDays(undefined)).toBeUndefined();
  });

  test("treats zero and negatives as 'off', matching the settings normalizer", () => {
    expect(gxserverPresentationSidebarAutoSettleAfterDays({ autoSettleAfterDays: 0 })).toBeNull();
    expect(gxserverPresentationSidebarAutoSettleAfterDays({ autoSettleAfterDays: -3 })).toBeNull();
    expect(
      gxserverPresentationSidebarAutoSettleAfterDays({ autoSettleAfterDays: Number.NaN }),
    ).toBeNull();
  });
});

describe("gitRemoteOriginUrl projection", () => {
  function createPresentationProject(
    overrides: Partial<GxserverPresentationProject> = {},
  ): GxserverPresentationProject {
    return {
      createdAt: "2026-06-30T00:00:00.000Z",
      groupIds: [],
      isFavorite: false,
      isPinned: false,
      path: "/Users/madda/dev/Ghostex",
      projectId: "P3a91" as GxserverProjectId,
      sortKey: "0",
      title: "Ghostex",
      updatedAt: "2026-06-30T00:00:00.000Z",
      ...overrides,
    };
  }

  function projectContextFor(project: GxserverPresentationProject) {
    return createGxserverPresentationSidebarGroup({
      project,
      resolveAgentIcon: () => undefined,
      sessions: [],
    }).projectContext;
  }

  test("carries a probed origin remote onto project context", () => {
    expect(
      projectContextFor(
        createPresentationProject({ gitRemoteOriginUrl: "git@github.com:ghostex/ghostex.git" }),
      )?.gitRemoteOriginUrl,
    ).toBe("git@github.com:ghostex/ghostex.git");
  });

  test("carries an explicit null (probed, no origin)", () => {
    const projectContext = projectContextFor(
      createPresentationProject({ gitRemoteOriginUrl: null }),
    );
    expect(projectContext?.gitRemoteOriginUrl).toBeNull();
    expect("gitRemoteOriginUrl" in (projectContext ?? {})).toBe(true);
  });

  test("omits the key entirely for an unprobed project", () => {
    const projectContext = projectContextFor(createPresentationProject());
    expect("gitRemoteOriginUrl" in (projectContext ?? {})).toBe(false);
  });

  /*
  CDXC:SidebarV2LogicalProjects 2026-07-29 (P5 fix round):
  The repository root travels the same way. Sidebar V2's "Repository + path"
  mode is derived from it, so a projection that dropped it would leave the mode
  inert no matter what the daemon probed.
  */
  test("carries the probed repository root onto project context", () => {
    expect(
      projectContextFor(
        createPresentationProject({
          gitRemoteOriginUrl: "git@github.com:ghostex/ghostex.git",
          gitRepositoryRootPath: "/Users/madda/dev/Ghostex",
        }),
      )?.gitRepositoryRootPath,
    ).toBe("/Users/madda/dev/Ghostex");
  });

  test("omits the repository root when the daemon published none", () => {
    const projectContext = projectContextFor(
      createPresentationProject({ gitRemoteOriginUrl: "git@github.com:ghostex/ghostex.git" }),
    );
    expect("gitRepositoryRootPath" in (projectContext ?? {})).toBe(false);
  });

  /*
  CDXC:SidebarV2ProjectIcons 2026-07-29:
  The TYPED project icon is the one most Ghostex projects actually have (a
  Tabler glyph with a color); a projection that carried only `iconDataUrl`
  showed those projects a generic folder in every sidebar surface.
  */
  test("carries the typed project icon from the overlay onto project context", () => {
    const projectContext = createGxserverPresentationSidebarGroup({
      project: createPresentationProject(),
      projectOverlay: {
        icon: { color: "#d6e0f3", icon: "archive", kind: "tabler" },
        projectId: "P3a91",
      },
      resolveAgentIcon: () => undefined,
      sessions: [],
    }).projectContext;
    expect(projectContext?.icon).toEqual({ color: "#d6e0f3", icon: "archive", kind: "tabler" });
  });

  /*
  CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
  The icon gxserver discovered inside the checkout rides the presentation
  PROJECT, not the host overlay, so it also reaches projects on remote machines
  (which have no local overlay at all). It must arrive beside the user's icon
  rather than merged into it, or the renderer could not keep the user's choice
  on top.
  */
  test("carries the discovered repository icon from the presentation project", () => {
    const discoveredIconDataUrl = "data:image/png;base64,ZGlzY292ZXJlZA==";
    const projectContext = createGxserverPresentationSidebarGroup({
      project: createPresentationProject({ discoveredIconDataUrl }),
      projectOverlay: {
        icon: { color: "#d6e0f3", icon: "archive", kind: "tabler" },
        projectId: "P3a91",
      },
      resolveAgentIcon: () => undefined,
      sessions: [],
    }).projectContext;
    expect(projectContext?.discoveredIconDataUrl).toBe(discoveredIconDataUrl);
    expect(projectContext?.icon).toEqual({ color: "#d6e0f3", icon: "archive", kind: "tabler" });
  });

  test("omits the discovered icon key when the daemon published none", () => {
    const projectContext = createGxserverPresentationSidebarGroup({
      project: createPresentationProject(),
      projectOverlay: { projectId: "P3a91" },
      resolveAgentIcon: () => undefined,
      sessions: [],
    }).projectContext;
    expect("discoveredIconDataUrl" in (projectContext ?? {})).toBe(false);
  });

  test("omits the icon key for a project with no icon at all", () => {
    const projectContext = createGxserverPresentationSidebarGroup({
      project: createPresentationProject(),
      projectOverlay: { projectId: "P3a91" },
      resolveAgentIcon: () => undefined,
      sessions: [],
    }).projectContext;
    expect("icon" in (projectContext ?? {})).toBe(false);
  });
});
