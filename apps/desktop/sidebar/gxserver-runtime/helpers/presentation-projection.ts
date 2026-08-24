/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_DEFAULT_VISIBLE_COUNT,
  GPUI_GXSERVER_CHATS_GROUP_ID,
  GPUI_GXSERVER_UNAVAILABLE_GROUP_ID,
} from '../constants';
import type { GpuiPresentationProjectProjectionMetadata, GpuiSidebarGroupsPatch } from '../types-and-protocol';
import { booleanFromRecord, optionalNumberField, stringFromRecord } from './records';
import {
  createGpuiProjectWorktreeParentCandidates,
  normalizeGpuiProjectPath,
  normalizeGpuiSidebarWorktreeMetadata,
  normalizeGpuiWorktreeParentProjectId,
  resolveGpuiProjectWorktreeParentMetadata,
} from './worktrees';
import type { GxserverPresentationSidebarProjectOverlay } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type {
  GxserverPresentationSnapshot,
  GxserverProjectDomainState,
  GxserverRecentProjectDomainState,
} from '@/packages/shared/gxserver-protocol';
import type { SidebarProjectSettingsItem, SidebarSessionGroup } from '@/packages/shared/session-grid-contract';
import type { SidebarAgentButton } from '@/packages/shared/sidebar-agents';
import { DEFAULT_SIDEBAR_AGENTS, getSidebarAgentIconById } from '@/packages/shared/sidebar-agents';
import type { WorkspaceProjectIcon } from '@/packages/shared/workspace-project-appearance';
import {
  normalizeWorkspaceProjectIcon,
  normalizeWorkspaceProjectIconDataUrl,
} from '@/packages/shared/workspace-project-appearance';

export function createGpuiPresentationProjectProjectionMetadata({
  domainProjects,
  presentation,
  projectOrder,
  recentProjects,
}: {
  domainProjects: readonly GxserverProjectDomainState[];
  presentation: GxserverPresentationSnapshot;
  projectOrder?: readonly string[];
  recentProjects?: readonly GxserverRecentProjectDomainState[];
}): GpuiPresentationProjectProjectionMetadata {
  const chatProjectIds = new Set<string>();
  /*
  CDXC:GPUIRecentProjects 2026-06-27-19:37:
  GPUI must match the macOS sidebar split: parked Recent Projects belong only in the React Recent Projects drawer, never in the main Projects list. Hide ids from both the domain project flag and the authoritative `/api/listRecentProjects` endpoint so presentation snapshots cannot briefly resurrect parked projects as normal groups.
  */
  const hiddenProjectIds = new Set(
    (recentProjects ?? [])
      .map((project) => (typeof project.projectId === 'string' ? project.projectId.trim() : ''))
      .filter((projectId) => projectId.length > 0)
  );
  const projectOverlaysById = new Map<string, GxserverPresentationSidebarProjectOverlay>();
  const domainProjectIds = new Set(domainProjects.map((project) => project.projectId));
  const orderIndexByProjectId = new Map((projectOrder ?? []).map((projectId, index) => [projectId, index]));
  const worktreeParentCandidates = createGpuiProjectWorktreeParentCandidates({
    domainProjects,
    presentation,
  });

  for (const project of domainProjects) {
    const isChatProject = isGpuiPresentationChatDomainProject(project);
    const isQuickProject = isGpuiPresentationQuickDomainProject(project);
    const iconDataUrl = gpuiPresentationProjectIconDataUrl(project);
    const icon = gpuiPresentationProjectIcon(project);
    const worktree = resolveGpuiProjectWorktreeParentMetadata(
      normalizeGpuiSidebarWorktreeMetadata(project.worktree),
      worktreeParentCandidates
    );
    if (project.isRecentProject === true) {
      hiddenProjectIds.add(project.projectId);
    }
    if (isChatProject || isQuickProject) {
      chatProjectIds.add(project.projectId);
    }
    mergeGpuiPresentationProjectOverlay(projectOverlaysById, project.projectId, {
      ...(icon ? { icon } : {}),
      ...(iconDataUrl ? { iconDataUrl } : {}),
      ...(isChatProject ? { isChatProject } : {}),
      ...(isQuickProject ? { isQuickProject } : {}),
      ...optionalNumberField('orderIndex', orderIndexByProjectId.get(project.projectId)),
      ...(worktree ? { worktree } : {}),
    });
  }

  for (const project of presentation.projects) {
    const orderIndex = orderIndexByProjectId.get(project.projectId);
    const worktree = resolveGpuiProjectWorktreeParentMetadata(
      normalizeGpuiSidebarWorktreeMetadata(project.worktree),
      worktreeParentCandidates
    );
    if (orderIndex !== undefined || worktree) {
      mergeGpuiPresentationProjectOverlay(projectOverlaysById, project.projectId, {
        ...optionalNumberField('orderIndex', orderIndex),
        ...(worktree ? { worktree } : {}),
      });
    }
    if (domainProjectIds.has(project.projectId) || !isGpuiPresentationChatProjectPath(project.path)) {
      continue;
    }
    chatProjectIds.add(project.projectId);
    mergeGpuiPresentationProjectOverlay(projectOverlaysById, project.projectId, {
      isChatProject: true,
      isQuickProject: true,
    });
  }

  return {
    chatProjectIds,
    hiddenProjectIds,
    projectOverlays: [...projectOverlaysById.values()],
  };
}

export function mergeGpuiPresentationProjectOverlay(
  overlaysById: Map<string, GxserverPresentationSidebarProjectOverlay>,
  projectId: string,
  patch: Partial<Omit<GxserverPresentationSidebarProjectOverlay, 'projectId'>>
): void {
  if (!overlaysById.has(projectId) && Object.values(patch).every((value) => value === undefined)) {
    return;
  }
  overlaysById.set(projectId, {
    ...overlaysById.get(projectId),
    ...patch,
    projectId,
  });
}

/*
CDXC:SidebarV2ProjectIcons 2026-07-29:
The TYPED project icon, from the same gxserver identity metadata as the image
data URL above it. Most Ghostex projects carry a Tabler glyph plus a color
rather than an uploaded image, so a sidebar that only receives `iconDataUrl`
shows almost every project a generic folder. Same sourcing rules apply: identity
metadata only, never inferred from paths, titles, sessions, or renderer state.
*/
export function gpuiPresentationProjectIcon(project: GxserverProjectDomainState): WorkspaceProjectIcon | undefined {
  return normalizeWorkspaceProjectIcon(project.identityIcon?.icon);
}

export function gpuiPresentationProjectIconDataUrl(project: GxserverProjectDomainState): string | undefined {
  /*
  CDXC:GPUISettingsNotifications 2026-06-26-07:22:
  Session-attention icon parity must source images only from gxserver project identity metadata already normalized for workspace project appearance. Do not infer icons from project paths, URLs, titles, sessions, browser favicons, logs, command output, or renderer-local state.
  */
  const identityIcon = project.identityIcon;
  if (!identityIcon) {
    return undefined;
  }
  const icon = normalizeWorkspaceProjectIcon(identityIcon.icon);
  if (icon?.kind === 'image') {
    return icon.dataUrl;
  }
  return normalizeWorkspaceProjectIconDataUrl(identityIcon.iconDataUrl);
}

export function isGpuiPresentationChatDomainProject(project: GxserverProjectDomainState | undefined): boolean {
  return (
    booleanFromRecord(project as Record<string, unknown> | undefined, 'isChat') === true ||
    booleanFromRecord(project?.launchSettings, 'isChat') === true ||
    isGpuiPresentationChatProjectPath(project?.path)
  );
}

export function isGpuiPresentationQuickDomainProject(project: GxserverProjectDomainState | undefined): boolean {
  return (
    booleanFromRecord(project as Record<string, unknown> | undefined, 'isQuick') === true ||
    booleanFromRecord(project?.launchSettings, 'isQuick') === true ||
    isGpuiPresentationChatDomainProject(project)
  );
}

export function isGpuiPresentationChatProjectPath(value: unknown): boolean {
  const path = normalizeGpuiProjectPath(value)?.replace(/\\/gu, '/').replace(/\/+$/u, '');
  if (!path) {
    return false;
  }
  /*
  CDXC:GPUISidebarProjectClassification 2026-06-24-22:51:
  Match macOS chat-project detection by storage root instead of display title. `~/ghostex/chats`, `~/.ghostex[-variant]/chats`, and host-provided Ghostex homes such as repo-local `.active/chats` are projectless Chats containers; arbitrary projects named "Chat ..." are not.
  */
  return (
    /(?:^|\/)(?:ghostex|\.ghostex(?:-[^/]+)?|\.active)\/chats(?:\/|$)/u.test(path) ||
    /^~\/(?:ghostex|\.ghostex(?:-[^/]+)?|\.active)\/chats(?:\/|$)/u.test(path)
  );
}

export function createGpuiProjectSettingsProjects(
  domainProjects: readonly GxserverProjectDomainState[],
  presentation: GxserverPresentationSnapshot | undefined
): SidebarProjectSettingsItem[] {
  if (domainProjects.length > 0) {
    return domainProjects.flatMap((project) => {
      const path = normalizeGpuiProjectPath(project.path);
      if (!path || project.isRecentProject === true || isGpuiPresentationQuickDomainProject(project)) {
        return [];
      }
      return [
        {
          ...optionalGpuiProjectSettingsString(
            'beadsDirectory',
            stringFromRecord(project.projectBoardConfig, 'beadsDirectory')
          ),
          ...optionalGpuiProjectSettingsString(
            'beadsDisplayKey',
            stringFromRecord(project.projectBoardConfig, 'beadsDisplayKey') ??
              stringFromRecord(project.gitConfig, 'beadsDisplayKey')
          ),
          ...optionalGpuiProjectSettingsString(
            'docsDirectory',
            stringFromRecord(project.projectBoardConfig, 'docsDirectory')
          ),
          name: project.name,
          path,
          projectId: project.projectId,
          ...optionalGpuiProjectSettingsString(
            'worktreeCommand',
            stringFromRecord(project.gitConfig, 'worktreeCommand')
          ),
          ...optionalGpuiProjectSettingsString(
            'worktreeParentProjectId',
            normalizeGpuiWorktreeParentProjectId(project.worktree)
          ),
        },
      ];
    });
  }
  return (presentation?.projects ?? []).flatMap((project) => {
    const path = normalizeGpuiProjectPath(project.path);
    if (!path || isGpuiPresentationChatProjectPath(path)) {
      return [];
    }
    return [
      {
        name: project.title,
        path,
        projectId: project.projectId,
        ...optionalGpuiProjectSettingsString(
          'worktreeParentProjectId',
          normalizeGpuiWorktreeParentProjectId(project.worktree)
        ),
      },
    ];
  });
}

export function optionalGpuiProjectSettingsString<TKey extends keyof SidebarProjectSettingsItem>(
  key: TKey,
  value: string | undefined
): Partial<Pick<SidebarProjectSettingsItem, TKey>> {
  return value ? ({ [key]: value } as Partial<Pick<SidebarProjectSettingsItem, TKey>>) : {};
}

export function normalizeGpuiPathForProjectComparison(path: string): string {
  return path.trim().replace(/\/+$/u, '') || path.trim();
}

export function createGpuiGxserverUnavailableSidebarGroups(): SidebarSessionGroup[] {
  return [
    {
      groupId: GPUI_GXSERVER_CHATS_GROUP_ID,
      isActive: false,
      isChatCollection: true,
      isFocusModeActive: false,
      kind: 'workspace',
      layoutVisibleCount: GPUI_DEFAULT_VISIBLE_COUNT,
      sessions: [],
      title: 'Chats',
      viewMode: 'grid',
      visibleCount: GPUI_DEFAULT_VISIBLE_COUNT,
    },
    {
      groupId: GPUI_GXSERVER_UNAVAILABLE_GROUP_ID,
      isActive: true,
      isFocusModeActive: false,
      kind: 'workspace',
      layoutVisibleCount: GPUI_DEFAULT_VISIBLE_COUNT,
      sessions: [],
      title: '',
      viewMode: 'grid',
      visibleCount: GPUI_DEFAULT_VISIBLE_COUNT,
    },
  ];
}

export function createGpuiSidebarGroupsPatch(
  previousGroups: readonly SidebarSessionGroup[],
  nextGroups: SidebarSessionGroup[]
): GpuiSidebarGroupsPatch {
  const previousGroupsById = new Map(previousGroups.map((group) => [group.groupId, group]));
  const nextGroupIds = new Set(nextGroups.map((group) => group.groupId));
  const previousSessionIds = new Set(
    previousGroups.flatMap((group) => group.sessions.map((session) => session.sessionId))
  );
  const nextSessionIds = new Set(nextGroups.flatMap((group) => group.sessions.map((session) => session.sessionId)));
  return {
    groupOrder: nextGroups.map((group) => group.groupId),
    /*
    CDXC:SidebarDiffStatsChurn 2026-08-16:
    The SidebarApp store merges patch groups by groupId and leaves untouched
    groups alone, so a patch only needs the groups that actually changed.
    Sending all groups on every publish forced the renderer to re-normalize
    and deep-compare the entire tree per message, which is what made routine
    background publishes expensive in large sidebars.
    */
    groups: nextGroups.filter((group) => {
      const previousGroup = previousGroupsById.get(group.groupId);
      return !previousGroup || !haveSameSidebarProjectionValue(previousGroup, group);
    }),
    removedGroupIds: [...previousGroupsById.keys()].filter((groupId) => !nextGroupIds.has(groupId)),
    removedSessionIds: [...previousSessionIds].filter((sessionId) => !nextSessionIds.has(sessionId)),
  };
}

/**
 * Structural equality for the JSON-serializable sidebar projection values that
 * cross the runtime -> SidebarApp postMessage boundary. Mirrors the store's
 * `haveSameSerializableValue` so both sides agree on what "unchanged" means.
 */
export function haveSameSidebarProjectionValue(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) {
    return true;
  }
  if (typeof left !== typeof right) {
    return false;
  }
  if (typeof left !== 'object' || left === null || right === null) {
    return false;
  }
  if (Array.isArray(left) || Array.isArray(right)) {
    if (!Array.isArray(left) || !Array.isArray(right) || left.length !== right.length) {
      return false;
    }
    return left.every((value, index) => haveSameSidebarProjectionValue(value, right[index]));
  }

  const leftRecord = left as Record<string, unknown>;
  const rightRecord = right as Record<string, unknown>;
  const leftKeys = Object.keys(leftRecord);
  const rightKeys = Object.keys(rightRecord);
  return (
    leftKeys.length === rightKeys.length &&
    leftKeys.every((key) => haveSameSidebarProjectionValue(leftRecord[key], rightRecord[key]))
  );
}

export function resolveGpuiSidebarAgentIcon(agentName: string | undefined): SidebarAgentButton['icon'] {
  const directIcon = getSidebarAgentIconById(agentName);
  if (directIcon) {
    return directIcon;
  }

  const normalizedAgentName = agentName?.trim().toLowerCase();
  if (!normalizedAgentName) {
    return undefined;
  }
  return DEFAULT_SIDEBAR_AGENTS.find(
    (agent) =>
      agent.agentId === normalizedAgentName ||
      agent.name.trim().toLowerCase() === normalizedAgentName ||
      agent.icon === normalizedAgentName
  )?.icon;
}

export function createGpuiSidebarSessionRoutingId(projectId: string, sessionId: string): string {
  return `${projectId}:${sessionId}`;
}
