import type { SidebarSessionGroup, SidebarSessionItem } from "../../shared/session-grid-contract";
import type {
  GxserverPresentationProject,
  GxserverPresentationSession,
  GxserverPresentationSnapshot,
} from "../../shared/gxserver-protocol";
import {
  createCombinedProjectGroupId,
  createCombinedProjectSessionId,
  parseCombinedProjectSessionId,
} from "./combined-sidebar-mode";
import {
  GXSERVER_PRESENTATION_CHATS_GROUP_ID,
  createGxserverPresentationSidebarGroup,
  createGxserverPresentationSidebarSession,
  createGxserverPresentationSidebarSessionKey,
  createGxserverPresentationSessionsByProjectFromGroups,
  orderGxserverPresentationSidebarProjects,
  presentationLifecycleStateForSidebar,
  providerSessionStateForGxserverPresentation,
  visibleCountForGxserverPresentationSidebarSessions,
  type GxserverPresentationCloseAfterDoneProjection,
  type GxserverPresentationDelayedSendProjection,
  type GxserverPresentationSidebarProjectOverlay,
  type GxserverPresentationSidebarSessionKey,
} from "../../shared/gxserver-presentation-sidebar-projection";

export const NATIVE_PRESENTATION_CHATS_GROUP_ID = GXSERVER_PRESENTATION_CHATS_GROUP_ID;

export type NativePresentationProjectionSessionKey = GxserverPresentationSidebarSessionKey;

export type NativePresentationDelayedSendProjection = GxserverPresentationDelayedSendProjection;

export type NativePresentationCloseAfterDoneProjection = GxserverPresentationCloseAfterDoneProjection;

export type NativePresentationProjectProjection = GxserverPresentationSidebarProjectOverlay & {
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
  resolveCloseAfterDone: (
    projectId: string,
    sessionId: string,
  ) => NativePresentationCloseAfterDoneProjection | undefined;
  resolveSessionRoutingId: (projectId: string, sessionId: string) => string | undefined;
  visibleSessionIds?: ReadonlySet<string>;
};

export { presentationLifecycleStateForSidebar, providerSessionStateForGxserverPresentation };

export function createNativePresentationProjectionSessionKey(
  projectId: string,
  sessionId: string,
): NativePresentationProjectionSessionKey {
  return createGxserverPresentationSidebarSessionKey(projectId, sessionId);
}

export function createNativePresentationSidebarGroups(
  input: NativePresentationProjectionInput,
): SidebarSessionGroup[] {
  /*
  CDXC:NativePresentationProjection 2026-06-13-00:49:
  Native gxserver presentation must be a pure value projection from gxserver rows plus macOS-local pane facts. Keep hidden overlays, local-only pane rows, Quick/Chats classification, and routing callbacks in the input so this module cannot mutate sidebar state, pane chrome, or publish state.

  CDXC:GxserverPresentationParity 2026-06-24-10:45:
  Shared gxserver projection owns presentation row mapping for macOS and GPUI. This wrapper keeps macOS-only overlays here: local browser pane rows, Quick/Chats carriers, remote-attach carrier suppression, delayed-send timers, and Close After Done countdowns.
  */
  const localProjectsById = new Map(input.localProjects.map((project) => [project.projectId, project]));
  const sessionsByProject = createGxserverPresentationSessionsByProjectFromGroups(input);
  const visibleProjects = input.presentation.projects.filter(
    (project) => !input.hiddenProjectIds?.has(project.projectId) &&
      localProjectsById.get(project.projectId)?.isRemoteAttachCarrier !== true &&
      !input.remoteAttachCarrierProjectIds?.has(project.projectId),
  );
  const chatProjects = orderGxserverPresentationSidebarProjects(
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
  const projectGroups = orderGxserverPresentationSidebarProjects(
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
      layoutVisibleCount: visibleCountForGxserverPresentationSidebarSessions(chatSessions),
      sessions: chatSessions,
      title: "Chats",
      viewMode: "grid",
      visibleCount: visibleCountForGxserverPresentationSidebarSessions(chatSessions),
    },
    ...projectGroups,
  ];
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

  CDXC:GxserverPresentationQuick 2026-06-16-01:05:
  Quick browser and local-only pane rows render inside one synthetic Quick group, so their click IDs must include the owning project. Keep local-only Quick rows combined the same way presentation rows and normal project web panes are combined; otherwise native focus resolves bare browser IDs through the currently active project and clicks on other Quick rows become no-ops.
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
      const localOnlySessions = project.localSidebarSessions
        .filter(
          (session) =>
            !presentationSessionIds.has(originalSidebarSessionId(session.sessionId)) &&
            !input.hiddenSessionKeys?.has(
              createNativePresentationProjectionSessionKey(project.projectId, originalSidebarSessionId(session.sessionId)),
            ),
        )
        .map((session) => combineLocalSidebarSession(project.projectId, session));
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

  Browser panes are macOS-local WKWebView sessions even when gxserver owns terminal presentation. Merge only those native pane cards into normal project groups with project-scoped ids so native tabs and the React sidebar stay aligned while stale pre-cutover terminal rows stay suppressed.
  */
  const presentationSessionIds = new Set<string>(sessions.map((session) => session.sessionId));
  const localRows = localProject?.localSidebarSessions ?? [];
  const localPaneSessions = localRows
    .filter(
      (session) =>
        session.sessionKind === "browser" &&
        !presentationSessionIds.has(originalSidebarSessionId(session.sessionId)) &&
        !input.hiddenSessionKeys?.has(
          createNativePresentationProjectionSessionKey(project.projectId, originalSidebarSessionId(session.sessionId)),
        ),
    )
    .map((session) => combineLocalSidebarSession(project.projectId, session));
  return createGxserverPresentationSidebarGroup({
    activeProjectId: input.activeProjectId,
    createProjectGroupId: createCombinedProjectGroupId,
    createProjectSessionId: createCombinedProjectSessionId,
    extraSessions: localPaneSessions,
    focusedSessionId: input.focusedSessionId,
    project,
    projectOverlay: localProject,
    resolveAgentIcon: input.resolveAgentIcon,
    resolveCloseAfterDone: input.resolveCloseAfterDone,
    resolveDelayedSend: input.resolveDelayedSend,
    resolveLocalSession: (_projectId, sessionId) => findLocalSidebarSession(localRows, sessionId),
    resolveProviderSessionState: (presentation, localSession) =>
      providerSessionStateForPresentationLocalPane(presentation, localSession?.nativePaneState),
    resolveSessionRoutingId: input.resolveSessionRoutingId,
    sessions,
    visibleSessionIds: input.visibleSessionIds,
  });
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
  /*
  CDXC:GxserverPresentationIdentity 2026-06-13-00:49:
  Presentation-backed rows receive captured provider session identity from gxserver. Prefer that server-owned identity so hover tooltips and resume actions show the Codex/Claude session id even when no local terminal row exists.

  CDXC:DelayedSend 2026-06-13-00:49:
  Delayed Send timers remain native window state keyed by project/session. Join that timer projection onto the presentation-backed row so the leading clock keeps precedence over tags and agent icons.

  CDXC:CloseAfterDone 2026-06-15-21:00:
  Close After Done is also native-window timer state keyed by project/session.
  Join it here so presentation-backed rows show the pastel red clock before
  and during the three-minute Done close countdown.
  */
  return createGxserverPresentationSidebarSession({
    createProjectSessionId: createCombinedProjectSessionId,
    focusedSessionId: input.focusedSessionId,
    index,
    isActiveProject,
    localSession,
    presentation,
    projectId,
    resolveAgentIcon: input.resolveAgentIcon,
    resolveCloseAfterDone: input.resolveCloseAfterDone,
    resolveDelayedSend: input.resolveDelayedSend,
    resolveProviderSessionState: (presentation, localSession) =>
      providerSessionStateForPresentationLocalPane(presentation, localSession?.nativePaneState),
    resolveSessionRoutingId: input.resolveSessionRoutingId,
    visibleSessionIds: input.visibleSessionIds,
  });
}

function providerSessionStateForPresentationLocalPane(
  presentation: Pick<GxserverPresentationSession, "lifecycleState" | "providerSessionState">,
  nativePaneState: SidebarSessionItem["nativePaneState"] | undefined,
): NonNullable<SidebarSessionItem["providerSessionState"]> {
  const providerSessionState = providerSessionStateForGxserverPresentation(presentation);
  /*
  CDXC:PaneTabs 2026-06-15-18:13:
  A locally mounted or mounting terminal pane is live in this macOS window, so its native tab must not render the zmx-missing moon from a stale gxserver presentation probe. Keep the missing marker only for rows with no live local pane.
  */
  if (
    providerSessionState === "missing" &&
    (nativePaneState === "mounted" || nativePaneState === "mounting")
  ) {
    return "exists";
  }
  return providerSessionState;
}

function combineLocalSidebarSession(
  projectId: string,
  session: SidebarSessionItem,
): SidebarSessionItem {
  return {
    ...session,
    sessionId: createCombinedProjectSessionId(projectId, originalSidebarSessionId(session.sessionId)),
  };
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
