/*
CDXC:RepoStructure 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  gpuiGxserverRpcErrorCode,
  gpuiGxserverRpcErrorMessage,
  isGxserverRpcSuccess,
  parseObject,
  readJson,
} from './helpers/records';
import { isPresentationDelta, isPresentationSnapshot } from './helpers/remote-presentation';
import type { GpuiPresentationSubscription, GpuiValidatedGxserverBootstrap } from './types-and-protocol';
import type {
  GxserverAppUserData,
  GxserverEndpointPath,
  GxserverPresentationDelta,
  GxserverPresentationSnapshot,
  GxserverProjectDomainState,
  GxserverRecentProjectDomainState,
  GxserverRpcErrorCode,
} from '@/packages/shared/gxserver-protocol';
import { GXSERVER_PROTOCOL_VERSION } from '@/packages/shared/gxserver-protocol';

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

  async fetchAppUserData(): Promise<GxserverAppUserData> {
    return this.rpc<GxserverAppUserData>('/api/readAppUserData');
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
    onNotificationFeedChanged,
    onSnapshot,
    onSnapshotCurrent,
  }: {
    clientId: string;
    lastRevision: number;
    onClose: () => void;
    onDelta: (delta: GxserverPresentationDelta, revision: number) => void;
    onError: () => void;
    onGlobalSidebarCommands?: () => void;
    onNotificationFeedChanged?: () => void;
    onSnapshot: (snapshot: GxserverPresentationSnapshot) => void;
    /**
     * The daemon's answer when `lastRevision` already names its current
     * revision: no snapshot follows, because nothing changed. Carries that
     * revision back for the caller to assert against.
     */
    onSnapshotCurrent?: (revision: number) => void;
  }): GpuiPresentationSubscription {
    const url = new URL(`${this.bootstrap.baseUrl}/api/events`);
    url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
    url.searchParams.set('protocolVersion', String(GXSERVER_PROTOCOL_VERSION));
    url.searchParams.set('authToken', this.bootstrap.authToken);

    const socket = new WebSocket(url.toString());
    let closedByClient = false;
    /*
    CDXC:CefRuntime 2026-09-25 WHY:
    This socket no longer asks for CLI renderer commands: the desktop store's
    own gx-client socket registers for them and answers them in Rust
    (apps/desktop/src/app/gx_store/renderer_commands/). Registering here too
    would split the CLI's commands between two answerers.
    */
    socket.addEventListener('open', () => {
      socket.send(
        JSON.stringify({
          clientId,
          lastRevision,
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
      if (message.type === 'presentationSnapshotCurrent' && typeof message.revision === 'number') {
        onSnapshotCurrent?.(message.revision);
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
      /*
      CDXC:AgentLauncher 2026-08-07:
      The Global Actions announcement carries no list — the handler refetches
      the HUD, which is the one projection of it — so there is no payload to
      shape-validate before forwarding it.
      */
      if (message.type === 'globalSidebarCommandsChanged' && onGlobalSidebarCommands) {
        onGlobalSidebarCommands();
        return;
      }
      // The feed announcement carries no rows either; the handler refetches the feed.
      if (message.type === 'notificationFeedChanged' && onNotificationFeedChanged) {
        onNotificationFeedChanged();
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
