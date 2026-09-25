/**
 * The app runtime's workspace group EDITS, frozen on 2026-09-25, the day F4 of the app runtime
 * port (docs/2026-09-25/app-runtime-port/PLAN.md) moved them to Rust.
 *
 * New Group, Rename, Close Group, the order writes of a drag (a remote row's included) and the
 * project order write (a remote machine's whole-order replacement included) are performed by the
 * Rust store now: packages/gx-core/src/workspace_groups/group_commands.rs,
 * packages/gx-core/src/sidebar_drag/order_write.rs and project_order_write.rs, with their hosts in
 * apps/desktop/src/app/gx_store/. The drag and round-trip gates still compare Rust with what the
 * app shipped, so the bodies are copied here VERBATIM and a gate installs them on its stand-in
 * runtime (`Object.assign(runtime, frozenWorkspaceGroupEditMethods)`), exactly as
 * `project-docs-server-sync-typescript.ts` froze the project documents' trio. Nothing in the
 * product runs this code any more.
 */
import {
  createGpuiWorkspaceSessionSubgroup,
  createGpuiWorkspaceSessionSubgroupId,
  getGpuiWorkspaceSessionSubgroups,
  moveGpuiWorkspaceSessionToSubgroup,
  parseGpuiWorkspaceSessionSubgroupId,
  removeGpuiWorkspaceSessionSubgroup,
  renameGpuiWorkspaceSessionSubgroup,
  syncGpuiWorkspaceProjectOrder,
  syncGpuiWorkspaceSessionOrderInSubgroup,
  syncGpuiWorkspaceSessionSubgroupOrder,
} from './workspace-session-groups-frozen';
import { createGpuiPresentationProjectProjectionMetadata } from '@/apps/desktop/sidebar/gxserver-runtime/helpers/presentation-projection';
import { frozenSidebarProjectionMethods } from './sidebar-projection-frozen';
import {
  createGpuiRemotePresentationGroupId,
  createGpuiRemotePresentationProjectId,
  createGpuiRemotePresentationSessionId,
  isWorkspaceSessionGroupsState,
  parseGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationSessionId,
} from '@/apps/desktop/sidebar/gxserver-runtime/helpers/remote-presentation';
import {
  createGxserverPresentationProjectGroupId,
  createGxserverPresentationProjectSessionId,
  parseGxserverPresentationProjectGroupId,
  parseGxserverPresentationProjectSessionId,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type { GxserverWorkspaceSessionGroupsState } from '@/packages/shared/gxserver-protocol';
import { orderProjectsWithWorktrees } from '@/packages/shared/project-worktree-order';
import type { SidebarProjectWorktreeMetadata } from '@/packages/shared/session-grid-contract';

type Json = any;

export const frozenWorkspaceGroupEditMethods = {
  // The projection and the hand-off these edits end in, frozen with them (sidebar-projection-frozen.ts).
  ...frozenSidebarProjectionMethods,
  async updateRemoteWorkspaceGroups(
    this: Json,
    remoteMachineId: string,
    projectOrder: readonly string[]
  ): Promise<void> {
    const workspaceProjects = this.remotePresentations.get(remoteMachineId)?.workspaceGroups?.projects ?? {};
    const state: GxserverWorkspaceSessionGroupsState = {
      projectOrder: [...projectOrder],
      projects: workspaceProjects,
    };
    const response = await this.requestRemoteGxserver<{ groups?: unknown }>(
      remoteMachineId,
      '/api/updateWorkspaceSessionGroups',
      { state }
    );
    if (!isWorkspaceSessionGroupsState(response.groups)) {
      throw new Error('Remote gxserver returned invalid workspace group order.');
    }
    const snapshot = this.remotePresentations.get(remoteMachineId);
    if (snapshot) {
      this.remotePresentations.set(remoteMachineId, {
        ...snapshot,
        workspaceGroups: response.groups,
      });
      this.publishRemotePresentationPatch();
    }
  },

  createWorkspaceGroup(this: Json, groupId?: string): void {
    const projectId = this.resolveWorkspaceGroupProjectId(groupId) ?? this.activeProjectId;
    if (!projectId) {
      return;
    }
    const result = createGpuiWorkspaceSessionSubgroup(this.workspaceGroups, projectId);
    if (!result.groupId) {
      this.postSidebarActionToast('info', 'Group limit reached for this project.');
      return;
    }
    this.workspaceGroups = result.state;
    this.persistWorkspaceGroups();
    if (!parseGpuiRemotePresentationProjectId(projectId)) {
      this.activeProjectId = projectId;
    }
    this.activeGroupId = createGpuiWorkspaceSessionSubgroupId(projectId, result.groupId);
    this.refreshSidebarHudFromClient();
    if (this.presentation) {
      this.publishPresentation('patch');
    } else {
      this.publishRemotePresentationPatch();
    }
  },

  createWorkspaceGroupFromSession(this: Json, sessionId: string): void {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    const reference = remoteSession
      ? {
          projectId: createGpuiRemotePresentationProjectId(remoteSession.machineId, remoteSession.projectId),
          sessionId: remoteSession.sessionId,
        }
      : parseGxserverPresentationProjectSessionId(sessionId);
    if (!reference) {
      return;
    }
    const result = createGpuiWorkspaceSessionSubgroup(this.workspaceGroups, reference.projectId, reference.sessionId);
    if (!result.groupId) {
      this.postSidebarActionToast('info', 'Group limit reached for this project.');
      return;
    }
    this.workspaceGroups = result.state;
    this.persistWorkspaceGroups();
    if (!remoteSession) {
      this.activeProjectId = reference.projectId;
    }
    this.activeGroupId = createGpuiWorkspaceSessionSubgroupId(reference.projectId, result.groupId);
    this.refreshSidebarHudFromClient();
    if (this.presentation) {
      this.publishPresentation('patch');
    } else {
      this.publishRemotePresentationPatch();
    }
  },

  resolveWorkspaceGroupProjectId(this: Json, groupId: string | undefined): string | undefined {
    if (!groupId) {
      return undefined;
    }
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (subgroup) {
      return subgroup.projectId;
    }
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      return createGpuiRemotePresentationProjectId(remoteGroup.machineId, remoteGroup.projectId);
    }
    return parseGxserverPresentationProjectGroupId(groupId);
  },

  renameWorkspaceGroup(this: Json, groupId: string, title: string): void {
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (!subgroup) {
      return;
    }
    const next = renameGpuiWorkspaceSessionSubgroup(this.workspaceGroups, subgroup.projectId, subgroup.groupId, title);
    if (next === this.workspaceGroups) {
      return;
    }
    this.workspaceGroups = next;
    this.persistWorkspaceGroups();
    if (this.presentation) {
      this.publishPresentation('patch');
    } else {
      this.publishRemotePresentationPatch();
    }
  },

  async closeWorkspaceGroup(this: Json, groupId: string): Promise<void> {
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (!subgroup) {
      return;
    }
    const remoteProject = parseGpuiRemotePresentationProjectId(subgroup.projectId);
    const memberIds = [
      ...(getGpuiWorkspaceSessionSubgroups(this.workspaceGroups, subgroup.projectId).find(
        (group) => group.groupId === subgroup.groupId
      )?.sessionIds ?? []),
    ];
    await Promise.all(
      memberIds.map((sessionId) =>
        this.transitionSession(
          remoteProject
            ? createGpuiRemotePresentationSessionId(remoteProject.machineId, remoteProject.projectId, sessionId)
            : createGxserverPresentationProjectSessionId(subgroup.projectId, sessionId),
          'close'
        )
      )
    );
    this.workspaceGroups = removeGpuiWorkspaceSessionSubgroup(
      this.workspaceGroups,
      subgroup.projectId,
      subgroup.groupId
    );
    this.persistWorkspaceGroups();
    if (this.activeGroupId === groupId) {
      this.activeGroupId = remoteProject
        ? createGpuiRemotePresentationGroupId(remoteProject.machineId, remoteProject.projectId)
        : createGxserverPresentationProjectGroupId(subgroup.projectId);
    }
    if (this.presentation) {
      this.publishPresentation('patch');
    } else {
      this.publishRemotePresentationPatch();
    }
  },

  moveSessionToWorkspaceGroup(
    this: Json,
    message: {
      groupId: string;
      sessionId: string;
      targetIndex?: number;
    }
  ): void {
    const remoteSession = parseGpuiRemotePresentationSessionId(message.sessionId);
    const reference = remoteSession
      ? {
          projectId: createGpuiRemotePresentationProjectId(remoteSession.machineId, remoteSession.projectId),
          sessionId: remoteSession.sessionId,
        }
      : parseGxserverPresentationProjectSessionId(message.sessionId);
    if (!reference) {
      return;
    }
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(message.groupId);
    if (subgroup) {
      if (subgroup.projectId !== reference.projectId) {
        return;
      }
      this.workspaceGroups = moveGpuiWorkspaceSessionToSubgroup(
        this.workspaceGroups,
        reference.projectId,
        reference.sessionId,
        subgroup.groupId,
        message.targetIndex
      );
    } else {
      const remoteGroup = parseGpuiRemotePresentationGroupId(message.groupId);
      const projectId = remoteGroup
        ? createGpuiRemotePresentationProjectId(remoteGroup.machineId, remoteGroup.projectId)
        : parseGxserverPresentationProjectGroupId(message.groupId);
      if (!projectId || projectId !== reference.projectId) {
        return;
      }
      this.workspaceGroups = moveGpuiWorkspaceSessionToSubgroup(
        this.workspaceGroups,
        reference.projectId,
        reference.sessionId,
        undefined
      );
    }
    this.persistWorkspaceGroups();
    if (this.presentation) {
      this.publishPresentation('patch');
    } else {
      this.publishRemotePresentationPatch();
    }
  },

  async syncWorkspaceGroupOrder(this: Json, groupIds: readonly string[]): Promise<void> {
    const remoteReferences = groupIds.map((groupId) => parseGpuiRemotePresentationGroupId(groupId));
    if (remoteReferences.some(Boolean)) {
      const machineId = remoteReferences[0]?.machineId;
      if (!machineId || remoteReferences.some((reference) => reference?.machineId !== machineId)) {
        return;
      }
      await this.updateRemoteWorkspaceGroups(
        machineId,
        remoteReferences.map((reference) => reference!.projectId)
      );
      return;
    }
    const before = this.workspaceGroups;
    const projectIds = groupIds
      .map((groupId) => parseGxserverPresentationProjectGroupId(groupId))
      .filter((projectId): projectId is string => Boolean(projectId));
    if (projectIds.length > 0) {
      this.workspaceGroups = syncGpuiWorkspaceProjectOrder(
        this.workspaceGroups,
        this.normalizeWorkspaceProjectOrder(projectIds)
      );
    }
    const subgroupOrderByProject = new Map<string, string[]>();
    for (const groupId of groupIds) {
      const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
      if (subgroup) {
        const order = subgroupOrderByProject.get(subgroup.projectId) ?? [];
        order.push(subgroup.groupId);
        subgroupOrderByProject.set(subgroup.projectId, order);
      }
    }
    for (const [projectId, order] of subgroupOrderByProject) {
      this.workspaceGroups = syncGpuiWorkspaceSessionSubgroupOrder(this.workspaceGroups, projectId, order);
    }
    if (this.workspaceGroups === before) {
      return;
    }
    this.persistWorkspaceGroups();
    this.publishPresentation('patch');
  },

  normalizeWorkspaceProjectOrder(this: Json, projectIds: readonly string[]): string[] {
    const projectIdSet = new Set(projectIds);
    const worktreeByProjectId = new Map<string, SidebarProjectWorktreeMetadata>();
    for (const group of this.latestGroups) {
      const projectId = parseGxserverPresentationProjectGroupId(group.groupId);
      const worktree = group.projectContext?.worktree;
      if (projectId && projectIdSet.has(projectId) && worktree) {
        worktreeByProjectId.set(projectId, worktree);
      }
    }

    if (this.presentation) {
      const projection = createGpuiPresentationProjectProjectionMetadata({
        domainProjects: this.domainProjects,
        presentation: this.presentation,
        projectOrder: projectIds,
        recentProjects: this.recentProjects,
      });
      for (const overlay of projection.projectOverlays) {
        if (projectIdSet.has(overlay.projectId) && overlay.worktree) {
          worktreeByProjectId.set(overlay.projectId, overlay.worktree);
        }
      }
    }

    return orderProjectsWithWorktrees(
      projectIds.map((projectId) => ({
        projectId,
        worktree: worktreeByProjectId.get(projectId),
      }))
    ).map((project) => project.projectId);
  },

  syncWorkspaceSubgroupSessionOrder(this: Json, groupId: string, sessionIds: readonly string[]): void {
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (!subgroup) {
      return;
    }
    const rawSessionIds = sessionIds
      .map((sessionId) => parseGxserverPresentationProjectSessionId(sessionId))
      .filter(
        (reference): reference is NonNullable<typeof reference> =>
          reference !== undefined && reference.projectId === subgroup.projectId
      )
      .map((reference) => reference.sessionId);
    const next = syncGpuiWorkspaceSessionOrderInSubgroup(
      this.workspaceGroups,
      subgroup.projectId,
      subgroup.groupId,
      rawSessionIds
    );
    if (next === this.workspaceGroups) {
      return;
    }
    this.workspaceGroups = next;
    this.persistWorkspaceGroups();
    this.publishPresentation('patch');
  },
};

/**
 * The `handleSidebarMessage` arms that reached the methods above, frozen with them: the
 * `syncSessionOrder` arm asked the group id first, and the rest were one call each.
 */
export async function frozenHandleWorkspaceGroupMessage(runtime: Json, message: Json): Promise<void> {
  switch (message.type) {
    case 'syncSessionOrder':
      if (parseGpuiWorkspaceSessionSubgroupId(message.groupId)) {
        runtime.syncWorkspaceSubgroupSessionOrder(message.groupId, message.sessionIds);
        return;
      }
      await runtime.syncSessionOrder(message.groupId, message.sessionIds);
      return;
    case 'createGroup':
      runtime.createWorkspaceGroup(message.groupId);
      return;
    case 'createGroupFromSession':
      runtime.createWorkspaceGroupFromSession(message.sessionId);
      return;
    case 'renameGroup':
      runtime.renameWorkspaceGroup(message.groupId, message.title);
      return;
    case 'closeGroup':
      await runtime.closeWorkspaceGroup(message.groupId);
      return;
    case 'moveSessionToGroup':
      runtime.moveSessionToWorkspaceGroup(message);
      return;
    case 'syncGroupOrder':
      await runtime.syncWorkspaceGroupOrder(message.groupIds);
      return;
    default:
      await runtime.handleSidebarMessage(message);
  }
}
