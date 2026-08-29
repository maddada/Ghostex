/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import { GPUI_REMOTE_MACHINE_RECONNECT_DELAYS_MS, GPUI_SIDEBAR_DEFAULT_CLIENT_ID } from './constants';
import type { GpuiSidebarRuntime } from './core';
import { createGpuiSidebarSettings } from './helpers/bootstrap';
import {
  countGpuiRemotePresentationProjectSessions,
  orderGpuiRecentProjects,
  writeStoredGpuiRemoteRecentProjects,
} from './helpers/recent-projects';
import { normalizeNonEmptyString } from './helpers/records';
import {
  compareGpuiRemoteAttachCandidateSessions,
  createGpuiRemotePresentationGroupId,
  createGpuiRemotePresentationProjectId,
  createGpuiRemotePresentationSessionId,
  isPresentationSnapshot,
  parseGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationProjectId,
} from './helpers/remote-presentation';
import { normalizeGpuiWorktreeParentProjectId } from './helpers/worktrees';
import type {
  GpuiRemoteProjectReference,
  GpuiRemoteProjectScope,
  GpuiRemoteSidebarHud,
  GpuiSidebarNativeProjectPathAction,
  GpuiSidebarRemoteGxserverResponseEvent,
} from './types-and-protocol';
import { postAppModalHostMessage } from '@/packages/core-ui/app-modal-host-bridge';
import type { AppToastLevel } from '@/packages/shared/app-toast-contract';
import { createAppToastRequest } from '@/packages/shared/app-toast-contract';
import { isRemoteMachineEnabledInSidebar, type PreferredAgentInterface } from '@/packages/shared/ghostex-settings';
import type {
  GxserverEndpointPath,
  GxserverPresentationProject,
  GxserverPresentationSession,
  GxserverProjectId,
  GxserverRecentProjectDomainState,
} from '@/packages/shared/gxserver-protocol';
import type { SidebarToExtensionMessage } from '@/packages/shared/session-grid-contract';

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeRemoteMachineMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeRemoteMachineMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeRemoteMachineMethods {
  connectSavedRemoteMachinesOnStartup(): void;
  reconcileRemoteMachineRetryTargets(): void;
  reconnectRemoteMachine(remoteMachineId: string, installApproved: boolean, automatic?: boolean): void;
  scheduleRemoteReconnect(remoteMachineId: string): void;
  clearRemoteReconnectTimeout(remoteMachineId: string): void;
  resetRemoteReconnect(remoteMachineId: string): void;
  startRemoteGxserverPresentationSubscription(remoteMachineId: string): void;
  requestRemoteGxserver<TResult = unknown>(
    remoteMachineId: string,
    path: GxserverEndpointPath,
    params: Record<string, unknown>,
    options?: { timeoutMs?: number }
  ): Promise<TResult>;
  resolveRemoteGxserverRequest(event: GpuiSidebarRemoteGxserverResponseEvent): void;
  postRemoteGxserverSidebarRequest(
    remoteMachineId: string,
    path: GxserverEndpointPath,
    params: Record<string, unknown>
  ): void;
  findRemotePresentationSession(reference: {
    machineId: string;
    projectId: string;
    sessionId: string;
  }): GxserverPresentationSession | undefined;
  postRemoteToast(level: AppToastLevel, title: string, options?: { description?: string }): void;
  resolveRemotePresentationProjectScope(
    input:
      | {
          groupId?: string;
          projectId?: string;
          remoteMachineId?: string;
        }
      | GpuiRemoteProjectReference
  ): GpuiRemoteProjectScope | undefined;
  findRemotePresentationProject(reference: GpuiRemoteProjectReference): GxserverPresentationProject | undefined;
  upsertRemotePresentationProject(remoteMachineId: string, nextProject: GxserverPresentationProject): void;
  removeRemotePresentationProject(remoteMachineId: string, projectId: string): void;
  remoteMachineName(machineId: string): string | undefined;
  resolveRemoteWorktreeFamilyParentProjectFromPresentation(
    sourceProject: GpuiRemoteProjectScope
  ): GpuiRemoteProjectScope | undefined;
  isTrustedRemoteExistingWorktreeKey(worktreeKey: string, sourceProject: GpuiRemoteProjectScope): boolean;
  resolveRemoteWorktreeMutationProject(
    remoteMachineId: string,
    project: GxserverPresentationProject | undefined
  ): Promise<GxserverPresentationProject>;
  refreshRemotePresentationFromGxserver(remoteMachineId: string): Promise<void>;
  refreshRemoteSidebarHudFromGxserver(remoteMachineId: string): Promise<void>;
  closeRemoteProjectForGroup(remoteScope: GpuiRemoteProjectScope, groupId: string): Promise<void>;
  restoreRemoteRecentProject(remoteReference: GpuiRemoteProjectReference): Promise<void>;
  removeRemoteRecentProject(remoteReference: GpuiRemoteProjectReference): Promise<void>;
  removeRemoteProject(remoteReference: GpuiRemoteProjectReference): Promise<void>;
  selectRemoteGroupAttachTarget(
    reference: GpuiRemoteProjectReference
  ): { machineId: string; projectId: string; sessionId: string } | undefined;
  postRemoteSessionNativeAction(
    action: Extract<
      GpuiSidebarNativeProjectPathAction,
      'openRemoteSessionTerminal' | 'copyRemoteAttachCommand' | 'copyRemoteResumeCommand'
    >,
    reference: { machineId: string; projectId: string; sessionId: string },
    originalMessage: SidebarToExtensionMessage,
    options?: { preferredInterface?: PreferredAgentInterface }
  ): boolean;
  postRemoteProjectNativeAction(
    action: Extract<
      GpuiSidebarNativeProjectPathAction,
      | 'copyRemoteProjectPath'
      | 'openRemoteProjectTerminal'
      | 'openRemoteWorkspaceProjectInIde'
      | 'openRemoteWorkspaceProjectInVscode'
      | 'openRemoteWorkspaceProjectInZed'
      | 'openRemoteExistingPullRequestInBrowser'
      | 'openRemoteSidebarGitChangedFileInIde'
      | 'openRemoteProjectPortsBrowser'
    >,
    reference: GpuiRemoteProjectReference,
    originalMessage: SidebarToExtensionMessage,
    options?: { filePath?: string }
  ): boolean;
}

export const gpuiSidebarRuntimeRemoteMachineMethods = {
  connectSavedRemoteMachinesOnStartup(this: GpuiSidebarRuntime): void {
    /*
    CDXC:GPUIRemoteStartupReconnect 2026-07-21:
    Rust-owned SSH tunnels are process-local and therefore never survive an
    app restart. Reconnect every saved machine after React has mounted its
    message-source listener so cached last-seen rows become live again and the
    header receives the normal connecting/connected status sequence. Reuse the
    explicit reconnect bridge; renderer code still sends only the saved id and
    never receives SSH details or tokens. Startup attempts enter the same
    lifecycle reconnect manager used after sleep/wake, so retryable failures
    back off without exhausting a launch-only budget.
    */
    if (this.didConnectSavedRemoteMachinesOnStartup || this.runtimeSettings?.settings === undefined) {
      return;
    }
    this.didConnectSavedRemoteMachinesOnStartup = true;
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    const enabledMachines = settings.remoteMachines.filter(isRemoteMachineEnabledInSidebar);
    this.enabledRemoteMachineIdsForReconnect = new Set(enabledMachines.map((machine) => machine.id));
    for (const machine of enabledMachines) {
      this.reconnectRemoteMachine(machine.id, false, true);
    }
  },

  reconcileRemoteMachineRetryTargets(this: GpuiSidebarRuntime): void {
    if (!this.didConnectSavedRemoteMachinesOnStartup) {
      return;
    }
    const enabledRemoteMachineIds = new Set(
      createGpuiSidebarSettings(this.runtimeSettings)
        .remoteMachines.filter(isRemoteMachineEnabledInSidebar)
        .map((machine) => machine.id)
    );
    const retryMachineIds = new Set([
      ...this.remoteReconnectAttempts.keys(),
      ...this.remoteReconnectInFlight,
      ...this.remoteReconnectTimeouts.keys(),
    ]);
    for (const machineId of retryMachineIds) {
      if (enabledRemoteMachineIds.has(machineId)) {
        continue;
      }
      this.resetRemoteReconnect(machineId);
    }
    for (const machineId of enabledRemoteMachineIds) {
      if (this.enabledRemoteMachineIdsForReconnect.has(machineId)) {
        continue;
      }
      this.reconnectRemoteMachine(machineId, false, true);
    }
    this.enabledRemoteMachineIdsForReconnect = enabledRemoteMachineIds;
  },

  reconnectRemoteMachine(
    this: GpuiSidebarRuntime,
    remoteMachineId: string,
    installApproved: boolean,
    automatic = false
  ): void {
    const normalizedMachineId = normalizeNonEmptyString(remoteMachineId);
    if (!normalizedMachineId) {
      return;
    }
    if (!automatic) {
      this.resetRemoteReconnect(normalizedMachineId);
    } else if (this.remoteReconnectInFlight.has(normalizedMachineId)) {
      return;
    }
    this.clearRemoteReconnectTimeout(normalizedMachineId);
    this.remoteReconnectInFlight.add(normalizedMachineId);
    try {
      postAppModalHostMessage(
        {
          automatic,
          installApproved,
          remoteMachineId: normalizedMachineId,
          type: 'reconnectRemoteMachine',
        },
        'GPUISidebarRemoteMachines:reconnect'
      );
      this.messageSource.postMessage({
        machineId: normalizedMachineId,
        state: 'connecting',
        type: 'remoteMachineStatus',
      });
    } catch {
      this.remoteReconnectInFlight.delete(normalizedMachineId);
      this.scheduleRemoteReconnect(normalizedMachineId);
      if (!automatic) {
        this.postRemoteToast('warning', 'Remote connect unavailable', {
          description: 'GPUI could not reach the native remote-machine bridge.',
        });
      }
    }
  },

  scheduleRemoteReconnect(this: GpuiSidebarRuntime, remoteMachineId: string): void {
    const normalizedMachineId = normalizeNonEmptyString(remoteMachineId);
    if (
      !normalizedMachineId ||
      this.remoteReconnectInFlight.has(normalizedMachineId) ||
      this.remoteReconnectTimeouts.has(normalizedMachineId)
    ) {
      return;
    }
    const isStillSaved = createGpuiSidebarSettings(this.runtimeSettings).remoteMachines.some(
      (machine) => machine.id === normalizedMachineId && isRemoteMachineEnabledInSidebar(machine)
    );
    if (!isStillSaved) {
      this.resetRemoteReconnect(normalizedMachineId);
      return;
    }
    const retryAttempts = this.remoteReconnectAttempts.get(normalizedMachineId) ?? 0;
    const delay =
      GPUI_REMOTE_MACHINE_RECONNECT_DELAYS_MS[
        Math.min(retryAttempts, GPUI_REMOTE_MACHINE_RECONNECT_DELAYS_MS.length - 1)
      ];
    const timeout = window.setTimeout(() => {
      this.remoteReconnectTimeouts.delete(normalizedMachineId);
      const remainsSaved = createGpuiSidebarSettings(this.runtimeSettings).remoteMachines.some(
        (machine) => machine.id === normalizedMachineId && isRemoteMachineEnabledInSidebar(machine)
      );
      if (!remainsSaved) {
        this.resetRemoteReconnect(normalizedMachineId);
        return;
      }
      this.remoteReconnectAttempts.set(normalizedMachineId, retryAttempts + 1);
      this.reconnectRemoteMachine(normalizedMachineId, false, true);
    }, delay);
    this.remoteReconnectTimeouts.set(normalizedMachineId, timeout);
  },

  clearRemoteReconnectTimeout(this: GpuiSidebarRuntime, remoteMachineId: string): void {
    const timeout = this.remoteReconnectTimeouts.get(remoteMachineId);
    if (timeout === undefined) {
      return;
    }
    window.clearTimeout(timeout);
    this.remoteReconnectTimeouts.delete(remoteMachineId);
  },

  resetRemoteReconnect(this: GpuiSidebarRuntime, remoteMachineId: string): void {
    this.clearRemoteReconnectTimeout(remoteMachineId);
    this.remoteReconnectAttempts.delete(remoteMachineId);
    this.remoteReconnectInFlight.delete(remoteMachineId);
  },

  startRemoteGxserverPresentationSubscription(this: GpuiSidebarRuntime, remoteMachineId: string): void {
    const normalizedMachineId = normalizeNonEmptyString(remoteMachineId);
    if (!normalizedMachineId) {
      return;
    }
    const snapshot = this.remotePresentations.get(normalizedMachineId);
    const requestId = `remote-presentation-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
    try {
      postAppModalHostMessage(
        {
          clientId: `${GPUI_SIDEBAR_DEFAULT_CLIENT_ID}:${normalizedMachineId}`,
          ...(snapshot ? { lastRevision: snapshot.revision } : {}),
          remoteMachineId: normalizedMachineId,
          requestId,
          type: 'remoteGxserverSubscribePresentation',
        },
        'GPUISidebarRemoteMachines:subscribePresentation'
      );
    } catch {
      this.postRemoteToast('warning', 'Remote sidebar stream unavailable', {
        description: 'GPUI could not reach the native remote presentation bridge.',
      });
    }
  },

  requestRemoteGxserver<TResult = unknown>(
    this: GpuiSidebarRuntime,
    remoteMachineId: string,
    path: GxserverEndpointPath,
    params: Record<string, unknown>,
    options: { timeoutMs?: number } = {}
  ): Promise<TResult> {
    const requestId = `remote-${Date.now().toString(36)}-${++this.remoteGxserverRequestSequence}`;
    const timeoutMs = Math.min(Math.max(options.timeoutMs ?? 20_000, 1_000), 130_000);
    return new Promise<TResult>((resolve, reject) => {
      const timeoutId = window.setTimeout(() => {
        this.pendingRemoteGxserverRequests.delete(requestId);
        reject(new Error('Remote gxserver request timed out.'));
      }, timeoutMs + 2_000);
      this.pendingRemoteGxserverRequests.set(requestId, {
        reject,
        resolve: (result) => resolve(result as TResult),
        timeoutId,
      });
      try {
        /*
        CDXC:GPUIRemoteMachines 2026-06-24-17:19:
        Response-capable remote sidebar RPCs still carry only a bounded request id plus the allowlisted endpoint params into Rust. Rust owns the live tunnel, token, endpoint allowlist, response sanitization, and presentation refresh; renderer code must not receive tokens, SSH details, command text, URLs, or raw daemon bodies.
        */
        postAppModalHostMessage(
          {
            params,
            path,
            remoteMachineId,
            requestId,
            timeoutMs,
            type: 'gpuiRemoteGxserverSidebarRequest',
          },
          'GPUISidebarRemoteMachines:request'
        );
      } catch (error) {
        window.clearTimeout(timeoutId);
        this.pendingRemoteGxserverRequests.delete(requestId);
        reject(error instanceof Error ? error : new Error('Remote gxserver bridge failed.'));
      }
    });
  },

  resolveRemoteGxserverRequest(this: GpuiSidebarRuntime, event: GpuiSidebarRemoteGxserverResponseEvent): void {
    const pending = this.pendingRemoteGxserverRequests.get(event.requestId);
    if (!pending) {
      return;
    }
    window.clearTimeout(pending.timeoutId);
    this.pendingRemoteGxserverRequests.delete(event.requestId);
    if (event.ok) {
      pending.resolve(event.result);
      return;
    }
    pending.reject(new Error(event.error || 'Remote gxserver request failed.'));
  },

  postRemoteGxserverSidebarRequest(
    this: GpuiSidebarRuntime,
    remoteMachineId: string,
    path: GxserverEndpointPath,
    params: Record<string, unknown>
  ): void {
    try {
      postAppModalHostMessage(
        {
          params,
          path,
          remoteMachineId,
          type: 'gpuiRemoteGxserverSidebarRequest',
        },
        'GPUISidebarRemoteMachines:request'
      );
    } catch {
      this.postRemoteToast('warning', 'Remote action unavailable', {
        description: 'GPUI could not reach the native remote gxserver bridge.',
      });
    }
  },

  findRemotePresentationSession(
    this: GpuiSidebarRuntime,
    reference: {
      machineId: string;
      projectId: string;
      sessionId: string;
    }
  ): GxserverPresentationSession | undefined {
    return this.remotePresentations
      .get(reference.machineId)
      ?.sessions.find(
        (session) => session.projectId === reference.projectId && session.sessionId === reference.sessionId
      );
  },

  postRemoteToast(
    this: GpuiSidebarRuntime,
    level: AppToastLevel,
    title: string,
    options: { description?: string } = {}
  ): void {
    try {
      postAppModalHostMessage(
        createAppToastRequest(level, title, options.description),
        'GPUISidebarRemoteMachines:toast'
      );
    } catch {
      /*
      CDXC:GPUIRemoteMachines 2026-06-24-16:48:
      Remote-machine operations must never depend on toast-host availability. If the shared app-modal toast bridge is missing, keep the native-owned request/status path honest and avoid logging payloads, SSH details, tokens, paths, daemon responses, or renderer contents.
      */
    }
  },

  resolveRemotePresentationProjectScope(
    this: GpuiSidebarRuntime,
    input:
      | {
          groupId?: string;
          projectId?: string;
          remoteMachineId?: string;
        }
      | GpuiRemoteProjectReference
  ): GpuiRemoteProjectScope | undefined {
    const groupReference =
      'groupId' in input && input.groupId ? parseGpuiRemotePresentationGroupId(input.groupId) : undefined;
    const projectReference =
      !groupReference && 'projectId' in input && input.projectId
        ? parseGpuiRemotePresentationProjectId(input.projectId)
        : undefined;
    const machineId =
      groupReference?.machineId ??
      projectReference?.machineId ??
      ('remoteMachineId' in input ? input.remoteMachineId?.trim() : undefined) ??
      ('machineId' in input ? input.machineId : undefined);
    const projectId =
      groupReference?.projectId ??
      projectReference?.projectId ??
      ('projectId' in input ? input.projectId?.trim() : undefined);
    if (!machineId || !projectId) {
      return undefined;
    }
    const presentation = this.remotePresentations.get(machineId);
    const project = presentation?.projects.find((candidate) => candidate.projectId === projectId);
    if (!project) {
      return undefined;
    }
    return {
      machineId,
      machineName: this.remoteMachineName(machineId),
      project,
      projectId,
    };
  },

  findRemotePresentationProject(
    this: GpuiSidebarRuntime,
    reference: GpuiRemoteProjectReference
  ): GxserverPresentationProject | undefined {
    return this.remotePresentations
      .get(reference.machineId)
      ?.projects.find((project) => project.projectId === reference.projectId);
  },

  upsertRemotePresentationProject(
    this: GpuiSidebarRuntime,
    remoteMachineId: string,
    nextProject: GxserverPresentationProject
  ): void {
    const presentation = this.remotePresentations.get(remoteMachineId);
    if (!presentation) {
      return;
    }
    const existingIndex = presentation.projects.findIndex((project) => project.projectId === nextProject.projectId);
    const projects =
      existingIndex >= 0
        ? presentation.projects.map((project, index) => (index === existingIndex ? nextProject : project))
        : [...presentation.projects, nextProject];
    this.remotePresentations.set(remoteMachineId, {
      ...presentation,
      projects,
    });
  },

  removeRemotePresentationProject(this: GpuiSidebarRuntime, remoteMachineId: string, projectId: string): void {
    const presentation = this.remotePresentations.get(remoteMachineId);
    if (!presentation) {
      return;
    }
    this.remotePresentations.set(remoteMachineId, {
      ...presentation,
      groups: presentation.groups.filter((group) => group.projectId !== projectId),
      projects: presentation.projects.filter((project) => project.projectId !== projectId),
      sessions: presentation.sessions.filter((session) => session.projectId !== projectId),
    });
  },

  remoteMachineName(this: GpuiSidebarRuntime, machineId: string): string | undefined {
    return createGpuiSidebarSettings(this.runtimeSettings).remoteMachines.find((machine) => machine.id === machineId)
      ?.name;
  },

  resolveRemoteWorktreeFamilyParentProjectFromPresentation(
    this: GpuiSidebarRuntime,
    sourceProject: GpuiRemoteProjectScope
  ): GpuiRemoteProjectScope | undefined {
    const parentProjectId = normalizeGpuiWorktreeParentProjectId(sourceProject.project.worktree);
    if (!parentProjectId) {
      return sourceProject;
    }
    const parentProject = this.remotePresentations
      .get(sourceProject.machineId)
      ?.projects.find((project) => project.projectId === parentProjectId);
    return parentProject
      ? {
          machineId: sourceProject.machineId,
          machineName: sourceProject.machineName,
          project: parentProject,
          projectId: parentProject.projectId,
        }
      : undefined;
  },

  isTrustedRemoteExistingWorktreeKey(
    this: GpuiSidebarRuntime,
    worktreeKey: string,
    sourceProject: GpuiRemoteProjectScope
  ): boolean {
    const trusted = this.trustedExistingWorktreeList;
    return Boolean(
      trusted &&
      trusted.remoteMachineId === sourceProject.machineId &&
      trusted.sourceProjectId === sourceProject.projectId &&
      trusted.worktreeKeys?.has(worktreeKey.trim())
    );
  },

  async resolveRemoteWorktreeMutationProject(
    this: GpuiSidebarRuntime,
    remoteMachineId: string,
    project: GxserverPresentationProject | undefined
  ): Promise<GxserverPresentationProject> {
    if (!project?.projectId) {
      throw new Error('Remote gxserver did not return a worktree project.');
    }
    this.upsertRemotePresentationProject(remoteMachineId, project);
    this.publishRemotePresentationPatch();
    await this.refreshRemotePresentationFromGxserver(remoteMachineId).catch(() => undefined);
    return (
      this.findRemotePresentationProject({
        machineId: remoteMachineId,
        projectId: project.projectId,
      }) ?? project
    );
  },

  async refreshRemotePresentationFromGxserver(this: GpuiSidebarRuntime, remoteMachineId: string): Promise<void> {
    const response = await this.requestRemoteGxserver<{ snapshot?: unknown }>(
      remoteMachineId,
      '/api/readPresentationSnapshot',
      {}
    );
    if (isPresentationSnapshot(response.snapshot)) {
      const previous = this.remotePresentations.get(remoteMachineId);
      const previousSessions = previous?.sessions ?? [];
      const snapshot = this.projectRemotePresentationAttentionAcknowledgementGuards(remoteMachineId, response.snapshot);
      if (previous && previous.revision > snapshot.revision) {
        return;
      }
      this.remotePresentations.set(remoteMachineId, snapshot);
      this.pruneRemoteWorkspaceGroupAssignments(remoteMachineId, snapshot);
      this.syncRemotePresentationAttentionTracking(remoteMachineId, previousSessions, snapshot.sessions);
      this.publishRemotePresentationPatch();
      await this.refreshRemoteSidebarHudFromGxserver(remoteMachineId).catch(() => undefined);
    }
  },

  async refreshRemoteSidebarHudFromGxserver(this: GpuiSidebarRuntime, remoteMachineId: string): Promise<void> {
    /*
    CDXC:RemoteProjectActions 2026-08-29:
    A remote project's Actions are stored by the daemon that owns the project,
    so the only place they can come from is that machine's own HUD projection.
    Ask for the per-project command block, exactly like the local HUD read, so
    every remote project row can render its own quick actions instead of the
    empty list a local-only read produced.
    */
    const hud = await this.requestRemoteGxserver<GpuiRemoteSidebarHud>(remoteMachineId, '/api/readSidebarHud', {
      includeAllProjectCommands: true,
    });
    if (!Array.isArray(hud.commands)) {
      return;
    }
    this.remoteSidebarHuds.set(remoteMachineId, hud);
    this.publishHudPatch();
  },

  async closeRemoteProjectForGroup(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    groupId: string
  ): Promise<void> {
    /*
    CDXC:GPUIRemoteProjects 2026-06-27-19:37:
    Remote Recent Projects are client-app state, not local Mac gxserver state
    and not the remote daemon's shared project state. GPUI parks a
    machine-scoped row in its own CEF storage so macOS and GPUI can connect to
    and organize the same remote machine independently.
    */
    const presentation = this.remotePresentations.get(remoteScope.machineId);
    const recentProject: GxserverRecentProjectDomainState = {
      path: remoteScope.project.path ?? '',
      projectId: remoteScope.projectId as GxserverProjectId,
      recentClosedAt: new Date().toISOString(),
      sessionCount: presentation ? countGpuiRemotePresentationProjectSessions(presentation, remoteScope.projectId) : 0,
      title: remoteScope.project.title,
    };
    const previousProjects = this.remoteRecentProjectsByMachineId.get(remoteScope.machineId) ?? [];
    this.remoteRecentProjectsByMachineId.set(
      remoteScope.machineId,
      orderGpuiRecentProjects([
        recentProject,
        ...previousProjects.filter((project) => project.projectId !== remoteScope.projectId),
      ])
    );
    writeStoredGpuiRemoteRecentProjects(this.remoteRecentProjectsByMachineId);
    if (this.activeGroupId === groupId) {
      this.activeGroupId = undefined;
    }
    this.publishRemotePresentationPatch();
  },

  async restoreRemoteRecentProject(
    this: GpuiSidebarRuntime,
    remoteReference: GpuiRemoteProjectReference
  ): Promise<void> {
    this.remoteRecentProjectsByMachineId.set(
      remoteReference.machineId,
      (this.remoteRecentProjectsByMachineId.get(remoteReference.machineId) ?? []).filter(
        (project) => project.projectId !== remoteReference.projectId
      )
    );
    writeStoredGpuiRemoteRecentProjects(this.remoteRecentProjectsByMachineId);
    this.activeGroupId = createGpuiRemotePresentationGroupId(remoteReference.machineId, remoteReference.projectId);
    if (!this.remotePresentations.has(remoteReference.machineId)) {
      this.reconnectRemoteMachine(remoteReference.machineId, false);
    }
    this.publishRemotePresentationPatch();
  },

  async removeRemoteRecentProject(
    this: GpuiSidebarRuntime,
    remoteReference: GpuiRemoteProjectReference
  ): Promise<void> {
    this.remoteRecentProjectsByMachineId.set(
      remoteReference.machineId,
      (this.remoteRecentProjectsByMachineId.get(remoteReference.machineId) ?? []).filter(
        (project) => project.projectId !== remoteReference.projectId
      )
    );
    writeStoredGpuiRemoteRecentProjects(this.remoteRecentProjectsByMachineId);
    this.publishRemotePresentationPatch();
  },

  async removeRemoteProject(this: GpuiSidebarRuntime, remoteReference: GpuiRemoteProjectReference): Promise<void> {
    try {
      await this.requestRemoteGxserver(remoteReference.machineId, '/api/removeProject', {
        projectId: remoteReference.projectId,
      });
      this.removeRemotePresentationProject(remoteReference.machineId, remoteReference.projectId);
      this.remoteRecentProjectsByMachineId.set(
        remoteReference.machineId,
        (this.remoteRecentProjectsByMachineId.get(remoteReference.machineId) ?? []).filter(
          (project) => project.projectId !== remoteReference.projectId
        )
      );
      writeStoredGpuiRemoteRecentProjects(this.remoteRecentProjectsByMachineId);
      this.publishRemotePresentationPatch();
    } catch {
      this.postRemoteToast('warning', 'Remote project removal failed', {
        description: 'The remote gxserver could not remove that project.',
      });
    }
  },

  selectRemoteGroupAttachTarget(
    this: GpuiSidebarRuntime,
    reference: GpuiRemoteProjectReference
  ): { machineId: string; projectId: string; sessionId: string } | undefined {
    const presentation = this.remotePresentations.get(reference.machineId);
    const session = (presentation?.sessions ?? [])
      .filter(
        (candidate) =>
          candidate.projectId === reference.projectId && (candidate.kind === 'terminal' || candidate.kind === 'agent')
      )
      .sort(compareGpuiRemoteAttachCandidateSessions)[0];
    return session
      ? {
          machineId: reference.machineId,
          projectId: reference.projectId,
          sessionId: session.sessionId,
        }
      : undefined;
  },

  postRemoteSessionNativeAction(
    this: GpuiSidebarRuntime,
    action: Extract<
      GpuiSidebarNativeProjectPathAction,
      'openRemoteSessionTerminal' | 'copyRemoteAttachCommand' | 'copyRemoteResumeCommand'
    >,
    reference: { machineId: string; projectId: string; sessionId: string },
    originalMessage: SidebarToExtensionMessage,
    options: { preferredInterface?: PreferredAgentInterface } = {}
  ): boolean {
    return this.postNativeProjectPathAction(
      action,
      createGpuiRemotePresentationSessionId(reference.machineId, reference.projectId, reference.sessionId),
      originalMessage,
      options
    );
  },

  postRemoteProjectNativeAction(
    this: GpuiSidebarRuntime,
    action: Extract<
      GpuiSidebarNativeProjectPathAction,
      | 'copyRemoteProjectPath'
      | 'openRemoteProjectTerminal'
      | 'openRemoteWorkspaceProjectInIde'
      | 'openRemoteWorkspaceProjectInVscode'
      | 'openRemoteWorkspaceProjectInZed'
      | 'openRemoteExistingPullRequestInBrowser'
      | 'openRemoteSidebarGitChangedFileInIde'
      | 'openRemoteProjectPortsBrowser'
    >,
    reference: GpuiRemoteProjectReference,
    originalMessage: SidebarToExtensionMessage,
    options: { filePath?: string } = {}
  ): boolean {
    return this.postNativeProjectPathAction(
      action,
      createGpuiRemotePresentationProjectId(reference.machineId, reference.projectId),
      originalMessage,
      options
    );
  },
};

const gpuiSidebarRuntimeRemoteMachineMethodsShapeCheck: GpuiSidebarRuntimeRemoteMachineMethods =
  gpuiSidebarRuntimeRemoteMachineMethods;
void gpuiSidebarRuntimeRemoteMachineMethodsShapeCheck;
