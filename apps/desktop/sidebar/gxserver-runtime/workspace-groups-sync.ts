/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  createGpuiWorkspaceSessionSubgroup,
  createGpuiWorkspaceSessionSubgroupId,
  findGpuiWorkspaceSessionSubgroupForSession,
  getGpuiWorkspaceSessionSubgroups,
  isEmptyGpuiWorkspaceSessionGroupsState,
  moveGpuiWorkspaceSessionToSubgroup,
  parseGpuiWorkspaceSessionGroupsState,
  parseGpuiWorkspaceSessionSubgroupId,
  removeGpuiWorkspaceSessionSubgroup,
  renameGpuiWorkspaceSessionSubgroup,
  syncGpuiWorkspaceProjectOrder,
  syncGpuiWorkspaceSessionOrderInSubgroup,
  syncGpuiWorkspaceSessionSubgroupOrder,
  writeStoredGpuiWorkspaceSessionGroupsState,
} from "../workspace-session-groups";
import {
  GPUI_PROJECT_COLLECTIONS_SERVER_SYNC_DELAY_MS,
  GPUI_PROJECT_COLLECTIONS_SERVER_SYNC_RETRY_DELAY_MS,
  GPUI_WORKSPACE_GROUPS_SERVER_SYNC_DELAY_MS,
  GPUI_WORKSPACE_GROUPS_SERVER_SYNC_RETRY_DELAY_MS,
} from "./constants";
import type { GpuiSidebarRuntime } from "./core";
import { createGpuiPresentationProjectProjectionMetadata } from "./helpers/presentation-projection";
import { writeStoredGpuiRemoteGroupOrder } from "./helpers/recent-projects";
import {
  createGpuiRemotePresentationGroupId,
  createGpuiRemotePresentationProjectId,
  createGpuiRemotePresentationSessionId,
  isSidebarProjectCollectionsState,
  parseGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationSessionId,
} from "./helpers/remote-presentation";
import {
  createGxserverPresentationProjectGroupId,
  createGxserverPresentationProjectSessionId,
  parseGxserverPresentationProjectGroupId,
  parseGxserverPresentationProjectSessionId,
} from "@/packages/shared/gxserver-presentation-sidebar-projection";
import type { GxserverSidebarProjectCollectionsState } from "@/packages/shared/gxserver-protocol";
import { orderProjectsWithWorktrees } from "@/packages/shared/project-worktree-order";
import type { SidebarProjectWorktreeMetadata } from "@/packages/shared/session-grid-contract";

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeWorkspaceGroupMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeWorkspaceGroupMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeWorkspaceGroupMethods {
  persistWorkspaceGroups(): void;
  scheduleWorkspaceGroupsServerSync(): void;
  pushWorkspaceGroupsToGxserver(): Promise<void>;
  adoptWorkspaceGroupsFromGxserver(serverState: unknown): void;
  queueSidebarProjectCollectionsServerSync(state: GxserverSidebarProjectCollectionsState): void;
  pushSidebarProjectCollectionsToGxserver(): Promise<void>;
  forwardSidebarProjectCollectionsFromGxserver(state: GxserverSidebarProjectCollectionsState): void;
  forwardRemoteSidebarProjectCollectionsFromGxserver(remoteMachineId: string, state: GxserverSidebarProjectCollectionsState): void;
  updateRemoteSidebarProjectCollections(remoteMachineId: string, state: GxserverSidebarProjectCollectionsState): Promise<void>;
  createWorkspaceGroup(groupId?: string): void;
  createWorkspaceGroupFromSession(sessionId: string): void;
  resolveWorkspaceGroupProjectId(groupId: string | undefined): string | undefined;
  renameWorkspaceGroup(groupId: string, title: string): void;
  closeWorkspaceGroup(groupId: string): Promise<void>;
  moveSessionToWorkspaceGroup(message: {
    groupId: string;
    sessionId: string;
    targetIndex?: number;
  }): void;
  syncWorkspaceGroupOrder(groupIds: readonly string[]): void;
  normalizeWorkspaceProjectOrder(projectIds: readonly string[]): string[];
  syncWorkspaceSubgroupSessionOrder(groupId: string, sessionIds: readonly string[]): void;
  workspaceSubgroupSidebarIdForSession(projectId: string, sessionId: string | undefined): string | undefined;
}

export const gpuiSidebarRuntimeWorkspaceGroupMethods = {

  /*
  CDXC:GPUIWorkspaceGroups 2026-07-02-03:49:
  GPUI sidebar named groups are a client-owned project overlay until gxserver exposes durable grouped workspace state.
  Route only local project/session ids through create, rename, close, move, and reorder operations; remote groups stay out of this path and localStorage mirrors macOS grouped workspace semantics.
  */
  persistWorkspaceGroups(this: GpuiSidebarRuntime): void {
    writeStoredGpuiWorkspaceSessionGroupsState(this.workspaceGroups);
    this.scheduleWorkspaceGroupsServerSync();
  },

  /*
  CDXC:WorkspaceSessionGroups 2026-07-12-00:00:
  gxserver now keeps a durable copy of this overlay so iOS/Android render the
  same named groups and ordering. localStorage stays the instant-edit source;
  the server copy is a debounced write-through so group editing never waits on
  an RPC. While a push is pending or failed, hydration must not clobber the
  newer local state.
  */
  scheduleWorkspaceGroupsServerSync(this: GpuiSidebarRuntime): void {
    this.workspaceGroupsServerSyncPending = true;
    if (this.workspaceGroupsServerSyncTimeoutId !== undefined) {
      window.clearTimeout(this.workspaceGroupsServerSyncTimeoutId);
    }
    this.workspaceGroupsServerSyncTimeoutId = window.setTimeout(() => {
      this.workspaceGroupsServerSyncTimeoutId = undefined;
      void this.pushWorkspaceGroupsToGxserver();
    }, GPUI_WORKSPACE_GROUPS_SERVER_SYNC_DELAY_MS);
  },

  async pushWorkspaceGroupsToGxserver(this: GpuiSidebarRuntime): Promise<void> {
    const client = this.client;
    if (!client) {
      return;
    }
    const pushed = this.workspaceGroups;
    try {
      await client.updateWorkspaceSessionGroups(pushed);
      if (this.workspaceGroups === pushed) {
        this.workspaceGroupsServerSyncPending = false;
      }
    } catch {
      if (
        this.client === client &&
        this.workspaceGroupsServerSyncTimeoutId === undefined &&
        this.workspaceGroupsServerSyncPending
      ) {
        this.workspaceGroupsServerSyncTimeoutId = window.setTimeout(() => {
          this.workspaceGroupsServerSyncTimeoutId = undefined;
          void this.pushWorkspaceGroupsToGxserver();
        }, GPUI_WORKSPACE_GROUPS_SERVER_SYNC_RETRY_DELAY_MS);
      }
    }
  },

  adoptWorkspaceGroupsFromGxserver(this: GpuiSidebarRuntime, serverState: unknown): void {
    if (serverState === undefined || this.workspaceGroupsServerSyncPending) {
      return;
    }
    const parsed = parseGpuiWorkspaceSessionGroupsState(serverState);
    if (isEmptyGpuiWorkspaceSessionGroupsState(parsed)) {
      if (!isEmptyGpuiWorkspaceSessionGroupsState(this.workspaceGroups)) {
        this.scheduleWorkspaceGroupsServerSync();
      }
      return;
    }
    if (JSON.stringify(parsed) === JSON.stringify(this.workspaceGroups)) {
      return;
    }
    this.workspaceGroups = parsed;
    writeStoredGpuiWorkspaceSessionGroupsState(parsed);
  },

  /*
  CDXC:SidebarProjectCollections 2026-07-18-00:00:
  Colored "Group N" project collections mirror the workspace-groups sync shape,
  but SidebarApp owns the localStorage overlay and the editing UI, so this
  runtime only relays: sidebar `updateSidebarProjectCollections` commands are
  debounced into gxserver write-throughs, and server state (startup snapshot,
  live sidebarProjectCollectionsChanged events, update acks) is forwarded back
  to SidebarApp for reconciliation. While a push is pending or failed, server
  forwards are suppressed so older server state cannot clobber newer local
  edits.
  */
  queueSidebarProjectCollectionsServerSync(this: GpuiSidebarRuntime,
    state: GxserverSidebarProjectCollectionsState,
  ): void {
    this.latestSidebarProjectCollectionsUpdate = state;
    this.sidebarProjectCollectionsServerSyncPending = true;
    if (this.sidebarProjectCollectionsServerSyncTimeoutId !== undefined) {
      window.clearTimeout(this.sidebarProjectCollectionsServerSyncTimeoutId);
    }
    this.sidebarProjectCollectionsServerSyncTimeoutId = window.setTimeout(() => {
      this.sidebarProjectCollectionsServerSyncTimeoutId = undefined;
      void this.pushSidebarProjectCollectionsToGxserver();
    }, GPUI_PROJECT_COLLECTIONS_SERVER_SYNC_DELAY_MS);
  },

  async pushSidebarProjectCollectionsToGxserver(this: GpuiSidebarRuntime): Promise<void> {
    const client = this.client;
    const pushed = this.latestSidebarProjectCollectionsUpdate;
    if (!client || !pushed) {
      return;
    }
    try {
      const normalized = await client.updateSidebarProjectCollections(pushed);
      if (this.latestSidebarProjectCollectionsUpdate === pushed) {
        this.sidebarProjectCollectionsServerSyncPending = false;
        if (isSidebarProjectCollectionsState(normalized)) {
          this.forwardSidebarProjectCollectionsFromGxserver(normalized);
        }
      }
    } catch {
      if (
        this.client === client &&
        this.sidebarProjectCollectionsServerSyncTimeoutId === undefined &&
        this.sidebarProjectCollectionsServerSyncPending
      ) {
        this.sidebarProjectCollectionsServerSyncTimeoutId = window.setTimeout(() => {
          this.sidebarProjectCollectionsServerSyncTimeoutId = undefined;
          void this.pushSidebarProjectCollectionsToGxserver();
        }, GPUI_PROJECT_COLLECTIONS_SERVER_SYNC_RETRY_DELAY_MS);
      }
    }
  },

  forwardSidebarProjectCollectionsFromGxserver(this: GpuiSidebarRuntime,
    state: GxserverSidebarProjectCollectionsState,
  ): void {
    if (this.sidebarProjectCollectionsServerSyncPending) {
      return;
    }
    const stateJson = JSON.stringify(state);
    if (stateJson === this.lastForwardedSidebarProjectCollectionsJson) {
      return;
    }
    this.lastForwardedSidebarProjectCollectionsJson = stateJson;
    this.messageSource.postMessage({
      sidebarProjectCollections: state,
      type: "sidebarProjectCollectionsChanged",
    });
  },

  forwardRemoteSidebarProjectCollectionsFromGxserver(this: GpuiSidebarRuntime,
    remoteMachineId: string,
    state: GxserverSidebarProjectCollectionsState,
  ): void {
    const stateJson = JSON.stringify(state);
    if (
      this.lastForwardedRemoteSidebarProjectCollectionsJsonByMachineId.get(remoteMachineId) ===
      stateJson
    ) {
      return;
    }
    this.lastForwardedRemoteSidebarProjectCollectionsJsonByMachineId.set(
      remoteMachineId,
      stateJson,
    );
    this.messageSource.postMessage({
      remoteMachineId,
      sidebarProjectCollections: state,
      type: "sidebarProjectCollectionsChanged",
    });
  },

  async updateRemoteSidebarProjectCollections(this: GpuiSidebarRuntime,
    remoteMachineId: string,
    state: GxserverSidebarProjectCollectionsState,
  ): Promise<void> {
    const response = await this.requestRemoteGxserver<{
      sidebarProjectCollections?: unknown;
    }>(remoteMachineId, "/api/updateSidebarProjectCollections", { state });
    if (!isSidebarProjectCollectionsState(response.sidebarProjectCollections)) {
      throw new Error("Remote gxserver returned invalid project collections.");
    }
    const snapshot = this.remotePresentations.get(remoteMachineId);
    if (snapshot) {
      this.remotePresentations.set(remoteMachineId, {
        ...snapshot,
        sidebarProjectCollections: response.sidebarProjectCollections,
      });
    }
    this.forwardRemoteSidebarProjectCollectionsFromGxserver(
      remoteMachineId,
      response.sidebarProjectCollections,
    );
  },

  createWorkspaceGroup(this: GpuiSidebarRuntime, groupId?: string): void {
    const projectId = this.resolveWorkspaceGroupProjectId(groupId) ?? this.activeProjectId;
    if (!projectId) {
      return;
    }
    const result = createGpuiWorkspaceSessionSubgroup(this.workspaceGroups, projectId);
    if (!result.groupId) {
      this.postSidebarActionToast("info", "Group limit reached for this project.");
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
      this.publishPresentation("patch");
    } else {
      this.publishRemotePresentationPatch();
    }
  },

  createWorkspaceGroupFromSession(this: GpuiSidebarRuntime, sessionId: string): void {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    const reference = remoteSession
      ? {
          projectId: createGpuiRemotePresentationProjectId(
            remoteSession.machineId,
            remoteSession.projectId,
          ),
          sessionId: remoteSession.sessionId,
        }
      : parseGxserverPresentationProjectSessionId(sessionId);
    if (!reference) {
      return;
    }
    const result = createGpuiWorkspaceSessionSubgroup(
      this.workspaceGroups,
      reference.projectId,
      reference.sessionId,
    );
    if (!result.groupId) {
      this.postSidebarActionToast("info", "Group limit reached for this project.");
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
      this.publishPresentation("patch");
    } else {
      this.publishRemotePresentationPatch();
    }
  },

  resolveWorkspaceGroupProjectId(this: GpuiSidebarRuntime, groupId: string | undefined): string | undefined {
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

  renameWorkspaceGroup(this: GpuiSidebarRuntime, groupId: string, title: string): void {
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (!subgroup) {
      return;
    }
    const next = renameGpuiWorkspaceSessionSubgroup(
      this.workspaceGroups,
      subgroup.projectId,
      subgroup.groupId,
      title,
    );
    if (next === this.workspaceGroups) {
      return;
    }
    this.workspaceGroups = next;
    this.persistWorkspaceGroups();
    if (this.presentation) {
      this.publishPresentation("patch");
    } else {
      this.publishRemotePresentationPatch();
    }
  },

  async closeWorkspaceGroup(this: GpuiSidebarRuntime, groupId: string): Promise<void> {
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (!subgroup) {
      return;
    }
    const remoteProject = parseGpuiRemotePresentationProjectId(subgroup.projectId);
    const memberIds = [
      ...(getGpuiWorkspaceSessionSubgroups(this.workspaceGroups, subgroup.projectId).find(
        (group) => group.groupId === subgroup.groupId,
      )?.sessionIds ?? []),
    ];
    await Promise.all(
      memberIds.map((sessionId) =>
        this.transitionSession(
          remoteProject
            ? createGpuiRemotePresentationSessionId(
                remoteProject.machineId,
                remoteProject.projectId,
                sessionId,
              )
            : createGxserverPresentationProjectSessionId(subgroup.projectId, sessionId),
          "close",
        ),
      ),
    );
    this.workspaceGroups = removeGpuiWorkspaceSessionSubgroup(
      this.workspaceGroups,
      subgroup.projectId,
      subgroup.groupId,
    );
    this.persistWorkspaceGroups();
    if (this.activeGroupId === groupId) {
      this.activeGroupId = remoteProject
        ? createGpuiRemotePresentationGroupId(remoteProject.machineId, remoteProject.projectId)
        : createGxserverPresentationProjectGroupId(subgroup.projectId);
    }
    if (this.presentation) {
      this.publishPresentation("patch");
    } else {
      this.publishRemotePresentationPatch();
    }
  },

  moveSessionToWorkspaceGroup(this: GpuiSidebarRuntime, message: {
    groupId: string;
    sessionId: string;
    targetIndex?: number;
  }): void {
    const remoteSession = parseGpuiRemotePresentationSessionId(message.sessionId);
    const reference = remoteSession
      ? {
          projectId: createGpuiRemotePresentationProjectId(
            remoteSession.machineId,
            remoteSession.projectId,
          ),
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
        message.targetIndex,
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
        undefined,
      );
    }
    this.persistWorkspaceGroups();
    if (this.presentation) {
      this.publishPresentation("patch");
    } else {
      this.publishRemotePresentationPatch();
    }
  },

  syncWorkspaceGroupOrder(this: GpuiSidebarRuntime, groupIds: readonly string[]): void {
    const remoteReferences = groupIds.map((groupId) => parseGpuiRemotePresentationGroupId(groupId));
    if (remoteReferences.some(Boolean)) {
      /*
      CDXC:RemoteGroupReorder 2026-07-12:
      A machine-scoped remote group reorder persists as an app-local order
      overlay for that machine's presentation projection. Mixed local/remote or
      cross-machine lists stay rejected.
      */
      const machineId = remoteReferences[0]?.machineId;
      if (!machineId || remoteReferences.some((reference) => reference?.machineId !== machineId)) {
        return;
      }
      this.remoteGroupOrderByMachineId.set(
        machineId,
        remoteReferences.map((reference) => reference!.projectId),
      );
      writeStoredGpuiRemoteGroupOrder(this.remoteGroupOrderByMachineId);
      if (this.presentation) {
        this.publishPresentation("patch");
      } else {
        this.publishRemotePresentationPatch();
      }
      return;
    }
    const before = this.workspaceGroups;
    const projectIds = groupIds
      .map((groupId) => parseGxserverPresentationProjectGroupId(groupId))
      .filter((projectId): projectId is string => Boolean(projectId));
    if (projectIds.length > 0) {
      this.workspaceGroups = syncGpuiWorkspaceProjectOrder(
        this.workspaceGroups,
        this.normalizeWorkspaceProjectOrder(projectIds),
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
      this.workspaceGroups = syncGpuiWorkspaceSessionSubgroupOrder(
        this.workspaceGroups,
        projectId,
        order,
      );
    }
    if (this.workspaceGroups === before) {
      return;
    }
    this.persistWorkspaceGroups();
    this.publishPresentation("patch");
  },

  normalizeWorkspaceProjectOrder(this: GpuiSidebarRuntime, projectIds: readonly string[]): string[] {
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
      })),
    ).map((project) => project.projectId);
  },

  syncWorkspaceSubgroupSessionOrder(this: GpuiSidebarRuntime, groupId: string, sessionIds: readonly string[]): void {
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (!subgroup) {
      return;
    }
    const rawSessionIds = sessionIds
      .map((sessionId) => parseGxserverPresentationProjectSessionId(sessionId))
      .filter(
        (reference): reference is NonNullable<typeof reference> =>
          reference !== undefined && reference.projectId === subgroup.projectId,
      )
      .map((reference) => reference.sessionId);
    const next = syncGpuiWorkspaceSessionOrderInSubgroup(
      this.workspaceGroups,
      subgroup.projectId,
      subgroup.groupId,
      rawSessionIds,
    );
    if (next === this.workspaceGroups) {
      return;
    }
    this.workspaceGroups = next;
    this.persistWorkspaceGroups();
    this.publishPresentation("patch");
  },

  workspaceSubgroupSidebarIdForSession(this: GpuiSidebarRuntime,
    projectId: string,
    sessionId: string | undefined,
  ): string | undefined {
    if (!sessionId) {
      return undefined;
    }
    const subgroup = findGpuiWorkspaceSessionSubgroupForSession(
      this.workspaceGroups,
      projectId,
      sessionId,
    );
    return subgroup ? createGpuiWorkspaceSessionSubgroupId(projectId, subgroup.groupId) : undefined;
  },
};

const gpuiSidebarRuntimeWorkspaceGroupMethodsShapeCheck: GpuiSidebarRuntimeWorkspaceGroupMethods = gpuiSidebarRuntimeWorkspaceGroupMethods;
void gpuiSidebarRuntimeWorkspaceGroupMethodsShapeCheck;
