import {
  GXSERVER_PRESENTATION_CHATS_GROUP_ID,
  createGxserverPresentationSessionsByProjectFromGroups,
  createGxserverPresentationSidebarGroup,
  createGxserverPresentationSidebarGroups,
  orderGxserverPresentationSidebarProjects,
  type GxserverPresentationSidebarProjectOverlay,
} from "@/shared/gxserver-presentation-sidebar-projection";
import type {
  GxserverPresentationSession,
  GxserverForkSessionResult,
  GxserverProjectDomainState,
  GxserverRecentProjectDomainState,
  GxserverSidebarHudResponse,
} from "@/shared/gxserver-protocol";
import {
  createDefaultSessionGridSnapshot,
  createSidebarHudState,
  type ExtensionToSidebarMessage,
  type SidebarHudState,
  type SidebarRecentProject,
  type SidebarSessionGroup,
  type SidebarToExtensionMessage,
} from "@/shared/session-grid-contract";
import {
  DEFAULT_SIDEBAR_AGENTS,
  getSidebarAgentIconById,
  type SidebarAgentButton,
} from "@/shared/sidebar-agents";
import {
  DEFAULT_ghostex_SETTINGS,
  type RemoteMachineSettings,
} from "@/shared/ghostex-settings";
import {
  normalizeWorkspaceProjectIcon,
  normalizeWorkspaceProjectIconDataUrl,
  normalizeWorkspaceThemeColor,
} from "@/shared/workspace-project-appearance";
import {
  getConnectionStates,
  rpcForMachine,
  subscribeConnectionStates,
} from "../connections/connection-registry";
import type { MachineConnectionState } from "../connections/types";
import {
  createSidebarGroupId,
  createSidebarProjectId,
  createSidebarSessionId,
  parseSidebarGroupId,
  parseSidebarProjectId,
  parseSidebarSessionId,
  type SidebarProjectReference,
  type SidebarSessionReference,
} from "./sidebar-ids";
import { setActiveSidebarProject } from "./active-project-store";

const DEBUG_SIDEBAR_STORAGE_KEY = "ghostexWeb.debugSidebar";
const DEFAULT_TERMINAL_TITLE = "Terminal";

type SidebarMessageSource = Pick<EventTarget, "addEventListener" | "removeEventListener">;

type MachineProjectMetadata = {
  projects: readonly GxserverProjectDomainState[];
  signature: string;
};

type MachineRecentProjects = {
  projects: readonly SidebarRecentProject[];
  signature: string;
};

export type GhostexWebFocusSessionDetail = SidebarSessionReference & {
  placement: "focusedPane";
  placementTargetSessionId?: string;
  source: "sidebar";
};

declare global {
  interface WindowEventMap {
    "ghostex-web:focusSession": CustomEvent<GhostexWebFocusSessionDetail>;
    "ghostex-web:activeSessionContext": CustomEvent<SidebarSessionReference>;
  }
}

class WebSidebarMessageSource extends EventTarget {
  postMessage(message: ExtensionToSidebarMessage): void {
    this.dispatchEvent(new MessageEvent<ExtensionToSidebarMessage>("message", { data: message }));
  }
}

export type WebSidebarRuntime = {
  messageSource: SidebarMessageSource;
  start(): void;
  stop(): void;
  vscode: {
    postMessage(message: SidebarToExtensionMessage): void;
  };
};

export function createWebSidebarRuntime(): WebSidebarRuntime {
  const messageSource = new WebSidebarMessageSource();
  let activeTarget: SidebarProjectReference | undefined;
  let focusedTarget: SidebarSessionReference | undefined;
  let pendingActiveSessionContext: SidebarSessionReference | undefined;
  let hasHydrated = false;
  let revision = 0;
  let running = false;
  let unsubscribeConnections: (() => void) | undefined;
  let hudRequestKey = "";
  let remoteHud: GxserverSidebarHudResponse | undefined;
  const projectMetadataByMachineId = new Map<string, MachineProjectMetadata>();
  const projectMetadataRequestSignatures = new Map<string, string>();
  const recentProjectsByMachineId = new Map<string, MachineRecentProjects>();
  const recentProjectsRequestSignatures = new Map<string, string>();

  const publish = (): void => {
    if (!running) {
      return;
    }
    const states = getConnectionStates();
    if (
      pendingActiveSessionContext
      && presentationHasSession(states, pendingActiveSessionContext)
    ) {
      activeTarget = {
        machineId: pendingActiveSessionContext.machineId,
        projectId: pendingActiveSessionContext.projectId,
      };
      focusedTarget = pendingActiveSessionContext;
      pendingActiveSessionContext = undefined;
    }
    activeTarget = reconcileActiveTarget(activeTarget, states);
    setActiveSidebarProject(activeTarget);
    focusedTarget = reconcileFocusedTarget(focusedTarget, states);
    const groups = createMergedSidebarGroups(
      states,
      activeTarget,
      focusedTarget,
      projectMetadataByMachineId,
    );
    debugLog("publish", {
      groupCount: groups.length,
      groups: groups.slice(0, 8).map((group) => ({
        sessionCount: group.sessions.length,
        title: group.title,
      })),
    });
    const hud = createWebSidebarHud(
      groups,
      focusedTarget,
      remoteHud,
      states,
      recentProjectsByMachineId,
    );
    const message: ExtensionToSidebarMessage = {
      groups,
      hud,
      pinnedPrompts: [],
      previousSessions: [],
      revision: ++revision,
      scratchPadContent: "",
      type: hasHydrated ? "sessionState" : "hydrate",
    };
    hasHydrated = true;
    messageSource.postMessage(message);
    publishMachineStatuses(states, messageSource);
    refreshProjectMetadata(states);
    refreshRecentProjects(states);
    refreshHud(activeTarget ?? primaryProjectTarget(states));
  };

  const applyRecentProjects = (
    machineId: string,
    recentProjects: readonly GxserverRecentProjectDomainState[],
    signature = createRecentProjectsSignature(
      getConnectionStates().find((state) => state.machine.machineId === machineId),
    ),
  ): void => {
    const state = getConnectionStates().find((candidate) => candidate.machine.machineId === machineId);
    if (!state) {
      return;
    }
    recentProjectsByMachineId.set(machineId, {
      projects: createWebRecentProjects(state, recentProjects),
      signature,
    });
    publish();
  };

  const refreshRecentProjects = (states: readonly MachineConnectionState[]): void => {
    const activeMachineIds = new Set(states.map((state) => state.machine.machineId));
    for (const machineId of recentProjectsByMachineId.keys()) {
      if (!activeMachineIds.has(machineId)) {
        recentProjectsByMachineId.delete(machineId);
        recentProjectsRequestSignatures.delete(machineId);
      }
    }
    for (const state of states) {
      if (state.status !== "connected") {
        continue;
      }
      const machineId = state.machine.machineId;
      const signature = createRecentProjectsSignature(state);
      if (
        recentProjectsByMachineId.get(machineId)?.signature === signature
        || recentProjectsRequestSignatures.get(machineId) === signature
      ) {
        continue;
      }
      recentProjectsRequestSignatures.set(machineId, signature);
      void rpcForMachine<{ recentProjects: GxserverRecentProjectDomainState[] }>(
        machineId,
        "/api/listRecentProjects",
      ).then(({ recentProjects }) => {
        if (!running || recentProjectsRequestSignatures.get(machineId) !== signature) {
          return;
        }
        recentProjectsRequestSignatures.delete(machineId);
        applyRecentProjects(machineId, recentProjects, signature);
      }).catch((error: unknown) => {
        if (recentProjectsRequestSignatures.get(machineId) === signature) {
          recentProjectsRequestSignatures.delete(machineId);
        }
        debugLog("recentProjectsError", {
          error: error instanceof Error ? error.message : String(error),
          machineId,
        });
      });
    }
  };

  const refreshProjectMetadata = (states: readonly MachineConnectionState[]): void => {
    for (const state of states) {
      const presentation = state.presentation;
      if (!presentation) {
        continue;
      }
      const signature = presentation.projects
        .map((project) => `${project.projectId}:${project.updatedAt}`)
        .join("|");
      if (
        projectMetadataByMachineId.get(state.machine.machineId)?.signature === signature
        || projectMetadataRequestSignatures.get(state.machine.machineId) === signature
      ) {
        continue;
      }
      projectMetadataRequestSignatures.set(state.machine.machineId, signature);
      void rpcForMachine<{ projects: GxserverProjectDomainState[] }>(
        state.machine.machineId,
        "/api/listProjects",
      ).then(({ projects }) => {
        if (!running || projectMetadataRequestSignatures.get(state.machine.machineId) !== signature) {
          return;
        }
        projectMetadataByMachineId.set(state.machine.machineId, { projects, signature });
        debugLog("projectMetadata", {
          chatProjectCount: projects.filter(isChatDomainProject).length,
          machineId: state.machine.machineId,
          projectCount: projects.length,
        });
        publish();
      }).catch((error: unknown) => {
        if (projectMetadataRequestSignatures.get(state.machine.machineId) === signature) {
          projectMetadataRequestSignatures.delete(state.machine.machineId);
        }
        debugLog("projectMetadataError", {
          error: error instanceof Error ? error.message : String(error),
          machineId: state.machine.machineId,
        });
      });
    }
  };

  const refreshHud = (target: SidebarProjectReference | undefined): void => {
    if (!target) {
      return;
    }
    const requestKey = `${target.machineId}\u0000${target.projectId}`;
    if (requestKey === hudRequestKey) {
      return;
    }
    hudRequestKey = requestKey;
    void rpcForMachine<GxserverSidebarHudResponse>(target.machineId, "/api/readSidebarHud", {
      activeProjectId: target.projectId,
    }).then((hud) => {
      if (!running || hudRequestKey !== requestKey) {
        return;
      }
      remoteHud = hud;
      publish();
    }).catch(() => {
      if (hudRequestKey === requestKey) {
        remoteHud = undefined;
      }
    });
  };

  const focusSession = async (sessionId: string): Promise<void> => {
    const target = parseSidebarSessionId(sessionId);
    if (!target || !presentationHasSession(getConnectionStates(), target)) {
      return;
    }
    activeTarget = target;
    focusedTarget = target;
    const session = findPresentationSession(getConnectionStates(), target);
    if (session?.activity === "attention") {
      void rpcForMachine(target.machineId, "/api/updateAgentActivity", {
        ...(session.agentName ? { agentName: session.agentName } : {}),
        event: "acknowledge",
        projectId: target.projectId,
        sessionId: target.sessionId,
      });
    }
    publish();
    dispatchFocusSession(target);
  };

  const createSession = async (groupId?: string): Promise<void> => {
    const target = (groupId ? parseSidebarGroupId(groupId) : undefined)
      ?? activeTarget
      ?? primaryProjectTarget(getConnectionStates());
    if (!target) {
      return;
    }
    const result = await rpcForMachine<{
      session?: { projectId?: string; sessionId?: string };
    }>(target.machineId, "/api/createSession", {
      kind: "terminal",
      lifecycleState: "running",
      projectId: target.projectId,
      surface: "workspace",
      title: DEFAULT_TERMINAL_TITLE,
    });
    const sessionId = result.session?.sessionId;
    if (sessionId) {
      const createdTarget = {
        machineId: target.machineId,
        projectId: result.session?.projectId ?? target.projectId,
        sessionId,
      };
      activeTarget = createdTarget;
      focusedTarget = createdTarget;
      dispatchFocusSession(createdTarget);
    }
  };

  const createQuickSession = async (kind: "agent" | "terminal", agentId?: string): Promise<void> => {
    const machineId = activeTarget?.machineId
      ?? getConnectionStates().find((state) => state.machine.machineId === "local")?.machine.machineId;
    if (!machineId) {
      return;
    }
    const { project } = await rpcForMachine<{ project: GxserverProjectDomainState }>(
      machineId,
      "/api/createQuickProject",
      { kind },
    );
    activeTarget = { machineId, projectId: project.projectId };
    hudRequestKey = "";
    if (kind === "agent" && agentId) {
      await createAgentSession(agentId);
      return;
    }
    await createSession(createSidebarGroupId(machineId, project.projectId));
  };

  const postMessage = (message: SidebarToExtensionMessage): void => {
    void handleSidebarMessage(message).catch((error: unknown) => {
      debugLog("actionError", {
        error: error instanceof Error ? error.message : String(error),
        type: message.type,
      });
    });
  };

  const handleSidebarMessage = async (message: SidebarToExtensionMessage): Promise<void> => {
    switch (message.type) {
      case "focusSession":
      case "focusSessionMode":
        await focusSession(message.sessionId);
        return;
      case "focusGroup": {
        const target = parseSidebarGroupId(message.groupId);
        if (target) {
          activeTarget = target;
          hudRequestKey = "";
          publish();
        }
        return;
      }
      case "createSession":
      case "createFullWidthTerminalPane":
        await createSession();
        return;
      case "createChat":
        await createQuickSession("terminal");
        return;
      case "createSessionInGroup":
        await createSession(message.groupId);
        return;
      case "setSessionSleeping": {
        const target = parseSidebarSessionId(message.sessionId);
        if (target) {
          await lifecycleRpc(target, message.sleeping ? "/api/sleepSession" : "/api/wakeSession");
        }
        return;
      }
      case "setSessionsSleeping":
        await Promise.all(message.sessionIds.map(async (sessionId) => {
          const target = parseSidebarSessionId(sessionId);
          if (target) {
            await lifecycleRpc(target, message.sleeping ? "/api/sleepSession" : "/api/wakeSession");
          }
        }));
        return;
      case "setGroupSleeping":
        await setGroupSleeping(message.groupId, message.sleeping);
        return;
      case "sleepInactiveProjectSessions":
        await transitionProjectSessions(message.groupId, "sleepInactive");
        return;
      case "wakeProjectSleepingSessions":
        await transitionProjectSessions(message.groupId, "wakeSleeping");
        return;
      case "closeInactiveProjectSessions":
        await transitionProjectSessions(message.groupId, "closeInactive");
        return;
      case "closeSession": {
        const target = parseSidebarSessionId(message.sessionId);
        if (target) {
          await lifecycleRpc(target, "/api/killSession");
        }
        return;
      }
      case "closeSessions":
        await Promise.all(message.sessionIds.map(async (sessionId) => {
          const target = parseSidebarSessionId(sessionId);
          if (target) {
            await lifecycleRpc(target, "/api/killSession");
          }
        }));
        return;
      case "forkSession": {
        const target = parseSidebarSessionId(message.sessionId);
        if (target) {
          const result = await rpcForMachine<GxserverForkSessionResult>(
            target.machineId,
            "/api/forkSession",
            lifecycleParams(target),
          );
          const createdTarget = {
            machineId: target.machineId,
            projectId: result.session.projectId,
            sessionId: result.session.sessionId,
          };
          activeTarget = createdTarget;
          focusedTarget = createdTarget;
          dispatchFocusSession(createdTarget, target.sessionId);
        }
        return;
      }
      case "renameSession": {
        const target = parseSidebarSessionId(message.sessionId);
        if (target) {
          await rpcForMachine(target.machineId, "/api/requestSessionRename", {
            ...(message.agentId ? { agentName: message.agentId } : {}),
            ...lifecycleParams(target),
            title: message.title,
            titleSource: message.shouldGenerateTitle ? "generated" : "user",
          });
        }
        return;
      }
      case "setSessionFavorite":
        await updateSession(message.sessionId, {
          isFavorite: message.favorite,
          sessionTag: message.favorite ? "favorite" : null,
        });
        return;
      case "setSessionTag":
        await updateSession(message.sessionId, {
          isFavorite: message.sessionTag === "favorite",
          sessionTag: message.sessionTag ?? null,
        });
        return;
      case "setSessionPinned":
        await updateSession(message.sessionId, { isPinned: message.pinned });
        return;
      case "syncSessionOrder":
        await syncSessionOrder(message.groupId, message.sessionIds);
        return;
      case "runSidebarAgent":
        if (message.groupId === GXSERVER_PRESENTATION_CHATS_GROUP_ID) {
          await createQuickSession("agent", message.agentId);
        } else {
          await createAgentSession(message.agentId, message.groupId);
        }
        return;
      case "renameWorkspaceProjectForGroup": {
        const target = parseSidebarGroupId(message.groupId);
        if (target) {
          await rpcForMachine(target.machineId, "/api/updateProject", {
            name: message.title,
            projectId: target.projectId,
          });
        }
        return;
      }
      case "closeWorkspaceProjectForGroup": {
        const target = parseSidebarGroupId(message.groupId);
        if (target) {
          const { recentProjects } = await rpcForMachine<{
            recentProjects: GxserverRecentProjectDomainState[];
          }>(target.machineId, "/api/closeProjectToRecent", { projectId: target.projectId });
          applyRecentProjects(target.machineId, recentProjects);
        }
        return;
      }
      case "removeWorkspaceProjectForGroup":
      {
        const target = parseSidebarGroupId(message.groupId);
        if (target) {
          await rpcForMachine(target.machineId, "/api/removeProject", {
            projectId: target.projectId,
          });
        }
        return;
      }
      case "restoreRecentProject":
      case "removeRecentProject": {
        const target = parseSidebarProjectId(message.projectId);
        if (target) {
          const { recentProjects } = await rpcForMachine<{
            recentProjects: GxserverRecentProjectDomainState[];
          }>(
            target.machineId,
            message.type === "restoreRecentProject"
              ? "/api/restoreRecentProject"
              : "/api/removeRecentProject",
            { projectId: target.projectId },
          );
          applyRecentProjects(target.machineId, recentProjects);
        }
        return;
      }
      case "cancelSidebarSessionFocusBorderHandoff":
      case "setSidebarSessionFocusBorderHandoffHitTarget":
      case "sidebarDebugLog":
      case "closeGroup":
      case "renameGroup":
        return;
      default:
        debugLog("nativeOnlyNoOp", { type: message.type });
    }
  };

  const setGroupSleeping = async (groupId: string, sleeping: boolean): Promise<void> => {
    const target = parseSidebarGroupId(groupId);
    if (!target) {
      return;
    }
    const sessions = projectSessions(getConnectionStates(), target);
    await Promise.all(sessions.map((session) => lifecycleRpc(
      { ...target, sessionId: session.sessionId },
      sleeping ? "/api/sleepSession" : "/api/wakeSession",
    )));
  };

  const transitionProjectSessions = async (
    groupId: string,
    action: "closeInactive" | "sleepInactive" | "wakeSleeping",
  ): Promise<void> => {
    const target = parseSidebarGroupId(groupId);
    if (!target) {
      return;
    }
    const sessions = projectSessions(getConnectionStates(), target).filter((session) => {
      if (action === "wakeSleeping") {
        return session.lifecycleState === "sleeping";
      }
      return session.activity === "idle";
    });
    await Promise.all(sessions.map((session) => lifecycleRpc(
      { ...target, sessionId: session.sessionId },
      action === "closeInactive"
        ? "/api/killSession"
        : action === "sleepInactive"
          ? "/api/sleepSession"
          : "/api/wakeSession",
    )));
  };

  const updateSession = async (sessionId: string, update: Record<string, unknown>): Promise<void> => {
    const target = parseSidebarSessionId(sessionId);
    if (target) {
      await rpcForMachine(target.machineId, "/api/updateSession", {
        ...update,
        projectId: target.projectId,
        sessionId: target.sessionId,
      });
    }
  };

  const syncSessionOrder = async (groupId: string, sessionIds: readonly string[]): Promise<void> => {
    const target = parseSidebarGroupId(groupId);
    if (!target) {
      return;
    }
    const routedIds = sessionIds.flatMap((sessionId) => {
      const session = parseSidebarSessionId(sessionId);
      return session?.machineId === target.machineId && session.projectId === target.projectId
        ? [session.sessionId]
        : [];
    });
    await rpcForMachine(target.machineId, "/api/updateSessionOrder", {
      projectId: target.projectId,
      sessionIds: routedIds,
    });
  };

  const createAgentSession = async (agentId: string, groupId?: string): Promise<void> => {
    const target = (groupId ? parseSidebarGroupId(groupId) : undefined) ?? activeTarget;
    if (!target || !agentId.trim()) {
      return;
    }
    await rpcForMachine(target.machineId, "/api/createAgentSession", {
      agentId: agentId.trim(),
      projectId: target.projectId,
      requireLaunchCommand: true,
      surface: "workspace",
      title: `${agentId.trim()} Session`,
    });
  };

  const onActiveSessionContext = (
    event: WindowEventMap["ghostex-web:activeSessionContext"],
  ): void => {
    const target = event.detail;
    if (!presentationHasSession(getConnectionStates(), target)) {
      pendingActiveSessionContext = target;
      return;
    }
    pendingActiveSessionContext = undefined;
    activeTarget = { machineId: target.machineId, projectId: target.projectId };
    focusedTarget = target;
    publish();
  };

  return {
    messageSource,
    start() {
      if (running) {
        return;
      }
      running = true;
      unsubscribeConnections = subscribeConnectionStates(publish);
      window.addEventListener("ghostex-web:activeSessionContext", onActiveSessionContext);
      queueMicrotask(publish);
    },
    stop() {
      running = false;
      unsubscribeConnections?.();
      unsubscribeConnections = undefined;
      window.removeEventListener("ghostex-web:activeSessionContext", onActiveSessionContext);
    },
    vscode: { postMessage },
  };
}

function dispatchFocusSession(
  target: SidebarSessionReference,
  placementTargetSessionId?: string,
): void {
  const detail: GhostexWebFocusSessionDetail = {
    ...target,
    placement: "focusedPane",
    ...(placementTargetSessionId ? { placementTargetSessionId } : {}),
    source: "sidebar",
  };
  window.dispatchEvent(new CustomEvent("ghostex-web:focusSession", { detail }));
  debugLog("focusSession", detail);
}

function createMergedSidebarGroups(
  states: readonly MachineConnectionState[],
  activeTarget: SidebarProjectReference | undefined,
  focusedTarget: SidebarSessionReference | undefined,
  projectMetadataByMachineId: ReadonlyMap<string, MachineProjectMetadata>,
): SidebarSessionGroup[] {
  return states.flatMap((state) => {
    const presentation = state.presentation;
    if (!presentation) {
      return [];
    }
    const projectMetadata = createProjectProjectionMetadata(
      projectMetadataByMachineId.get(state.machine.machineId)?.projects ?? [],
    );
    if (state.machine.machineId === "local") {
      return createGxserverPresentationSidebarGroups({
        activeProjectId: activeTarget?.machineId === "local" ? activeTarget.projectId : undefined,
        chatProjectIds: projectMetadata.chatProjectIds,
        focusedSessionId: focusedTarget?.machineId === "local" ? focusedTarget.sessionId : undefined,
        hiddenProjectIds: projectMetadata.hiddenProjectIds,
        presentation,
        projectOverlays: projectMetadata.projectOverlays,
        resolveAgentIcon,
        resolveSessionRoutingId: (projectId, sessionId) => `${projectId}:${sessionId}`,
      });
    }

    const sessionsByProject = createGxserverPresentationSessionsByProjectFromGroups({ presentation });
    return orderGxserverPresentationSidebarProjects(
      presentation.projects.filter((project) => !projectMetadata.hiddenProjectIds.has(project.projectId)),
    ).map((project) => {
      const machineId = state.machine.machineId;
      const group = createGxserverPresentationSidebarGroup({
        activeProjectId: activeTarget?.machineId === machineId ? activeTarget.projectId : undefined,
        createProjectGroupId: (projectId) => createSidebarGroupId(machineId, projectId),
        createProjectSessionId: (projectId, sessionId) =>
          createSidebarSessionId(machineId, projectId, sessionId),
        focusedSessionId: focusedTarget?.machineId === machineId ? focusedTarget.sessionId : undefined,
        project,
        resolveAgentIcon,
        resolveSessionRoutingId: (projectId, sessionId) => `${machineId}:${projectId}:${sessionId}`,
        sessions: sessionsByProject.get(project.projectId) ?? [],
      });
      const scopedProjectId = createSidebarProjectId(machineId, project.projectId);
      return {
        ...group,
        projectContext: group.projectContext
          ? {
              ...group.projectContext,
              canRemoveProject: false,
              editor: { ...group.projectContext.editor, projectId: scopedProjectId },
            }
          : undefined,
        remoteMachineContext: {
          machineId,
          machineName: state.machine.label,
        },
      };
    });
  });
}

function createProjectProjectionMetadata(
  projects: readonly GxserverProjectDomainState[],
): {
  chatProjectIds: ReadonlySet<string>;
  hiddenProjectIds: ReadonlySet<string>;
  projectOverlays: readonly GxserverPresentationSidebarProjectOverlay[];
} {
  const chatProjectIds = new Set<string>();
  const hiddenProjectIds = new Set<string>();
  const projectOverlays: GxserverPresentationSidebarProjectOverlay[] = [];
  for (const project of projects) {
    const isChatProject = isChatDomainProject(project);
    const isQuickProject = project.launchSettings.isQuick === true || isChatProject;
    if (isChatProject || isQuickProject) {
      chatProjectIds.add(project.projectId);
    }
    if (
      project.isRecentProject
      || project.visibility === "hidden"
      || project.systemKind === "remoteAttachCarrier"
    ) {
      hiddenProjectIds.add(project.projectId);
    }
    projectOverlays.push({
      isChatProject,
      isQuickProject,
      path: project.path,
      projectId: project.projectId,
      title: project.name,
    });
  }
  return { chatProjectIds, hiddenProjectIds, projectOverlays };
}

function isChatDomainProject(project: GxserverProjectDomainState): boolean {
  if (project.launchSettings.isChat === true) {
    return true;
  }
  const path = project.path?.replace(/\\/gu, "/").replace(/\/+$/u, "");
  return Boolean(path) && (
    /(?:^|\/)(?:ghostex|\.ghostex(?:-[^/]+)?|\.active)\/chats(?:\/|$)/u.test(path!)
    || /^~\/(?:ghostex|\.ghostex(?:-[^/]+)?|\.active)\/chats(?:\/|$)/u.test(path!)
  );
}

function createWebSidebarHud(
  groups: readonly SidebarSessionGroup[],
  focusedTarget: SidebarSessionReference | undefined,
  remoteHud: GxserverSidebarHudResponse | undefined,
  states: readonly MachineConnectionState[],
  recentProjectsByMachineId: ReadonlyMap<string, MachineRecentProjects>,
): SidebarHudState {
  const hud = createSidebarHudState(createDefaultSessionGridSnapshot(), "plain-dark");
  const visibleSessions = groups.flatMap((group) => group.sessions.filter((session) => session.isVisible));
  const focusedSessionId = focusedTarget
    ? createSidebarSessionId(
        focusedTarget.machineId,
        focusedTarget.projectId,
        focusedTarget.sessionId,
      )
    : undefined;
  const focusedSession = groups
    .flatMap((group) => group.sessions)
    .find((session) => session.sessionId === focusedSessionId);
  return {
    ...hud,
    agents: remoteHud?.agents.map((agent) => ({
      ...agent,
      icon: resolveAgentIcon(agent.icon ?? agent.agentId),
    })) ?? hud.agents,
    appIconPickerUnavailable: true,
    commands: remoteHud?.commands as SidebarHudState["commands"] ?? hud.commands,
    focusedSessionTitle:
      focusedSession?.displayTitle ?? focusedSession?.primaryTitle ?? focusedSession?.alias,
    recentProjects: states.flatMap(
      (state) => recentProjectsByMachineId.get(state.machine.machineId)?.projects ?? [],
    ),
    settings: {
      ...DEFAULT_ghostex_SETTINGS,
      remoteMachines: createRemoteMachineSettings(states),
    },
    visibleSlotLabels: visibleSessions.map((session) => session.shortcutLabel),
  };
}

function createRecentProjectsSignature(state: MachineConnectionState | undefined): string {
  if (!state) {
    return "missing";
  }
  return [
    state.status,
    ...(state.presentation?.projects.map((project) => `${project.projectId}:${project.updatedAt}`) ?? []),
  ].join("|");
}

function createWebRecentProjects(
  state: MachineConnectionState,
  recentProjects: readonly GxserverRecentProjectDomainState[],
): SidebarRecentProject[] {
  const machineId = state.machine.machineId;
  return recentProjects.flatMap((project) => {
    const projectId = String(project.projectId).trim();
    const path = project.path.trim();
    const title = project.title.trim();
    if (!projectId || !path || !title) {
      return [];
    }
    const icon = normalizeWorkspaceProjectIcon(project.icon);
    const iconDataUrl = normalizeWorkspaceProjectIconDataUrl(project.iconDataUrl);
    const themeColor = normalizeWorkspaceThemeColor(project.themeColor);
    return [{
      ...(icon ? { icon } : {}),
      ...(iconDataUrl ? { iconDataUrl } : {}),
      ...(project.recentClosedAt ? { recentClosedAt: project.recentClosedAt } : {}),
      ...(machineId === "local"
        ? {}
        : { remoteMachineId: machineId, remoteMachineName: state.machine.label }),
      ...(themeColor ? { themeColor } : {}),
      path,
      projectId: createSidebarProjectId(machineId, projectId),
      sessionCount: Number.isFinite(project.sessionCount)
        ? Math.max(0, Math.floor(project.sessionCount))
        : 0,
      title,
    }];
  });
}

function createRemoteMachineSettings(
  states: readonly MachineConnectionState[],
): RemoteMachineSettings[] {
  return states.flatMap((state) => state.machine.machineId === "local"
    ? []
    : [{
        id: state.machine.machineId,
        name: state.machine.label,
        sshHost: new URL(state.machine.baseUrl).hostname,
      }]);
}

function resolveAgentIcon(agentName: string | undefined): SidebarAgentButton["icon"] {
  const direct = getSidebarAgentIconById(agentName);
  if (direct) {
    return direct;
  }
  const normalized = agentName?.trim().toLowerCase();
  if (!normalized) {
    return undefined;
  }
  return DEFAULT_SIDEBAR_AGENTS.find((agent) =>
    agent.agentId === normalized
    || agent.name.trim().toLowerCase() === normalized
    || agent.icon === normalized
  )?.icon;
}

function publishMachineStatuses(
  states: readonly MachineConnectionState[],
  messageSource: WebSidebarMessageSource,
): void {
  for (const state of states) {
    if (state.machine.machineId === "local") {
      continue;
    }
    messageSource.postMessage({
      machineId: state.machine.machineId,
      ...(state.error ? { message: state.error } : {}),
      state: state.status,
      type: "remoteMachineStatus",
    });
  }
}

function reconcileActiveTarget(
  target: SidebarProjectReference | undefined,
  states: readonly MachineConnectionState[],
): SidebarProjectReference | undefined {
  return target && presentationHasProject(states, target) ? target : primaryProjectTarget(states);
}

function reconcileFocusedTarget(
  target: SidebarSessionReference | undefined,
  states: readonly MachineConnectionState[],
): SidebarSessionReference | undefined {
  return target && presentationHasSession(states, target) ? target : undefined;
}

function primaryProjectTarget(
  states: readonly MachineConnectionState[],
): SidebarProjectReference | undefined {
  const primary = states.find((state) => state.machine.machineId === "local" && state.presentation)
    ?? states.find((state) => state.presentation);
  const projectId = primary?.presentation?.projects[0]?.projectId;
  return primary && projectId ? { machineId: primary.machine.machineId, projectId } : undefined;
}

function presentationHasProject(
  states: readonly MachineConnectionState[],
  target: SidebarProjectReference,
): boolean {
  return states.some((state) =>
    state.machine.machineId === target.machineId
    && state.presentation?.projects.some((project) => project.projectId === target.projectId),
  );
}

function presentationHasSession(
  states: readonly MachineConnectionState[],
  target: SidebarSessionReference,
): boolean {
  return findPresentationSession(states, target) !== undefined;
}

function findPresentationSession(
  states: readonly MachineConnectionState[],
  target: SidebarSessionReference,
): GxserverPresentationSession | undefined {
  return states
    .find((state) => state.machine.machineId === target.machineId)
    ?.presentation?.sessions.find((session) =>
      session.projectId === target.projectId && session.sessionId === target.sessionId,
    );
}

function projectSessions(
  states: readonly MachineConnectionState[],
  target: SidebarProjectReference,
): readonly GxserverPresentationSession[] {
  return states
    .find((state) => state.machine.machineId === target.machineId)
    ?.presentation?.sessions.filter((session) => session.projectId === target.projectId) ?? [];
}

function lifecycleParams(target: SidebarSessionReference): Record<string, unknown> {
  return {
    projectId: target.projectId,
    reason: "ghostex-web-sidebar",
    sessionId: target.sessionId,
  };
}

function lifecycleRpc(
  target: SidebarSessionReference,
  endpoint: "/api/killSession" | "/api/sleepSession" | "/api/wakeSession",
): Promise<unknown> {
  return rpcForMachine(target.machineId, endpoint, lifecycleParams(target));
}

function debugLog(event: string, detail: unknown): void {
  if (window.localStorage.getItem(DEBUG_SIDEBAR_STORAGE_KEY) === "1") {
    console.info(`[ghostex-web sidebar] ${event} ${JSON.stringify(detail)}`);
  }
}
