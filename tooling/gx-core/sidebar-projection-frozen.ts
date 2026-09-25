/**
 * The app runtime's sidebar projection (`gxserver-runtime/sidebar-groups.ts`) and its workspace
 * groups hand-off (`persistWorkspaceGroups`), frozen on 2026-09-25 when the app runtime port's
 * sweep deleted them: the runtime no longer holds the workspace session groups document or a copy
 * of the remote machines' presentations, and nothing drew its projection. The drag, project move
 * and group command gates still compare Rust with the projection the app shipped, so the bodies
 * are copied here VERBATIM from d3644561e and installed on the gates' stand-in runtime through
 * `frozenWorkspaceGroupEditMethods`. Nothing in the product runs this code. Deleted with the
 * runtime in step 3 (docs/2026-09-25/app-runtime-port/PLAN.md).
 */
import {
  createGpuiWorkspaceSessionSubgroupId,
  getGpuiWorkspaceSessionSubgroups,
  parseGpuiWorkspaceSessionSubgroupId,
  pruneGpuiWorkspaceSessionSubgroups,
} from './workspace-session-groups-frozen';
import {
  GPUI_GXSERVER_CHATS_GROUP_ID,
  GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE,
  GPUI_QUICK_AUTOMATIONS_PROJECT_ID,
  GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID,
  GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_TYPE,
  GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_VERSION,
} from '@/apps/desktop/sidebar/gxserver-runtime/constants';
import {
  createEmptyGpuiAppUserData,
  createGpuiSidebarSettings,
} from '@/apps/desktop/sidebar/gxserver-runtime/helpers/bootstrap';
import { relayoutGpuiSidebarSessions } from '@/apps/desktop/sidebar/gxserver-runtime/helpers/browser-tabs';
import {
  createGpuiGxserverUnavailableSidebarGroups,
  createGpuiPresentationProjectProjectionMetadata,
  createGpuiSidebarSessionRoutingId,
  resolveGpuiSidebarAgentIcon,
} from '@/apps/desktop/sidebar/gxserver-runtime/helpers/presentation-projection';
import {
  createGpuiRemotePresentationProjectId,
  createGpuiRemotePresentationSessionId,
  createGpuiRemotePresentationSidebarGroups,
  isCustomSessionTagsState,
  isSidebarProjectCollectionsState,
  isSidebarSpacesState,
  parseGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationSessionId,
} from '@/apps/desktop/sidebar/gxserver-runtime/helpers/remote-presentation';
import { boundedGpuiActiveWorkspaceTabSessionTitle } from '@/apps/desktop/sidebar/gxserver-runtime/helpers/status-indicators';
import type {
  GpuiActiveWorkspaceTabSessionPayload,
  GpuiPresentationProjectProjectionMetadata,
  GpuiSidebarRuntimeSnapshotKind,
} from '@/apps/desktop/sidebar/gxserver-runtime/types-and-protocol';
import {
  createGxserverPresentationProjectGroupId,
  createGxserverPresentationProjectSessionId,
  createGxserverPresentationSidebarGroup,
  createGxserverPresentationSidebarGroups,
  createGxserverPresentationSidebarSessionKey,
  parseGxserverPresentationProjectGroupId,
  parseGxserverPresentationProjectSessionId,
  visibleCountForGxserverPresentationSidebarSessions,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type {
  GxserverPresentationDelta,
  GxserverPresentationSession,
  GxserverPresentationSnapshot,
} from '@/packages/shared/gxserver-protocol';
import { createDefaultSidebarProjectDiffStats } from '@/packages/shared/project-diff-stats';
import type { SidebarSessionGroup, SidebarSessionItem } from '@/packages/shared/session-grid-contract';
import { DEFAULT_TERMINAL_SESSION_TITLE, GRID_COLUMN_COUNT } from '@/packages/shared/session-grid-contract';
import { postGpuiSidebarRuntimeFactsRows } from '@/apps/desktop/sidebar/gxserver-runtime/sidebar-runtime-facts';

import { reorderPresentationProjectSessions } from '@/packages/shared/gxserver-presentation-cache';
import type { GxserverProjectId, GxserverSessionId } from '@/packages/shared/gxserver-protocol';

type GpuiSidebarRuntime = any;

export const frozenSidebarProjectionMethods = {
  /**
   * `sessions-and-focus.ts:syncSessionOrder`, deleted with the runtime's focus in d3644561e: the
   * project row's order write the frozen `syncSessionOrder` arm ends in. Verbatim from d3644561e^.
   */
  async syncSessionOrder(this: GpuiSidebarRuntime, groupId: string, sessionIds: readonly string[]): Promise<void> {
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (!projectId || !this.client || !this.presentation) {
      return;
    }
    const gxserverSessionIds = sessionIds.flatMap((sessionId) => {
      const reference = parseGxserverPresentationProjectSessionId(sessionId);
      return reference?.projectId === projectId ? [reference.sessionId] : [];
    });
    if (gxserverSessionIds.length === 0) {
      return;
    }
    this.presentation = reorderPresentationProjectSessions(
      this.presentation,
      projectId as GxserverProjectId,
      gxserverSessionIds as GxserverSessionId[]
    );
    this.publishPresentation('patch');
    await this.client.rpc('/api/updateSessionOrder', {
      projectId,
      sessionIds: gxserverSessionIds,
    });
  },

  /** `workspace-groups-sync.ts:persistWorkspaceGroups`: the hand-off the frozen edits end in. */
  persistWorkspaceGroups(this: GpuiSidebarRuntime): void {
    window.webkit?.messageHandlers?.ghostexNativeHost?.postMessage({
      state: this.workspaceGroups,
      type: 'persistWorkspaceGroups',
    });
  },

  publishPresentation(this: GpuiSidebarRuntime, kind: GpuiSidebarRuntimeSnapshotKind): void {
    const presentation = this.presentation;
    if (!presentation) {
      this.publishUnavailable('presentation-missing');
      return;
    }

    const groups = this.createSidebarGroups(presentation);
    this.hasHydrated = true;
    this.latestGroups = groups;
    postGpuiSidebarRuntimeFactsRows(this);
  },

  publishUnavailable(this: GpuiSidebarRuntime, _reason: string): void {
    this.presentation = undefined;
    this.appUserData = createEmptyGpuiAppUserData();
    this.domainProjects = [];
    this.recentProjects = [];
    this.latestGroups = [...createGpuiGxserverUnavailableSidebarGroups(), ...this.createRemoteSidebarGroups()];
    this.hasHydrated = true;
    postGpuiSidebarRuntimeFactsRows(this);
  },

  publishRemotePresentationPatch(this: GpuiSidebarRuntime): void {
    for (const [machineId, snapshot] of this.remotePresentations) {
      if (isSidebarProjectCollectionsState(snapshot.sidebarProjectCollections)) {
        this.forwardRemoteSidebarProjectCollectionsFromGxserver(machineId, snapshot.sidebarProjectCollections);
      }
      if (isSidebarSpacesState(snapshot.sidebarSpaces)) {
        this.forwardRemoteSidebarSpacesFromGxserver(machineId, snapshot.sidebarSpaces);
      }
      if (isCustomSessionTagsState(snapshot.customSessionTags)) {
        this.forwardRemoteCustomSessionTagsFromGxserver(machineId, snapshot.customSessionTags);
      }
    }
    const groups = this.presentation
      ? this.createSidebarGroups(this.presentation)
      : [...createGpuiGxserverUnavailableSidebarGroups(), ...this.createRemoteSidebarGroups()];
    this.hasHydrated = true;
    this.latestGroups = groups;
    postGpuiSidebarRuntimeFactsRows(this);
  },

  applyDomainProjectDelta(this: GpuiSidebarRuntime, delta: GxserverPresentationDelta): void {
    if ('domainProject' in delta && delta.domainProject) {
      const nextProject = delta.domainProject;
      const existingIndex = this.domainProjects.findIndex((project) => project.projectId === nextProject.projectId);
      this.domainProjects =
        existingIndex >= 0
          ? this.domainProjects.map((project, index) => (index === existingIndex ? nextProject : project))
          : [...this.domainProjects, nextProject];
      if (
        nextProject.isRecentProject === true ||
        this.recentProjects.some((project) => project.projectId === nextProject.projectId)
      ) {
        this.refreshRecentProjectsFromClient();
      }
      return;
    }
    if (delta.type === 'projectRemoved') {
      this.domainProjects = this.domainProjects.filter((project) => project.projectId !== delta.projectId);
      this.refreshRecentProjectsFromClient();
    }
  },

  refreshRecentProjectsFromClient(this: GpuiSidebarRuntime): void {
    const client = this.client;
    if (!client) {
      return;
    }
    void client
      .fetchRecentProjects()
      .then((recentProjects) => {
        if (this.client !== client) {
          return;
        }
        this.recentProjects = [...recentProjects];
        if (this.presentation) {
          this.publishPresentation('patch');
        }
      })
      .catch(() => undefined);
  },

  createSidebarGroups(this: GpuiSidebarRuntime, presentation: GxserverPresentationSnapshot): SidebarSessionGroup[] {
    this.pruneWorkspaceGroupAssignments(presentation);
    const projectProjection = createGpuiPresentationProjectProjectionMetadata({
      domainProjects: this.domainProjects,
      presentation,
      recentProjects: this.recentProjects,
      projectOrder: this.workspaceGroups.projectOrder,
    });
    const subgroupHiddenSessionKeys = this.collectWorkspaceSubgroupSessionKeys(presentation);
    const hiddenSessionKeys =
      subgroupHiddenSessionKeys.size > 0
        ? new Set([...this.localFirstHiddenPresentationSessionKeys, ...subgroupHiddenSessionKeys])
        : this.localFirstHiddenPresentationSessionKeys;
    /*
    CDXC:FocusRouting 2026-09-25 WHY:
    This runtime owns no focus any more (the Rust store does, apps/desktop/src/app/gx_store/focus_publish.rs), so its projection carries no active group, focused row or visible fill; nothing it builds is drawn.
    */
    const projectGroups = createGxserverPresentationSidebarGroups({
      activeProjectId: undefined,
      chatProjectIds: projectProjection.chatProjectIds,
      focusedSessionId: undefined,
      hiddenProjectIds: projectProjection.hiddenProjectIds,
      hiddenSessionKeys,
      presentation,
      projectOverlays: projectProjection.projectOverlays,
      resolveAgentIcon: resolveGpuiSidebarAgentIcon,
      resolveCloseAfterDone: (projectId, sessionId) =>
        this.getCloseAfterDoneProjection(createGxserverPresentationProjectSessionId(projectId, sessionId)),
      resolveSessionRoutingId: createGpuiSidebarSessionRoutingId,
      visibleSessionIds: new Set<string>(),
    });
    const groups = this.spliceWorkspaceSubgroups(projectGroups, presentation, projectProjection);

    const localGroups = groups.map((group) => ({
      ...group,
      isActive: false,
      sessions: group.sessions.map((session) => ({
        ...session,
        isFocused: false,
        isVisible: false,
      })),
    }));
    return [...localGroups, ...this.createRemoteSidebarGroups()];
  },

  /*
  CDXC:Sessions 2026-09-21 WHY:
  This prune no longer persists. It ran on EVERY build and wrote whatever this page held, so in the
  window between an edit made in the app and the daemon echoing it back here, a session vanishing
  was enough to write this page's older document over the key and push it, undoing the user's move
  (declared difference 29). The app prunes the document it holds and hands the result back
  (apps/desktop/src/app/gx_store/workspace_groups.rs); this keeps pruning its own copy so the
  projection built in this very frame lists no member whose session is gone.
  */
  pruneWorkspaceGroupAssignments(this: GpuiSidebarRuntime, presentation: GxserverPresentationSnapshot): void {
    let next = this.workspaceGroups;
    for (const project of presentation.projects) {
      if (!next.projects[project.projectId]) {
        continue;
      }
      const existingSessionIds = new Set(
        presentation.sessions
          .filter((session) => session.projectId === project.projectId)
          .map((session) => session.sessionId)
      );
      next = pruneGpuiWorkspaceSessionSubgroups(next, project.projectId, existingSessionIds);
    }
    if (next !== this.workspaceGroups) {
      this.workspaceGroups = next;
    }
  },

  pruneRemoteWorkspaceGroupAssignments(
    this: GpuiSidebarRuntime,
    machineId: string,
    snapshot: GxserverPresentationSnapshot
  ): void {
    let next = this.workspaceGroups;
    for (const project of snapshot.projects) {
      const scopedProjectId = createGpuiRemotePresentationProjectId(machineId, project.projectId);
      if (!next.projects[scopedProjectId]) {
        continue;
      }
      const existingSessionIds = new Set(
        snapshot.sessions
          .filter((session) => session.projectId === project.projectId)
          .map((session) => session.sessionId)
      );
      next = pruneGpuiWorkspaceSessionSubgroups(next, scopedProjectId, existingSessionIds);
    }
    if (next !== this.workspaceGroups) {
      this.workspaceGroups = next;
    }
  },

  collectWorkspaceSubgroupSessionKeys(
    this: GpuiSidebarRuntime,
    presentation: GxserverPresentationSnapshot
  ): Set<string> {
    const keys = new Set<string>();
    for (const project of presentation.projects) {
      for (const subgroup of getGpuiWorkspaceSessionSubgroups(this.workspaceGroups, project.projectId)) {
        for (const sessionId of subgroup.sessionIds) {
          keys.add(createGxserverPresentationSidebarSessionKey(project.projectId, sessionId));
        }
      }
    }
    return keys;
  },

  spliceWorkspaceSubgroups(
    this: GpuiSidebarRuntime,
    groups: SidebarSessionGroup[],
    presentation: GxserverPresentationSnapshot,
    projectProjection: GpuiPresentationProjectProjectionMetadata
  ): SidebarSessionGroup[] {
    /*
    Keyed by plain string: the lookup key is decoded out of a presentation group
    id, which is an opaque string rather than a `GxserverProjectId` the compiler
    can vouch for.
    */
    const projectsById = new Map<string, GxserverPresentationSnapshot['projects'][number]>(
      presentation.projects.map((project) => [project.projectId, project])
    );
    const sessionsByProject = new Map<string, Map<string, GxserverPresentationSession>>();
    for (const session of presentation.sessions) {
      const byId = sessionsByProject.get(session.projectId) ?? new Map();
      byId.set(session.sessionId, session);
      sessionsByProject.set(session.projectId, byId);
    }
    const result: SidebarSessionGroup[] = [];
    for (const group of groups) {
      const projectId = parseGxserverPresentationProjectGroupId(group.groupId);
      if (!projectId || projectProjection.chatProjectIds.has(projectId)) {
        result.push(group);
        continue;
      }
      result.push({ ...group, canCreateSessionGroup: true });
      const subgroups = getGpuiWorkspaceSessionSubgroups(this.workspaceGroups, projectId);
      if (subgroups.length === 0) {
        continue;
      }
      const project = projectsById.get(projectId);
      if (!project) {
        continue;
      }
      const rowsById = sessionsByProject.get(projectId) ?? new Map();
      for (const subgroup of subgroups) {
        const memberRows = subgroup.sessionIds
          .map((sessionId) => rowsById.get(sessionId))
          .filter((row): row is GxserverPresentationSession => row !== undefined);
        const subgroupSidebarId = createGpuiWorkspaceSessionSubgroupId(projectId, subgroup.groupId);
        const built = createGxserverPresentationSidebarGroup({
          activeProjectId: undefined,
          canRemoveProject: false,
          createProjectGroupId: () => subgroupSidebarId,
          focusedSessionId: undefined,
          project,
          resolveAgentIcon: resolveGpuiSidebarAgentIcon,
          resolveCloseAfterDone: (resolvedProjectId, sessionId) =>
            this.getCloseAfterDoneProjection(createGxserverPresentationProjectSessionId(resolvedProjectId, sessionId)),
          resolveSessionRoutingId: createGpuiSidebarSessionRoutingId,
          sessions: memberRows,
          visibleSessionIds: new Set<string>(),
        });
        result.push({
          ...built,
          canCreateSessionGroup: true,
          canFocusMode: false,
          groupId: subgroupSidebarId,
          kind: 'workspace',
          projectContext: undefined,
          title: subgroup.title,
        });
      }
    }
    return result;
  },

  createRemoteSidebarGroups(this: GpuiSidebarRuntime): SidebarSessionGroup[] {
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    /*
    CDXC:RemoteMachines 2026-07-12:
    Disconnected machines keep rendering their last-seen presentation as
    stale (faded, non-interactive terminals) instead of disappearing, so the
    user still sees which projects and sessions live on the machine and can
    keep using its local browser tabs. Live presentations refresh the
    client-persisted last-seen copy; machines with only a last-seen copy
    render with `isStale`.
    */
    this.captureRemoteLastSeenPresentations();
    const savedMachineIds = new Set(settings.remoteMachines.map((machine) => machine.id));
    const presentationsByMachineId = new Map(this.remotePresentations);
    const staleMachineIds = new Set<string>();
    for (const [machineId, snapshot] of this.remoteLastSeenPresentations) {
      if (presentationsByMachineId.has(machineId) || !savedMachineIds.has(machineId)) {
        continue;
      }
      presentationsByMachineId.set(machineId, snapshot);
      staleMachineIds.add(machineId);
    }
    const groups = createGpuiRemotePresentationSidebarGroups({
      activeGroupId: undefined,
      focusedSessionId: undefined,
      presentationsByMachineId,
      remoteGroupOrderByMachineId: this.remoteGroupOrderByMachineId,
      remoteRecentProjectsByMachineId: this.remoteRecentProjectsByMachineId,
      resolveAgentIcon: resolveGpuiSidebarAgentIcon,
      resolveCloseAfterDone: (machineId, projectId, sessionId) =>
        this.getCloseAfterDoneProjection(createGpuiRemotePresentationSessionId(machineId, projectId, sessionId)),
      settings,
      visibleSessionIds: new Set<string>(),
    });
    return groups.flatMap((group) => {
      const expanded = this.expandRemoteSidebarGroup(group);
      const machineId = group.remoteMachineContext?.machineId;
      if (!machineId || !staleMachineIds.has(machineId)) {
        return expanded;
      }
      return expanded.map((expandedGroup) => ({
        ...expandedGroup,
        isStale: true,
      }));
    });
  },

  /*
  CDXC:RemoteMachines 2026-09-25 WHY:
  Only this runtime's in-memory copy is kept here: the stored per-machine
  copy has one writer, Rust (apps/desktop/src/app/gx_store/remote_last_seen.rs,
  and remote_last_seen_prune.rs for a machine removed from Settings).
  */
  captureRemoteLastSeenPresentations(this: GpuiSidebarRuntime): void {
    if (this.runtimeSettings?.settings === undefined) return;
    const savedMachineIds = new Set(
      createGpuiSidebarSettings(this.runtimeSettings).remoteMachines.map((machine) => machine.id)
    );
    for (const machineId of this.remoteLastSeenPresentations.keys()) {
      if (!savedMachineIds.has(machineId)) {
        this.remoteLastSeenPresentations.delete(machineId);
      }
    }
    for (const [machineId, snapshot] of this.remotePresentations) {
      if (savedMachineIds.has(machineId)) {
        this.remoteLastSeenPresentations.set(machineId, snapshot);
      }
    }
  },

  /*
  CDXC:RemoteMachines 2026-07-12:
  Remote project groups reuse the local sidebar overlays instead of a reduced
  remote feature set: the client-owned named session groups overlay applies to
  remote projects through their machine-scoped project ids. Machine-scoped
  browser tabs used to splice in as browser rows here; they left the sidebar
  with every other browser tab on 2026-09-20.
  */
  expandRemoteSidebarGroup(this: GpuiSidebarRuntime, group: SidebarSessionGroup): SidebarSessionGroup[] {
    const remoteGroup = parseGpuiRemotePresentationGroupId(group.groupId);
    if (!remoteGroup) {
      return [group];
    }
    const scopedProjectId = createGpuiRemotePresentationProjectId(remoteGroup.machineId, remoteGroup.projectId);
    return this.spliceRemoteWorkspaceSubgroups(group, scopedProjectId);
  },

  spliceRemoteWorkspaceSubgroups(
    this: GpuiSidebarRuntime,
    group: SidebarSessionGroup,
    scopedProjectId: string
  ): SidebarSessionGroup[] {
    const subgroups = getGpuiWorkspaceSessionSubgroups(this.workspaceGroups, scopedProjectId);
    if (subgroups.length === 0) {
      return [group];
    }
    const sessionsByRawId = new Map<string, SidebarSessionItem>();
    for (const session of group.sessions) {
      const reference = parseGpuiRemotePresentationSessionId(session.sessionId);
      if (reference) {
        sessionsByRawId.set(reference.sessionId, session);
      }
    }
    const claimedRawIds = new Set<string>();
    const subgroupGroups = subgroups.map((subgroup) => {
      const members = subgroup.sessionIds.flatMap((rawSessionId) => {
        const session = sessionsByRawId.get(rawSessionId);
        if (!session) {
          return [];
        }
        claimedRawIds.add(rawSessionId);
        return [session];
      });
      const subgroupSidebarId = createGpuiWorkspaceSessionSubgroupId(scopedProjectId, subgroup.groupId);
      const sessions = relayoutGpuiSidebarSessions(members);
      const visibleCount = visibleCountForGxserverPresentationSidebarSessions(sessions);
      return {
        ...group,
        canCreateSessionGroup: true,
        canFocusMode: false,
        groupId: subgroupSidebarId,
        isActive: false,
        kind: 'workspace' as const,
        layoutVisibleCount: visibleCount,
        projectContext: undefined,
        sessions,
        title: subgroup.title,
        visibleCount,
      };
    });
    const remaining = relayoutGpuiSidebarSessions(
      group.sessions.filter((session) => {
        const reference = parseGpuiRemotePresentationSessionId(session.sessionId);
        return !reference || !claimedRawIds.has(reference.sessionId);
      })
    );
    const visibleCount = visibleCountForGxserverPresentationSidebarSessions(remaining);
    return [
      {
        ...group,
        layoutVisibleCount: visibleCount,
        sessions: remaining,
        visibleCount,
      },
      ...subgroupGroups,
    ];
  },
};
