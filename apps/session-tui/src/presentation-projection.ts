/*
CDXC:Projects 2026-09-25 WHY:
The project metadata this debug view builds its groups from (chat and parked projects, icons,
worktree parents, the stored order) came from the desktop's old app runtime
(`apps/desktop/sidebar/gxserver-runtime/helpers/presentation-projection.ts`, `worktrees.ts`,
`records.ts`), which was deleted with QuickJS on 2026-09-25. It moved here unchanged, because this
TUI is its only reader; the desktop sidebar builds the same metadata in Rust
(packages/gx-core/src/sidebar_view/projects.rs).
*/
import type { GxserverPresentationSidebarProjectOverlay } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type {
  GxserverPresentationSnapshot,
  GxserverProjectDomainState,
  GxserverRecentProjectDomainState,
} from '@/packages/shared/gxserver-protocol';
import type { SidebarProjectWorktreeMetadata } from '@/packages/shared/session-grid-contract';
import type { SidebarAgentButton } from '@/packages/shared/sidebar-agents';
import { DEFAULT_SIDEBAR_AGENTS, getSidebarAgentIconById } from '@/packages/shared/sidebar-agents';
import type { WorkspaceProjectIcon } from '@/packages/shared/workspace-project-appearance';
import {
  normalizeWorkspaceProjectIcon,
  normalizeWorkspaceProjectIconDataUrl,
} from '@/packages/shared/workspace-project-appearance';

export type GpuiPresentationProjectProjectionMetadata = {
  chatProjectIds: ReadonlySet<string>;
  hiddenProjectIds: ReadonlySet<string>;
  projectOverlays: readonly GxserverPresentationSidebarProjectOverlay[];
};

type GpuiProjectWorktreeParentCandidate = {
  name?: string;
  path?: string;
  projectId: string;
  worktree?: Record<string, unknown>;
};

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
  CDXC:Projects 2026-06-27-19:37:
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

function mergeGpuiPresentationProjectOverlay(
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
CDXC:Icons 2026-07-29:
The TYPED project icon, from the same gxserver identity metadata as the image
data URL above it. Most Ghostex projects carry a Tabler glyph plus a color
rather than an uploaded image, so a sidebar that only receives `iconDataUrl`
shows almost every project a generic folder. Same sourcing rules apply: identity
metadata only, never inferred from paths, titles, sessions, or renderer state.
*/
function gpuiPresentationProjectIcon(project: GxserverProjectDomainState): WorkspaceProjectIcon | undefined {
  return normalizeWorkspaceProjectIcon(project.identityIcon?.icon);
}

function gpuiPresentationProjectIconDataUrl(project: GxserverProjectDomainState): string | undefined {
  /*
  CDXC:Notifications 2026-06-26-07:22:
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

function isGpuiPresentationChatDomainProject(project: GxserverProjectDomainState | undefined): boolean {
  return (
    booleanFromRecord(project as Record<string, unknown> | undefined, 'isChat') === true ||
    booleanFromRecord(project?.launchSettings, 'isChat') === true ||
    isGpuiPresentationChatProjectPath(project?.path)
  );
}

function isGpuiPresentationQuickDomainProject(project: GxserverProjectDomainState | undefined): boolean {
  return (
    booleanFromRecord(project as Record<string, unknown> | undefined, 'isQuick') === true ||
    booleanFromRecord(project?.launchSettings, 'isQuick') === true ||
    isGpuiPresentationChatDomainProject(project)
  );
}

function isGpuiPresentationChatProjectPath(value: unknown): boolean {
  const path = normalizeGpuiProjectPath(value)?.replace(/\\/gu, '/').replace(/\/+$/u, '');
  if (!path) {
    return false;
  }
  /*
  CDXC:Projects 2026-06-24-22:51:
  Match macOS chat-project detection by storage root instead of display title. `~/ghostex/chats`, `~/.ghostex[-variant]/chats`, and host-provided Ghostex homes such as repo-local `.active/chats` are projectless Chats containers; arbitrary projects named "Chat ..." are not.
  */
  return (
    /(?:^|\/)(?:ghostex|\.ghostex(?:-[^/]+)?|\.active)\/chats(?:\/|$)/u.test(path) ||
    /^~\/(?:ghostex|\.ghostex(?:-[^/]+)?|\.active)\/chats(?:\/|$)/u.test(path)
  );
}

function normalizeGpuiPathForProjectComparison(path: string): string {
  return path.trim().replace(/\/+$/u, '') || path.trim();
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

function createGpuiProjectWorktreeParentCandidates({
  domainProjects,
  presentation,
}: {
  domainProjects: readonly GxserverProjectDomainState[];
  presentation: GxserverPresentationSnapshot;
}): GpuiProjectWorktreeParentCandidate[] {
  return [
    ...presentation.projects.map((project) => ({
      name: project.title,
      path: project.path,
      projectId: project.projectId,
      worktree: project.worktree,
    })),
    ...domainProjects.map((project) => ({
      name: project.name,
      path: project.path,
      projectId: project.projectId,
      worktree: project.worktree,
    })),
  ];
}

function resolveGpuiProjectWorktreeParentMetadata(
  worktree: SidebarProjectWorktreeMetadata | undefined,
  candidates: readonly GpuiProjectWorktreeParentCandidate[]
): SidebarProjectWorktreeMetadata | undefined {
  if (!worktree) {
    return undefined;
  }
  const parentPath = normalizeGpuiPathForProjectComparison(worktree.parentProjectPath);
  const canonicalParent = candidates.find((candidate) => {
    if (candidate.projectId === worktree.parentProjectId || !candidate.path) {
      return false;
    }
    if (normalizeGpuiPathForProjectComparison(candidate.path) !== parentPath) {
      return false;
    }
    return !normalizeGpuiWorktreeParentProjectId(candidate.worktree);
  });
  if (!canonicalParent) {
    return worktree;
  }
  const canonicalParentPath = canonicalParent.path?.trim();
  return {
    ...worktree,
    parentProjectId: canonicalParent.projectId,
    parentProjectName: canonicalParent.name?.trim() || worktree.parentProjectName,
    parentProjectPath: canonicalParentPath || worktree.parentProjectPath,
  };
}

function normalizeGpuiWorktreeParentProjectId(worktree: Record<string, unknown> | undefined): string | undefined {
  return stringFromRecord(worktree, 'parentProjectId');
}

function normalizeGpuiSidebarWorktreeMetadata(
  worktree: Record<string, unknown> | undefined
): SidebarProjectWorktreeMetadata | undefined {
  const branch = stringFromRecord(worktree, 'branch');
  const name = stringFromRecord(worktree, 'name');
  const parentProjectId = normalizeGpuiWorktreeParentProjectId(worktree);
  const parentProjectName = stringFromRecord(worktree, 'parentProjectName');
  const parentProjectPath = stringFromRecord(worktree, 'parentProjectPath');
  if (!branch || !name || !parentProjectId || !parentProjectName || !parentProjectPath) {
    return undefined;
  }
  const createdAt = stringFromRecord(worktree, 'createdAt');
  return {
    branch,
    ...(createdAt && !Number.isNaN(Date.parse(createdAt)) ? { createdAt } : {}),
    name,
    parentProjectId,
    parentProjectName,
    parentProjectPath,
  };
}

function normalizeGpuiProjectPath(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim().length > 0 ? value.trim().replace(/\/+$/u, '') : undefined;
}

function stringFromRecord(record: Record<string, unknown> | undefined, key: string): string | undefined {
  const value = record?.[key];
  return typeof value === 'string' && value.trim().length > 0 ? value.trim() : undefined;
}

function booleanFromRecord(record: Record<string, unknown> | undefined, key: string): boolean | undefined {
  const value = record?.[key];
  return typeof value === 'boolean' ? value : undefined;
}

function optionalNumberField<TKey extends string>(
  key: TKey,
  value: number | undefined
): Partial<Record<TKey, number>> {
  return value !== undefined ? ({ [key]: value } as Partial<Record<TKey, number>>) : {};
}
