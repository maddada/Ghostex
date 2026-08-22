import {
  GRID_COLUMN_COUNT,
  clampVisibleSessionCount,
  type SidebarSessionGroup,
  type SidebarSessionItem,
  type SidebarSessionLifecycleCapabilities,
} from "./session-grid-contract";
import type {
  GxserverDomainLifecycleState,
  GxserverPresentationProject,
  GxserverPresentationSession,
  GxserverPresentationSnapshot,
} from "./gxserver-protocol";
import { createDefaultSidebarProjectDiffStats } from "./project-diff-stats";
import { orderProjectsWithWorktrees } from "./project-worktree-order";

export const GXSERVER_PRESENTATION_CHATS_GROUP_ID = "combined-chats";

const GXSERVER_PRESENTATION_PROJECT_GROUP_ID_PREFIX = "combined-project:";
const GXSERVER_PRESENTATION_PROJECT_SESSION_ID_PREFIX = "combined-session:";
const GXSERVER_PRESENTATION_ID_SEPARATOR = ":";

export type GxserverPresentationSidebarSessionKey = string;

export type GxserverPresentationSidebarSessionReference = {
  projectId: string;
  sessionId: string;
};

export type GxserverPresentationDelayedSendProjection = {
  deadlineAt?: string;
  remainingLabel?: string;
  remainingMs?: number;
  sendWhenAllProjectSessionsStopActive?: boolean;
  sendWhenAgentStopsActive?: boolean;
};

export type GxserverPresentationCloseAfterDoneProjection = {
  armed: boolean;
  deadlineAt?: string;
  remainingLabel?: string;
  remainingMs?: number;
};

export type GxserverPresentationSidebarProjectOverlay = {
  editor?: NonNullable<SidebarSessionGroup["projectContext"]>["editor"];
  /*
  CDXC:SidebarV2ProjectIcons 2026-07-29:
  The typed project icon rides beside the image-only `iconDataUrl` because a
  Tabler glyph plus a color is the icon most Ghostex projects actually have.
  Hosts that only know about image icons keep publishing `iconDataUrl` and
  nothing changes for them.
  */
  icon?: NonNullable<SidebarSessionGroup["projectContext"]>["icon"];
  iconDataUrl?: string;
  isChatProject?: boolean;
  isQuickProject?: boolean;
  orderIndex?: number;
  path?: string;
  projectId: string;
  theme?: NonNullable<SidebarSessionGroup["projectContext"]>["theme"];
  themeColor?: string;
  title?: string;
  worktree?: NonNullable<SidebarSessionGroup["projectContext"]>["worktree"];
};

export type GxserverPresentationSidebarInput = {
  activeProjectId?: string;
  chatProjectIds?: ReadonlySet<string>;
  chatsGroupId?: string;
  createProjectGroupId?: (projectId: string) => string;
  createProjectSessionId?: (projectId: string, sessionId: string) => string;
  focusedSessionId?: string;
  hiddenProjectIds?: ReadonlySet<string>;
  hiddenSessionKeys?: ReadonlySet<GxserverPresentationSidebarSessionKey>;
  presentation: GxserverPresentationSnapshot;
  projectOverlays?: readonly GxserverPresentationSidebarProjectOverlay[];
  resolveAgentIcon: (agentName: string | undefined) => SidebarSessionItem["agentIcon"];
  resolveCloseAfterDone?: (
    projectId: string,
    sessionId: string,
  ) => GxserverPresentationCloseAfterDoneProjection | undefined;
  resolveDelayedSend?: (
    projectId: string,
    sessionId: string,
  ) => GxserverPresentationDelayedSendProjection | undefined;
  resolveSessionRoutingId?: (projectId: string, sessionId: string) => string | undefined;
  visibleSessionIds?: ReadonlySet<string>;
};

export type GxserverPresentationSidebarSessionInput = {
  createProjectSessionId?: (projectId: string, sessionId: string) => string;
  focusedSessionId?: string;
  index: number;
  isActiveProject: boolean;
  localSession?: SidebarSessionItem;
  presentation: GxserverPresentationSession;
  projectId: string;
  resolveAgentIcon: (agentName: string | undefined) => SidebarSessionItem["agentIcon"];
  resolveCloseAfterDone?: (
    projectId: string,
    sessionId: string,
  ) => GxserverPresentationCloseAfterDoneProjection | undefined;
  resolveDelayedSend?: (
    projectId: string,
    sessionId: string,
  ) => GxserverPresentationDelayedSendProjection | undefined;
  resolveProviderSessionState?: (
    presentation: Pick<GxserverPresentationSession, "lifecycleState" | "providerSessionState">,
    localSession: SidebarSessionItem | undefined,
  ) => NonNullable<SidebarSessionItem["providerSessionState"]>;
  resolveSessionRoutingId?: (projectId: string, sessionId: string) => string | undefined;
  visibleSessionIds?: ReadonlySet<string>;
};

export type GxserverPresentationSidebarGroupInput = {
  activeProjectId?: string;
  canRemoveProject?: boolean;
  createProjectGroupId?: (projectId: string) => string;
  createProjectSessionId?: (projectId: string, sessionId: string) => string;
  extraSessions?: readonly SidebarSessionItem[];
  focusedSessionId?: string;
  project: GxserverPresentationProject;
  projectOverlay?: GxserverPresentationSidebarProjectOverlay;
  resolveAgentIcon: (agentName: string | undefined) => SidebarSessionItem["agentIcon"];
  resolveCloseAfterDone?: (
    projectId: string,
    sessionId: string,
  ) => GxserverPresentationCloseAfterDoneProjection | undefined;
  resolveDelayedSend?: (
    projectId: string,
    sessionId: string,
  ) => GxserverPresentationDelayedSendProjection | undefined;
  resolveLocalSession?: (projectId: string, sessionId: string) => SidebarSessionItem | undefined;
  resolveProviderSessionState?: (
    presentation: Pick<GxserverPresentationSession, "lifecycleState" | "providerSessionState">,
    localSession: SidebarSessionItem | undefined,
  ) => NonNullable<SidebarSessionItem["providerSessionState"]>;
  resolveSessionRoutingId?: (projectId: string, sessionId: string) => string | undefined;
  sessions: readonly GxserverPresentationSession[];
  visibleSessionIds?: ReadonlySet<string>;
};

/*
CDXC:GxserverPresentationParity 2026-06-24-10:45:
GPUI must render gxserver sessions through the same React sidebar contract as macOS. This shared projection maps gxserver presentation snapshots into sidebar groups without AppKit, filesystem, browser, or native pane ownership; platform wrappers may join local resource state through explicit overlay inputs.
*/
export function createGxserverPresentationSidebarGroups(
  input: GxserverPresentationSidebarInput,
): SidebarSessionGroup[] {
  const projectOverlaysById = new Map(
    (input.projectOverlays ?? []).map((project) => [project.projectId, project]),
  );
  const sessionsByProject = createGxserverPresentationSessionsByProjectFromGroups({
    hiddenProjectIds: input.hiddenProjectIds,
    hiddenSessionKeys: input.hiddenSessionKeys,
    presentation: input.presentation,
  });
  const visibleProjects = input.presentation.projects.filter(
    (project) => !input.hiddenProjectIds?.has(project.projectId),
  );
  const chatProjects = orderGxserverPresentationSidebarProjects(
    visibleProjects.filter((project) =>
      isGxserverPresentationChatProject(input, project, projectOverlaysById.get(project.projectId)),
    ),
    projectOverlaysById,
  );
  const chatSessions = chatProjects.flatMap((project) => {
    const isActiveProject = project.projectId === input.activeProjectId;
    return (sessionsByProject.get(project.projectId) ?? []).map((session, index) =>
      createGxserverPresentationSidebarSession({
        createProjectSessionId: input.createProjectSessionId,
        focusedSessionId: input.focusedSessionId,
        index,
        isActiveProject,
        presentation: session,
        projectId: project.projectId,
        resolveAgentIcon: input.resolveAgentIcon,
        resolveCloseAfterDone: input.resolveCloseAfterDone,
        resolveDelayedSend: input.resolveDelayedSend,
        resolveSessionRoutingId: input.resolveSessionRoutingId,
        visibleSessionIds: input.visibleSessionIds,
      }),
    );
  });
  const projectGroups = orderGxserverPresentationSidebarProjects(
    visibleProjects.filter((project) =>
      !isGxserverPresentationChatProject(input, project, projectOverlaysById.get(project.projectId)),
    ),
    projectOverlaysById,
  ).map((project) =>
    createGxserverPresentationSidebarGroup({
      activeProjectId: input.activeProjectId,
      createProjectGroupId: input.createProjectGroupId,
      createProjectSessionId: input.createProjectSessionId,
      focusedSessionId: input.focusedSessionId,
      project,
      projectOverlay: projectOverlaysById.get(project.projectId),
      resolveAgentIcon: input.resolveAgentIcon,
      resolveCloseAfterDone: input.resolveCloseAfterDone,
      resolveDelayedSend: input.resolveDelayedSend,
      resolveSessionRoutingId: input.resolveSessionRoutingId,
      sessions: sessionsByProject.get(project.projectId) ?? [],
      visibleSessionIds: input.visibleSessionIds,
    }),
  );

  return [
    {
      groupId: input.chatsGroupId ?? GXSERVER_PRESENTATION_CHATS_GROUP_ID,
      isActive:
        chatProjects.some((project) => project.projectId === input.activeProjectId) ||
        (input.projectOverlays ?? []).some(
          (project) =>
            project.projectId === input.activeProjectId &&
            (project.isQuickProject === true || project.isChatProject === true),
        ),
      isChatCollection: true,
      isFocusModeActive: false,
      kind: "workspace",
      layoutVisibleCount: visibleCountForGxserverPresentationSidebarSessions(chatSessions),
      sessions: chatSessions,
      title: "Chats",
      viewMode: "grid",
      visibleCount: visibleCountForGxserverPresentationSidebarSessions(chatSessions),
    },
    ...projectGroups,
  ];
}

export function createGxserverPresentationSessionsByProjectFromGroups({
  hiddenProjectIds,
  hiddenSessionKeys,
  presentation,
}: {
  hiddenProjectIds?: ReadonlySet<string>;
  hiddenSessionKeys?: ReadonlySet<GxserverPresentationSidebarSessionKey>;
  presentation: GxserverPresentationSnapshot;
}): Map<string, GxserverPresentationSession[]> {
  const sessionByProjectSessionKey = new Map(
    presentation.sessions.map((session) => [
      createGxserverPresentationSidebarSessionKey(session.projectId, session.sessionId),
      session,
    ]),
  );
  const sessionsByProject = new Map<string, GxserverPresentationSession[]>();
  for (const group of presentation.groups) {
    if (hiddenProjectIds?.has(group.projectId)) {
      continue;
    }
    const sessions = sessionsByProject.get(group.projectId) ?? [];
    for (const sessionId of group.sessionIds) {
      const session = sessionByProjectSessionKey.get(
        createGxserverPresentationSidebarSessionKey(group.projectId, sessionId),
      );
      if (
        !session ||
        session.visibleInSidebarByDefault !== true ||
        session.surface === "commands" ||
        hiddenSessionKeys?.has(
          createGxserverPresentationSidebarSessionKey(session.projectId, session.sessionId),
        )
      ) {
        continue;
      }
      sessions.push(session);
    }
    sessionsByProject.set(group.projectId, sessions);
  }
  return sessionsByProject;
}

export function createGxserverPresentationSidebarGroup({
  activeProjectId,
  canRemoveProject = true,
  createProjectGroupId = createGxserverPresentationProjectGroupId,
  createProjectSessionId = createGxserverPresentationProjectSessionId,
  extraSessions = [],
  focusedSessionId,
  project,
  projectOverlay,
  resolveAgentIcon,
  resolveCloseAfterDone,
  resolveDelayedSend,
  resolveLocalSession,
  resolveProviderSessionState,
  resolveSessionRoutingId,
  sessions,
  visibleSessionIds,
}: GxserverPresentationSidebarGroupInput): SidebarSessionGroup {
  const isActiveProject = project.projectId === activeProjectId;
  const presentationSidebarSessions = sessions.map((session, index) =>
    createGxserverPresentationSidebarSession({
      createProjectSessionId,
      focusedSessionId,
      index,
      isActiveProject,
      localSession: resolveLocalSession?.(project.projectId, session.sessionId),
      presentation: session,
      projectId: project.projectId,
      resolveAgentIcon,
      resolveCloseAfterDone,
      resolveDelayedSend,
      resolveProviderSessionState,
      resolveSessionRoutingId,
      visibleSessionIds,
    }),
  );
  const sidebarSessions = [...presentationSidebarSessions, ...extraSessions];
  const projectContext = {
    canRemoveProject,
    editor: projectOverlay?.editor ?? createIdleGxserverPresentationProjectEditorState(project.projectId),
    /*
    CDXC:SidebarV2LogicalProjects 2026-07-29:
    Carried straight through, including the absent-vs-null distinction: the
    daemon owns the probe and the client owns the normalization. Spreading only
    when the key is present keeps "not probed" from being flattened into
    "no origin" by the projection.
    */
    ...(project.gitRemoteOriginUrl === undefined
      ? {}
      : { gitRemoteOriginUrl: project.gitRemoteOriginUrl }),
    /*
    CDXC:SidebarV2LogicalProjects 2026-07-29 (P5 fix round):
    The repository root rides through the same way, and for the same reason:
    the daemon owns the probe, the client owns the interpretation. It has no
    null state, so an absent key is simply not spread.
    */
    ...(project.gitRepositoryRootPath === undefined
      ? {}
      : { gitRepositoryRootPath: project.gitRepositoryRootPath }),
    /*
    CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
    The icon the project's own repository ships, straight off the presentation
    project. It rides BESIDE the overlay's user-chosen icon rather than merging
    into it, because the renderer resolves them in a fixed order (user IMAGE,
    then discovered icon, then typed glyph, then folder) which a single merged
    field could not express.
    */
    ...(project.discoveredIconDataUrl === undefined
      ? {}
      : { discoveredIconDataUrl: project.discoveredIconDataUrl }),
    ...(projectOverlay?.icon === undefined ? {} : { icon: projectOverlay.icon }),
    iconDataUrl: projectOverlay?.iconDataUrl,
    path: projectOverlay?.path || project.path || "",
    pathState: project.pathState,
    theme: projectOverlay?.theme,
    themeColor: projectOverlay?.themeColor,
    worktree: projectOverlay?.worktree,
  };
  return {
    groupId: createProjectGroupId(project.projectId),
    canFocusMode: false,
    isActive: isActiveProject,
    isFocusModeActive: false,
    kind: "workspace",
    layoutVisibleCount: visibleCountForGxserverPresentationSidebarSessions(sidebarSessions),
    projectContext,
    sessions: sidebarSessions,
    title: project.title,
    viewMode: "grid",
    visibleCount: visibleCountForGxserverPresentationSidebarSessions(sidebarSessions),
  };
}

export function createGxserverPresentationSidebarSession({
  createProjectSessionId = createGxserverPresentationProjectSessionId,
  focusedSessionId,
  index,
  isActiveProject,
  localSession,
  presentation,
  projectId,
  resolveAgentIcon,
  resolveCloseAfterDone,
  resolveDelayedSend,
  resolveProviderSessionState,
  resolveSessionRoutingId,
  visibleSessionIds,
}: GxserverPresentationSidebarSessionInput): SidebarSessionItem {
  const lifecycleState = presentationLifecycleStateForSidebar(presentation.lifecycleState);
  const nativePaneState = localSession?.nativePaneState;
  const providerSessionState =
    resolveProviderSessionState?.(presentation, localSession) ??
    providerSessionStateForGxserverPresentation(presentation);
  const isLive =
    providerSessionState === "exists" ||
    nativePaneState === "mounted" ||
    nativePaneState === "mounting";
  const closeAfterDone = resolveCloseAfterDone?.(projectId, presentation.sessionId);
  const serverDelayedSend =
    presentation.delayedSendDeadlineAt ||
    presentation.delayedSendRemainingLabel ||
    presentation.delayedSendRemainingMs !== undefined ||
    presentation.sendWhenAllProjectSessionsStopActive === true ||
    presentation.sendWhenAgentStopsActive === true
      ? {
          deadlineAt: presentation.delayedSendDeadlineAt,
          remainingLabel: presentation.delayedSendRemainingLabel,
          remainingMs: presentation.delayedSendRemainingMs,
          sendWhenAllProjectSessionsStopActive:
            presentation.sendWhenAllProjectSessionsStopActive,
          sendWhenAgentStopsActive: presentation.sendWhenAgentStopsActive,
        }
      : undefined;
  const delayedSend = serverDelayedSend ?? resolveDelayedSend?.(projectId, presentation.sessionId);
  const agentIcon = resolveAgentIcon(
    presentation.agentIcon ?? presentation.agentName ?? presentation.agentId,
  );
  return {
    activity: presentation.activity,
    agentIcon,
    agentSessionId: presentation.agentSessionId ?? localSession?.agentSessionId,
    alias: presentation.title,
    closeAfterDone: closeAfterDone?.armed,
    closeAfterDoneDeadlineAt: closeAfterDone?.deadlineAt,
    closeAfterDoneRemainingLabel: closeAfterDone?.remainingLabel,
    closeAfterDoneRemainingMs: closeAfterDone?.remainingMs,
    column: index % GRID_COLUMN_COUNT,
    /*
    CDXC:SidebarV2 2026-07-29:
    gxserver already stamps every presentation session with `createdAt`; carry
    it through so the V2 inbox can hold a position-stable creation order
    instead of inferring one from first-seen client state.
    */
    createdAt: presentation.createdAt,
    /*
    CDXC:SidebarV2Worktree 2026-07-29:
    The session's cwd IS its worktree in V2's model, so the inbox needs it to
    tell a managed `ghostex/…` checkout from a session in the project root and
    to name the folder its cleanup prompt would remove.
    */
    cwd: presentation.cwd,
    detail: presentation.subtitle,
    delayedSendDeadlineAt: delayedSend?.deadlineAt,
    delayedSendRemainingLabel: delayedSend?.remainingLabel,
    delayedSendRemainingMs: delayedSend?.remainingMs,
    sendWhenAllProjectSessionsStopActive:
      delayedSend?.sendWhenAllProjectSessionsStopActive === true ? true : undefined,
    sendWhenAgentStopsActive:
      delayedSend?.sendWhenAgentStopsActive === true ? true : undefined,
    /*
    CDXC:SessionChatPromptQueue 2026-08-21:
    The queued-prompt count is daemon-owned, exactly like the Delayed Send
    countdown above it, so it is copied straight through. Anything that is not
    a positive number collapses to `undefined` so a `0` or a garbled value from
    an unknown daemon can never render an empty badge.

    CDXC:SessionChatPromptQueue 2026-08-21-b:
    The count now includes `failed` rows, and the failed tally rides with it so
    the badge can turn red. The two are projected independently on purpose: a
    daemon that predates the failed tally still badges yellow with a correct
    count rather than dropping the badge entirely.
    */
    queuedPromptCount:
      typeof presentation.queuedPromptCount === "number" && presentation.queuedPromptCount > 0
        ? Math.floor(presentation.queuedPromptCount)
        : undefined,
    queuedPromptFailedCount:
      typeof presentation.queuedPromptFailedCount === "number" &&
      presentation.queuedPromptFailedCount > 0
        ? Math.floor(presentation.queuedPromptFailedCount)
        : undefined,
    displayTitle: presentation.displayTitle,
    displayTitleTooltip: presentation.displayTitleTooltip,
    /*
    CDXC:SidebarV2Git 2026-07-29:
    Git/PR state is probed server-side from the session cwd, so it is copied
    through by reference exactly like the lifecycle fields: undefined stays
    undefined, and the card simply has no branch line for a session (or a
    daemon) with nothing to say.
    */
    gitStatus: presentation.gitStatus,
    isFavorite: presentation.isFavorite,
    isFocused: isActiveProject && focusedSessionId === presentation.sessionId,
    isGeneratingFirstPromptTitle: presentation.isGeneratingFirstPromptTitle,
    isLive,
    isPinned: presentation.isPinned,
    isPrimaryTitleTerminalTitle: presentation.isPrimaryTitleTerminalTitle,
    isRunning: isLive,
    isSleeping: lifecycleState === "sleeping",
    isVisible: isActiveProject && (
      visibleSessionIds?.has(presentation.sessionId) === true ||
      index === 0
    ),
    lastInteractionAt:
      presentation.meaningfulActivityAt ?? presentation.lastActiveAt ?? presentation.updatedAt,
    lifecycleState,
    nativePaneState,
    primaryTitle: presentation.primaryTitle ?? presentation.title,
    providerSessionState,
    row: Math.floor(index / GRID_COLUMN_COUNT),
    sessionId: createProjectSessionId(projectId, presentation.sessionId),
    sessionKind: presentation.kind === "agent" ? "terminal" : presentation.kind,
    sessionTag: presentation.sessionTag,
    sessionNumber: String(index + 1),
    sessionPersistenceName: presentation.zmxName,
    sessionPersistenceProvider: presentation.sessionPersistenceProvider,
    sessionRoutingId: resolveSessionRoutingId?.(projectId, presentation.sessionId),
    /*
    CDXC:SidebarV2Lifecycle 2026-07-29:
    Settle/snooze state is server-owned, so it is copied through verbatim
    (undefined stays undefined) instead of being defaulted here. A daemon that
    predates the lifecycle publishes none of these fields, and the V2 partition
    reads that absence as "not settled, not snoozed" — the same answer the
    capability flags force anyway.
    */
    settledAt: presentation.settledAt,
    settledOverride: presentation.settledOverride,
    shortcutLabel: String(index + 1),
    snoozedAt: presentation.snoozedAt,
    snoozedUntil: presentation.snoozedUntil,
    terminalTitle: presentation.terminalTitle,
    titleObservation: presentation.titleObservation,
    workingStartedAt: presentation.workingStartedAt,
  };
}

/**
 * CDXC:SidebarV2Lifecycle 2026-07-29:
 * One daemon's settle/snooze capability, projected for `SidebarHudState`.
 * Returns `undefined` when the snapshot carries no `capabilities` block at all
 * — that is exactly how an older gxserver looks, and the sidebar must hide the
 * lifecycle affordances rather than assume support.
 *
 * CDXC:SidebarV2Git 2026-07-29:
 * `sessionGitStatus` is normalized here too, and every flag is normalized to a
 * real boolean rather than passed through: a daemon that publishes the block
 * for settle/snooze but predates the git probe omits the key, and the sidebar
 * must read that as "no git data from this machine".
 */
export function gxserverPresentationSidebarLifecycleCapabilities(
  presentation: Pick<GxserverPresentationSnapshot, "capabilities"> | undefined,
): SidebarSessionLifecycleCapabilities | undefined {
  const capabilities = presentation?.capabilities;
  if (!capabilities) {
    return undefined;
  }
  return {
    sessionGitStatus: capabilities.sessionGitStatus === true,
    sessionSettlement: capabilities.sessionSettlement === true,
    sessionSnooze: capabilities.sessionSnooze === true,
    /*
    CDXC:SidebarV2Worktree 2026-07-29:
    Normalized to a real boolean for the same reason as the git flag: a daemon
    that publishes this block for settle/snooze but predates the worktree flow
    omits the key, and the sidebar must read that as "this machine cannot cut
    worktrees" rather than as undefined-means-maybe.
    */
    worktreeSessions: capabilities.worktreeSessions === true,
  };
}

/*
CDXC:SidebarV2LogicalProjects 2026-07-29:
The auto-settle window a daemon states for ITSELF. `undefined` is preserved as
"this daemon never said" — the caller decides what that means per machine — and
anything the daemon publishes is normalized with the exact rule the settings
schema and server already share: a finite positive number is a window,
every other number (0, negatives, NaN) means auto-settle is off, and so does an
explicit null. A non-number value is treated as unstated rather than as off, so
a malformed field cannot silently disable a machine's shelf.
*/
export function gxserverPresentationSidebarAutoSettleAfterDays(
  presentation: Pick<GxserverPresentationSnapshot, "autoSettleAfterDays"> | undefined,
): number | null | undefined {
  const value = presentation?.autoSettleAfterDays;
  if (value === undefined) {
    return undefined;
  }
  if (value === null) {
    return null;
  }
  if (typeof value !== "number") {
    return undefined;
  }
  return Number.isFinite(value) && value > 0 ? value : null;
}

export function presentationLifecycleStateForSidebar(
  lifecycleState: GxserverDomainLifecycleState,
): NonNullable<SidebarSessionItem["lifecycleState"]> {
  switch (lifecycleState) {
    case "running":
      return "running";
    case "sleeping":
      return "sleeping";
    case "missing":
    case "unknown":
      return "error";
    case "stopped":
    default:
      return "done";
  }
}

export function providerSessionStateForGxserverPresentation(
  presentation: Pick<GxserverPresentationSession, "lifecycleState" | "providerSessionState">,
): NonNullable<SidebarSessionItem["providerSessionState"]> {
  /*
  CDXC:GxserverPresentation 2026-06-24-10:45:
  Provider liveness is a shared gxserver presentation concept, while platform panes are optional overlays. Resolve the daemon-published provider state here so macOS and GPUI cards agree on zmx/tmux/zellij liveness before any local resource override is applied.
  */
  if (presentation.providerSessionState) {
    return presentation.providerSessionState;
  }
  switch (presentation.lifecycleState) {
    case "running":
      return "exists";
    case "sleeping":
    case "missing":
    case "stopped":
      return "missing";
    case "unknown":
    default:
      return "unknown";
  }
}

export function orderGxserverPresentationSidebarProjects(
  presentationProjects: readonly GxserverPresentationProject[],
  projectOverlaysById: ReadonlyMap<string, GxserverPresentationSidebarProjectOverlay> = new Map(),
): GxserverPresentationProject[] {
  const presentationProjectById = new Map(
    presentationProjects.map((project) => [project.projectId, project]),
  );
  return orderProjectsWithWorktrees(
    [...presentationProjects]
      .sort((left, right) => {
        const leftLocalIndex = projectOverlaysById.get(left.projectId)?.orderIndex;
        const rightLocalIndex = projectOverlaysById.get(right.projectId)?.orderIndex;
        if (leftLocalIndex !== undefined || rightLocalIndex !== undefined) {
          return (leftLocalIndex ?? Number.MAX_SAFE_INTEGER) - (rightLocalIndex ?? Number.MAX_SAFE_INTEGER);
        }
        return (
          left.sortKey.localeCompare(right.sortKey) ||
          right.updatedAt.localeCompare(left.updatedAt) ||
          left.projectId.localeCompare(right.projectId)
        );
      })
      .map((project) => {
        const overlay = projectOverlaysById.get(project.projectId);
        return {
          isChat: overlay?.isChatProject,
          isQuick: overlay?.isQuickProject,
          orderIndex: overlay?.orderIndex ?? Number.MAX_SAFE_INTEGER,
          project,
          projectId: project.projectId,
          sortKey: project.sortKey,
          updatedAt: project.updatedAt,
          worktree: overlay?.worktree,
        };
      }),
  )
    .map((item) => presentationProjectById.get(item.projectId))
    .filter((project): project is GxserverPresentationProject => project !== undefined);
}

export function createGxserverPresentationSidebarSessionKey(
  projectId: string,
  sessionId: string,
): GxserverPresentationSidebarSessionKey {
  return `${projectId}\u0000${sessionId}`;
}

export function createGxserverPresentationProjectGroupId(projectId: string): string {
  return `${GXSERVER_PRESENTATION_PROJECT_GROUP_ID_PREFIX}${encodeGxserverPresentationIdPart(projectId)}`;
}

export function parseGxserverPresentationProjectGroupId(groupId: string): string | undefined {
  if (!groupId.startsWith(GXSERVER_PRESENTATION_PROJECT_GROUP_ID_PREFIX)) {
    return undefined;
  }
  return decodeGxserverPresentationIdPart(groupId.slice(GXSERVER_PRESENTATION_PROJECT_GROUP_ID_PREFIX.length));
}

export function createGxserverPresentationProjectSessionId(projectId: string, sessionId: string): string {
  return [
    GXSERVER_PRESENTATION_PROJECT_SESSION_ID_PREFIX,
    encodeGxserverPresentationIdPart(projectId),
    GXSERVER_PRESENTATION_ID_SEPARATOR,
    encodeGxserverPresentationIdPart(sessionId),
  ].join("");
}

export function parseGxserverPresentationProjectSessionId(
  sessionId: string,
): GxserverPresentationSidebarSessionReference | undefined {
  if (!sessionId.startsWith(GXSERVER_PRESENTATION_PROJECT_SESSION_ID_PREFIX)) {
    return undefined;
  }
  const payload = sessionId.slice(GXSERVER_PRESENTATION_PROJECT_SESSION_ID_PREFIX.length);
  const separatorIndex = payload.indexOf(GXSERVER_PRESENTATION_ID_SEPARATOR);
  if (separatorIndex < 0) {
    return undefined;
  }
  const projectId = decodeGxserverPresentationIdPart(payload.slice(0, separatorIndex));
  const originalSessionId = decodeGxserverPresentationIdPart(payload.slice(separatorIndex + 1));
  if (!projectId || !originalSessionId) {
    return undefined;
  }
  return {
    projectId,
    sessionId: originalSessionId,
  };
}

export function originalGxserverPresentationSidebarSessionId(sessionId: string): string {
  return parseGxserverPresentationProjectSessionId(sessionId)?.sessionId ?? sessionId;
}

export function combineGxserverPresentationSidebarSession(
  projectId: string,
  session: SidebarSessionItem,
  createProjectSessionId = createGxserverPresentationProjectSessionId,
): SidebarSessionItem {
  return {
    ...session,
    sessionId: createProjectSessionId(projectId, originalGxserverPresentationSidebarSessionId(session.sessionId)),
  };
}

export function createIdleGxserverPresentationProjectEditorState(
  projectId: string,
): NonNullable<SidebarSessionGroup["projectContext"]>["editor"] {
  return {
    diffStats: createDefaultSidebarProjectDiffStats(),
    isOpen: false,
    isSleeping: false,
    projectId,
    status: "idle",
  };
}

export function visibleCountForGxserverPresentationSidebarSessions(
  sessions: readonly SidebarSessionItem[],
) {
  return clampVisibleSessionCount(Math.max(1, sessions.filter((session) => session.isVisible).length));
}

function isGxserverPresentationChatProject(
  input: Pick<GxserverPresentationSidebarInput, "chatProjectIds">,
  project: GxserverPresentationProject,
  projectOverlay: GxserverPresentationSidebarProjectOverlay | undefined,
): boolean {
  return projectOverlay?.isQuickProject === true ||
    projectOverlay?.isChatProject === true ||
    input.chatProjectIds?.has(project.projectId) === true;
}

function encodeGxserverPresentationIdPart(value: string): string {
  return encodeURIComponent(value);
}

function decodeGxserverPresentationIdPart(value: string): string | undefined {
  try {
    return decodeURIComponent(value);
  } catch {
    return undefined;
  }
}
