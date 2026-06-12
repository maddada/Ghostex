import {
  GRID_COLUMN_COUNT,
  clampVisibleSessionCount,
  type SidebarSessionGroup,
  type SidebarSessionItem,
} from "../../shared/session-grid-contract";
import type {
  GxserverDomainLifecycleState,
  GxserverPresentationProject,
  GxserverPresentationSession,
  GxserverPresentationSnapshot,
} from "../../shared/gxserver-protocol";
import { orderProjectsWithWorktrees } from "../../shared/project-worktree-order";
import {
  createCombinedProjectGroupId,
  createCombinedProjectSessionId,
  parseCombinedProjectSessionId,
} from "./combined-sidebar-mode";
import { createDefaultSidebarProjectDiffStats } from "../../shared/project-diff-stats";

export const NATIVE_PRESENTATION_CHATS_GROUP_ID = "combined-chats";

export type NativePresentationProjectionSessionKey = string;

export type NativePresentationDelayedSendProjection = {
  deadlineAt?: string;
  remainingLabel?: string;
  remainingMs?: number;
};

export type NativePresentationProjectProjection = {
  editor?: NonNullable<SidebarSessionGroup["projectContext"]>["editor"];
  isChatProject?: boolean;
  isQuickProject?: boolean;
  isRecentProject?: boolean;
  isRemoteAttachCarrier?: boolean;
  localSidebarSessions: readonly SidebarSessionItem[];
  orderIndex?: number;
  path: string;
  projectId: string;
  theme?: NonNullable<SidebarSessionGroup["projectContext"]>["theme"];
  themeColor?: string;
  title: string;
  worktree?: NonNullable<SidebarSessionGroup["projectContext"]>["worktree"];
};

export type NativePresentationProjectionInput = {
  activeProjectId?: string;
  chatProjectIds?: ReadonlySet<string>;
  focusedSessionId?: string;
  hiddenProjectIds?: ReadonlySet<string>;
  hiddenSessionKeys?: ReadonlySet<NativePresentationProjectionSessionKey>;
  localProjects: readonly NativePresentationProjectProjection[];
  presentation: GxserverPresentationSnapshot;
  remoteAttachCarrierProjectIds?: ReadonlySet<string>;
  resolveAgentIcon: (agentName: string | undefined) => SidebarSessionItem["agentIcon"];
  resolveDelayedSend: (
    projectId: string,
    sessionId: string,
  ) => NativePresentationDelayedSendProjection | undefined;
  resolveSessionRoutingId: (projectId: string, sessionId: string) => string | undefined;
  visibleSessionIds?: ReadonlySet<string>;
};

type OrderedPresentationProject = {
  isChat?: boolean;
  isQuick?: boolean;
  orderIndex: number;
  project: GxserverPresentationProject;
  projectId: string;
  sortKey: string;
  updatedAt?: string;
  worktree?: NonNullable<SidebarSessionGroup["projectContext"]>["worktree"];
};

export function createNativePresentationProjectionSessionKey(
  projectId: string,
  sessionId: string,
): NativePresentationProjectionSessionKey {
  return `${projectId}\u0000${sessionId}`;
}

export function createNativePresentationSidebarGroups(
  input: NativePresentationProjectionInput,
): SidebarSessionGroup[] {
  /*
  CDXC:NativePresentationProjection 2026-06-13-00:49:
  Native gxserver presentation must be a pure value projection from gxserver rows plus macOS-local pane facts. Keep hidden overlays, local-only pane rows, Quick/Chats classification, and routing callbacks in the input so this module cannot mutate sidebar state, pane chrome, or publish state.
  */
  const localProjectsById = new Map(input.localProjects.map((project) => [project.projectId, project]));
  const sessionsByProject = createPresentationSessionsByProjectFromGroups(input);
  const visibleProjects = input.presentation.projects.filter(
    (project) => !input.hiddenProjectIds?.has(project.projectId) &&
      localProjectsById.get(project.projectId)?.isRemoteAttachCarrier !== true &&
      !input.remoteAttachCarrierProjectIds?.has(project.projectId),
  );
  const chatProjects = orderPresentationProjects(
    visibleProjects.filter((project) =>
      isPresentationChatProject(input, project, localProjectsById.get(project.projectId)),
    ),
    localProjectsById,
  );
  const chatSessions = createPresentationQuickSidebarSessions({
    chatProjects,
    input,
    localProjectsById,
    sessionsByProject,
  });
  const projectGroups = orderPresentationProjects(
    visibleProjects.filter((project) =>
      !isPresentationChatProject(input, project, localProjectsById.get(project.projectId)),
    ),
    localProjectsById,
  ).flatMap((project) => {
    const localProject = localProjectsById.get(project.projectId);
    if (localProject?.isRecentProject === true || localProject?.isRemoteAttachCarrier === true) {
      return [];
    }
    return [
      createPresentationProjectSidebarGroup({
        input,
        localProject,
        project,
        sessions: sessionsByProject.get(project.projectId) ?? [],
      }),
    ];
  });

  return [
    {
      groupId: NATIVE_PRESENTATION_CHATS_GROUP_ID,
      isActive:
        chatProjects.some((project) => project.projectId === input.activeProjectId) ||
        input.localProjects.some(
          (project) =>
            project.projectId === input.activeProjectId &&
            project.isRecentProject !== true &&
            project.isRemoteAttachCarrier !== true &&
            (project.isQuickProject === true || project.isChatProject === true),
        ),
      isChatCollection: true,
      isFocusModeActive: false,
      kind: "workspace",
      layoutVisibleCount: visibleCountForSessions(chatSessions),
      sessions: chatSessions,
      title: "Chats",
      viewMode: "grid",
      visibleCount: visibleCountForSessions(chatSessions),
    },
    ...projectGroups,
  ];
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
  lifecycleState: GxserverDomainLifecycleState,
): NonNullable<SidebarSessionItem["providerSessionState"]> {
  /*
  CDXC:PaneTabs 2026-06-13-00:49:
  Native tab moons follow zmx provider liveness, not mounted renderer state. gxserver sleep/stop transitions remove the named provider session, while unknown must avoid claiming the provider is inactive.
  */
  switch (lifecycleState) {
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

function createPresentationSessionsByProjectFromGroups(
  input: NativePresentationProjectionInput,
): Map<string, GxserverPresentationSession[]> {
  const sessionByProjectSessionKey = new Map(
    input.presentation.sessions.map((session) => [
      createNativePresentationProjectionSessionKey(session.projectId, session.sessionId),
      session,
    ]),
  );
  const sessionsByProject = new Map<string, GxserverPresentationSession[]>();
  for (const group of input.presentation.groups) {
    if (input.hiddenProjectIds?.has(group.projectId)) {
      continue;
    }
    const sessions = sessionsByProject.get(group.projectId) ?? [];
    for (const sessionId of group.sessionIds) {
      const session = sessionByProjectSessionKey.get(
        createNativePresentationProjectionSessionKey(group.projectId, sessionId),
      );
      if (
        !session ||
        session.visibleInSidebarByDefault !== true ||
        session.surface === "commands" ||
        input.hiddenSessionKeys?.has(
          createNativePresentationProjectionSessionKey(session.projectId, session.sessionId),
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

function createPresentationQuickSidebarSessions({
  chatProjects,
  input,
  localProjectsById,
  sessionsByProject,
}: {
  chatProjects: readonly GxserverPresentationProject[];
  input: NativePresentationProjectionInput;
  localProjectsById: ReadonlyMap<string, NativePresentationProjectProjection>;
  sessionsByProject: ReadonlyMap<string, readonly GxserverPresentationSession[]>;
}): SidebarSessionItem[] {
  /*
  CDXC:GxserverPresentationQuick 2026-06-13-00:49:
  Quick browser/file rows are still local macOS panes, not gxserver sessions. Merge those local Quick rows into the synthetic Chats group while preferring gxserver projection for terminal rows that already have presentation entries.

  CDXC:RemoteAttach 2026-06-13-00:49:
  Remote attach terminals use local Quick projects only as native carriers. Honor local hidden-session overlays during the Quick local-only merge so suppressed gxserver rows do not reappear as duplicate Quick cards.
  */
  const chatProjectsById = new Map<string, GxserverPresentationProject>(
    chatProjects.map((project) => [project.projectId, project]),
  );
  const localQuickProjects = orderLocalProjects(
    input.localProjects.filter(
      (project) =>
        project.isRecentProject !== true &&
        (project.isQuickProject === true || project.isChatProject === true),
    ),
  );
  const localQuickProjectIds = new Set(localQuickProjects.map((project) => project.projectId));
  const presentationOnlyChatProjects = chatProjects.filter((project) => !localQuickProjectIds.has(project.projectId));

  return [
    ...localQuickProjects.flatMap((project) => {
      const presentationProject = chatProjectsById.get(project.projectId);
      const presentationSessionIds = new Set<string>(
        (sessionsByProject.get(project.projectId) ?? []).map((session) => session.sessionId),
      );
      const isActiveProject = project.projectId === input.activeProjectId;
      const presentationSessions = (sessionsByProject.get(project.projectId) ?? []).map((session, index) =>
        createPresentationSidebarSession({
          index,
          input,
          isActiveProject,
          localSession: findLocalSidebarSession(project.localSidebarSessions, session.sessionId),
          presentation: session,
          projectId: project.projectId,
        }),
      );
      const localOnlySessions = project.localSidebarSessions.filter(
        (session) =>
          !presentationSessionIds.has(originalSidebarSessionId(session.sessionId)) &&
          !input.hiddenSessionKeys?.has(
            createNativePresentationProjectionSessionKey(project.projectId, originalSidebarSessionId(session.sessionId)),
          ),
      );
      return presentationProject || localOnlySessions.length > 0
        ? [...presentationSessions, ...localOnlySessions]
        : localOnlySessions;
    }),
    ...presentationOnlyChatProjects.flatMap((project) => {
      const localProject = localProjectsById.get(project.projectId);
      const isActiveProject = project.projectId === input.activeProjectId;
      return (sessionsByProject.get(project.projectId) ?? []).map((session, index) =>
        createPresentationSidebarSession({
          index,
          input,
          isActiveProject,
          localSession: localProject
            ? findLocalSidebarSession(localProject.localSidebarSessions, session.sessionId)
            : undefined,
          presentation: session,
          projectId: project.projectId,
        }),
      );
    }),
  ];
}

function createPresentationProjectSidebarGroup({
  input,
  localProject,
  project,
  sessions,
}: {
  input: NativePresentationProjectionInput;
  localProject: NativePresentationProjectProjection | undefined;
  project: GxserverPresentationProject;
  sessions: readonly GxserverPresentationSession[];
}): SidebarSessionGroup {
  /*
  CDXC:GxserverPresentationProjects 2026-06-13-00:49:
  Project rows are not session rows. A visible gxserver project must stay in the Projects section even when it has no workspace sessions yet.

  CDXC:T3Code 2026-06-13-00:49:
  T3 Code and browser panes are macOS-local WKWebView sessions even when gxserver owns terminal presentation. Merge only those native pane cards into normal project groups with project-scoped ids so native tabs and the React sidebar stay aligned while stale pre-cutover terminal rows stay suppressed.
  */
  const isActiveProject = project.projectId === input.activeProjectId;
  const presentationSessionIds = new Set<string>(sessions.map((session) => session.sessionId));
  const localRows = localProject?.localSidebarSessions ?? [];
  const presentationSidebarSessions = sessions.map((session, index) =>
    createPresentationSidebarSession({
      index,
      input,
      isActiveProject,
      localSession: findLocalSidebarSession(localRows, session.sessionId),
      presentation: session,
      projectId: project.projectId,
    }),
  );
  const localPaneSessions = localRows
    .filter(
      (session) =>
        (session.sessionKind === "t3" || session.sessionKind === "browser") &&
        !presentationSessionIds.has(originalSidebarSessionId(session.sessionId)) &&
        !input.hiddenSessionKeys?.has(
          createNativePresentationProjectionSessionKey(project.projectId, originalSidebarSessionId(session.sessionId)),
        ),
    )
    .map((session) => ({
      ...session,
      sessionId: createCombinedProjectSessionId(project.projectId, originalSidebarSessionId(session.sessionId)),
    }));
  const sidebarSessions = [...presentationSidebarSessions, ...localPaneSessions];
  const projectContext = localProject
    ? {
        canRemoveProject: true,
        editor: localProject.editor ?? createIdlePresentationProjectEditorState(project.projectId),
        path: localProject.path || project.path || "",
        theme: localProject.theme,
        themeColor: localProject.themeColor,
        worktree: localProject.worktree,
      }
    : {
        canRemoveProject: true,
        editor: createIdlePresentationProjectEditorState(project.projectId),
        path: project.path ?? "",
      };
  return {
    groupId: createCombinedProjectGroupId(project.projectId),
    canFocusMode: false,
    isActive: isActiveProject,
    isFocusModeActive: false,
    kind: "workspace",
    layoutVisibleCount: visibleCountForSessions(sidebarSessions),
    projectContext,
    sessions: sidebarSessions,
    title: project.title,
    viewMode: "grid",
    visibleCount: visibleCountForSessions(sidebarSessions),
  };
}

function createPresentationSidebarSession({
  index,
  input,
  isActiveProject,
  localSession,
  presentation,
  projectId,
}: {
  index: number;
  input: NativePresentationProjectionInput;
  isActiveProject: boolean;
  localSession: SidebarSessionItem | undefined;
  presentation: GxserverPresentationSession;
  projectId: string;
}): SidebarSessionItem {
  const lifecycleState = presentationLifecycleStateForSidebar(presentation.lifecycleState);
  const providerSessionState = providerSessionStateForGxserverPresentation(presentation.lifecycleState);
  const isLive = presentation.lifecycleState === "running";
  const delayedSend = input.resolveDelayedSend(projectId, presentation.sessionId);
  return {
    activity: presentation.activity,
    agentIcon: input.resolveAgentIcon(presentation.agentIcon ?? presentation.agentName ?? presentation.agentId),
    /*
    CDXC:GxserverPresentationIdentity 2026-06-13-00:49:
    Presentation-backed rows receive captured provider session identity from gxserver. Prefer that server-owned identity so hover tooltips and resume actions show the Codex/Claude session id even when no local terminal row exists.

    CDXC:DelayedSend 2026-06-13-00:49:
    Delayed Send timers remain native window state keyed by project/session. Join that timer projection onto the presentation-backed row so the leading clock keeps precedence over tags and agent icons.
    */
    agentSessionId: presentation.agentSessionId ?? localSession?.agentSessionId,
    alias: presentation.title,
    column: index % GRID_COLUMN_COUNT,
    detail: presentation.subtitle,
    delayedSendDeadlineAt: delayedSend?.deadlineAt,
    delayedSendRemainingLabel: delayedSend?.remainingLabel,
    delayedSendRemainingMs: delayedSend?.remainingMs,
    displayTitle: presentation.displayTitle,
    displayTitleTooltip: presentation.displayTitleTooltip,
    isFavorite: presentation.isFavorite,
    isFocused: isActiveProject && input.focusedSessionId === presentation.sessionId,
    isGeneratingFirstPromptTitle: presentation.isGeneratingFirstPromptTitle,
    isLive,
    isPinned: presentation.isPinned,
    isPrimaryTitleTerminalTitle: presentation.isPrimaryTitleTerminalTitle,
    isRunning: isLive,
    isSleeping: lifecycleState === "sleeping",
    isVisible: isActiveProject && (
      input.visibleSessionIds?.has(presentation.sessionId) === true ||
      index === 0
    ),
    lastInteractionAt: presentation.lastActiveAt ?? presentation.updatedAt,
    lifecycleState,
    nativePaneState: localSession?.nativePaneState,
    primaryTitle: presentation.primaryTitle ?? presentation.title,
    providerSessionState,
    row: Math.floor(index / GRID_COLUMN_COUNT),
    sessionId: createCombinedProjectSessionId(projectId, presentation.sessionId),
    sessionKind: presentation.kind === "agent" ? "terminal" : presentation.kind,
    sessionTag: presentation.sessionTag,
    sessionNumber: String(index + 1),
    sessionPersistenceName: presentation.zmxName,
    sessionPersistenceProvider: "zmx",
    sessionRoutingId: input.resolveSessionRoutingId(projectId, presentation.sessionId),
    shortcutLabel: String(index + 1),
    terminalTitle: presentation.terminalTitle,
    titleObservation: presentation.titleObservation,
  };
}

function createIdlePresentationProjectEditorState(
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

function orderPresentationProjects(
  presentationProjects: readonly GxserverPresentationProject[],
  localProjectsById: ReadonlyMap<string, NativePresentationProjectProjection>,
): GxserverPresentationProject[] {
  const presentationProjectById = new Map(
    presentationProjects.map((project) => [project.projectId, project]),
  );
  return orderProjectsWithWorktrees(
    [...presentationProjects]
      .sort((left, right) => {
        const leftLocalIndex = localProjectsById.get(left.projectId)?.orderIndex;
        const rightLocalIndex = localProjectsById.get(right.projectId)?.orderIndex;
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
        const localProject = localProjectsById.get(project.projectId);
        return {
          isChat: localProject?.isChatProject,
          isQuick: localProject?.isQuickProject,
          orderIndex: localProject?.orderIndex ?? Number.MAX_SAFE_INTEGER,
          project,
          projectId: project.projectId,
          sortKey: project.sortKey,
          updatedAt: project.updatedAt,
          worktree: localProject?.worktree,
        };
      }),
  )
    .map((item) => presentationProjectById.get(item.projectId))
    .filter((project): project is GxserverPresentationProject => project !== undefined);
}

function orderLocalProjects(
  projects: readonly NativePresentationProjectProjection[],
): NativePresentationProjectProjection[] {
  return [...projects].sort((left, right) =>
    (left.orderIndex ?? Number.MAX_SAFE_INTEGER) - (right.orderIndex ?? Number.MAX_SAFE_INTEGER) ||
    left.title.localeCompare(right.title) ||
    left.projectId.localeCompare(right.projectId),
  );
}

function isPresentationChatProject(
  input: NativePresentationProjectionInput,
  project: GxserverPresentationProject,
  localProject: NativePresentationProjectProjection | undefined,
): boolean {
  return localProject?.isQuickProject === true ||
    localProject?.isChatProject === true ||
    input.chatProjectIds?.has(project.projectId) === true;
}

function findLocalSidebarSession(
  sessions: readonly SidebarSessionItem[],
  sessionId: string,
): SidebarSessionItem | undefined {
  return sessions.find((session) => originalSidebarSessionId(session.sessionId) === sessionId);
}

function originalSidebarSessionId(sessionId: string): string {
  return parseCombinedProjectSessionId(sessionId)?.sessionId ?? sessionId;
}

function visibleCountForSessions(sessions: readonly SidebarSessionItem[]) {
  return clampVisibleSessionCount(Math.max(1, sessions.filter((session) => session.isVisible).length));
}
