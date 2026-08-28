/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_GXSERVER_CHATS_GROUP_ID,
  GPUI_REMOTE_MACHINE_STATUS_MESSAGE_MAX_CHARS,
  GPUI_REMOTE_MACHINE_STATUS_STATES,
} from '../constants';
import type { GpuiSidebarRemoteEvent } from '../types-and-protocol';
import { isGpuiPresentationChatProjectPath } from './presentation-projection';
import { normalizeNonEmptyString, parseObject } from './records';
import { normalizeGpuiSidebarWorktreeMetadata } from './worktrees';
import type { ghostexSettings } from '@/packages/shared/ghostex-settings';
import type { GxserverPresentationCloseAfterDoneProjection } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import { createGxserverPresentationSidebarGroups } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type {
  GxserverPresentationDelta,
  GxserverPresentationSession,
  GxserverPresentationSnapshot,
  GxserverRecentProjectDomainState,
  GxserverSidebarProjectCollectionsState,
  GxserverSidebarSpacesState,
  GxserverWorkspaceSessionGroupsState,
} from '@/packages/shared/gxserver-protocol';
import { createDefaultSidebarProjectDiffStats } from '@/packages/shared/project-diff-stats';
import {
  createRemoteProjectId,
  createRemoteTerminalSessionId,
  parseRemoteProjectId,
  parseRemoteTerminalSessionId,
} from '@/packages/shared/remote-terminal-selection';
import type { GxserverSessionChatEvent } from '@/packages/shared/session-chat';
import type { SidebarRemoteMachineStatusMessage, SidebarSessionGroup } from '@/packages/shared/session-grid-contract';
import { resolveSidebarTheme } from '@/packages/shared/session-grid-contract';
import type { SidebarAgentButton } from '@/packages/shared/sidebar-agents';

export function createGpuiRemotePresentationSidebarGroups({
  activeGroupId,
  focusedSessionId,
  presentationsByMachineId,
  remoteGroupOrderByMachineId,
  remoteRecentProjectsByMachineId,
  resolveAgentIcon,
  resolveCloseAfterDone,
  settings,
  visibleSessionIds,
}: {
  activeGroupId?: string;
  focusedSessionId?: string;
  presentationsByMachineId: ReadonlyMap<string, GxserverPresentationSnapshot>;
  remoteGroupOrderByMachineId?: ReadonlyMap<string, readonly string[]>;
  remoteRecentProjectsByMachineId?: ReadonlyMap<string, readonly GxserverRecentProjectDomainState[]>;
  resolveAgentIcon: (agentName: string | undefined) => SidebarAgentButton['icon'];
  resolveCloseAfterDone?: (
    machineId: string,
    projectId: string,
    sessionId: string
  ) => GxserverPresentationCloseAfterDoneProjection | undefined;
  settings: ghostexSettings;
  visibleSessionIds?: ReadonlySet<string>;
}): SidebarSessionGroup[] {
  /*
  CDXC:GPUIRemoteMachines 2026-06-24-16:48:
  GPUI remote machine sections must render only saved machines with Rust-delivered gxserver presentation snapshots. Prefix every project/session id with the machine id so reused SidebarApp rows cannot collide with local gxserver rows or another remote machine, while tokens, SSH hosts, usernames, key paths, and remote URLs stay outside renderer state.
  */
  return settings.remoteMachines.flatMap((machine) => {
    const presentation = presentationsByMachineId.get(machine.id);
    if (!presentation) {
      return [];
    }
    /*
    Keyed by plain string: the lookup key is decoded out of a remote
    presentation group id, which is an opaque string rather than a
    `GxserverProjectId` the compiler can vouch for.
    */
    const projectsById = new Map<string, GxserverPresentationSnapshot['projects'][number]>(
      presentation.projects.map((project) => [project.projectId, project])
    );
    const orderedGroups = orderGpuiRemotePresentationGroups(
      presentation.groups,
      presentation.workspaceGroups?.projectOrder ?? remoteGroupOrderByMachineId?.get(machine.id)
    );
    const orderIndexByProjectId = new Map(orderedGroups.map((group, index) => [group.projectId, index]));
    const hiddenProjectIds = new Set(
      presentation.projects.flatMap((project) =>
        isGpuiRemoteProjectClosedToRecent(machine.id, project.projectId, remoteRecentProjectsByMachineId)
          ? [project.projectId]
          : []
      )
    );
    const chatProjectIds = new Set(
      presentation.projects.flatMap((project) =>
        isGpuiPresentationChatProjectPath(project.path) ? [project.projectId] : []
      )
    );
    const activeRemoteGroup = activeGroupId ? parseGpuiRemotePresentationGroupId(activeGroupId) : undefined;
    const focusedRemoteSession = focusedSessionId ? parseGpuiRemotePresentationSessionId(focusedSessionId) : undefined;
    const activeProjectId =
      activeRemoteGroup?.machineId === machine.id && activeRemoteGroup.projectId !== GPUI_GXSERVER_CHATS_GROUP_ID
        ? activeRemoteGroup.projectId
        : focusedRemoteSession?.machineId === machine.id
          ? focusedRemoteSession.projectId
          : undefined;
    const focusedRawSessionId =
      focusedRemoteSession?.machineId === machine.id ? focusedRemoteSession.sessionId : undefined;
    const visibleRawSessionIds = new Set(
      [...(visibleSessionIds ?? [])].flatMap((sessionId) => {
        const reference = parseGpuiRemotePresentationSessionId(sessionId);
        return reference?.machineId === machine.id ? [reference.sessionId] : [];
      })
    );
    const groups = createGxserverPresentationSidebarGroups({
      activeProjectId,
      chatProjectIds,
      chatsGroupId: createGpuiRemotePresentationGroupId(machine.id, GPUI_GXSERVER_CHATS_GROUP_ID),
      createProjectGroupId: (projectId) => createGpuiRemotePresentationGroupId(machine.id, projectId),
      createProjectSessionId: (projectId, sessionId) =>
        createGpuiRemotePresentationSessionId(machine.id, projectId, sessionId),
      focusedSessionId: focusedRawSessionId,
      hiddenProjectIds,
      presentation,
      projectOverlays: presentation.projects.map((project) => {
        const worktree = normalizeGpuiSidebarWorktreeMetadata(project.worktree);
        return {
          editor: {
            diffStats: createDefaultSidebarProjectDiffStats(),
            isOpen: false,
            isSleeping: false,
            projectId: createGpuiRemotePresentationProjectId(machine.id, project.projectId),
            status: 'idle' as const,
          },
          orderIndex: orderIndexByProjectId.get(project.projectId),
          path: project.path ?? '',
          projectId: project.projectId,
          theme: resolveSidebarTheme(settings.sidebarTheme, 'dark'),
          ...(worktree ? { worktree } : {}),
        };
      }),
      resolveAgentIcon,
      resolveCloseAfterDone: resolveCloseAfterDone
        ? (projectId, sessionId) => resolveCloseAfterDone(machine.id, projectId, sessionId)
        : undefined,
      resolveSessionRoutingId: (projectId, sessionId) =>
        createGpuiRemotePresentationSessionRoutingId(machine.id, projectId, sessionId),
      visibleSessionIds: visibleRawSessionIds,
    });
    return groups.map((group) => {
      const reference = parseGpuiRemotePresentationGroupId(group.groupId);
      const project =
        reference && reference.projectId !== GPUI_GXSERVER_CHATS_GROUP_ID
          ? projectsById.get(reference.projectId)
          : undefined;
      return {
        ...group,
        canCreateSessionGroup: project !== undefined,
        projectContext: group.projectContext
          ? {
              ...group.projectContext,
              canRemoveProject: true,
              path: project?.path ?? group.projectContext.path,
            }
          : undefined,
        remoteMachineContext: {
          machineId: machine.id,
          machineName: machine.name,
          ...(project ? { projectId: project.projectId } : {}),
        },
        sessions: group.sessions.map((session) => ({
          ...session,
          canPopOutPane:
            session.sessionKind === 'terminal' &&
            Boolean(session.agentIcon) &&
            session.isSleeping !== true &&
            session.lifecycleState !== 'sleeping',
          canScheduleDelayedSend: session.sessionKind === 'terminal',
          canToggleCloseAfterDone: session.sessionKind === 'terminal',
          /*
          CDXC:GPUIRemoteVisibleFallback 2026-08-15:
          Mirror the local-group override in createSidebarGroups: the shared
          projection's first-row visible fallback must not survive into
          remote groups. Whenever a remote project became active, it marked
          the projection's index-0 session visible (fill highlight) on top of
          the actually focused session. Remote visibility is owned by the
          native workspace callback's visibleSessionIds, exactly like local
          terminals. Match the complete machine/project/session-scoped row id
          because gxserver session ids are unique only within a project.
          */
          isVisible: group.isActive === true && visibleSessionIds?.has(session.sessionId) === true,
        })),
      };
    });
  });
}

export function orderGpuiRemotePresentationGroups<Group extends { projectId: string }>(
  groups: readonly Group[],
  storedProjectIdOrder: readonly string[] | undefined
): Group[] {
  /*
  CDXC:RemoteGroupReorder 2026-07-12:
  Apply the machine's gxserver-owned project order as a stable sort. The caller
  supplies the legacy app-local overlay only when an older snapshot has no
  workspaceGroups field. Known ids render in stored order, and new projects
  keep their remote presentation position after them.
  */
  if (!storedProjectIdOrder || storedProjectIdOrder.length === 0) {
    return [...groups];
  }
  const orderIndexByProjectId = new Map(storedProjectIdOrder.map((projectId, index) => [projectId, index]));
  const ordered = groups.filter((group) => orderIndexByProjectId.has(group.projectId));
  const unordered = groups.filter((group) => !orderIndexByProjectId.has(group.projectId));
  ordered.sort(
    (left, right) => orderIndexByProjectId.get(left.projectId)! - orderIndexByProjectId.get(right.projectId)!
  );
  return [...ordered, ...unordered];
}

export function isGpuiRemoteProjectClosedToRecent(
  machineId: string,
  projectId: string,
  recentProjectsByMachineId: ReadonlyMap<string, readonly GxserverRecentProjectDomainState[]> | undefined
): boolean {
  /*
  CDXC:GPUIRemoteProjects 2026-06-27-19:37:
  Connected remote presentation projects render under their saved-machine sections, while client-parked remote projects render only as machine-scoped rows in Recent Projects. Filter the remote machine projection with GPUI's app-local recent list instead of mutating the remote gxserver project state.
  */
  return (recentProjectsByMachineId?.get(machineId) ?? []).some((project) => project.projectId === projectId);
}

export function compareGpuiRemoteAttachCandidateSessions(
  left: GxserverPresentationSession,
  right: GxserverPresentationSession
): number {
  const score = (session: GxserverPresentationSession): number => {
    let value = 0;
    if (session.lifecycleState === 'running') {
      value += 100;
    }
    if (session.activity === 'attention') {
      value += 40;
    } else if (session.activity === 'working') {
      value += 30;
    }
    if (session.isPinned) {
      value += 10;
    }
    if (session.isFavorite) {
      value += 5;
    }
    return value;
  };
  const scoreDelta = score(right) - score(left);
  if (scoreDelta !== 0) {
    return scoreDelta;
  }
  const rightTime = Date.parse(right.lastActiveAt ?? right.updatedAt ?? right.createdAt);
  const leftTime = Date.parse(left.lastActiveAt ?? left.updatedAt ?? left.createdAt);
  return (Number.isFinite(rightTime) ? rightTime : 0) - (Number.isFinite(leftTime) ? leftTime : 0);
}

export function createGpuiRemotePresentationGroupId(machineId: string, projectId: string): string {
  return `remote:${machineId}:group:${projectId}`;
}

export function parseGpuiRemotePresentationGroupId(
  groupId: string
): { machineId: string; projectId: string } | undefined {
  const match = /^remote:([^:]+):group:(.+)$/u.exec(groupId);
  if (!match) {
    return undefined;
  }
  return { machineId: match[1]!, projectId: match[2]! };
}

export function createGpuiRemotePresentationProjectId(machineId: string, projectId: string): string {
  return createRemoteProjectId({ machineId, projectId });
}

export function parseGpuiRemotePresentationProjectId(
  projectId: string
): { machineId: string; projectId: string } | undefined {
  return parseRemoteProjectId(projectId);
}

export function createGpuiRemotePresentationSessionId(machineId: string, projectId: string, sessionId: string): string {
  return createRemoteTerminalSessionId({ machineId, projectId, sessionId });
}

export function parseGpuiRemotePresentationSessionId(
  sessionId: string
): { machineId: string; projectId: string; sessionId: string } | undefined {
  return parseRemoteTerminalSessionId(sessionId);
}

export function createGpuiRemotePresentationSessionRoutingId(
  machineId: string,
  projectId: string,
  sessionId: string
): string {
  return `${machineId}:${projectId}:${sessionId}`;
}

export function isPresentationSnapshot(value: unknown): value is GxserverPresentationSnapshot {
  return (
    Boolean(value) &&
    typeof value === 'object' &&
    Array.isArray((value as GxserverPresentationSnapshot).groups) &&
    Array.isArray((value as GxserverPresentationSnapshot).projects) &&
    Array.isArray((value as GxserverPresentationSnapshot).sessions) &&
    typeof (value as GxserverPresentationSnapshot).revision === 'number'
  );
}

export function isPresentationDelta(value: unknown): value is GxserverPresentationDelta {
  return Boolean(value) && typeof value === 'object' && typeof (value as { type?: unknown }).type === 'string';
}

export function isSidebarProjectCollectionsState(value: unknown): value is GxserverSidebarProjectCollectionsState {
  return (
    Boolean(value) &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    typeof (value as GxserverSidebarProjectCollectionsState).collections === 'object' &&
    !Array.isArray((value as GxserverSidebarProjectCollectionsState).collections) &&
    Array.isArray((value as GxserverSidebarProjectCollectionsState).order) &&
    typeof (value as GxserverSidebarProjectCollectionsState).nextCollectionNumber === 'number'
  );
}

export function isSidebarSpacesState(value: unknown): value is GxserverSidebarSpacesState {
  return (
    Boolean(value) &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    typeof (value as GxserverSidebarSpacesState).spaces === 'object' &&
    !Array.isArray((value as GxserverSidebarSpacesState).spaces) &&
    Array.isArray((value as GxserverSidebarSpacesState).order)
  );
}

export function isGpuiSessionChatEventMessage(
  value: Record<string, unknown>
): value is Record<string, unknown> & GxserverSessionChatEvent {
  /*
  CDXC:SessionChatCore 2026-07-31:
  Shape validator for the four sessionChat* event frames, matching the
  presentation-frame validator pattern: identity + epoch/seq cursors must be
  present before a handler sees the frame. Message-array payloads are trusted
  from the authenticated local socket like presentation snapshots are.
  */
  if (
    typeof value.projectId !== 'string' ||
    value.projectId.length === 0 ||
    typeof value.sessionId !== 'string' ||
    value.sessionId.length === 0 ||
    typeof value.epoch !== 'number' ||
    typeof value.seq !== 'number'
  ) {
    return false;
  }
  if (
    (value.type === 'sessionChatSnapshot' ||
      value.type === 'sessionChatAppended' ||
      value.type === 'sessionChatReplaced') &&
    !Array.isArray(value.messages)
  ) {
    return false;
  }
  if (value.type === 'sessionChatState' && typeof value.status !== 'string') {
    return false;
  }
  return true;
}

export function normalizeGpuiSidebarRemoteEvent(value: unknown): GpuiSidebarRemoteEvent | undefined {
  const event = parseObject(value);
  if (!event || typeof event.type !== 'string') {
    return undefined;
  }
  if (event.type === 'remoteMachineStatus') {
    const machineId = normalizeNonEmptyString(event.machineId);
    const state = event.state;
    if (!machineId || !GPUI_REMOTE_MACHINE_STATUS_STATES.has(state as string)) {
      return undefined;
    }
    const message = normalizeNonEmptyString(event.message)?.slice(0, GPUI_REMOTE_MACHINE_STATUS_MESSAGE_MAX_CHARS);
    return {
      machineId,
      ...(message ? { message } : {}),
      state: state as SidebarRemoteMachineStatusMessage['state'],
      type: 'remoteMachineStatus',
    };
  }
  if (event.type === 'remoteGxserverResponse') {
    const remoteMachineId = normalizeNonEmptyString(event.remoteMachineId);
    const requestId = normalizeNonEmptyString(event.requestId);
    if (!remoteMachineId || !requestId || typeof event.ok !== 'boolean') {
      return undefined;
    }
    return {
      error: normalizeNonEmptyString(event.error),
      ok: event.ok,
      remoteMachineId,
      requestId,
      result: event.result,
      type: 'remoteGxserverResponse',
    };
  }
  if (event.type !== 'remoteGxserverPresentation') {
    return undefined;
  }
  const remoteMachineId = normalizeNonEmptyString(event.remoteMachineId);
  const payload = parseObject(event.payload);
  if (!remoteMachineId || !payload || typeof payload.type !== 'string') {
    return undefined;
  }
  if (payload.type === 'presentationSnapshot' && isPresentationSnapshot(payload.snapshot)) {
    return {
      payload: {
        snapshot: payload.snapshot,
        type: 'presentationSnapshot',
      },
      remoteMachineId,
      type: 'remoteGxserverPresentation',
    };
  }
  if (
    payload.type === 'presentationDelta' &&
    isPresentationDelta(payload.delta) &&
    typeof payload.revision === 'number'
  ) {
    return {
      payload: {
        delta: payload.delta,
        revision: payload.revision,
        type: 'presentationDelta',
      },
      remoteMachineId,
      type: 'remoteGxserverPresentation',
    };
  }
  if (
    payload.type === 'sidebarProjectCollectionsChanged' &&
    isSidebarProjectCollectionsState(payload.sidebarProjectCollections) &&
    typeof payload.revision === 'number'
  ) {
    return {
      payload: {
        revision: payload.revision,
        sidebarProjectCollections: payload.sidebarProjectCollections,
        type: 'sidebarProjectCollectionsChanged',
      },
      remoteMachineId,
      type: 'remoteGxserverPresentation',
    };
  }
  if (
    payload.type === 'sidebarSpacesChanged' &&
    isSidebarSpacesState(payload.sidebarSpaces) &&
    typeof payload.revision === 'number'
  ) {
    return {
      payload: {
        revision: payload.revision,
        sidebarSpaces: payload.sidebarSpaces,
        type: 'sidebarSpacesChanged',
      },
      remoteMachineId,
      type: 'remoteGxserverPresentation',
    };
  }
  if (
    payload.type === 'workspaceGroupsChanged' &&
    isWorkspaceSessionGroupsState(payload.groups) &&
    typeof payload.revision === 'number'
  ) {
    return {
      payload: {
        groups: payload.groups,
        revision: payload.revision,
        type: 'workspaceGroupsChanged',
      },
      remoteMachineId,
      type: 'remoteGxserverPresentation',
    };
  }
  return undefined;
}

export function isWorkspaceSessionGroupsState(value: unknown): value is GxserverWorkspaceSessionGroupsState {
  return (
    Boolean(value) &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    Array.isArray((value as GxserverWorkspaceSessionGroupsState).projectOrder) &&
    typeof (value as GxserverWorkspaceSessionGroupsState).projects === 'object' &&
    !Array.isArray((value as GxserverWorkspaceSessionGroupsState).projects)
  );
}
