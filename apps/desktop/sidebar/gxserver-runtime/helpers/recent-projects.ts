/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_REMOTE_GROUP_ORDER_STORAGE_KEY,
  GPUI_REMOTE_LAST_SEEN_PRESENTATIONS_STORAGE_KEY,
  GPUI_REMOTE_RECENT_PROJECTS_STORAGE_KEY,
} from '../constants';
import { normalizeGpuiSidebarTheme } from './bootstrap';
import { normalizeNonEmptyString } from './records';
import { createGpuiRemotePresentationProjectId, isPresentationSnapshot } from './remote-presentation';
import { normalizeGpuiProjectPath } from './worktrees';
import type { ghostexSettings } from '@/packages/shared/ghostex-settings';
import type {
  GxserverPresentationSnapshot,
  GxserverProjectId,
  GxserverRecentProjectDomainState,
} from '@/packages/shared/gxserver-protocol';
import type { SidebarRecentProject } from '@/packages/shared/session-grid-contract';
import { resolveSidebarTheme } from '@/packages/shared/session-grid-contract';
import {
  normalizeWorkspaceProjectIcon,
  normalizeWorkspaceProjectIconDataUrl,
  normalizeWorkspaceThemeColor,
} from '@/packages/shared/workspace-project-appearance';

export function createGpuiRecentProjects(
  recentProjects: readonly GxserverRecentProjectDomainState[],
  settings: ghostexSettings
): SidebarRecentProject[] {
  return recentProjects
    .flatMap((project) => {
      const projectId = typeof project.projectId === 'string' ? project.projectId.trim() : '';
      const title = typeof project.title === 'string' ? project.title.trim() : '';
      const path = normalizeGpuiProjectPath(project.path);
      if (!projectId || !title || !path) {
        return [];
      }
      const icon = normalizeWorkspaceProjectIcon(project.icon);
      const iconDataUrl = normalizeWorkspaceProjectIconDataUrl(project.iconDataUrl);
      const theme = normalizeGpuiSidebarTheme(project.theme) ?? resolveSidebarTheme(settings.sidebarTheme, 'dark');
      const themeColor = normalizeWorkspaceThemeColor(project.themeColor);
      const recentClosedAt =
        typeof project.recentClosedAt === 'string' && project.recentClosedAt.trim().length > 0
          ? project.recentClosedAt.trim()
          : undefined;
      return [
        {
          ...(icon ? { icon } : {}),
          ...(iconDataUrl ? { iconDataUrl } : {}),
          ...(recentClosedAt ? { recentClosedAt } : {}),
          ...(themeColor ? { themeColor } : {}),
          path,
          projectId,
          sessionCount: Number.isFinite(project.sessionCount) ? Math.max(0, Math.floor(project.sessionCount)) : 0,
          theme,
          title,
        },
      ];
    })
    .sort(compareGpuiRecentProjectsByClosedAt);
}

export function createGpuiRemoteRecentProjects(
  recentProjectsByMachineId: ReadonlyMap<string, readonly GxserverRecentProjectDomainState[]> | undefined,
  presentationsByMachineId: ReadonlyMap<string, GxserverPresentationSnapshot> | undefined,
  settings: ghostexSettings
): SidebarRecentProject[] {
  /*
  CDXC:GPUIRemoteProjects 2026-06-27-19:37:
  Remote Recent Projects are GPUI-client-local parking rows. Keep ids
  machine-scoped and reconcile display fields from a live remote presentation
  when connected, but do not call the remote daemon's recent endpoints or share
  the parked state with the macOS app.
  */
  if (!recentProjectsByMachineId) {
    return [];
  }
  const remoteMachinesById = new Map(settings.remoteMachines.map((machine) => [machine.id, machine]));
  return [...recentProjectsByMachineId.entries()].flatMap(([machineId, recentProjects]) => {
    const machine = remoteMachinesById.get(machineId);
    if (!machine) {
      return [];
    }
    const presentation = presentationsByMachineId?.get(machineId);
    return recentProjects.flatMap((project) => {
      const projectId = typeof project.projectId === 'string' ? project.projectId.trim() : '';
      const presentationProject = presentation?.projects.find((candidate) => candidate.projectId === projectId);
      if (presentation && !presentationProject) {
        return [];
      }
      const title =
        presentationProject?.title.trim() || (typeof project.title === 'string' ? project.title.trim() : '');
      const path = normalizeGpuiProjectPath(presentationProject?.path ?? project.path);
      if (!projectId || !title || !path) {
        return [];
      }
      const icon = normalizeWorkspaceProjectIcon(project.icon);
      const iconDataUrl = normalizeWorkspaceProjectIconDataUrl(project.iconDataUrl);
      const theme = normalizeGpuiSidebarTheme(project.theme) ?? resolveSidebarTheme(settings.sidebarTheme, 'dark');
      const themeColor = normalizeWorkspaceThemeColor(project.themeColor);
      const recentClosedAt =
        typeof project.recentClosedAt === 'string' && project.recentClosedAt.trim().length > 0
          ? project.recentClosedAt.trim()
          : undefined;
      return [
        {
          ...(icon ? { icon } : {}),
          ...(iconDataUrl ? { iconDataUrl } : {}),
          ...(recentClosedAt ? { recentClosedAt } : {}),
          ...(themeColor ? { themeColor } : {}),
          path,
          projectId: createGpuiRemotePresentationProjectId(machineId, projectId),
          remoteMachineId: machineId,
          remoteMachineName: machine.name || 'Remote',
          sessionCount: presentation
            ? countGpuiRemotePresentationProjectSessions(presentation, projectId)
            : Number.isFinite(project.sessionCount)
              ? Math.max(0, Math.floor(project.sessionCount))
              : 0,
          theme,
          title,
        },
      ];
    });
  });
}

/*
CDXC:RemoteGroupReorder 2026-07-12:
Per-machine remote project group order is app-client presentation state, like
the remote recent-projects list: the remote gxserver keeps publishing its own
group order and this map only reorders the projection locally. Persist only
machine ids and remote project ids.
*/
export function readStoredGpuiRemoteGroupOrder(): Map<string, string[]> {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(GPUI_REMOTE_GROUP_ORDER_STORAGE_KEY) ?? '{}');
    if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
      return new Map();
    }
    const next = new Map<string, string[]>();
    for (const [machineId, order] of Object.entries(raw)) {
      if (!machineId.trim() || !Array.isArray(order)) {
        continue;
      }
      const projectIds = order.filter(
        (projectId): projectId is string => typeof projectId === 'string' && projectId.trim().length > 0
      );
      if (projectIds.length > 0) {
        next.set(machineId, projectIds);
      }
    }
    return next;
  } catch {
    return new Map();
  }
}

export function writeStoredGpuiRemoteGroupOrder(orderByMachineId: ReadonlyMap<string, readonly string[]>): void {
  try {
    localStorage.setItem(GPUI_REMOTE_GROUP_ORDER_STORAGE_KEY, JSON.stringify(Object.fromEntries(orderByMachineId)));
  } catch {
    // CEF storage may be unavailable in tests or early bootstrap; the in-memory order still drives this session.
  }
}

export function readStoredGpuiRemoteLastSeenPresentations(): Map<string, GxserverPresentationSnapshot> {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(GPUI_REMOTE_LAST_SEEN_PRESENTATIONS_STORAGE_KEY) ?? '{}');
    if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
      return new Map();
    }
    const next = new Map<string, GxserverPresentationSnapshot>();
    for (const [machineId, snapshot] of Object.entries(raw)) {
      if (!machineId.trim() || !isPresentationSnapshot(snapshot)) {
        continue;
      }
      next.set(machineId, snapshot);
    }
    return next;
  } catch {
    return new Map();
  }
}

export function writeStoredGpuiRemoteLastSeenPresentations(
  presentationsByMachineId: ReadonlyMap<string, GxserverPresentationSnapshot>
): void {
  /*
  CDXC:GPUIRemoteLastSeen 2026-07-12:
  Last-seen remote presentations are the same sanitized snapshots the sidebar
  already renders (project titles/paths, session titles, states). Persisting
  them app-client-locally lets disconnected machines keep their faded project
  view across restarts; no tokens, SSH details, or daemon internals exist in
  these snapshots.
  */
  try {
    localStorage.setItem(
      GPUI_REMOTE_LAST_SEEN_PRESENTATIONS_STORAGE_KEY,
      JSON.stringify(Object.fromEntries(presentationsByMachineId))
    );
  } catch {
    // CEF storage may be unavailable in tests or early bootstrap; the in-memory copy still drives this session.
  }
}

export function readStoredGpuiRemoteRecentProjects(): Map<string, GxserverRecentProjectDomainState[]> {
  try {
    return groupGpuiRemoteRecentProjectsByMachine(
      normalizeStoredGpuiRemoteRecentProjects(
        JSON.parse(localStorage.getItem(GPUI_REMOTE_RECENT_PROJECTS_STORAGE_KEY) ?? '[]')
      )
    );
  } catch {
    return new Map();
  }
}

export function writeStoredGpuiRemoteRecentProjects(
  projectsByMachineId: ReadonlyMap<string, readonly GxserverRecentProjectDomainState[]>
): void {
  try {
    const rows = [...projectsByMachineId.entries()].flatMap(([machineId, projects]) =>
      projects.flatMap((project) => {
        const projectId = typeof project.projectId === 'string' ? project.projectId.trim() : '';
        const title = typeof project.title === 'string' ? project.title.trim() : '';
        const path = typeof project.path === 'string' ? project.path.trim() : '';
        if (!machineId.trim() || !projectId || !title) {
          return [];
        }
        return [
          {
            machineId: machineId.trim(),
            path,
            projectId,
            recentClosedAt: typeof project.recentClosedAt === 'string' ? project.recentClosedAt : undefined,
            sessionCount: Number.isFinite(project.sessionCount) ? Math.max(0, Math.floor(project.sessionCount)) : 0,
            title,
          },
        ];
      })
    );
    /*
    CDXC:GPUIRemoteProjects 2026-06-27-19:37:
    GPUI remote recent rows are app-client state. Persist only machine id,
    remote project id, title/path needed for the disconnected drawer, timestamp,
    and count; do not persist tokens, SSH hosts, usernames, command text,
    terminal output, or local gxserver project rows.
    */
    localStorage.setItem(GPUI_REMOTE_RECENT_PROJECTS_STORAGE_KEY, JSON.stringify(rows));
  } catch {
    // CEF storage may be unavailable in tests or early bootstrap; the in-memory rows still drive this session.
  }
}

export function normalizeStoredGpuiRemoteRecentProjects(
  value: unknown
): Array<{ machineId: string; project: GxserverRecentProjectDomainState }> {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.flatMap((candidate) => {
    if (!candidate || typeof candidate !== 'object') {
      return [];
    }
    const record = candidate as Record<string, unknown>;
    const machineId = normalizeNonEmptyString(record.machineId);
    const projectId = normalizeNonEmptyString(record.projectId);
    const title = normalizeNonEmptyString(record.title);
    if (!machineId || !projectId || !title) {
      return [];
    }
    const path = typeof record.path === 'string' ? record.path.trim() : '';
    const recentClosedAt =
      typeof record.recentClosedAt === 'string' &&
      record.recentClosedAt.trim().length > 0 &&
      Number.isFinite(Date.parse(record.recentClosedAt))
        ? record.recentClosedAt.trim()
        : undefined;
    const sessionCount = Number(record.sessionCount);
    return [
      {
        machineId,
        project: {
          path,
          projectId: projectId as GxserverProjectId,
          ...(recentClosedAt ? { recentClosedAt } : {}),
          sessionCount: Number.isFinite(sessionCount) && sessionCount > 0 ? Math.floor(sessionCount) : 0,
          title,
        },
      },
    ];
  });
}

/*
CDXC:GPUIRemoteProjects 2026-06-27-21:59:
The GPUI start build runs through Vite/Rolldown, whose transformer accepts readonly array shorthand and ReadonlyArray<T> but rejects `readonly Array<T>`. Keep this helper input in ReadonlyArray<T> form so Remote Recent Projects packaging does not break local GPUI startup.
*/
export function groupGpuiRemoteRecentProjectsByMachine(
  rows: ReadonlyArray<{ machineId: string; project: GxserverRecentProjectDomainState }>
): Map<string, GxserverRecentProjectDomainState[]> {
  const projectsByMachineId = new Map<string, GxserverRecentProjectDomainState[]>();
  for (const row of rows) {
    projectsByMachineId.set(
      row.machineId,
      orderGpuiRecentProjects([
        row.project,
        ...(projectsByMachineId.get(row.machineId) ?? []).filter(
          (project) => project.projectId !== row.project.projectId
        ),
      ])
    );
  }
  return projectsByMachineId;
}

export function orderGpuiRecentProjects(
  projects: readonly GxserverRecentProjectDomainState[]
): GxserverRecentProjectDomainState[] {
  return [...projects].sort(
    (left, right) => Date.parse(right.recentClosedAt ?? '') - Date.parse(left.recentClosedAt ?? '')
  );
}

export function countGpuiRemotePresentationProjectSessions(
  presentation: GxserverPresentationSnapshot,
  projectId: string
): number {
  return presentation.sessions.filter(
    (session) =>
      session.projectId === projectId && session.visibleInSidebarByDefault === true && session.surface !== 'commands'
  ).length;
}

export function compareGpuiRecentProjectsByClosedAt(left: SidebarRecentProject, right: SidebarRecentProject): number {
  /*
  CDXC:GPUIRecentProjects 2026-06-25-19:22:
  Native `compareRecentProjectsByClosedAt` only sorts parsed close time descending. The Recent Projects drawer contract does not include gxserver `updatedAt`, so GPUI must not invent title or id tie-breaks; stable sort preserves producer order for equal timestamps.
  */
  return gpuiRecentProjectClosedAtMillis(right) - gpuiRecentProjectClosedAtMillis(left);
}

export function gpuiRecentProjectClosedAtMillis(project: SidebarRecentProject): number {
  const millis = Date.parse(project.recentClosedAt ?? '');
  return Number.isFinite(millis) ? millis : 0;
}
