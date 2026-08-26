/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import type { GpuiWorkspaceSessionGroupsState } from '../workspace-session-groups';
import {
  gpuiGxserverRpcErrorCode,
  gpuiGxserverRpcErrorMessage,
  isGxserverRpcSuccess,
  parseObject,
  readJson,
} from './helpers/records';
import {
  isGpuiSessionChatEventMessage,
  isPresentationDelta,
  isPresentationSnapshot,
  isSidebarProjectCollectionsState,
} from './helpers/remote-presentation';
import { handleGpuiRendererCommand, isGpuiRendererCommand } from './helpers/renderer-commands';
import type {
  GpuiPresentationSubscription,
  GpuiRendererCommandHandler,
  GpuiValidatedGxserverBootstrap,
} from './types-and-protocol';
import type {
  GxserverAppUserData,
  GxserverEndpointPath,
  GxserverPresentationDelta,
  GxserverPresentationSnapshot,
  GxserverProjectDomainState,
  GxserverRecentProjectDomainState,
  GxserverRpcErrorCode,
  GxserverSidebarHudResponse,
  GxserverSidebarHudSettingsMutationParams,
  GxserverSidebarHudSettingsMutationResult,
  GxserverSidebarProjectCollectionsState,
} from '@/packages/shared/gxserver-protocol';
import { GXSERVER_PROTOCOL_VERSION } from '@/packages/shared/gxserver-protocol';
import type { GxserverSessionChatEvent } from '@/packages/shared/session-chat';
import { isSessionChatEventType } from '@/packages/shared/session-chat';

export class GpuiGxserverRpcError extends Error {
  readonly code?: GxserverRpcErrorCode;

  constructor(message: string, code?: GxserverRpcErrorCode) {
    super(message);
    this.name = 'GpuiGxserverRpcError';
    this.code = code;
  }
}

export class GpuiGxserverClient {
  constructor(private readonly bootstrap: GpuiValidatedGxserverBootstrap) {}

  async fetchPresentationSnapshot(): Promise<GxserverPresentationSnapshot> {
    const { snapshot } = await this.rpc<{ snapshot: GxserverPresentationSnapshot }>('/api/readPresentationSnapshot');
    return snapshot;
  }

  async fetchProjectList(): Promise<GxserverProjectDomainState[]> {
    const { projects } = await this.rpc<{ projects: GxserverProjectDomainState[] }>('/api/listProjects');
    return projects;
  }

  async fetchRecentProjects(): Promise<GxserverRecentProjectDomainState[]> {
    const { recentProjects } = await this.rpc<{
      recentProjects: GxserverRecentProjectDomainState[];
    }>('/api/listRecentProjects');
    return recentProjects;
  }

  async fetchSidebarHud(activeProjectId: string | undefined): Promise<GxserverSidebarHudResponse> {
    const normalizedActiveProjectId = activeProjectId?.trim();
    /*
     * CDXC:ProjectActions 2026-08-01:
     * The GPUI sidebar renders showOnProjectRow quick actions on every project
     * row, so the HUD read always asks for the per-project command block.
     */
    return this.rpc<GxserverSidebarHudResponse>('/api/readSidebarHud', {
      includeAllProjectCommands: true,
      ...(normalizedActiveProjectId ? { activeProjectId: normalizedActiveProjectId } : {}),
    });
  }

  async mutateSidebarHudSettings(
    params: GxserverSidebarHudSettingsMutationParams
  ): Promise<GxserverSidebarHudSettingsMutationResult> {
    return this.rpc<GxserverSidebarHudSettingsMutationResult>('/api/mutateSidebarHudSettings', params);
  }

  async fetchWorkspaceSessionGroups(): Promise<unknown> {
    const { groups } = await this.rpc<{ groups?: unknown }>('/api/readWorkspaceSessionGroups');
    return groups;
  }

  async updateWorkspaceSessionGroups(state: GpuiWorkspaceSessionGroupsState): Promise<void> {
    await this.rpc('/api/updateWorkspaceSessionGroups', { state });
  }

  async updateSidebarProjectCollections(state: GxserverSidebarProjectCollectionsState): Promise<unknown> {
    const { sidebarProjectCollections } = await this.rpc<{
      sidebarProjectCollections?: unknown;
    }>('/api/updateSidebarProjectCollections', { state });
    return sidebarProjectCollections;
  }

  async fetchAppUserData(): Promise<GxserverAppUserData> {
    return this.rpc<GxserverAppUserData>('/api/readAppUserData');
  }

  async saveScratchPad(content: string): Promise<GxserverAppUserData> {
    return this.rpc<GxserverAppUserData>('/api/saveScratchPad', { content });
  }

  async savePinnedPrompt(params: { content: string; promptId?: string; title: string }): Promise<GxserverAppUserData> {
    return this.rpc<GxserverAppUserData>('/api/savePinnedPrompt', params);
  }

  async rpc<TResult>(path: GxserverEndpointPath, params: Record<string, unknown> = {}): Promise<TResult> {
    const response = await fetch(`${this.bootstrap.baseUrl}${path}`, {
      body: JSON.stringify({
        params,
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      }),
      headers: {
        authorization: `Bearer ${this.bootstrap.authToken}`,
        'content-type': 'application/json',
        'x-gxserver-protocol-version': String(GXSERVER_PROTOCOL_VERSION),
      },
      method: 'POST',
    });
    const body = await readJson(response);
    if (!response.ok || !isGxserverRpcSuccess<TResult>(body)) {
      const errorMessage = gpuiGxserverRpcErrorMessage(body);
      throw new GpuiGxserverRpcError(
        errorMessage ?? `gxserver rejected ${path} (${response.status > 0 ? response.status : 'no response'}).`,
        gpuiGxserverRpcErrorCode(body)
      );
    }
    if (body.protocolVersion !== GXSERVER_PROTOCOL_VERSION) {
      throw new Error('gxserver protocol mismatch.');
    }
    return body.result;
  }

  subscribePresentation({
    clientId,
    lastRevision,
    onClose,
    onDelta,
    onError,
    onGlobalSidebarCommands,
    onRendererCommand,
    onSessionChatEvent,
    onSidebarProjectCollections,
    onSnapshot,
    onWorkspaceGroups,
  }: {
    clientId: string;
    lastRevision: number;
    onClose: () => void;
    onDelta: (delta: GxserverPresentationDelta, revision: number) => void;
    onError: () => void;
    onGlobalSidebarCommands?: () => void;
    onRendererCommand?: GpuiRendererCommandHandler;
    onSessionChatEvent?: (event: GxserverSessionChatEvent) => void;
    onSidebarProjectCollections?: (state: GxserverSidebarProjectCollectionsState) => void;
    onSnapshot: (snapshot: GxserverPresentationSnapshot) => void;
    onWorkspaceGroups?: (state: unknown) => void;
  }): GpuiPresentationSubscription {
    const url = new URL(`${this.bootstrap.baseUrl}/api/events`);
    url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
    url.searchParams.set('protocolVersion', String(GXSERVER_PROTOCOL_VERSION));
    url.searchParams.set('authToken', this.bootstrap.authToken);

    const socket = new WebSocket(url.toString());
    let closedByClient = false;
    socket.addEventListener('open', () => {
      socket.send(
        JSON.stringify({
          clientId,
          lastRevision,
          ...(onRendererCommand ? { rendererCommands: true } : {}),
          type: 'subscribePresentation',
        })
      );
    });
    socket.addEventListener('message', (event) => {
      const message = parseObject(event.data);
      if (!message) {
        return;
      }
      if (message.type === 'presentationSnapshot' && isPresentationSnapshot(message.snapshot)) {
        onSnapshot(message.snapshot);
        return;
      }
      if (
        message.type === 'presentationDelta' &&
        typeof message.revision === 'number' &&
        isPresentationDelta(message.delta)
      ) {
        onDelta(message.delta, message.revision);
        return;
      }
      if (message.type === 'rendererCommand' && onRendererCommand && isGpuiRendererCommand(message.command)) {
        void handleGpuiRendererCommand(socket, message.command, onRendererCommand);
        return;
      }
      if (
        message.type === 'sidebarProjectCollectionsChanged' &&
        onSidebarProjectCollections &&
        isSidebarProjectCollectionsState(message.sidebarProjectCollections)
      ) {
        onSidebarProjectCollections(message.sidebarProjectCollections);
        return;
      }
      if (message.type === 'workspaceGroupsChanged' && onWorkspaceGroups && parseObject(message.groups)) {
        onWorkspaceGroups(message.groups);
        return;
      }
      /*
      CDXC:GlobalActions 2026-08-07:
      The Global Actions announcement carries no list — the handler refetches
      the HUD, which is the one projection of it — so there is no payload to
      shape-validate before forwarding it.
      */
      if (message.type === 'globalSidebarCommandsChanged' && onGlobalSidebarCommands) {
        onGlobalSidebarCommands();
        return;
      }
      /*
      CDXC:SessionChatCore 2026-07-31:
      Session-chat frames ride the same local /api/events socket as
      presentation. The runtime only forwards shape-validated frames to an
      opted-in handler; the gpui chat CEF surface owns its own subscription,
      so this branch exists for parity with the shared native client switch
      and stays inert unless a handler is provided.
      */
      if (
        typeof message.type === 'string' &&
        isSessionChatEventType(message.type) &&
        onSessionChatEvent &&
        isGpuiSessionChatEventMessage(message)
      ) {
        onSessionChatEvent(message);
      }
    });
    socket.addEventListener('error', () => {
      onError();
    });
    socket.addEventListener('close', () => {
      if (!closedByClient) {
        onClose();
      }
    });
    return {
      close: () => {
        closedByClient = true;
        socket.close();
      },
    };
  }
}
