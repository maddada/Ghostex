/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import { createGpuiSidebarActiveProjectContextPayloadFromGroups } from '../active-project-context';
import {
  createGpuiWorkspaceSessionSubgroupId,
  getGpuiWorkspaceSessionSubgroups,
  parseGpuiWorkspaceSessionSubgroupId,
  pruneGpuiWorkspaceSessionSubgroups,
} from '../workspace-session-groups';
import {
  GPUI_GXSERVER_CHATS_GROUP_ID,
  GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE,
  GPUI_QUICK_AUTOMATIONS_PROJECT_ID,
  GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID,
  GPUI_REMOTE_LAST_SEEN_PRESENTATIONS_PERSIST_DELAY_MS,
  GPUI_SIDEBAR_BOOTSTRAP_MAX_ATTEMPTS,
  GPUI_SIDEBAR_BOOTSTRAP_RETRY_DELAY_MS,
  GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_TYPE,
  GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_VERSION,
  GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_TYPE,
  GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_VERSION,
  GPUI_TAB_STRIP_MAX_GLOBAL_ACTIONS,
} from './constants';
import type { GpuiSidebarRuntime } from './core';
import { createEmptyGpuiAppUserData, createGpuiSidebarSettings } from './helpers/bootstrap';
import { gpuiBrowserSidebarSessionId, relayoutGpuiSidebarSessions } from './helpers/browser-tabs';
import { createGpuiSidebarHudState } from './helpers/command-pane';
import {
  createGpuiGxserverUnavailableSidebarGroups,
  createGpuiPresentationProjectProjectionMetadata,
  createGpuiSidebarGroupsPatch,
  createGpuiSidebarSessionRoutingId,
  haveSameSidebarProjectionValue,
  resolveGpuiSidebarAgentIcon,
} from './helpers/presentation-projection';
import { writeStoredGpuiRemoteLastSeenPresentations } from './helpers/recent-projects';
import {
  createGpuiRemotePresentationProjectId,
  createGpuiRemotePresentationSessionId,
  createGpuiRemotePresentationSidebarGroups,
  isSidebarProjectCollectionsState,
  parseGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationSessionId,
} from './helpers/remote-presentation';
import {
  boundedGpuiActiveWorkspaceTabSessionTitle,
  createGpuiPetOverlayStatePayload,
  createGpuiSessionStatusIndicatorCandidatesFromSidebarGroups,
  createGpuiSessionStatusIndicatorsPayload,
} from './helpers/status-indicators';
import type {
  GpuiActiveWorkspaceTabSessionPayload,
  GpuiPresentationProjectProjectionMetadata,
  GpuiSidebarRuntimeSnapshotKind,
} from './types-and-protocol';
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
  GxserverSidebarProjectCollectionsState,
} from '@/packages/shared/gxserver-protocol';
import { createDefaultSidebarProjectDiffStats } from '@/packages/shared/project-diff-stats';
import type {
  SidebarHudState,
  SidebarHydrateMessage,
  SidebarSessionGroup,
  SidebarSessionItem,
} from '@/packages/shared/session-grid-contract';
import { DEFAULT_TERMINAL_SESSION_TITLE, GRID_COLUMN_COUNT } from '@/packages/shared/session-grid-contract';
import { createDefaultSidebarGitState } from '@/packages/shared/sidebar-git';

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeSidebarGroupMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeSidebarGroupMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeSidebarGroupMethods {
  publishPresentation(kind: GpuiSidebarRuntimeSnapshotKind): void;
  postSidebarProjectionPatchMessages(
    previousGroups: readonly SidebarSessionGroup[],
    groups: SidebarSessionGroup[],
    previousHud: SidebarHudState
  ): void;
  publishUnavailable(_reason: string): void;
  publishRemotePresentationPatch(): void;
  applyDomainProjectDelta(delta: GxserverPresentationDelta): void;
  refreshRecentProjectsFromClient(): void;
  refreshSidebarHudFromClient(): void;
  publishHudPatch(): void;
  postActiveProjectContext(attempt?: number): void;
  postGxserverPresentationFocusState(): void;
  activeWorkspaceTabSessionsFromLatestGroups(): GpuiActiveWorkspaceTabSessionPayload[];
  postGpuiGlobalActions(): void;
  postGpuiStatusPetState(): void;
  createHydrateMessage(groups: SidebarSessionGroup[], hud: SidebarHudState): SidebarHydrateMessage;
  remoteSidebarProjectCollectionsByMachineId(): Readonly<Record<string, GxserverSidebarProjectCollectionsState>>;
  createSidebarGroups(presentation: GxserverPresentationSnapshot): SidebarSessionGroup[];
  withQuickAutomationsOverviewGroup(groups: SidebarSessionGroup[]): SidebarSessionGroup[];
  createQuickAutomationsSidebarSession(): SidebarSessionItem;
  quickAutomationsSidebarSessionId(): string;
  isQuickAutomationsSidebarSessionId(sessionId: string): boolean;
  createQuickAutomationsProjectContext(): NonNullable<SidebarSessionGroup['projectContext']>;
  activeProjectContextGroups(): SidebarSessionGroup[];
  overlayProjectDiffStats(groups: SidebarSessionGroup[]): SidebarSessionGroup[];
  pruneWorkspaceGroupAssignments(presentation: GxserverPresentationSnapshot): void;
  pruneRemoteWorkspaceGroupAssignments(machineId: string, snapshot: GxserverPresentationSnapshot): void;
  collectWorkspaceSubgroupSessionKeys(presentation: GxserverPresentationSnapshot): Set<string>;
  spliceWorkspaceSubgroups(
    groups: SidebarSessionGroup[],
    presentation: GxserverPresentationSnapshot,
    projectProjection: GpuiPresentationProjectProjectionMetadata
  ): SidebarSessionGroup[];
  createRemoteSidebarGroups(): SidebarSessionGroup[];
  captureRemoteLastSeenPresentations(): void;
  expandRemoteSidebarGroup(group: SidebarSessionGroup): SidebarSessionGroup[];
  withRemoteBrowserTabSessions(group: SidebarSessionGroup, scopedProjectId: string): SidebarSessionGroup;
  spliceRemoteWorkspaceSubgroups(group: SidebarSessionGroup, scopedProjectId: string): SidebarSessionGroup[];
  ensureActiveProject(
    presentation: GxserverPresentationSnapshot,
    projectProjection: GpuiPresentationProjectProjectionMetadata
  ): void;
}

export const gpuiSidebarRuntimeSidebarGroupMethods = {
  publishPresentation(this: GpuiSidebarRuntime, kind: GpuiSidebarRuntimeSnapshotKind): void {
    const presentation = this.presentation;
    if (!presentation) {
      this.publishUnavailable('presentation-missing');
      return;
    }

    const previousGroups = this.latestGroups;
    const groups = this.createSidebarGroups(presentation);
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-23:24:
    Sidebar session-card wake decisions should use the lifecycle state that was just rendered from gxserver presentation. Cache only bounded local project/session routing ids for sleeping rows before emitting hydrate/patch so a same-tick click cannot miss the sleeping state and fall through to plain focus.
    */
    this.sleepingLocalSidebarSessionIds = new Set(
      groups.flatMap((group) =>
        group.sessions.flatMap((session) => {
          const reference = parseGxserverPresentationProjectSessionId(session.sessionId);
          return reference && (session.lifecycleState === 'sleeping' || session.isSleeping === true)
            ? [createGxserverPresentationProjectSessionId(reference.projectId, reference.sessionId)]
            : [];
        })
      )
    );
    const previousHud = this.latestHud;
    this.latestHud = createGpuiSidebarHudState({
      activeProjectId: this.activeProjectId,
      commandPaneSessions: this.commandPaneSessions,
      focusedSessionId: this.focusedSessionId,
      git: this.gitStateForHud(),
      groups,
      presentation,
      runtimeSettings: this.runtimeSettings,
      domainProjects: this.domainProjects,
      recentProjects: this.recentProjects,
      remoteRecentProjectsByMachineId: this.remoteRecentProjectsByMachineId,
      remotePresentationsByMachineId: this.remotePresentations,
      sidebarHud: this.sidebarHud,
    });

    if (kind === 'hydrate' || !this.hasHydrated) {
      this.messageSource.postMessage(this.createHydrateMessage(groups, this.latestHud));
      this.hasHydrated = true;
    } else {
      this.postSidebarProjectionPatchMessages(previousGroups, groups, previousHud);
    }
    this.latestGroups = groups;
    this.postGpuiStatusPetState();
    this.postActiveProjectContext();
    this.postGxserverPresentationFocusState();
    this.postTitlebarGitMenuState();
    this.refreshGitStateForActiveProjectIfNeeded();
  },

  /*
  CDXC:SidebarDiffStatsChurn 2026-08-16:
  Routine publishes frequently rebuild a projection identical to the last one
  (background pollers, presentation deltas that only touch non-rendered
  state). Sending those anyway made the renderer re-normalize the full tree
  and deep-compare the whole HUD per message. Skip the groups message when the
  diffed patch carries nothing and skip the HUD message when the rebuilt HUD
  is structurally identical to the one already published.
  */
  postSidebarProjectionPatchMessages(
    this: GpuiSidebarRuntime,
    previousGroups: readonly SidebarSessionGroup[],
    groups: SidebarSessionGroup[],
    previousHud: SidebarHudState
  ): void {
    const patch = createGpuiSidebarGroupsPatch(previousGroups, groups);
    const groupOrderChanged =
      patch.groupOrder.length !== previousGroups.length ||
      patch.groupOrder.some((groupId, index) => previousGroups[index]?.groupId !== groupId);
    if (
      patch.groups.length > 0 ||
      patch.removedGroupIds.length > 0 ||
      patch.removedSessionIds.length > 0 ||
      groupOrderChanged
    ) {
      this.messageSource.postMessage({
        groupOrder: patch.groupOrder,
        groups: patch.groups,
        removedGroupIds: patch.removedGroupIds,
        removedSessionIds: patch.removedSessionIds,
        revision: ++this.revision,
        type: 'sidebarGroupsChanged',
      });
    }
    if (!haveSameSidebarProjectionValue(previousHud, this.latestHud)) {
      this.messageSource.postMessage({
        hud: this.latestHud,
        revision: ++this.revision,
        type: 'sidebarHudChanged',
      });
    }
  },

  publishUnavailable(this: GpuiSidebarRuntime, _reason: string): void {
    if (this.presentation) {
      this.syncLocalPresentationAttentionTracking(this.presentation.sessions, []);
    }
    this.presentation = undefined;
    this.appUserData = createEmptyGpuiAppUserData();
    this.domainProjects = [];
    this.dropLocalPresentationSessionFocus();
    this.gitState = createDefaultSidebarGitState();
    this.lastGitRefreshProjectId = undefined;
    /*
    CDXC:SidebarGitMemo 2026-07-29:
    gxserver went away, so nothing memoized about its projects can be trusted
    or republished. Drop both leases and cancel any in-flight GitHub probe so a
    reconnect starts from real probes.
    */
    this.gitStateMemoByProjectId.clear();
    this.gitHubStateMemoByProjectId.clear();
    this.gitRepoProjectIds.clear();
    for (const timeoutId of this.gitHubProbeTimeoutIds) {
      window.clearTimeout(timeoutId);
    }
    this.gitHubProbeTimeoutIds.clear();
    this.pendingGitHubProbeProjectIds.clear();
    this.pendingGitCommitRequests.clear();
    this.recentProjects = [];
    this.sidebarHud = undefined;
    this.latestGroups = this.overlayProjectDiffStats([
      ...createGpuiGxserverUnavailableSidebarGroups(),
      ...this.createRemoteSidebarGroups(),
    ]);
    this.latestHud = createGpuiSidebarHudState({
      activeProjectId: this.activeProjectId,
      commandPaneSessions: this.commandPaneSessions,
      git: this.gitStateForHud(),
      groups: this.latestGroups,
      runtimeSettings: this.runtimeSettings,
      domainProjects: this.domainProjects,
      recentProjects: this.recentProjects,
      remoteRecentProjectsByMachineId: this.remoteRecentProjectsByMachineId,
      remotePresentationsByMachineId: this.remotePresentations,
      sidebarHud: this.sidebarHud,
    });
    this.messageSource.postMessage(this.createHydrateMessage(this.latestGroups, this.latestHud));
    this.hasHydrated = true;
    this.postGpuiStatusPetState();
    this.postActiveProjectContext();
    this.postGxserverPresentationFocusState();
    this.postTitlebarGitMenuState();
  },

  publishRemotePresentationPatch(this: GpuiSidebarRuntime): void {
    for (const [machineId, snapshot] of this.remotePresentations) {
      if (isSidebarProjectCollectionsState(snapshot.sidebarProjectCollections)) {
        this.forwardRemoteSidebarProjectCollectionsFromGxserver(machineId, snapshot.sidebarProjectCollections);
      }
    }
    const previousGroups = this.latestGroups;
    const previousHud = this.latestHud;
    const groups = this.presentation
      ? this.createSidebarGroups(this.presentation)
      : this.overlayProjectDiffStats([
          ...createGpuiGxserverUnavailableSidebarGroups(),
          ...this.createRemoteSidebarGroups(),
        ]);
    this.latestHud = createGpuiSidebarHudState({
      activeProjectId: this.activeProjectId,
      commandPaneSessions: this.commandPaneSessions,
      focusedSessionId: this.focusedSessionId,
      git: this.gitStateForHud(),
      groups,
      presentation: this.presentation,
      runtimeSettings: this.runtimeSettings,
      domainProjects: this.domainProjects,
      recentProjects: this.recentProjects,
      remoteRecentProjectsByMachineId: this.remoteRecentProjectsByMachineId,
      remotePresentationsByMachineId: this.remotePresentations,
      sidebarHud: this.sidebarHud,
    });
    if (!this.hasHydrated) {
      this.messageSource.postMessage(this.createHydrateMessage(groups, this.latestHud));
      this.hasHydrated = true;
    } else {
      this.postSidebarProjectionPatchMessages(previousGroups, groups, previousHud);
    }
    this.latestGroups = groups;
    this.postGpuiStatusPetState();
    this.postActiveProjectContext();
    this.postGxserverPresentationFocusState();
    this.postTitlebarGitMenuState();
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
      this.refreshSidebarHudFromClient();
      return;
    }
    if (delta.type === 'projectRemoved') {
      this.domainProjects = this.domainProjects.filter((project) => project.projectId !== delta.projectId);
      this.refreshRecentProjectsFromClient();
      this.refreshSidebarHudFromClient();
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
          return;
        }
        this.publishHudPatch();
      })
      .catch(() => undefined);
  },

  refreshSidebarHudFromClient(this: GpuiSidebarRuntime): void {
    const client = this.client;
    if (!client) {
      return;
    }
    void client
      .fetchSidebarHud(this.activeProjectId)
      .then((sidebarHud) => {
        if (this.client !== client) {
          return;
        }
        this.sidebarHud = sidebarHud;
        this.publishHudPatch();
      })
      .catch(() => {
        /*
         * CDXC:SidebarHudContract 2026-06-24-20:34:
         * Sidebar HUD projection refresh is best-effort after active-project or
         * project-metadata changes. Failure keeps the previous gxserver
         * projection instead of rebuilding custom launcher/action rows from
         * raw project metadata in the renderer.
         */
      });
  },

  publishHudPatch(this: GpuiSidebarRuntime): void {
    const previousHud = this.latestHud;
    this.latestHud = createGpuiSidebarHudState({
      activeProjectId: this.activeProjectId,
      commandPaneSessions: this.commandPaneSessions,
      focusedSessionId: this.focusedSessionId,
      git: this.gitStateForHud(),
      groups: this.latestGroups,
      presentation: this.presentation,
      runtimeSettings: this.runtimeSettings,
      domainProjects: this.domainProjects,
      recentProjects: this.recentProjects,
      remoteRecentProjectsByMachineId: this.remoteRecentProjectsByMachineId,
      remotePresentationsByMachineId: this.remotePresentations,
      sidebarHud: this.sidebarHud,
    });
    this.postTitlebarGitMenuState();
    if (!this.hasHydrated || haveSameSidebarProjectionValue(previousHud, this.latestHud)) {
      return;
    }
    this.messageSource.postMessage({
      hud: this.latestHud,
      revision: ++this.revision,
      type: 'sidebarHudChanged',
    });
  },

  postActiveProjectContext(this: GpuiSidebarRuntime, attempt = 0): void {
    /*
    CDXC:NavigationHistory 2026-08-19:
    Every path that republishes active-project identity lands here, which makes
    it the one place the trail has to be fed from. The controller collapses an
    unchanged target to a string compare, so this stays free on the hot path.
    */
    this.navigationHistory.recordVisit(this.createNavigationHistoryEntry());

    if (this.activeProjectContextRetryId !== undefined) {
      window.clearTimeout(this.activeProjectContextRetryId);
      this.activeProjectContextRetryId = undefined;
    }

    const postActiveProjectContext = window.ghostexGpui?.postActiveProjectContext;
    if (typeof postActiveProjectContext !== 'function') {
      /*
      CDXC:GPUISidebarGxserverRuntime 2026-06-24-11:00:
      CEF may install the sidebar bridge after the React entrypoint starts. Retry only the bridge send and rebuild the active-project payload from the latest live groups at send time, so startup never replays a stale fixture/workspace payload.
      */
      if (attempt < GPUI_SIDEBAR_BOOTSTRAP_MAX_ATTEMPTS) {
        this.activeProjectContextRetryId = window.setTimeout(() => {
          this.postActiveProjectContext(attempt + 1);
        }, GPUI_SIDEBAR_BOOTSTRAP_RETRY_DELAY_MS);
      }
      return;
    }

    const payload = createGpuiSidebarActiveProjectContextPayloadFromGroups({
      groups: this.activeProjectContextGroups(),
    });
    /*
    CDXC:GPUIAutomateWorkarea 2026-07-04-23:18:
    The active-project helper owns Source/Kanban/Automate/Docs surface identity. Post its payload unchanged so Rust can strictly accept `automateBoardId` beside `kanbanBoardId` before issuing the bundled Automate runtime URL.
    */
    postActiveProjectContext(JSON.stringify(payload));
  },

  postGxserverPresentationFocusState(this: GpuiSidebarRuntime): void {
    const postFocusState = window.ghostexGpui?.postGxserverPresentationFocusState;
    if (typeof postFocusState !== 'function') {
      return;
    }
    const focusedRemoteSession = this.focusedSessionId
      ? parseGpuiRemotePresentationSessionId(this.focusedSessionId)
      : undefined;
    /*
    CDXC:GPUIRemoteWorkspaceProjectKey 2026-07-30:
    Rust treats this snapshot's activeProjectId as the authoritative Agents
    workspace switch target. `this.activeProjectId` stays a local-only concept
    while a remote session or remote group is active, so publishing it here
    yanked the workspace back to the last local project on every routine
    remote presentation patch. Publish the machine-scoped remote project id —
    the same key the active-project context bridge uses — whenever the remote
    machine owns focus. Tab sessions use the same already-projected SidebarApp
    group for local and remote workspaces. Remote rows retain their
    machine-scoped project identity so Rust can reconcile restored attach tabs
    without confusing them with local gxserver sessions.
    */
    const activeGroupRemoteReference = (() => {
      if (!this.activeGroupId) {
        return undefined;
      }
      const remoteGroup = parseGpuiRemotePresentationGroupId(this.activeGroupId);
      if (remoteGroup) {
        return remoteGroup;
      }
      const subgroup = parseGpuiWorkspaceSessionSubgroupId(this.activeGroupId);
      return subgroup ? parseGpuiRemotePresentationProjectId(subgroup.projectId) : undefined;
    })();
    const activeRemoteReference = focusedRemoteSession ?? activeGroupRemoteReference;
    const activeTabSessions = this.activeWorkspaceTabSessionsFromLatestGroups();
    const activeProjectId = activeRemoteReference
      ? createGpuiRemotePresentationProjectId(activeRemoteReference.machineId, activeRemoteReference.projectId)
      : this.activeProjectId;
    const payload = JSON.stringify({
      activeProjectId,
      tabSessions: activeTabSessions,
      focusedSessionId: this.focusedSessionId,
      type: GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_GXSERVER_FOCUS_STATE_MESSAGE_VERSION,
      visibleSessionIds: [...this.visibleSessionIds],
    });
    try {
      postFocusState(payload);
    } catch {
      /*
      CDXC:GPUISidebarGxserverFocusState 2026-06-24-21:07:
      Focus-state publication is a sidebar-native synchronization hint for Rust bootstrap replay only. A missing or rejecting CEF bridge must not change gxserver data, create fallback focus ids, log renderer payloads, or block the visible SidebarApp state that React already owns.
      */
    }
  },

  activeWorkspaceTabSessionsFromLatestGroups(this: GpuiSidebarRuntime): GpuiActiveWorkspaceTabSessionPayload[] {
    /*
    CDXC:GPUIWorkspaceTabsParity 2026-07-05:
    The native GPUI Agents tab strip mirrors the already-projected active
    SidebarApp group. Hidden, companion, carrier, and subgroup filtering stays
    upstream in createSidebarGroups; this bridge only serializes the active
    gxserver rows in their rendered order with the same visible title chain
    used by the SidebarApp cards and macOS pane tabs. Remote rows carry their
    machine-scoped project id plus the owning daemon's raw session id; titles
    are never reconstructed from remote attach metadata.
    */
    const activeGroup =
      this.latestGroups.find((group) => group.groupId === this.activeGroupId) ??
      this.latestGroups.find((group) => group.isActive);
    if (!activeGroup) {
      return [];
    }
    const seen = new Set<string>();
    const sessions: GpuiActiveWorkspaceTabSessionPayload[] = [];
    for (const session of activeGroup.sessions) {
      const localReference = parseGxserverPresentationProjectSessionId(session.sessionId);
      const remoteReference = parseGpuiRemotePresentationSessionId(session.sessionId);
      if (!localReference && !remoteReference) {
        continue;
      }
      if (localReference?.projectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID) {
        continue;
      }
      const kind = session.sessionKind;
      if (kind !== 'terminal') {
        continue;
      }
      const projectId = remoteReference
        ? createGpuiRemotePresentationProjectId(remoteReference.machineId, remoteReference.projectId)
        : localReference!.projectId;
      const sessionId = remoteReference?.sessionId ?? localReference!.sessionId;
      const key = remoteReference
        ? session.sessionId
        : createGxserverPresentationProjectSessionId(projectId, sessionId);
      if (seen.has(key)) {
        continue;
      }
      seen.add(key);
      sessions.push({
        activity: session.activity,
        ...(session.agentIcon ? { agentIcon: session.agentIcon } : {}),
        ...(session.agentSessionId?.trim() ? { agentSessionId: session.agentSessionId.trim() } : {}),
        isGeneratingFirstPromptTitle: session.isGeneratingFirstPromptTitle === true,
        isSleeping: session.isSleeping === true,
        kind,
        ...(session.lifecycleState ? { lifecycleState: session.lifecycleState } : {}),
        projectId,
        sessionId,
        title: boundedGpuiActiveWorkspaceTabSessionTitle(
          session.displayTitle?.trim() ||
            session.primaryTitle?.trim() ||
            session.terminalTitle?.trim() ||
            session.alias.trim() ||
            DEFAULT_TERMINAL_SESSION_TITLE
        ),
      });
    }
    return sessions;
  },

  /*
   * CDXC:GlobalActions 2026-08-01:
   * Publish only what the native strip draws: bounded action id, display name,
   * and icon slug. Command text, URLs, links, and run state deliberately stay
   * on this side — a strip click sends the id back and this runtime resolves
   * the trusted definition, so gpui never holds anything executable.
   */
  postGpuiGlobalActions(this: GpuiSidebarRuntime): void {
    const actions = (this.sidebarHudState?.globalCommands ?? [])
      .slice(0, GPUI_TAB_STRIP_MAX_GLOBAL_ACTIONS)
      .map((command) => ({
        commandId: command.commandId,
        ...(command.icon ? { icon: command.icon } : {}),
        name: command.name,
      }));
    const payload = JSON.stringify({
      actions,
      type: GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_VERSION,
    });
    if (payload === this.postedGlobalActionsPayload) {
      return;
    }
    /*
     * Cache only what the bridge confirmed it took. An absent CEF function
     * makes the optional call return undefined WITHOUT throwing, and a rejected
     * payload returns false, so caching before the call would record an
     * undelivered payload as sent and leave the strip empty until some
     * unrelated HUD change happened to produce a different payload. Leaving the
     * cache unset instead means the next HUD refresh retries on its own.
     */
    let delivered = false;
    try {
      delivered = window.ghostexGpui?.postGlobalActions?.(payload) === true;
    } catch {
      /*
       * The strip is presentation-only. Keep this runtime authoritative and do
       * not log raw payloads or invent native state.
       */
      delivered = false;
    }
    this.postedGlobalActionsPayload = delivered ? payload : undefined;
  },

  postGpuiStatusPetState(this: GpuiSidebarRuntime): void {
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    const candidates = createGpuiSessionStatusIndicatorCandidatesFromSidebarGroups(this.latestGroups);
    const statusPayload = createGpuiSessionStatusIndicatorsPayload(candidates, settings);
    const petPayload = createGpuiPetOverlayStatePayload(candidates, settings);
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-04:38:
    GPUI status indicators and the pet overlay consume the same saved shared Settings object as SidebarApp hydrate. Publish only bounded counts, booleans, pet id, and sidebar-projected project/session ids/titles through fixed bridge functions.

    CDXC:GPUIStatusPetOverlay 2026-06-27-20:11:
    The standalone GPUI floating session indicator was removed. Keep posting
    status counts/projects for the menu bar and pet badge surfaces, but do not
    include floating visibility or floating size settings in the status payload.
    */
    try {
      window.ghostexGpui?.postSessionStatusIndicators?.(JSON.stringify(statusPayload));
      window.ghostexGpui?.postPetOverlayState?.(JSON.stringify(petPayload));
    } catch {
      /*
      CDXC:GPUIStatusPetOverlay 2026-06-26-04:38:
      The status/pet bridge is presentation-only. If CEF has not installed the fixed functions or rejects a payload, keep SidebarApp state authoritative and avoid fallback UI state, raw JSON logging, project/path/title side channels, or invented native indicators.
      */
    }
  },

  createHydrateMessage(
    this: GpuiSidebarRuntime,
    groups: SidebarSessionGroup[],
    hud: SidebarHudState
  ): SidebarHydrateMessage {
    return {
      groups,
      hud,
      pinnedPrompts: [...this.appUserData.pinnedPrompts],
      previousSessions: [],
      remoteSidebarProjectCollectionsByMachineId: this.remoteSidebarProjectCollectionsByMachineId(),
      revision: ++this.revision,
      scratchPadContent: this.appUserData.scratchPadContent,
      type: 'hydrate',
    };
  },

  remoteSidebarProjectCollectionsByMachineId(
    this: GpuiSidebarRuntime
  ): Readonly<Record<string, GxserverSidebarProjectCollectionsState>> {
    const result: Record<string, GxserverSidebarProjectCollectionsState> = {};
    const savedMachineIds = new Set(
      createGpuiSidebarSettings(this.runtimeSettings).remoteMachines.map((machine) => machine.id)
    );
    for (const [machineId, snapshot] of this.remoteLastSeenPresentations) {
      if (savedMachineIds.has(machineId) && isSidebarProjectCollectionsState(snapshot.sidebarProjectCollections)) {
        result[machineId] = snapshot.sidebarProjectCollections;
      }
    }
    for (const [machineId, snapshot] of this.remotePresentations) {
      if (savedMachineIds.has(machineId) && isSidebarProjectCollectionsState(snapshot.sidebarProjectCollections)) {
        result[machineId] = snapshot.sidebarProjectCollections;
      }
    }
    return result;
  },

  createSidebarGroups(this: GpuiSidebarRuntime, presentation: GxserverPresentationSnapshot): SidebarSessionGroup[] {
    this.refreshCloseAfterDoneTimers();
    this.pruneWorkspaceGroupAssignments(presentation);
    const projectProjection = createGpuiPresentationProjectProjectionMetadata({
      domainProjects: this.domainProjects,
      presentation,
      recentProjects: this.recentProjects,
      projectOrder: this.workspaceGroups.projectOrder,
    });
    this.ensureActiveProject(presentation, projectProjection);
    const subgroupHiddenSessionKeys = this.collectWorkspaceSubgroupSessionKeys(presentation);
    const hiddenSessionKeys =
      subgroupHiddenSessionKeys.size > 0
        ? new Set([...this.localFirstHiddenPresentationSessionKeys, ...subgroupHiddenSessionKeys])
        : this.localFirstHiddenPresentationSessionKeys;
    const projectGroups = createGxserverPresentationSidebarGroups({
      activeProjectId: this.activeProjectId,
      chatProjectIds: projectProjection.chatProjectIds,
      focusedSessionId: this.focusedSessionId,
      hiddenProjectIds: projectProjection.hiddenProjectIds,
      hiddenSessionKeys,
      presentation,
      projectOverlays: projectProjection.projectOverlays,
      resolveAgentIcon: resolveGpuiSidebarAgentIcon,
      resolveCloseAfterDone: (projectId, sessionId) =>
        this.getCloseAfterDoneProjection(createGxserverPresentationProjectSessionId(projectId, sessionId)),
      resolveDelayedSend: (projectId, sessionId) =>
        this.getDelayedSendProjection(createGxserverPresentationProjectSessionId(projectId, sessionId)),
      resolveSessionRoutingId: createGpuiSidebarSessionRoutingId,
      visibleSessionIds: this.visibleSessionIds,
    }).map((group) => {
      const projectId = group.projectContext?.editor.projectId;
      if (!projectId) {
        return group;
      }
      const browserSessions = this.browserTabs
        .filter((tab) => tab.projectId === projectId)
        .map((tab, index): SidebarSessionItem => ({
          activity: 'idle',
          agentIcon: 'browser',
          alias: tab.title,
          column: index % GRID_COLUMN_COUNT,
          displayTitle: tab.title,
          ...(tab.faviconUrl ? { faviconDataUrl: tab.faviconUrl } : {}),
          isFocused: tab.isActive && this.activeProjectId === projectId,
          isLive: !tab.isSleeping,
          isRunning: !tab.isSleeping,
          isSleeping: tab.isSleeping,
          isVisible: tab.isVisible && this.activeProjectId === projectId,
          kind: 'browser',
          lifecycleState: tab.isSleeping ? 'sleeping' : 'running',
          nativePaneState: tab.isSleeping ? 'unmounted' : 'mounted',
          primaryTitle: tab.title,
          row: Math.floor(index / GRID_COLUMN_COUNT),
          sessionId: gpuiBrowserSidebarSessionId(tab),
          sessionKind: 'browser',
          shortcutLabel: '',
        }));
      if (browserSessions.length === 0) {
        return group;
      }
      const sessions = [...browserSessions, ...group.sessions];
      const visibleCount = visibleCountForGxserverPresentationSidebarSessions(sessions);
      return {
        ...group,
        layoutVisibleCount: visibleCount,
        sessions,
        visibleCount,
      };
    });
    const groups = this.spliceWorkspaceSubgroups(projectGroups, presentation, projectProjection);

    if (!this.activeGroupId) {
      this.activeGroupId =
        groups.find((group) => group.isActive)?.groupId ??
        groups.find((group) => group.projectContext)?.groupId ??
        groups.find((group) => group.isChatCollection)?.groupId;
    }

    const localGroups = groups.map((group) => {
      const isActiveGroup = group.groupId === this.activeGroupId;
      const browserOwnsFocus =
        isActiveGroup && group.sessions.some((session) => session.sessionKind === 'browser' && session.isFocused);
      return {
        ...group,
        isActive: isActiveGroup,
        sessions: group.sessions.map((session) => ({
          ...session,
          isFocused:
            isActiveGroup &&
            (session.sessionKind === 'browser'
              ? session.isFocused
              : !browserOwnsFocus &&
                this.focusedSessionId === parseGxserverPresentationProjectSessionId(session.sessionId)?.sessionId),
          /*
          GPUI terminal visibility is owned by the native workspace callback.
          Do not preserve the shared projection's first-row fallback here:
          pinned sessions sort first and would otherwise look surfaced without
          owning a pane. Browser rows keep their separate browser-pane state.
          */
          isVisible:
            isActiveGroup &&
            (session.sessionKind === 'browser'
              ? session.isVisible
              : this.visibleSessionIds.has(
                  parseGxserverPresentationProjectSessionId(session.sessionId)?.sessionId ?? session.sessionId
                )),
        })),
      };
    });
    return this.overlayProjectDiffStats([
      ...this.withQuickAutomationsOverviewGroup(localGroups),
      ...this.createRemoteSidebarGroups(),
    ]);
  },

  withQuickAutomationsOverviewGroup(this: GpuiSidebarRuntime, groups: SidebarSessionGroup[]): SidebarSessionGroup[] {
    if (!this.quickAutomationsOverviewOpen) {
      return groups;
    }
    const quickSession = this.createQuickAutomationsSidebarSession();
    const nextGroups = groups.map((group) => {
      if (group.groupId !== GPUI_GXSERVER_CHATS_GROUP_ID) {
        return group;
      }
      const sessions = [
        quickSession,
        ...group.sessions.filter((session) => !this.isQuickAutomationsSidebarSessionId(session.sessionId)),
      ].map((session, index) => ({ ...session, column: index % GRID_COLUMN_COUNT, row: index }));
      const visibleCount = visibleCountForGxserverPresentationSidebarSessions(sessions);
      return {
        ...group,
        isActive: this.activeProjectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID || group.isActive,
        layoutVisibleCount: visibleCount,
        sessions,
        visibleCount,
      };
    });
    if (nextGroups.some((group) => group.groupId === GPUI_GXSERVER_CHATS_GROUP_ID)) {
      return nextGroups;
    }
    const sessions = [quickSession];
    const visibleCount = visibleCountForGxserverPresentationSidebarSessions(sessions);
    return [
      {
        groupId: GPUI_GXSERVER_CHATS_GROUP_ID,
        isActive: this.activeProjectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID,
        isChatCollection: true,
        isFocusModeActive: false,
        kind: 'workspace',
        layoutVisibleCount: visibleCount,
        sessions,
        title: 'Chats',
        viewMode: 'grid',
        visibleCount,
      },
      ...groups,
    ];
  },

  createQuickAutomationsSidebarSession(this: GpuiSidebarRuntime): SidebarSessionItem {
    const sessionId = this.quickAutomationsSidebarSessionId();
    const isActive = this.activeProjectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID;
    /*
    CDXC:GPUIAutomationsOverview 2026-07-08:
    Mirror macOS `createQuickAutomationsSidebarSession` and
    `isQuickAutomationsSidebarReference`: the overview is one synthetic Quick
    row named Automations Overview, scoped to project id `quick-automations`,
    and removed from the session-local runtime projection when closed.
    */
    return {
      activity: 'idle',
      alias: GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE,
      column: 0,
      detail: 'All projects',
      displayTitle: GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE,
      isFocused: isActive && this.focusedSessionId === GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID,
      isLive: false,
      isRunning: false,
      isVisible: isActive,
      lifecycleState: 'done',
      nativePaneState: 'unmounted',
      primaryTitle: GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE,
      providerSessionState: 'missing',
      row: 0,
      sessionId,
      shortcutLabel: '',
    };
  },

  quickAutomationsSidebarSessionId(this: GpuiSidebarRuntime): string {
    return createGxserverPresentationProjectSessionId(
      GPUI_QUICK_AUTOMATIONS_PROJECT_ID,
      GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID
    );
  },

  isQuickAutomationsSidebarSessionId(this: GpuiSidebarRuntime, sessionId: string): boolean {
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    return (
      reference?.projectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID &&
      reference.sessionId === GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID
    );
  },

  createQuickAutomationsProjectContext(this: GpuiSidebarRuntime): NonNullable<SidebarSessionGroup['projectContext']> {
    return {
      canRemoveProject: false,
      editor: {
        diffStats: createDefaultSidebarProjectDiffStats(),
        isOpen: true,
        isSleeping: false,
        projectId: GPUI_QUICK_AUTOMATIONS_PROJECT_ID,
        status: 'running',
      },
      path: '',
    };
  },

  activeProjectContextGroups(this: GpuiSidebarRuntime): SidebarSessionGroup[] {
    if (!this.quickAutomationsOverviewOpen || this.activeProjectId !== GPUI_QUICK_AUTOMATIONS_PROJECT_ID) {
      return this.latestGroups;
    }
    return this.latestGroups.map((group) =>
      group.groupId === GPUI_GXSERVER_CHATS_GROUP_ID
        ? {
            ...group,
            projectContext: this.createQuickAutomationsProjectContext(),
            title: GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE,
          }
        : group
    );
  },

  overlayProjectDiffStats(this: GpuiSidebarRuntime, groups: SidebarSessionGroup[]): SidebarSessionGroup[] {
    // Mirrors the macOS pre-publish overlay: header +/- counts come from the
    // background numstat loop, keyed by the projection's editor project id
    // (plain local ids, machine-scoped remote ids).
    return groups.map((group) => {
      const projectContext = group.projectContext;
      if (!projectContext) {
        return group;
      }
      const stats = this.projectDiffStatsByProjectId.get(projectContext.editor.projectId);
      if (!stats) {
        return group;
      }
      return {
        ...group,
        projectContext: {
          ...projectContext,
          editor: { ...projectContext.editor, diffStats: stats },
        },
      };
    });
  },

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
      this.persistWorkspaceGroups();
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
      this.persistWorkspaceGroups();
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
          activeProjectId: this.activeProjectId,
          canRemoveProject: false,
          createProjectGroupId: () => subgroupSidebarId,
          focusedSessionId: this.focusedSessionId,
          project,
          resolveAgentIcon: resolveGpuiSidebarAgentIcon,
          resolveCloseAfterDone: (resolvedProjectId, sessionId) =>
            this.getCloseAfterDoneProjection(createGxserverPresentationProjectSessionId(resolvedProjectId, sessionId)),
          resolveDelayedSend: (resolvedProjectId, sessionId) =>
            this.getDelayedSendProjection(createGxserverPresentationProjectSessionId(resolvedProjectId, sessionId)),
          resolveSessionRoutingId: createGpuiSidebarSessionRoutingId,
          sessions: memberRows,
          visibleSessionIds: this.visibleSessionIds,
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
    CDXC:GPUIRemoteLastSeen 2026-07-12:
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
      activeGroupId: this.activeGroupId,
      focusedSessionId: this.focusedSessionId,
      presentationsByMachineId,
      remoteGroupOrderByMachineId: this.remoteGroupOrderByMachineId,
      remoteRecentProjectsByMachineId: this.remoteRecentProjectsByMachineId,
      resolveAgentIcon: resolveGpuiSidebarAgentIcon,
      resolveCloseAfterDone: (machineId, projectId, sessionId) =>
        this.getCloseAfterDoneProjection(createGpuiRemotePresentationSessionId(machineId, projectId, sessionId)),
      settings,
      visibleSessionIds: this.visibleSessionIds,
    });
    return groups.flatMap((group) => {
      const expanded = this.expandRemoteSidebarGroup(group);
      const machineId = group.remoteMachineContext?.machineId;
      if (!machineId || !staleMachineIds.has(machineId)) {
        return expanded;
      }
      return expanded.map((expandedGroup) => ({ ...expandedGroup, isStale: true }));
    });
  },

  captureRemoteLastSeenPresentations(this: GpuiSidebarRuntime): void {
    let changed = false;
    for (const [machineId, snapshot] of this.remotePresentations) {
      if (this.remoteLastSeenPresentations.get(machineId) !== snapshot) {
        this.remoteLastSeenPresentations.set(machineId, snapshot);
        changed = true;
      }
    }
    if (!changed) {
      return;
    }
    if (this.remoteLastSeenPersistTimeoutId !== undefined) {
      return;
    }
    this.remoteLastSeenPersistTimeoutId = window.setTimeout(() => {
      this.remoteLastSeenPersistTimeoutId = undefined;
      writeStoredGpuiRemoteLastSeenPresentations(this.remoteLastSeenPresentations);
    }, GPUI_REMOTE_LAST_SEEN_PRESENTATIONS_PERSIST_DELAY_MS);
  },

  /*
  CDXC:GPUIRemoteSidebarParity 2026-07-12:
  Remote project groups reuse the local sidebar overlays instead of a reduced
  remote feature set: machine-scoped browser tabs splice in as browser session
  rows, and the client-owned named session groups overlay applies to remote
  projects through their machine-scoped project ids.
  */
  expandRemoteSidebarGroup(this: GpuiSidebarRuntime, group: SidebarSessionGroup): SidebarSessionGroup[] {
    const remoteGroup = parseGpuiRemotePresentationGroupId(group.groupId);
    if (!remoteGroup) {
      return [group];
    }
    const scopedProjectId = createGpuiRemotePresentationProjectId(remoteGroup.machineId, remoteGroup.projectId);
    return this.spliceRemoteWorkspaceSubgroups(
      this.withRemoteBrowserTabSessions(group, scopedProjectId),
      scopedProjectId
    );
  },

  withRemoteBrowserTabSessions(
    this: GpuiSidebarRuntime,
    group: SidebarSessionGroup,
    scopedProjectId: string
  ): SidebarSessionGroup {
    const browserSessions = this.browserTabs
      .filter((tab) => tab.projectId === scopedProjectId)
      .map((tab, index): SidebarSessionItem => ({
        activity: 'idle',
        agentIcon: 'browser',
        alias: tab.title,
        column: index % GRID_COLUMN_COUNT,
        displayTitle: tab.title,
        ...(tab.faviconUrl ? { faviconDataUrl: tab.faviconUrl } : {}),
        isFocused: tab.isActive && this.activeGroupId === group.groupId,
        isLive: !tab.isSleeping,
        isRunning: !tab.isSleeping,
        isSleeping: tab.isSleeping,
        isVisible: tab.isVisible && this.activeGroupId === group.groupId,
        kind: 'browser',
        lifecycleState: tab.isSleeping ? 'sleeping' : 'running',
        nativePaneState: tab.isSleeping ? 'unmounted' : 'mounted',
        primaryTitle: tab.title,
        row: Math.floor(index / GRID_COLUMN_COUNT),
        sessionId: gpuiBrowserSidebarSessionId(tab),
        sessionKind: 'browser',
        shortcutLabel: '',
      }));
    if (browserSessions.length === 0) {
      return group;
    }
    const sessions = [...browserSessions, ...group.sessions];
    const visibleCount = visibleCountForGxserverPresentationSidebarSessions(sessions);
    return {
      ...group,
      layoutVisibleCount: visibleCount,
      sessions,
      visibleCount,
    };
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
        isActive: this.activeGroupId === subgroupSidebarId,
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

  ensureActiveProject(
    this: GpuiSidebarRuntime,
    presentation: GxserverPresentationSnapshot,
    projectProjection: GpuiPresentationProjectProjectionMetadata
  ): void {
    const projectIds = new Set<string>(presentation.projects.map((project) => project.projectId));
    if (this.quickAutomationsOverviewOpen && this.activeProjectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID) {
      if (this.activeGroupId !== GPUI_GXSERVER_CHATS_GROUP_ID) {
        this.activeGroupId = GPUI_GXSERVER_CHATS_GROUP_ID;
      }
      return;
    }
    if (this.focusedSessionId) {
      /*
      CDXC:GPUIWorkspaceSessionFocus 2026-06-27-13:22:
      Re-clicking a local session in the GPUI sidebar must keep behaving like the macOS app: the focused terminal owns the active project. Bootstrap can replay a stale initial project beside the current focused session, so resolve the session from the fresh presentation snapshot before rendering groups.
      */
      const focusedProjectId = presentation.sessions.find(
        (session) => session.sessionId === this.focusedSessionId
      )?.projectId;
      if (
        focusedProjectId &&
        projectIds.has(focusedProjectId) &&
        !projectProjection.hiddenProjectIds.has(focusedProjectId)
      ) {
        const focusedGroupId = projectProjection.chatProjectIds.has(focusedProjectId)
          ? GPUI_GXSERVER_CHATS_GROUP_ID
          : (this.workspaceSubgroupSidebarIdForSession(focusedProjectId, this.focusedSessionId) ??
            createGxserverPresentationProjectGroupId(focusedProjectId));
        if (this.activeProjectId !== focusedProjectId || this.activeGroupId !== focusedGroupId) {
          this.activeProjectId = focusedProjectId;
          this.activeGroupId = focusedGroupId;
          this.refreshSidebarHudFromClient();
        }
        return;
      }
    }
    if (
      this.activeProjectId &&
      projectIds.has(this.activeProjectId) &&
      !projectProjection.hiddenProjectIds.has(this.activeProjectId)
    ) {
      if (projectProjection.chatProjectIds.has(this.activeProjectId)) {
        if (this.activeGroupId !== GPUI_GXSERVER_CHATS_GROUP_ID) {
          this.activeGroupId = GPUI_GXSERVER_CHATS_GROUP_ID;
          this.refreshSidebarHudFromClient();
        }
        return;
      }
      return;
    }
    const firstProject = presentation.projects.find(
      (project) =>
        !projectProjection.hiddenProjectIds.has(project.projectId) &&
        !projectProjection.chatProjectIds.has(project.projectId)
    );
    if (firstProject) {
      this.focusProjectId(firstProject.projectId);
      return;
    }
    this.activeProjectId = undefined;
    this.activeGroupId = GPUI_GXSERVER_CHATS_GROUP_ID;
    this.refreshSidebarHudFromClient();
  },
};

const gpuiSidebarRuntimeSidebarGroupMethodsShapeCheck: GpuiSidebarRuntimeSidebarGroupMethods =
  gpuiSidebarRuntimeSidebarGroupMethods;
void gpuiSidebarRuntimeSidebarGroupMethodsShapeCheck;
