/*
CDXC:RepoStructure 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_SIDEBAR_DEFAULT_CLIENT_ID,
  GPUI_STALE_REMOTE_PRESENTATION_REFRESH_COOLDOWN_MS,
} from './constants';
import type { GpuiSidebarRuntime } from './core';
import { createGpuiSidebarSettings } from './helpers/bootstrap';
import { writeStoredGpuiRemoteRecentProjects } from './helpers/recent-projects';
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
import type {
  GpuiRemoteProjectReference,
  GpuiRemoteProjectScope,
  GpuiRemoteSidebarHud,
  GpuiSidebarNativeProjectPathAction,
  GpuiSidebarRemoteGxserverResponseEvent,
  GpuiWorkspaceTerminalFocusPlacement,
} from './types-and-protocol';
import { postAppModalHostMessage } from '@/packages/core-ui/app-modal-host-bridge';
import type { AppToastLevel } from '@/packages/shared/app-toast-contract';
import { createAppToastRequest } from '@/packages/shared/app-toast-contract';
import { isRemoteMachineEnabledInSidebar, type PreferredAgentInterface } from '@/packages/shared/ghostex-settings';
import type {
  GxserverEndpointPath,
  GxserverPresentationProject,
  GxserverPresentationSession,
} from '@/packages/shared/gxserver-protocol';
import type { SidebarToExtensionMessage } from '@/packages/shared/session-grid-contract';

/*
CDXC:RepoStructure 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeRemoteMachineMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeRemoteMachineMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeRemoteMachineMethods {
  requestRemoteGxserver<TResult = unknown>(
    remoteMachineId: string,
    path: GxserverEndpointPath,
    params: Record<string, unknown>,
    options?: { timeoutMs?: number }
  ): Promise<TResult>;
  resolveRemoteGxserverRequest(event: GpuiSidebarRemoteGxserverResponseEvent): void;
  findRemotePresentationSession(reference: {
    machineId: string;
    projectId: string;
    sessionId: string;
  }): GxserverPresentationSession | undefined;
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
  refreshRemotePresentationFromGxserver(remoteMachineId: string): Promise<void>;
  scheduleStaleRemotePresentationRefresh(remoteMachineId: string): void;
  forgetStaleRemotePresentationRefresh(remoteMachineId: string): void;
}

export const gpuiSidebarRuntimeRemoteMachineMethods = {
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
        CDXC:RemoteMachines 2026-06-24-17:19:
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

  async refreshRemotePresentationFromGxserver(this: GpuiSidebarRuntime, remoteMachineId: string): Promise<void> {
    const response = await this.requestRemoteGxserver<{ snapshot?: unknown }>(
      remoteMachineId,
      '/api/readPresentationSnapshot',
      {}
    );
    if (isPresentationSnapshot(response.snapshot)) {
      const previous = this.remotePresentations.get(remoteMachineId);
      const snapshot = response.snapshot;
      if (previous && previous.revision > snapshot.revision) {
        return;
      }
      this.remotePresentations.set(remoteMachineId, snapshot);
      this.pruneRemoteWorkspaceGroupAssignments(remoteMachineId, snapshot);
      this.publishRemotePresentationPatch();
    }
  },

  /*
  CDXC:StateSync 2026-09-01:
  A remote delta that does not advance the cached revision means this app and
  the machine disagree about where the stream is, and the repair is to refetch
  the whole snapshot. That verdict is unchanged — but the deltas that trigger it
  arrive in bursts (one per changed row), and every one of them used to fire its
  own full snapshot read over the SSH hop. Collapse a burst to a single refetch
  and, if more staleness shows up inside the cooldown, one trailing refetch
  after it, so the repair still converges without hammering the tunnel.
  */
  scheduleStaleRemotePresentationRefresh(this: GpuiSidebarRuntime, remoteMachineId: string): void {
    const now = Date.now();
    const entry = this.staleRemotePresentationRefreshes.get(remoteMachineId);
    const runRefresh = (startedAt: number) => {
      this.staleRemotePresentationRefreshes.set(remoteMachineId, { lastStartedAt: startedAt });
      void this.refreshRemotePresentationFromGxserver(remoteMachineId).catch(() => undefined);
    };
    if (!entry || now - entry.lastStartedAt >= GPUI_STALE_REMOTE_PRESENTATION_REFRESH_COOLDOWN_MS) {
      if (entry?.trailingTimeoutId !== undefined) {
        window.clearTimeout(entry.trailingTimeoutId);
      }
      runRefresh(now);
      return;
    }
    if (entry.trailingTimeoutId !== undefined) {
      return;
    }
    entry.trailingTimeoutId = window.setTimeout(
      () => {
        const current = this.staleRemotePresentationRefreshes.get(remoteMachineId);
        if (!current || current.trailingTimeoutId === undefined) {
          return;
        }
        runRefresh(Date.now());
      },
      GPUI_STALE_REMOTE_PRESENTATION_REFRESH_COOLDOWN_MS - (now - entry.lastStartedAt)
    );
  },

  forgetStaleRemotePresentationRefresh(this: GpuiSidebarRuntime, remoteMachineId: string): void {
    const entry = this.staleRemotePresentationRefreshes.get(remoteMachineId);
    if (!entry) {
      return;
    }
    if (entry.trailingTimeoutId !== undefined) {
      window.clearTimeout(entry.trailingTimeoutId);
    }
    this.staleRemotePresentationRefreshes.delete(remoteMachineId);
  },

};

const gpuiSidebarRuntimeRemoteMachineMethodsShapeCheck: GpuiSidebarRuntimeRemoteMachineMethods =
  gpuiSidebarRuntimeRemoteMachineMethods;
void gpuiSidebarRuntimeRemoteMachineMethodsShapeCheck;
