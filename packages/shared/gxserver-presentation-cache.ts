import type {
  GxserverPresentationDelta,
  GxserverPresentationGroup,
  GxserverPresentationProject,
  GxserverPresentationSnapshot,
  GxserverProjectDomainState,
  GxserverProjectId,
  GxserverSessionId,
} from './gxserver-protocol';

/*
CDXC:GxserverPresentationParity 2026-06-24-10:45:
GPUI and macOS sidebar clients must apply gxserver presentation deltas through the same platform-neutral reducer. Keep ordering, membership reconciliation, and project-cache updates in packages/shared/ so cross-platform sidebars do not fork session presentation behavior.
*/
export function reduceGxserverPresentationDelta(
  presentation: GxserverPresentationSnapshot,
  delta: GxserverPresentationDelta,
  revision: number
): GxserverPresentationSnapshot {
  const nextRevision = revision as GxserverPresentationSnapshot['revision'];
  switch (delta.type) {
    case 'sessionAdded':
    case 'sessionUpdated':
    case 'sessionMoved':
    case 'sessionTitleChanged':
    case 'sessionActivityChanged':
    case 'sessionLifecycleChanged':
    case 'sessionSurfaceChanged':
    case 'sessionPresentationChanged': {
      const sessions = orderPresentationSessions(upsertPresentationSession(presentation.sessions, delta.session));
      return {
        ...presentation,
        groups: reconcilePresentationGroupSessionIds(presentation.groups, sessions),
        revision: nextRevision,
        sessions,
      };
    }
    case 'sessionRemoved': {
      const sessions = presentation.sessions.filter(
        (session) => session.projectId !== delta.projectId || session.sessionId !== delta.sessionId
      );
      return {
        ...presentation,
        groups: reconcilePresentationGroupSessionIds(presentation.groups, sessions),
        revision: nextRevision,
        sessions,
      };
    }
    case 'projectAdded':
    case 'projectUpdated':
      return {
        ...presentation,
        groups: upsertPresentationProjectGroup(presentation.groups, delta.project),
        projects: upsertPresentationProject(presentation.projects, delta.project),
        revision: nextRevision,
      };
    case 'projectRemoved':
      return {
        ...presentation,
        groups: presentation.groups.filter((group) => group.projectId !== delta.projectId),
        projects: presentation.projects.filter((project) => project.projectId !== delta.projectId),
        sessions: presentation.sessions.filter((session) => session.projectId !== delta.projectId),
        revision: nextRevision,
      };
    case 'groupAdded':
    case 'groupUpdated':
    case 'groupOrderChanged':
      return {
        ...presentation,
        groups: upsertPresentationGroup(presentation.groups, delta.group),
        revision: nextRevision,
      };
    case 'groupRemoved': {
      const groups = presentation.groups.filter((group) => group.groupId !== delta.groupId);
      return {
        ...presentation,
        groups,
        revision: nextRevision,
        sessions: presentation.sessions.filter(
          (session) => session.projectId !== delta.projectId || session.groupId !== delta.groupId
        ),
      };
    }
    default:
      return { ...presentation, revision: nextRevision };
  }
}

export function reduceGxserverProjectCacheForPresentationDelta(
  projects: readonly GxserverProjectDomainState[],
  delta: GxserverPresentationDelta
): GxserverProjectDomainState[] {
  if ((delta.type === 'projectAdded' || delta.type === 'projectUpdated') && delta.domainProject) {
    return upsertGxserverProjectDomainState(projects, delta.domainProject);
  }
  if (delta.type === 'projectRemoved') {
    return projects.filter((project) => project.projectId !== delta.projectId);
  }
  return [...projects];
}

export function reorderPresentationProjectSessions(
  presentation: GxserverPresentationSnapshot,
  projectId: GxserverProjectId,
  orderedSessionIds: readonly GxserverSessionId[]
): GxserverPresentationSnapshot {
  const sidebarOrderBySessionId = new Map<GxserverSessionId, number>();
  orderedSessionIds.forEach((sessionId, index) => {
    sidebarOrderBySessionId.set(sessionId, (index + 1) * 1000);
  });
  let didChange = false;
  /*
  CDXC:PinnedSessions 2026-06-02-20:11:
  Dragging pinned sessions posts an order against the synthetic project group while the visible sidebar renders from gxserver presentation. Apply the same explicit sidebar order to the local presentation cache first so the row moves immediately and then persists through gxserver.

  CDXC:ManualSessionSorting 2026-06-05-12:30:
  Manual session snapshots use the same local-first path for every project row.
  Start saved rows at 1000 so future new sessions with sidebar order 0 appear
  at the top of the manual list.
  */
  const sessions = presentation.sessions.map((session) => {
    if (session.projectId !== projectId) {
      return session;
    }
    const sidebarOrder = sidebarOrderBySessionId.get(session.sessionId);
    if (sidebarOrder === undefined || session.sidebarOrder === sidebarOrder) {
      return session;
    }
    didChange = true;
    return {
      ...session,
      sidebarOrder,
      sortKey: createPresentationSessionSortKeyWithSidebarOrder(session, sidebarOrder),
    };
  });
  if (!didChange) {
    return presentation;
  }
  const orderedSessions = orderPresentationSessions(sessions);
  return {
    ...presentation,
    groups: reconcilePresentationGroupSessionIds(presentation.groups, orderedSessions),
    sessions: orderedSessions,
  };
}

export function createPresentationProjectFromGxserverProject(
  project: GxserverProjectDomainState
): GxserverPresentationProject {
  const pinRank = project.isPinned ? '0' : project.isFavorite ? '1' : '2';
  return {
    createdAt: project.createdAt,
    groupIds: [`${project.projectId}:active`],
    isFavorite: project.isFavorite,
    isPinned: project.isPinned,
    path: project.path,
    projectId: project.projectId,
    sortKey: `${pinRank}:${project.name.toLocaleLowerCase()}:${project.projectId}`,
    title: project.name,
    updatedAt: project.updatedAt,
    ...(project.worktree ? { worktree: project.worktree } : {}),
  };
}

export function upsertGxserverProjectDomainState(
  projects: readonly GxserverProjectDomainState[],
  nextProject: GxserverProjectDomainState
): GxserverProjectDomainState[] {
  const index = projects.findIndex((project) => project.projectId === nextProject.projectId);
  if (index === -1) {
    return [...projects, nextProject];
  }
  const nextProjects = [...projects];
  nextProjects[index] = nextProject;
  return nextProjects;
}

export function upsertPresentationProject(
  projects: readonly GxserverPresentationProject[],
  nextProject: GxserverPresentationProject
): GxserverPresentationProject[] {
  const index = projects.findIndex((project) => project.projectId === nextProject.projectId);
  if (index === -1) {
    return orderPresentationProjects([...projects, nextProject]);
  }
  const nextProjects = [...projects];
  nextProjects[index] = nextProject;
  return orderPresentationProjects(nextProjects);
}

export function upsertPresentationProjectGroup(
  groups: readonly GxserverPresentationGroup[],
  project: GxserverPresentationProject
): GxserverPresentationGroup[] {
  const groupId = project.groupIds[0] ?? `${project.projectId}:active`;
  const index = groups.findIndex((group) => group.projectId === project.projectId || group.groupId === groupId);
  if (index === -1) {
    return orderPresentationGroups([
      ...groups,
      {
        groupId,
        projectId: project.projectId,
        sessionIds: [],
        sortKey: `${project.sortKey}:active`,
        title: 'Active',
      },
    ]);
  }
  const nextGroups = [...groups];
  nextGroups[index] = {
    ...nextGroups[index],
    groupId,
    projectId: project.projectId,
    sortKey: `${project.sortKey}:active`,
  };
  return orderPresentationGroups(nextGroups);
}

function upsertPresentationSession(
  sessions: readonly GxserverPresentationSnapshot['sessions'][number][],
  nextSession: GxserverPresentationSnapshot['sessions'][number]
): GxserverPresentationSnapshot['sessions'][number][] {
  const index = sessions.findIndex(
    (session) => session.projectId === nextSession.projectId && session.sessionId === nextSession.sessionId
  );
  if (index === -1) {
    return [...sessions, nextSession];
  }
  const nextSessions = [...sessions];
  nextSessions[index] = nextSession;
  return nextSessions;
}

function upsertPresentationGroup(
  groups: readonly GxserverPresentationGroup[],
  nextGroup: GxserverPresentationGroup
): GxserverPresentationGroup[] {
  const index = groups.findIndex((group) => group.groupId === nextGroup.groupId);
  if (index === -1) {
    return orderPresentationGroups([...groups, nextGroup]);
  }
  const nextGroups = [...groups];
  nextGroups[index] = nextGroup;
  return orderPresentationGroups(nextGroups);
}

function reconcilePresentationGroupSessionIds(
  groups: readonly GxserverPresentationGroup[],
  sessions: readonly GxserverPresentationSnapshot['sessions'][number][]
): GxserverPresentationGroup[] {
  const sessionIdsByGroupKey = new Map<string, GxserverSessionId[]>();
  for (const session of orderPresentationSessions(sessions)) {
    const key = presentationGroupKey(session.projectId, session.groupId);
    const sessionIds = sessionIdsByGroupKey.get(key) ?? [];
    sessionIds.push(session.sessionId);
    sessionIdsByGroupKey.set(key, sessionIds);
  }
  return orderPresentationGroups(
    groups.map((group) => ({
      ...group,
      sessionIds: sessionIdsByGroupKey.get(presentationGroupKey(group.projectId, group.groupId)) ?? [],
    }))
  );
}

function presentationGroupKey(projectId: string, groupId: string): string {
  return `${projectId}\u0000${groupId}`;
}

function orderPresentationProjects(projects: readonly GxserverPresentationProject[]): GxserverPresentationProject[] {
  return [...projects].sort(
    (left, right) => left.sortKey.localeCompare(right.sortKey) || left.projectId.localeCompare(right.projectId)
  );
}

function orderPresentationGroups(groups: readonly GxserverPresentationGroup[]): GxserverPresentationGroup[] {
  return [...groups].sort(
    (left, right) => left.sortKey.localeCompare(right.sortKey) || left.groupId.localeCompare(right.groupId)
  );
}

function orderPresentationSessions(
  sessions: readonly GxserverPresentationSnapshot['sessions'][number][]
): GxserverPresentationSnapshot['sessions'][number][] {
  return [...sessions].sort(
    (left, right) =>
      left.projectId.localeCompare(right.projectId) ||
      left.groupId.localeCompare(right.groupId) ||
      left.sortKey.localeCompare(right.sortKey) ||
      left.sessionId.localeCompare(right.sessionId)
  );
}

function createPresentationSessionSortKeyWithSidebarOrder(
  session: GxserverPresentationSnapshot['sessions'][number],
  sidebarOrder: number
): string {
  const activeRank = session.lifecycleState === 'running' || session.lifecycleState === 'sleeping' ? '0' : '1';
  const pinRank = session.isPinned
    ? '0'
    : session.isParked
      ? '3'
      : session.sessionTag === 'favorite' || session.isFavorite
        ? '1'
        : '2';
  const timestamp = session.lastActiveAt ?? session.updatedAt;
  /*
  CDXC:ManualSessionSorting 2026-06-05-12:30:
  Local-first presentation reorders must update the same sidebar-order segment
  for pinned and non-pinned sessions. gxserver owns the durable value, while
  the sidebar cache mirrors it immediately so switching to Manual Sorting or
  dragging rows does not wait for the next presentation delta. Put manual order
  before active/pinned ranks so saved Manual Sorting order can be exact.
  */
  return `${String(sidebarOrder).padStart(12, '0')}:${activeRank}:${pinRank}:${timestamp}:${session.sessionId}`;
}
