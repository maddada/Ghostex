/*
 * CDXC:GPUIWorkspaceGroups 2026-07-02-03:49:
 * GPUI needs the sidebar's named session-group controls before gxserver has durable group storage.
 * Persist a per-project client overlay so users can create, rename, move, reorder, and prune groups without changing server session records.
 *
 * GPUI-owned client-side named session groups. gxserver has no group storage
 * (matching the macOS app, which keeps its grouped workspace state in
 * localStorage), so GPUI persists a per-project overlay that assigns
 * presentation sessions to user-defined sub-groups and remembers project
 * order. Semantics originally mirrored the equivalent shared grouped
 * session-workspace-state module, deleted in the 2026-08-22 restructure once
 * its last production importers were gone: group ids are minted as
 * `group-${n}`, titles default to `Group ${n}`, and a project holds at most
 * MAX_GROUP_COUNT groups including its implicit main (project) group.
 */

export const GPUI_WORKSPACE_SESSION_GROUPS_STORAGE_KEY =
  "ghostex-gpui-workspace-session-groups";
export const GPUI_WORKSPACE_SESSION_GROUP_MAX_COUNT = 20;

const GPUI_WORKSPACE_SESSION_SUBGROUP_ID_PREFIX = "gpui-wsg:";

export type GpuiWorkspaceSessionSubgroup = {
  groupId: string;
  sessionIds: string[];
  title: string;
};

export type GpuiProjectWorkspaceGroups = {
  groups: GpuiWorkspaceSessionSubgroup[];
  nextGroupNumber: number;
};

export type GpuiWorkspaceSessionGroupsState = {
  projectOrder: string[];
  projects: Record<string, GpuiProjectWorkspaceGroups>;
};

export function createEmptyGpuiWorkspaceSessionGroupsState(): GpuiWorkspaceSessionGroupsState {
  return { projectOrder: [], projects: {} };
}

export function createGpuiWorkspaceSessionSubgroupId(
  projectId: string,
  groupId: string,
): string {
  return `${GPUI_WORKSPACE_SESSION_SUBGROUP_ID_PREFIX}${encodeURIComponent(projectId)}:${groupId}`;
}

export function parseGpuiWorkspaceSessionSubgroupId(
  value: string,
): { groupId: string; projectId: string } | undefined {
  if (!value.startsWith(GPUI_WORKSPACE_SESSION_SUBGROUP_ID_PREFIX)) {
    return undefined;
  }
  const rest = value.slice(GPUI_WORKSPACE_SESSION_SUBGROUP_ID_PREFIX.length);
  const separator = rest.indexOf(":");
  if (separator <= 0 || separator === rest.length - 1) {
    return undefined;
  }
  try {
    return {
      groupId: rest.slice(separator + 1),
      projectId: decodeURIComponent(rest.slice(0, separator)),
    };
  } catch {
    return undefined;
  }
}

function projectGroups(
  state: GpuiWorkspaceSessionGroupsState,
  projectId: string,
): GpuiProjectWorkspaceGroups {
  return state.projects[projectId] ?? { groups: [], nextGroupNumber: 2 };
}

function withProjectGroups(
  state: GpuiWorkspaceSessionGroupsState,
  projectId: string,
  next: GpuiProjectWorkspaceGroups,
): GpuiWorkspaceSessionGroupsState {
  const projects = { ...state.projects };
  if (next.groups.length === 0 && next.nextGroupNumber === 2) {
    delete projects[projectId];
  } else {
    projects[projectId] = next;
  }
  return { ...state, projects };
}

export function getGpuiWorkspaceSessionSubgroups(
  state: GpuiWorkspaceSessionGroupsState,
  projectId: string,
): readonly GpuiWorkspaceSessionSubgroup[] {
  return state.projects[projectId]?.groups ?? [];
}

export function findGpuiWorkspaceSessionSubgroupForSession(
  state: GpuiWorkspaceSessionGroupsState,
  projectId: string,
  sessionId: string,
): GpuiWorkspaceSessionSubgroup | undefined {
  return projectGroups(state, projectId).groups.find((group) =>
    group.sessionIds.includes(sessionId),
  );
}

export function createGpuiWorkspaceSessionSubgroup(
  state: GpuiWorkspaceSessionGroupsState,
  projectId: string,
  initialSessionId?: string,
): { groupId?: string; state: GpuiWorkspaceSessionGroupsState } {
  const current = projectGroups(state, projectId);
  if (current.groups.length + 1 >= GPUI_WORKSPACE_SESSION_GROUP_MAX_COUNT) {
    return { state };
  }
  const groupNumber = current.nextGroupNumber;
  const groupId = `group-${groupNumber}`;
  const withoutSession = initialSessionId
    ? removeSessionFromSubgroups(current, initialSessionId)
    : current;
  const next: GpuiProjectWorkspaceGroups = {
    groups: [
      ...withoutSession.groups,
      {
        groupId,
        sessionIds: initialSessionId ? [initialSessionId] : [],
        title: `Group ${groupNumber}`,
      },
    ],
    nextGroupNumber: groupNumber + 1,
  };
  return { groupId, state: withProjectGroups(state, projectId, next) };
}

export function renameGpuiWorkspaceSessionSubgroup(
  state: GpuiWorkspaceSessionGroupsState,
  projectId: string,
  groupId: string,
  title: string,
): GpuiWorkspaceSessionGroupsState {
  const trimmed = title.trim();
  if (!trimmed) {
    return state;
  }
  const current = projectGroups(state, projectId);
  if (!current.groups.some((group) => group.groupId === groupId)) {
    return state;
  }
  const next: GpuiProjectWorkspaceGroups = {
    ...current,
    groups: current.groups.map((group) =>
      group.groupId === groupId && group.title !== trimmed
        ? { ...group, title: trimmed }
        : group,
    ),
  };
  return withProjectGroups(state, projectId, next);
}

export function removeGpuiWorkspaceSessionSubgroup(
  state: GpuiWorkspaceSessionGroupsState,
  projectId: string,
  groupId: string,
): GpuiWorkspaceSessionGroupsState {
  const current = projectGroups(state, projectId);
  if (!current.groups.some((group) => group.groupId === groupId)) {
    return state;
  }
  const next: GpuiProjectWorkspaceGroups = {
    ...current,
    groups: current.groups.filter((group) => group.groupId !== groupId),
  };
  return withProjectGroups(state, projectId, next);
}

export function moveGpuiWorkspaceSessionToSubgroup(
  state: GpuiWorkspaceSessionGroupsState,
  projectId: string,
  sessionId: string,
  targetGroupId: string | undefined,
  targetIndex?: number,
): GpuiWorkspaceSessionGroupsState {
  const current = projectGroups(state, projectId);
  const withoutSession = removeSessionFromSubgroups(current, sessionId);
  if (!targetGroupId) {
    return withProjectGroups(state, projectId, withoutSession);
  }
  if (!withoutSession.groups.some((group) => group.groupId === targetGroupId)) {
    return state;
  }
  const next: GpuiProjectWorkspaceGroups = {
    ...withoutSession,
    groups: withoutSession.groups.map((group) => {
      if (group.groupId !== targetGroupId) {
        return group;
      }
      const sessionIds = [...group.sessionIds];
      const clampedIndex =
        targetIndex === undefined
          ? sessionIds.length
          : Math.max(0, Math.min(targetIndex, sessionIds.length));
      sessionIds.splice(clampedIndex, 0, sessionId);
      return { ...group, sessionIds };
    }),
  };
  return withProjectGroups(state, projectId, next);
}

export function syncGpuiWorkspaceSessionSubgroupOrder(
  state: GpuiWorkspaceSessionGroupsState,
  projectId: string,
  orderedGroupIds: readonly string[],
): GpuiWorkspaceSessionGroupsState {
  const current = projectGroups(state, projectId);
  if (current.groups.length === 0) {
    return state;
  }
  const byId = new Map(current.groups.map((group) => [group.groupId, group]));
  const ordered = orderedGroupIds
    .map((groupId) => byId.get(groupId))
    .filter((group): group is GpuiWorkspaceSessionSubgroup => group !== undefined);
  const orderedIds = new Set(ordered.map((group) => group.groupId));
  const remaining = current.groups.filter((group) => !orderedIds.has(group.groupId));
  const next: GpuiProjectWorkspaceGroups = {
    ...current,
    groups: [...ordered, ...remaining],
  };
  return withProjectGroups(state, projectId, next);
}

export function syncGpuiWorkspaceSessionOrderInSubgroup(
  state: GpuiWorkspaceSessionGroupsState,
  projectId: string,
  groupId: string,
  sessionIds: readonly string[],
): GpuiWorkspaceSessionGroupsState {
  const current = projectGroups(state, projectId);
  const group = current.groups.find((candidate) => candidate.groupId === groupId);
  if (!group) {
    return state;
  }
  const memberIds = new Set(group.sessionIds);
  const ordered = sessionIds.filter((sessionId) => memberIds.has(sessionId));
  const orderedSet = new Set(ordered);
  const remaining = group.sessionIds.filter((sessionId) => !orderedSet.has(sessionId));
  const next: GpuiProjectWorkspaceGroups = {
    ...current,
    groups: current.groups.map((candidate) =>
      candidate.groupId === groupId
        ? { ...candidate, sessionIds: [...ordered, ...remaining] }
        : candidate,
    ),
  };
  return withProjectGroups(state, projectId, next);
}

export function pruneGpuiWorkspaceSessionSubgroups(
  state: GpuiWorkspaceSessionGroupsState,
  projectId: string,
  existingSessionIds: ReadonlySet<string>,
): GpuiWorkspaceSessionGroupsState {
  const current = state.projects[projectId];
  if (!current) {
    return state;
  }
  let changed = false;
  const groups = current.groups.map((group) => {
    const sessionIds = group.sessionIds.filter((sessionId) =>
      existingSessionIds.has(sessionId),
    );
    if (sessionIds.length === group.sessionIds.length) {
      return group;
    }
    changed = true;
    return { ...group, sessionIds };
  });
  if (!changed) {
    return state;
  }
  return withProjectGroups(state, projectId, { ...current, groups });
}

export function syncGpuiWorkspaceProjectOrder(
  state: GpuiWorkspaceSessionGroupsState,
  orderedProjectIds: readonly string[],
): GpuiWorkspaceSessionGroupsState {
  const deduped = [...new Set(orderedProjectIds)];
  if (
    deduped.length === state.projectOrder.length &&
    deduped.every((projectId, index) => state.projectOrder[index] === projectId)
  ) {
    return state;
  }
  return { ...state, projectOrder: deduped };
}

export function orderGpuiWorkspaceProjects<TProject extends { projectId: string }>(
  projects: readonly TProject[],
  projectOrder: readonly string[],
): TProject[] {
  if (projectOrder.length === 0) {
    return [...projects];
  }
  const byId = new Map(projects.map((project) => [project.projectId, project]));
  const orderedIds = new Set<string>();
  const ordered: TProject[] = [];
  for (const projectId of projectOrder) {
    const project = byId.get(projectId);
    if (project && !orderedIds.has(projectId)) {
      ordered.push(project);
      orderedIds.add(projectId);
    }
  }
  const remaining = projects.filter((project) => !orderedIds.has(project.projectId));
  return [...ordered, ...remaining];
}

function removeSessionFromSubgroups(
  current: GpuiProjectWorkspaceGroups,
  sessionId: string,
): GpuiProjectWorkspaceGroups {
  if (!current.groups.some((group) => group.sessionIds.includes(sessionId))) {
    return current;
  }
  return {
    ...current,
    groups: current.groups.map((group) =>
      group.sessionIds.includes(sessionId)
        ? { ...group, sessionIds: group.sessionIds.filter((id) => id !== sessionId) }
        : group,
    ),
  };
}

export function parseGpuiWorkspaceSessionGroupsState(
  parsed: unknown,
): GpuiWorkspaceSessionGroupsState {
  if (typeof parsed !== "object" || parsed === null) {
    return createEmptyGpuiWorkspaceSessionGroupsState();
  }
  const record = parsed as { projectOrder?: unknown; projects?: unknown };
  const projectOrder = Array.isArray(record.projectOrder)
    ? record.projectOrder.filter(
        (value): value is string => typeof value === "string" && value.length > 0,
      )
    : [];
  const projects: Record<string, GpuiProjectWorkspaceGroups> = {};
  if (typeof record.projects === "object" && record.projects !== null) {
    for (const [projectId, value] of Object.entries(record.projects)) {
      const normalized = normalizeStoredProjectGroups(value);
      if (normalized) {
        projects[projectId] = normalized;
      }
    }
  }
  return { projectOrder, projects };
}

export function isEmptyGpuiWorkspaceSessionGroupsState(
  state: GpuiWorkspaceSessionGroupsState,
): boolean {
  return state.projectOrder.length === 0 && Object.keys(state.projects).length === 0;
}

export function readStoredGpuiWorkspaceSessionGroupsState(): GpuiWorkspaceSessionGroupsState {
  try {
    const raw = window.localStorage.getItem(GPUI_WORKSPACE_SESSION_GROUPS_STORAGE_KEY);
    if (!raw) {
      return createEmptyGpuiWorkspaceSessionGroupsState();
    }
    return parseGpuiWorkspaceSessionGroupsState(JSON.parse(raw) as unknown);
  } catch {
    return createEmptyGpuiWorkspaceSessionGroupsState();
  }
}

export function writeStoredGpuiWorkspaceSessionGroupsState(
  state: GpuiWorkspaceSessionGroupsState,
): void {
  try {
    if (state.projectOrder.length === 0 && Object.keys(state.projects).length === 0) {
      window.localStorage.removeItem(GPUI_WORKSPACE_SESSION_GROUPS_STORAGE_KEY);
      return;
    }
    window.localStorage.setItem(
      GPUI_WORKSPACE_SESSION_GROUPS_STORAGE_KEY,
      JSON.stringify(state),
    );
  } catch {
    // Storage availability must never gate sidebar group behavior.
  }
}

function normalizeStoredProjectGroups(value: unknown): GpuiProjectWorkspaceGroups | undefined {
  if (typeof value !== "object" || value === null) {
    return undefined;
  }
  const record = value as { groups?: unknown; nextGroupNumber?: unknown };
  const groups: GpuiWorkspaceSessionSubgroup[] = [];
  if (Array.isArray(record.groups)) {
    for (const entry of record.groups) {
      if (typeof entry !== "object" || entry === null) {
        continue;
      }
      const group = entry as { groupId?: unknown; sessionIds?: unknown; title?: unknown };
      if (typeof group.groupId !== "string" || !group.groupId) {
        continue;
      }
      groups.push({
        groupId: group.groupId,
        sessionIds: Array.isArray(group.sessionIds)
          ? group.sessionIds.filter(
              (sessionId): sessionId is string =>
                typeof sessionId === "string" && sessionId.length > 0,
            )
          : [],
        title:
          typeof group.title === "string" && group.title.trim()
            ? group.title
            : group.groupId,
      });
    }
  }
  const nextGroupNumber =
    typeof record.nextGroupNumber === "number" &&
    Number.isInteger(record.nextGroupNumber) &&
    record.nextGroupNumber >= 2
      ? record.nextGroupNumber
      : Math.max(
          2,
          ...groups.map((group) => {
            const match = /^group-(\d+)$/.exec(group.groupId);
            return match ? Number.parseInt(match[1], 10) + 1 : 2;
          }),
        );
  if (groups.length === 0 && nextGroupNumber === 2) {
    return undefined;
  }
  return { groups, nextGroupNumber };
}
