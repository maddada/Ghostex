import {
  GXSERVER_PRESENTATION_CHATS_GROUP_ID,
  createGxserverPresentationSessionsByProjectFromGroups,
  createGxserverPresentationSidebarGroup,
  createGxserverPresentationSidebarGroups,
  orderGxserverPresentationSidebarProjects,
  type GxserverPresentationSidebarProjectOverlay,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type {
  GxserverPresentationSession,
  GxserverForkSessionResult,
  GxserverProjectDomainState,
  GxserverRecentProjectDomainState,
  GxserverSidebarHudResponse,
  GxserverSidebarProjectCollectionsState,
} from '@/packages/shared/gxserver-protocol';
import {
  createDefaultSessionGridSnapshot,
  createSidebarHudState,
  type ExtensionToSidebarMessage,
  type SidebarHudState,
  type SidebarRecentProject,
  type SidebarSessionGroup,
  type SidebarToExtensionMessage,
} from '@/packages/shared/session-grid-contract';
import {
  DEFAULT_SIDEBAR_AGENTS,
  getSidebarAgentIconById,
  type SidebarAgentButton,
} from '@/packages/shared/sidebar-agents';
import { type RemoteMachineSettings, type ghostexSettings } from '@/packages/shared/ghostex-settings';
import { readWebSettings } from '../app/web-settings';
import {
  normalizeWorkspaceProjectIcon,
  normalizeWorkspaceProjectIconDataUrl,
  normalizeWorkspaceThemeColor,
} from '@/packages/shared/workspace-project-appearance';
import type { OpenAddProjectModalDetail } from '../app/action-events';
import { getConnectionStates, rpcForMachine, subscribeConnectionStates } from '../connections/connection-registry';
import type { MachineConnectionState } from '../connections/types';
import { getMachineCatalogState, reorderRemoteMachines } from '../machines/machine-catalog';
import { orderMachineConnectionStates } from '../machines/machine-order';
import {
  createSidebarGroupId,
  createSidebarProjectId,
  createSidebarSessionId,
  parseSidebarGroupId,
  parseSidebarProjectId,
  parseSidebarSessionId,
  type SidebarProjectReference,
  type SidebarSessionReference,
} from './sidebar-ids';
import { setActiveSidebarProject } from './active-project-store';
import type { NavigationHistoryEntry } from '@/packages/shared/navigation-history/navigation-history-contract';
import { NAVIGATION_HISTORY_SCOPE_WEB } from '@/packages/shared/navigation-history/navigation-history-contract';
import {
  NavigationHistoryController,
  type NavigationHistoryRpc,
} from '@/packages/shared/navigation-history/navigation-history-controller';
import {
  installNavigationHistoryHotkeys,
  navigationHistoryHotkeyDirection,
} from '@/packages/shared/navigation-history/navigation-history-hotkeys';

const DEBUG_SIDEBAR_STORAGE_KEY = 'ghostexWeb.debugSidebar';
const DEFAULT_TERMINAL_TITLE = 'Terminal';

type SidebarMessageSource = Pick<EventTarget, 'addEventListener' | 'removeEventListener'>;

type MachineProjectMetadata = {
  projects: readonly GxserverProjectDomainState[];
  signature: string;
};

type MachineRecentProjects = {
  projects: readonly SidebarRecentProject[];
  signature: string;
};

export type GhostexWebFocusSessionDetail = SidebarSessionReference & {
  placement: 'focusedPane';
  placementTargetSessionId?: string;
  source: 'sidebar';
};

declare global {
  interface WindowEventMap {
    'ghostex-web:focusSession': CustomEvent<GhostexWebFocusSessionDetail>;
    'ghostex-web:activeSessionContext': CustomEvent<SidebarSessionReference>;
    'ghostex-web:openFindPrompts': CustomEvent<undefined>;
  }
}

class WebSidebarMessageSource extends EventTarget {
  postMessage(message: ExtensionToSidebarMessage): void {
    this.dispatchEvent(new MessageEvent<ExtensionToSidebarMessage>('message', { data: message }));
  }
}

export type WebSidebarRuntime = {
  messageSource: SidebarMessageSource;
  /**
   * CDXC:NavigationHistory 2026-08-19:
   * The titlebar's Back/Forward pair reads this controller. It is owned by the
   * runtime, not the titlebar component, because the trail is fed by every
   * active-target change the runtime publishes — not only by clicks.
   */
  navigationHistory: NavigationHistoryController;
  start(): void;
  stop(): void;
  updateSettings(settings: ghostexSettings): void;
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
  let uninstallNavigationHistoryHotkeys: (() => void) | undefined;
  let hudRequestKey = '';
  let remoteHud: GxserverSidebarHudResponse | undefined;
  let settings = readWebSettings();
  const projectMetadataByMachineId = new Map<string, MachineProjectMetadata>();
  const projectMetadataRequestSignatures = new Map<string, string>();
  const recentProjectsByMachineId = new Map<string, MachineRecentProjects>();
  const recentProjectsRequestSignatures = new Map<string, string>();
  const pendingRecentProjectMutations = new Map<string, Promise<void>>();

  /*
   * CDXC:NavigationHistory 2026-08-19:
   * The trail lives on ONE daemon even though entries can point at several
   * machines, because a back stack split per machine has no meaningful order.
   * The local daemon (the one serving this page) owns it; if it is not
   * connected, the first connected machine stands in and the buttons simply
   * stay disabled until some daemon answers.
   */
  const navigationHistoryOwnerMachineId = (): string | undefined => {
    const states = getConnectionStates();
    const local = states.find((state) => state.machine.machineId === 'local');
    return (local ?? states[0])?.machine.machineId;
  };

  const navigationHistoryRpc = (): NavigationHistoryRpc | undefined => {
    const machineId = navigationHistoryOwnerMachineId();
    if (!machineId) {
      return undefined;
    }
    return (path, params) => rpcForMachine<unknown>(machineId, path, params);
  };

  const activateNavigationHistoryEntry = (entry: NavigationHistoryEntry): boolean => {
    const states = getConnectionStates();
    const sessionTarget = entry.sessionId ? parseSidebarSessionId(entry.sessionId) : undefined;
    if (sessionTarget) {
      if (!presentationHasSession(states, sessionTarget)) {
        return false;
      }
      void focusSession(entry.sessionId as string);
      return true;
    }
    const projectTarget = entry.groupId ? parseSidebarGroupId(entry.groupId) : parseSidebarProjectId(entry.projectId);
    if (!projectTarget || !presentationHasProject(states, projectTarget)) {
      return false;
    }
    activeTarget = projectTarget;
    hudRequestKey = '';
    publish();
    return true;
  };

  const navigationHistory = new NavigationHistoryController({
    activate: activateNavigationHistoryEntry,
    onError: (error) =>
      debugLog('navigationHistoryError', {
        error: error instanceof Error ? error.message : String(error),
      }),
    resolveRpc: navigationHistoryRpc,
    scopeId: NAVIGATION_HISTORY_SCOPE_WEB,
  });

  const publish = (): void => {
    if (!running) {
      return;
    }
    const states = orderMachineConnectionStates(getConnectionStates(), getMachineCatalogState().machines);
    if (pendingActiveSessionContext && presentationHasSession(states, pendingActiveSessionContext)) {
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
    // Coalesced inside the controller: an unchanged target costs one string
    // compare here and never reaches the daemon.
    navigationHistory.recordVisit(createNavigationHistoryEntry(states, activeTarget, focusedTarget));
    const groups = createMergedSidebarGroups(states, activeTarget, focusedTarget, projectMetadataByMachineId);
    debugLog('publish', {
      groupCount: groups.length,
      groups: groups.slice(0, 8).map((group) => ({
        sessionCount: group.sessions.length,
        title: group.title,
      })),
    });
    const hud = createWebSidebarHud(groups, focusedTarget, remoteHud, states, recentProjectsByMachineId, settings);
    const localSidebarProjectCollections = states.find((state) => state.machine.machineId === 'local')?.presentation
      ?.sidebarProjectCollections;
    const remoteSidebarProjectCollectionsByMachineId = Object.fromEntries(
      states.flatMap((state) => {
        const collections = state.presentation?.sidebarProjectCollections;
        return state.machine.machineId !== 'local' && collections
          ? [[state.machine.machineId, collections] as const]
          : [];
      })
    );
    const message: ExtensionToSidebarMessage = {
      groups,
      hud,
      pinnedPrompts: [],
      previousSessions: [],
      remoteSidebarProjectCollectionsByMachineId,
      revision: ++revision,
      scratchPadContent: '',
      ...(localSidebarProjectCollections ? { sidebarProjectCollections: localSidebarProjectCollections } : {}),
      type: hasHydrated ? 'sessionState' : 'hydrate',
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
      getConnectionStates().find((state) => state.machine.machineId === machineId)
    )
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
      if (state.status !== 'connected') {
        continue;
      }
      const machineId = state.machine.machineId;
      const signature = createRecentProjectsSignature(state);
      if (
        recentProjectsByMachineId.get(machineId)?.signature === signature ||
        recentProjectsRequestSignatures.get(machineId) === signature
      ) {
        continue;
      }
      recentProjectsRequestSignatures.set(machineId, signature);
      void rpcForMachine<{ recentProjects: GxserverRecentProjectDomainState[] }>(machineId, '/api/listRecentProjects')
        .then(({ recentProjects }) => {
          if (!running || recentProjectsRequestSignatures.get(machineId) !== signature) {
            return;
          }
          recentProjectsRequestSignatures.delete(machineId);
          applyRecentProjects(machineId, recentProjects, signature);
        })
        .catch((error: unknown) => {
          if (recentProjectsRequestSignatures.get(machineId) === signature) {
            recentProjectsRequestSignatures.delete(machineId);
          }
          debugLog('recentProjectsError', {
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
      const signature = presentation.projects.map((project) => `${project.projectId}:${project.updatedAt}`).join('|');
      if (
        projectMetadataByMachineId.get(state.machine.machineId)?.signature === signature ||
        projectMetadataRequestSignatures.get(state.machine.machineId) === signature
      ) {
        continue;
      }
      projectMetadataRequestSignatures.set(state.machine.machineId, signature);
      void rpcForMachine<{ projects: GxserverProjectDomainState[] }>(state.machine.machineId, '/api/listProjects')
        .then(({ projects }) => {
          if (!running || projectMetadataRequestSignatures.get(state.machine.machineId) !== signature) {
            return;
          }
          projectMetadataByMachineId.set(state.machine.machineId, { projects, signature });
          debugLog('projectMetadata', {
            chatProjectCount: projects.filter(isChatDomainProject).length,
            machineId: state.machine.machineId,
            projectCount: projects.length,
          });
          publish();
        })
        .catch((error: unknown) => {
          if (projectMetadataRequestSignatures.get(state.machine.machineId) === signature) {
            projectMetadataRequestSignatures.delete(state.machine.machineId);
          }
          debugLog('projectMetadataError', {
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
    void rpcForMachine<GxserverSidebarHudResponse>(target.machineId, '/api/readSidebarHud', {
      activeProjectId: target.projectId,
    })
      .then((hud) => {
        if (!running || hudRequestKey !== requestKey) {
          return;
        }
        remoteHud = hud;
        publish();
      })
      .catch(() => {
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
    if (session?.activity === 'attention') {
      void rpcForMachine(target.machineId, '/api/updateAgentActivity', {
        ...(session.agentName ? { agentName: session.agentName } : {}),
        event: 'acknowledge',
        projectId: target.projectId,
        sessionId: target.sessionId,
      });
    }
    publish();
    dispatchFocusSession(target);
  };

  const createSession = async (groupId?: string): Promise<void> => {
    const target =
      (groupId ? parseSidebarGroupId(groupId) : undefined) ??
      activeTarget ??
      primaryProjectTarget(getConnectionStates());
    if (!target) {
      return;
    }
    const result = await rpcForMachine<{
      session?: { projectId?: string; sessionId?: string };
    }>(target.machineId, '/api/createSession', {
      kind: 'terminal',
      lifecycleState: 'running',
      projectId: target.projectId,
      surface: 'workspace',
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

  const createQuickSession = async (kind: 'agent' | 'terminal', agentId?: string): Promise<void> => {
    const machineId =
      activeTarget?.machineId ??
      getConnectionStates().find((state) => state.machine.machineId === 'local')?.machine.machineId;
    if (!machineId) {
      return;
    }
    const { project } = await rpcForMachine<{ project: GxserverProjectDomainState }>(
      machineId,
      '/api/createQuickProject',
      { kind }
    );
    activeTarget = { machineId, projectId: project.projectId };
    hudRequestKey = '';
    if (kind === 'agent' && agentId) {
      await createAgentSession(agentId);
      return;
    }
    await createSession(createSidebarGroupId(machineId, project.projectId));
  };

  const postMessage = (message: SidebarToExtensionMessage): void => {
    void handleSidebarMessage(message).catch((error: unknown) => {
      debugLog('actionError', {
        error: error instanceof Error ? error.message : String(error),
        type: message.type,
      });
    });
  };

  const handleSidebarMessage = async (message: SidebarToExtensionMessage): Promise<void> => {
    switch (message.type) {
      case 'focusSession':
      case 'focusSessionMode':
        await focusSession(message.sessionId);
        return;
      case 'focusGroup': {
        const target = parseSidebarGroupId(message.groupId);
        if (target) {
          activeTarget = target;
          hudRequestKey = '';
          publish();
        }
        return;
      }
      case 'createSession':
      case 'createFullWidthTerminalPane':
        await createSession();
        return;
      case 'createChat':
        await createQuickSession('terminal');
        return;
      case 'createSessionInGroup':
        await createSession(message.groupId);
        return;
      case 'setSessionSleeping': {
        const target = parseSidebarSessionId(message.sessionId);
        if (target) {
          await lifecycleRpc(target, message.sleeping ? '/api/sleepSession' : '/api/wakeSession');
        }
        return;
      }
      case 'setSessionsSleeping':
        await Promise.all(
          message.sessionIds.map(async (sessionId) => {
            const target = parseSidebarSessionId(sessionId);
            if (target) {
              await lifecycleRpc(target, message.sleeping ? '/api/sleepSession' : '/api/wakeSession');
            }
          })
        );
        return;
      case 'setGroupSleeping':
        await setGroupSleeping(message.groupId, message.sleeping);
        return;
      case 'sleepInactiveProjectSessions':
        await transitionProjectSessions(message.groupId, 'sleepInactive');
        return;
      case 'wakeProjectSleepingSessions':
        await transitionProjectSessions(message.groupId, 'wakeSleeping');
        return;
      case 'closeInactiveProjectSessions':
        await transitionProjectSessions(message.groupId, 'closeInactive');
        return;
      case 'closeSession': {
        const target = parseSidebarSessionId(message.sessionId);
        if (target) {
          await lifecycleRpc(target, '/api/killSession');
        }
        return;
      }
      case 'closeSessions':
        await Promise.all(
          message.sessionIds.map(async (sessionId) => {
            const target = parseSidebarSessionId(sessionId);
            if (target) {
              await lifecycleRpc(target, '/api/killSession');
            }
          })
        );
        return;
      case 'forkSession': {
        const target = parseSidebarSessionId(message.sessionId);
        if (target) {
          const result = await rpcForMachine<GxserverForkSessionResult>(
            target.machineId,
            '/api/forkSession',
            lifecycleParams(target)
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
      case 'renameSession': {
        const target = parseSidebarSessionId(message.sessionId);
        if (target) {
          await rpcForMachine(target.machineId, '/api/requestSessionRename', {
            ...(message.agentId ? { agentName: message.agentId } : {}),
            ...lifecycleParams(target),
            title: message.title,
            titleSource: message.shouldGenerateTitle ? 'generated' : 'user',
          });
        }
        return;
      }
      case 'setSessionFavorite':
        await updateSession(message.sessionId, {
          isFavorite: message.favorite,
          sessionTag: message.favorite ? 'favorite' : null,
        });
        return;
      case 'setSessionTag':
        await updateSession(message.sessionId, {
          isFavorite: message.sessionTag === 'favorite',
          sessionTag: message.sessionTag ?? null,
        });
        return;
      /*
      CDXC:SessionAgentNotes 2026-08-24:
      The note is keyed by the session's PROVIDER conversation id, which only
      the daemon can resolve, so the client sends the session reference and the
      text and nothing else. No optimistic patch: gxserver schedules a
      presentation delta after a successful save, and that delta is what puts
      the note on the row. An empty note is the explicit clear.
      */
      case 'setSessionNote': {
        const target = parseSidebarSessionId(message.sessionId);
        if (target) {
          await rpcForMachine(target.machineId, '/api/saveSessionAgentNote', {
            note: message.note,
            projectId: target.projectId,
            sessionId: target.sessionId,
          });
        }
        return;
      }
      case 'setSessionPinned':
        await updateSession(message.sessionId, { isPinned: message.pinned });
        return;
      case 'setSessionParked':
        await updateSession(message.sessionId, { isParked: message.parked });
        return;
      case 'syncSessionOrder':
        await syncSessionOrder(message.groupId, message.sessionIds);
        return;
      case 'syncGroupOrder': {
        const targets = message.groupIds.flatMap((groupId) => {
          const target = parseSidebarGroupId(groupId);
          return target ? [target] : [];
        });
        const machineId = targets[0]?.machineId;
        if (
          !machineId ||
          targets.length !== message.groupIds.length ||
          targets.some((target) => target.machineId !== machineId)
        ) {
          return;
        }
        const presentation = getConnectionStates().find((state) => state.machine.machineId === machineId)?.presentation;
        await rpcForMachine(machineId, '/api/updateWorkspaceSessionGroups', {
          state: {
            projectOrder: targets.map((target) => target.projectId),
            projects: presentation?.workspaceGroups?.projects ?? {},
          },
        });
        return;
      }
      case 'updateSidebarProjectCollections': {
        const machineId = message.remoteMachineId ?? 'local';
        await rpcForMachine<{ sidebarProjectCollections: GxserverSidebarProjectCollectionsState }>(
          machineId,
          '/api/updateSidebarProjectCollections',
          { state: message.state }
        );
        return;
      }
      case 'updateSettingsPatch': {
        if (message.source === 'sidebar:remoteMachineOrder' && message.patch.remoteMachines) {
          const changed = reorderRemoteMachines(message.patch.remoteMachines.map((machine) => machine.id));
          if (changed) {
            publish();
          }
        }
        return;
      }
      case 'runSidebarAgent':
        if (message.groupId === GXSERVER_PRESENTATION_CHATS_GROUP_ID) {
          await createQuickSession('agent', message.agentId);
        } else {
          await createAgentSession(message.agentId, message.groupId);
        }
        return;
      case 'renameWorkspaceProjectForGroup': {
        const target = parseSidebarGroupId(message.groupId);
        if (target) {
          await rpcForMachine(target.machineId, '/api/updateProject', {
            name: message.title,
            projectId: target.projectId,
          });
        }
        return;
      }
      case 'closeWorkspaceProjectForGroup': {
        const target = parseSidebarGroupId(message.groupId);
        if (target) {
          const { recentProjects } = await rpcForMachine<{
            recentProjects: GxserverRecentProjectDomainState[];
          }>(target.machineId, '/api/closeProjectToRecent', { projectId: target.projectId });
          applyRecentProjects(target.machineId, recentProjects);
        }
        return;
      }
      case 'requestRecentProjects': {
        const requestedMachineId = message.machineId;
        const state = resolveRecentProjectsMachineState(getConnectionStates(), requestedMachineId);
        let recentProjects: SidebarRecentProject[] = [];
        if (state) {
          const pendingMutation = pendingRecentProjectMutations.get(state.machine.machineId);
          if (pendingMutation) {
            try {
              await pendingMutation;
            } catch {
              // The action path reports its own failure; still refresh the canonical list.
            }
          }
          try {
            const result = await rpcForMachine<{
              recentProjects: GxserverRecentProjectDomainState[];
            }>(state.machine.machineId, '/api/listRecentProjects');
            recentProjects = createWebRecentProjects(state, result.recentProjects);
            recentProjectsByMachineId.set(state.machine.machineId, {
              projects: recentProjects,
              signature: createRecentProjectsSignature(state),
            });
          } catch (error) {
            debugLog('requestRecentProjectsError', {
              error: error instanceof Error ? error.message : String(error),
              machineId: state.machine.machineId,
            });
          }
        }
        messageSource.postMessage({
          ...(requestedMachineId === undefined ? {} : { machineId: requestedMachineId }),
          recentProjects,
          type: 'recentProjectsResult',
        });
        return;
      }
      case 'removeWorkspaceProjectForGroup': {
        const target = parseSidebarGroupId(message.groupId);
        if (target) {
          await rpcForMachine(target.machineId, '/api/removeProject', {
            projectId: target.projectId,
          });
        }
        return;
      }
      case 'restoreRecentProject':
      case 'removeRecentProject': {
        const target = parseSidebarProjectId(message.projectId);
        if (target) {
          const mutation = rpcForMachine<{
            recentProjects: GxserverRecentProjectDomainState[];
          }>(
            target.machineId,
            message.type === 'restoreRecentProject' ? '/api/restoreRecentProject' : '/api/removeRecentProject',
            { projectId: target.projectId }
          ).then(({ recentProjects }) => {
            applyRecentProjects(target.machineId, recentProjects);
          });
          pendingRecentProjectMutations.set(target.machineId, mutation);
          try {
            await mutation;
          } finally {
            if (pendingRecentProjectMutations.get(target.machineId) === mutation) {
              pendingRecentProjectMutations.delete(target.machineId);
            }
          }
        }
        return;
      }
      case 'copyRecentProjectPath': {
        const target = parseSidebarProjectId(message.projectId);
        const project = target
          ? recentProjectsByMachineId
              .get(target.machineId)
              ?.projects.find((candidate) => candidate.projectId === message.projectId)
          : undefined;
        if (project) {
          await navigator.clipboard.writeText(project.path);
        }
        return;
      }
      case 'copyWorkspaceProjectRemoteUrl': {
        const remoteUrl = message.remoteUrl.trim();
        if (remoteUrl) {
          await navigator.clipboard.writeText(remoteUrl);
        }
        return;
      }
      case 'pickWorkspaceFolder': {
        /*
         * CDXC:AddProject 2026-07-30:
         * `pickWorkspaceFolder` is the sidebar's local "Add project" affordance
         * (Projects header button and the command palette's Add Project
         * command). Native opens an OS folder picker, which the browser has no
         * equivalent for, so web answers the same intent with the shared
         * add-project dialog instead of no-oping. Nothing is optimistic: the
         * added project reaches the sidebar as an ordinary presentation delta.
         */
        const detail: OpenAddProjectModalDetail = {};
        window.dispatchEvent(new CustomEvent('ghostex-web:openAddProjectModal', { detail }));
        return;
      }
      case 'searchPreviousSessionsByText': {
        window.dispatchEvent(new CustomEvent('ghostex-web:openFindPrompts'));
        return;
      }
      case 'runGhostexHotkeyAction': {
        /*
         * CDXC:NavigationHistory 2026-08-19:
         * The shared command palette forwards host-owned hotkey rows as action
         * ids. Back/Forward, Find, and Open Commands Panel are owned here;
         * everything else stays a native-only no-op.
         */
        const direction = navigationHistoryHotkeyDirection(message.actionId);
        if (direction) {
          void navigationHistory.navigate(direction);
          return;
        }
        /*
         * CDXC:AgentHistorySearch 2026-08-20:
         * Find is an app-level modal, matching Settings. The root modal host
         * owns presentation, so native-style command actions only announce
         * the intent here and never mutate the focused workspace pane.
         */
        if (message.actionId === 'openFindPrompts') {
          window.dispatchEvent(new CustomEvent('ghostex-web:openFindPrompts'));
          return;
        }
        if (message.actionId === 'openCommandsPanel') {
          window.dispatchEvent(new CustomEvent('ghostex-web:openCommandPane', { detail: { toggle: true } }));
          return;
        }
        debugLog('nativeOnlyNoOp', { actionId: message.actionId, type: message.type });
        return;
      }
      case 'openRecentProjectInFinder':
        console.warn('[ghostex-web] Open in Finder is unavailable in the browser.');
        return;
      case 'cancelSidebarSessionFocusBorderHandoff':
      case 'setSidebarSessionFocusBorderHandoffHitTarget':
      case 'sidebarDebugLog':
      case 'closeGroup':
      case 'renameGroup':
        return;
      default:
        debugLog('nativeOnlyNoOp', { type: message.type });
    }
  };

  const setGroupSleeping = async (groupId: string, sleeping: boolean): Promise<void> => {
    const target = parseSidebarGroupId(groupId);
    if (!target) {
      return;
    }
    const sessions = projectSessions(getConnectionStates(), target);
    await Promise.all(
      sessions.map((session) =>
        lifecycleRpc({ ...target, sessionId: session.sessionId }, sleeping ? '/api/sleepSession' : '/api/wakeSession')
      )
    );
  };

  const transitionProjectSessions = async (
    groupId: string,
    action: 'closeInactive' | 'sleepInactive' | 'wakeSleeping'
  ): Promise<void> => {
    const target = parseSidebarGroupId(groupId);
    if (!target) {
      return;
    }
    const sessions = projectSessions(getConnectionStates(), target).filter((session) => {
      if (action === 'wakeSleeping') {
        return session.lifecycleState === 'sleeping';
      }
      return session.activity === 'idle';
    });
    await Promise.all(
      sessions.map((session) =>
        lifecycleRpc(
          { ...target, sessionId: session.sessionId },
          action === 'closeInactive'
            ? '/api/killSession'
            : action === 'sleepInactive'
              ? '/api/sleepSession'
              : '/api/wakeSession'
        )
      )
    );
  };

  const updateSession = async (sessionId: string, update: Record<string, unknown>): Promise<void> => {
    const target = parseSidebarSessionId(sessionId);
    if (target) {
      await rpcForMachine(target.machineId, '/api/updateSession', {
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
    await rpcForMachine(target.machineId, '/api/updateSessionOrder', {
      projectId: target.projectId,
      sessionIds: routedIds,
    });
  };

  const createAgentSession = async (agentId: string, groupId?: string): Promise<void> => {
    const target = (groupId ? parseSidebarGroupId(groupId) : undefined) ?? activeTarget;
    if (!target || !agentId.trim()) {
      return;
    }
    await rpcForMachine(target.machineId, '/api/createAgentSession', {
      agentId: agentId.trim(),
      projectId: target.projectId,
      requireLaunchCommand: true,
      surface: 'workspace',
      title: `${agentId.trim()} Session`,
    });
  };

  const onActiveSessionContext = (event: WindowEventMap['ghostex-web:activeSessionContext']): void => {
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
    navigationHistory,
    start() {
      if (running) {
        return;
      }
      running = true;
      unsubscribeConnections = subscribeConnectionStates(publish);
      window.addEventListener('ghostex-web:activeSessionContext', onActiveSessionContext);
      uninstallNavigationHistoryHotkeys = installNavigationHistoryHotkeys({
        navigate: (direction) => void navigationHistory.navigate(direction),
        readHotkeys: () => settings.hotkeys,
      });
      queueMicrotask(publish);
      // Adopt the trail this scope already has on the daemon, so a page reload
      // keeps Back working instead of starting from an empty stack.
      void navigationHistory.refresh();
    },
    stop() {
      running = false;
      unsubscribeConnections?.();
      unsubscribeConnections = undefined;
      window.removeEventListener('ghostex-web:activeSessionContext', onActiveSessionContext);
      uninstallNavigationHistoryHotkeys?.();
      uninstallNavigationHistoryHotkeys = undefined;
    },
    updateSettings(nextSettings) {
      settings = nextSettings;
      publish();
    },
    vscode: { postMessage },
  };
}

function dispatchFocusSession(target: SidebarSessionReference, placementTargetSessionId?: string): void {
  const detail: GhostexWebFocusSessionDetail = {
    ...target,
    placement: 'focusedPane',
    ...(placementTargetSessionId ? { placementTargetSessionId } : {}),
    source: 'sidebar',
  };
  window.dispatchEvent(new CustomEvent('ghostex-web:focusSession', { detail }));
  debugLog('focusSession', detail);
}

function createMergedSidebarGroups(
  states: readonly MachineConnectionState[],
  activeTarget: SidebarProjectReference | undefined,
  focusedTarget: SidebarSessionReference | undefined,
  projectMetadataByMachineId: ReadonlyMap<string, MachineProjectMetadata>
): SidebarSessionGroup[] {
  return states.flatMap((state) => {
    const presentation = state.presentation;
    if (!presentation) {
      return [];
    }
    const projectMetadata = createProjectProjectionMetadata(
      projectMetadataByMachineId.get(state.machine.machineId)?.projects ?? [],
      presentation.workspaceGroups?.projectOrder
    );
    if (state.machine.machineId === 'local') {
      return createGxserverPresentationSidebarGroups({
        activeProjectId: activeTarget?.machineId === 'local' ? activeTarget.projectId : undefined,
        chatProjectIds: projectMetadata.chatProjectIds,
        focusedSessionId: focusedTarget?.machineId === 'local' ? focusedTarget.sessionId : undefined,
        hiddenProjectIds: projectMetadata.hiddenProjectIds,
        presentation,
        projectOverlays: projectMetadata.projectOverlays,
        resolveAgentIcon,
        resolveSessionRoutingId: (projectId, sessionId) => `${projectId}:${sessionId}`,
      });
    }

    const sessionsByProject = createGxserverPresentationSessionsByProjectFromGroups({ presentation });
    const projectOrderIndex = new Map(
      (presentation.workspaceGroups?.projectOrder ?? []).map((projectId, index) => [projectId, index])
    );
    const visibleProjects = presentation.projects.filter(
      (project) => !projectMetadata.hiddenProjectIds.has(project.projectId)
    );
    const orderedProjects = [
      ...visibleProjects
        .filter((project) => projectOrderIndex.has(project.projectId))
        .sort((left, right) => projectOrderIndex.get(left.projectId)! - projectOrderIndex.get(right.projectId)!),
      ...orderGxserverPresentationSidebarProjects(
        visibleProjects.filter((project) => !projectOrderIndex.has(project.projectId))
      ),
    ];
    return orderedProjects.map((project) => {
      const machineId = state.machine.machineId;
      const group = createGxserverPresentationSidebarGroup({
        activeProjectId: activeTarget?.machineId === machineId ? activeTarget.projectId : undefined,
        createProjectGroupId: (projectId) => createSidebarGroupId(machineId, projectId),
        createProjectSessionId: (projectId, sessionId) => createSidebarSessionId(machineId, projectId, sessionId),
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
  projectOrder: readonly string[] | undefined
): {
  chatProjectIds: ReadonlySet<string>;
  hiddenProjectIds: ReadonlySet<string>;
  projectOverlays: readonly GxserverPresentationSidebarProjectOverlay[];
} {
  const chatProjectIds = new Set<string>();
  const hiddenProjectIds = new Set<string>();
  const projectOverlays: GxserverPresentationSidebarProjectOverlay[] = [];
  const orderIndexByProjectId = new Map((projectOrder ?? []).map((projectId, index) => [projectId, index]));
  const projectedProjectIds = new Set<string>();
  for (const project of projects) {
    const orderIndex = orderIndexByProjectId.get(project.projectId);
    const isChatProject = isChatDomainProject(project);
    const isQuickProject = project.launchSettings.isQuick === true || isChatProject;
    if (isChatProject || isQuickProject) {
      chatProjectIds.add(project.projectId);
    }
    if (project.isRecentProject || project.visibility === 'hidden' || project.systemKind === 'remoteAttachCarrier') {
      hiddenProjectIds.add(project.projectId);
    }
    projectedProjectIds.add(project.projectId);
    projectOverlays.push({
      isChatProject,
      isQuickProject,
      path: project.path,
      projectId: project.projectId,
      ...(orderIndex === undefined ? {} : { orderIndex }),
      title: project.name,
    });
  }
  for (const [projectId, orderIndex] of orderIndexByProjectId) {
    if (!projectedProjectIds.has(projectId)) {
      projectOverlays.push({ orderIndex, projectId });
    }
  }
  return { chatProjectIds, hiddenProjectIds, projectOverlays };
}

function isChatDomainProject(project: GxserverProjectDomainState): boolean {
  if (project.launchSettings.isChat === true) {
    return true;
  }
  const path = project.path?.replace(/\\/gu, '/').replace(/\/+$/u, '');
  return (
    Boolean(path) &&
    (/(?:^|\/)(?:ghostex|\.ghostex(?:-[^/]+)?|\.active)\/chats(?:\/|$)/u.test(path!) ||
      /^~\/(?:ghostex|\.ghostex(?:-[^/]+)?|\.active)\/chats(?:\/|$)/u.test(path!))
  );
}

function createWebSidebarHud(
  groups: readonly SidebarSessionGroup[],
  focusedTarget: SidebarSessionReference | undefined,
  remoteHud: GxserverSidebarHudResponse | undefined,
  states: readonly MachineConnectionState[],
  recentProjectsByMachineId: ReadonlyMap<string, MachineRecentProjects>,
  settings: ghostexSettings
): SidebarHudState {
  const hud = createSidebarHudState(createDefaultSessionGridSnapshot(), 'plain-dark');
  const visibleSessions = groups.flatMap((group) => group.sessions.filter((session) => session.isVisible));
  const focusedSessionId = focusedTarget
    ? createSidebarSessionId(focusedTarget.machineId, focusedTarget.projectId, focusedTarget.sessionId)
    : undefined;
  const focusedSession = groups
    .flatMap((group) => group.sessions)
    .find((session) => session.sessionId === focusedSessionId);
  return {
    ...hud,
    agents:
      remoteHud?.agents.map((agent) => ({
        ...agent,
        icon: resolveAgentIcon(agent.icon ?? agent.agentId),
      })) ?? hud.agents,
    appIconPickerUnavailable: true,
    commands: (remoteHud?.commands as SidebarHudState['commands']) ?? hud.commands,
    focusedSessionTitle: focusedSession?.displayTitle ?? focusedSession?.primaryTitle ?? focusedSession?.alias,
    recentProjects: states.flatMap((state) => recentProjectsByMachineId.get(state.machine.machineId)?.projects ?? []),
    settings: {
      ...settings,
      remoteMachines: createRemoteMachineSettings(states),
    },
    visibleSlotLabels: visibleSessions.map((session) => session.shortcutLabel),
  };
}

function createRecentProjectsSignature(state: MachineConnectionState | undefined): string {
  if (!state) {
    return 'missing';
  }
  return [
    state.status,
    ...(state.presentation?.projects.map((project) => `${project.projectId}:${project.updatedAt}`) ?? []),
  ].join('|');
}

function resolveRecentProjectsMachineState(
  states: readonly MachineConnectionState[],
  machineId: string | undefined
): MachineConnectionState | undefined {
  const requestedMachineId = machineId ?? 'local';
  return states.find((state) => state.machine.machineId === requestedMachineId);
}

function createWebRecentProjects(
  state: MachineConnectionState,
  recentProjects: readonly GxserverRecentProjectDomainState[]
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
    return [
      {
        ...(icon ? { icon } : {}),
        ...(iconDataUrl ? { iconDataUrl } : {}),
        ...(project.recentClosedAt ? { recentClosedAt: project.recentClosedAt } : {}),
        ...(machineId === 'local' ? {} : { remoteMachineId: machineId, remoteMachineName: state.machine.label }),
        ...(themeColor ? { themeColor } : {}),
        path,
        projectId: createSidebarProjectId(machineId, projectId),
        sessionCount: Number.isFinite(project.sessionCount) ? Math.max(0, Math.floor(project.sessionCount)) : 0,
        title,
      },
    ];
  });
}

function createRemoteMachineSettings(states: readonly MachineConnectionState[]): RemoteMachineSettings[] {
  return states.flatMap((state) =>
    state.machine.machineId === 'local'
      ? []
      : [
          {
            id: state.machine.machineId,
            name: state.machine.label,
            sshHost: new URL(state.machine.baseUrl).hostname,
          },
        ]
  );
}

function resolveAgentIcon(agentName: string | undefined): SidebarAgentButton['icon'] {
  const direct = getSidebarAgentIconById(agentName);
  if (direct) {
    return direct;
  }
  const normalized = agentName?.trim().toLowerCase();
  if (!normalized) {
    return undefined;
  }
  return DEFAULT_SIDEBAR_AGENTS.find(
    (agent) =>
      agent.agentId === normalized || agent.name.trim().toLowerCase() === normalized || agent.icon === normalized
  )?.icon;
}

function publishMachineStatuses(
  states: readonly MachineConnectionState[],
  messageSource: WebSidebarMessageSource
): void {
  for (const state of states) {
    if (state.machine.machineId === 'local') {
      continue;
    }
    messageSource.postMessage({
      machineId: state.machine.machineId,
      ...(state.error ? { message: state.error } : {}),
      state: state.status,
      type: 'remoteMachineStatus',
    });
  }
}

function reconcileActiveTarget(
  target: SidebarProjectReference | undefined,
  states: readonly MachineConnectionState[]
): SidebarProjectReference | undefined {
  return target && presentationHasProject(states, target) ? target : primaryProjectTarget(states);
}

function reconcileFocusedTarget(
  target: SidebarSessionReference | undefined,
  states: readonly MachineConnectionState[]
): SidebarSessionReference | undefined {
  return target && presentationHasSession(states, target) ? target : undefined;
}

function primaryProjectTarget(states: readonly MachineConnectionState[]): SidebarProjectReference | undefined {
  const primary =
    states.find((state) => state.machine.machineId === 'local' && state.presentation) ??
    states.find((state) => state.presentation);
  const projectId = primary?.presentation?.projects[0]?.projectId;
  return primary && projectId ? { machineId: primary.machine.machineId, projectId } : undefined;
}

function presentationHasProject(states: readonly MachineConnectionState[], target: SidebarProjectReference): boolean {
  return states.some(
    (state) =>
      state.machine.machineId === target.machineId &&
      state.presentation?.projects.some((project) => project.projectId === target.projectId)
  );
}

function presentationHasSession(states: readonly MachineConnectionState[], target: SidebarSessionReference): boolean {
  return findPresentationSession(states, target) !== undefined;
}

function findPresentationSession(
  states: readonly MachineConnectionState[],
  target: SidebarSessionReference
): GxserverPresentationSession | undefined {
  return states
    .find((state) => state.machine.machineId === target.machineId)
    ?.presentation?.sessions.find(
      (session) => session.projectId === target.projectId && session.sessionId === target.sessionId
    );
}

function projectSessions(
  states: readonly MachineConnectionState[],
  target: SidebarProjectReference
): readonly GxserverPresentationSession[] {
  return (
    states
      .find((state) => state.machine.machineId === target.machineId)
      ?.presentation?.sessions.filter((session) => session.projectId === target.projectId) ?? []
  );
}

function lifecycleParams(target: SidebarSessionReference): Record<string, unknown> {
  return {
    projectId: target.projectId,
    reason: 'ghostex-web-sidebar',
    sessionId: target.sessionId,
  };
}

/*
 * CDXC:NavigationHistory 2026-08-19:
 * A trail stop is the project the user is on plus the session inside it, in the
 * SIDEBAR id vocabulary, so activating one later is a plain focusSession /
 * focusGroup call with no re-resolution. Labels are the same titles the sidebar
 * renders, carried only for the "Back to …" tooltip.
 */
function createNavigationHistoryEntry(
  states: readonly MachineConnectionState[],
  activeTarget: SidebarProjectReference | undefined,
  focusedTarget: SidebarSessionReference | undefined
): NavigationHistoryEntry | undefined {
  if (!activeTarget) {
    return undefined;
  }
  const state = states.find((candidate) => candidate.machine.machineId === activeTarget.machineId);
  const project = state?.presentation?.projects.find((candidate) => candidate.projectId === activeTarget.projectId);
  if (!project) {
    return undefined;
  }
  const session =
    focusedTarget &&
    focusedTarget.machineId === activeTarget.machineId &&
    focusedTarget.projectId === activeTarget.projectId
      ? findPresentationSession(states, focusedTarget)
      : undefined;
  const sessionTitle = session ? (session.displayTitle ?? session.primaryTitle ?? session.title) : undefined;
  return {
    groupId: createSidebarGroupId(activeTarget.machineId, activeTarget.projectId),
    projectId: createSidebarProjectId(activeTarget.machineId, activeTarget.projectId),
    ...(project.title ? { projectLabel: project.title } : {}),
    ...(focusedTarget && session
      ? {
          sessionId: createSidebarSessionId(focusedTarget.machineId, focusedTarget.projectId, focusedTarget.sessionId),
        }
      : {}),
    ...(sessionTitle ? { sessionLabel: sessionTitle } : {}),
  };
}

function lifecycleRpc(
  target: SidebarSessionReference,
  endpoint: '/api/killSession' | '/api/sleepSession' | '/api/wakeSession'
): Promise<unknown> {
  return rpcForMachine(target.machineId, endpoint, lifecycleParams(target));
}

function debugLog(event: string, detail: unknown): void {
  if (window.localStorage.getItem(DEBUG_SIDEBAR_STORAGE_KEY) === '1') {
    console.info(`[ghostex-web sidebar] ${event} ${JSON.stringify(detail)}`);
  }
}
