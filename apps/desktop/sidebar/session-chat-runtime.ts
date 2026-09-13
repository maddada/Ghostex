import type { PendingDraft } from '@/packages/core-ui/chat/session-chat-draft-outbox';
import { GXSERVER_PROTOCOL_VERSION } from '@/packages/shared/gxserver-protocol';
import { mergeSessionChatMessagesWith } from '@/packages/core-ui/chat/session-chat-merge';
import type { SessionChatTransport } from '@/packages/core-ui/chat/session-chat-transport';
import type { GxserverReadSessionChatResult, GxserverSessionChatEvent } from '@/packages/shared/session-chat';
import { foldSessionChatAppend, foldSessionChatState } from './session-chat-runtime/fold';
import type { SessionChatRuntimeEndpoint } from './session-chat-runtime/socket';
export type { SessionChatRuntimeIdentity } from './session-chat-runtime/store';
import type { SessionChatRuntimeIdentity } from './session-chat-runtime/store';

type Listener = Parameters<SessionChatTransport['subscribe']>[0];
type Message = {
  kind: 'response' | 'event' | 'reset' | 'chunk';
  transferId?: string;
  index?: number;
  total?: number;
  data?: string;
  ok?: boolean;
  requestId?: string;
  snapshot?: GxserverReadSessionChatResult;
  event?: GxserverSessionChatEvent;
  error?: string;
};
interface Host {
  generation: string;
  initialSnapshot?: GxserverReadSessionChatResult;
  send: (payload: Record<string, unknown>) => void;
}
const endpoints = new Map<string, SessionChatRuntimeEndpoint>();
const clients = new Map<
  string,
  {
    machineId: string;
    endpointChanged: () => void;
    adopt: (drafts: PendingDraft[]) => Promise<void>;
    dispose: () => void;
  }
>();

export function retainSessionChatRuntimeEndpoint(
  machineId: string,
  candidate: SessionChatRuntimeEndpoint
): SessionChatRuntimeEndpoint {
  let endpoint = endpoints.get(machineId);
  if (endpoint) Object.assign(endpoint, candidate);
  else {
    endpoint = { ...candidate };
    endpoints.set(machineId, endpoint);
  }
  return endpoint;
}
export function updateSessionChatRuntimeEndpoint(machineId: string, endpoint?: SessionChatRuntimeEndpoint): void {
  retainSessionChatRuntimeEndpoint(machineId, endpoint ?? { baseUrl: '', authToken: '' });
  for (const client of clients.values()) if (client.machineId === machineId) client.endpointChanged();
}

/** Only the mounted conversation lives here; the sidebar owns inactive snapshots and subscriptions. */
export function retainSessionChatTransport(
  identity: SessionChatRuntimeIdentity,
  raw: Omit<SessionChatTransport, 'subscribe'>,
  endpoint: SessionChatRuntimeEndpoint,
  host: Host
): SessionChatTransport {
  retainSessionChatRuntimeEndpoint(identity.machineId, endpoint);
  let snapshot = host.initialSnapshot;
  let sequence = 0;
  let disposed = false;
  let recovering = false;
  let serverId = '';
  let confirmed = false;
  let streamRevision = 0;
  const listeners = new Set<Listener>();
  const pending = new Map<
    string,
    {
      resolve: (value: unknown) => void;
      reject: (error: Error) => void;
      timer: ReturnType<typeof setTimeout>;
      beforeOffset?: number;
      revision: number;
    }
  >();
  const namespace = window.ghostexGpui as typeof window.ghostexGpui & {
    onSessionChatRuntimeMessage?: (message: Message) => void;
  };
  const send = (method: string, params?: Record<string, unknown>, requestId?: string): void => {
    if (!disposed) host.send({ method, params, requestId });
  };
  const subscribe = (): void =>
    send('subscribe', { limit: Math.max(120, ...[...listeners].map((listener) => listener.currentLimit?.() ?? 0)) });
  const rejectPending = (reason: string): void => {
    for (const request of pending.values()) {
      clearTimeout(request.timer);
      request.reject(new Error(reason));
    }
    pending.clear();
  };
  const transfers = new Map<
    string,
    { parts: string[]; total: number; length: number; timer: ReturnType<typeof setTimeout> }
  >();
  const clearTransfers = (): void => {
    for (const transfer of transfers.values()) clearTimeout(transfer.timer);
    transfers.clear();
  };
  const recoverStream = (): void => {
    if (recovering || disposed || !listeners.size) return;
    recovering = true;
    void request('read', { limit: Math.max(120, snapshot?.messages.length ?? 0) })
      .then(
        (next) => {
          for (const listener of listeners)
            listener.onEvent({
              ...next,
              type: 'sessionChatSnapshot',
              projectId: identity.projectId,
              sessionId: identity.sessionId,
              serverId,
              protocolVersion: GXSERVER_PROTOCOL_VERSION,
            });
        },
        (error: unknown) => {
          const next = {
            messages: [],
            hasMore: false,
            beforeOffset: 0,
            epoch: 0,
            seq: 0,
            ...snapshot,
            status: 'error' as const,
            error: error instanceof Error ? error.message : String(error),
          };
          for (const listener of listeners)
            listener.onEvent({
              ...next,
              type: 'sessionChatSnapshot',
              projectId: identity.projectId,
              sessionId: identity.sessionId,
              serverId,
              protocolVersion: GXSERVER_PROTOCOL_VERSION,
            });
        }
      )
      .finally(() => {
        recovering = false;
      });
  };
  const receive = (message: Message): void => {
    if (disposed) return;
    if (message.kind === 'chunk') {
      const { transferId, index, total, data } = message;
      if (
        !transferId ||
        !Number.isInteger(index) ||
        !Number.isInteger(total) ||
        total! < 1 ||
        total! > 683 ||
        typeof data !== 'string' ||
        data.length > 96 * 1024
      ) {
        rejectPending('Invalid shared chat transfer.');
        clearTransfers();
        recoverStream();
        return;
      }
      let transfer = transfers.get(transferId);
      if (!transfer && index === 0 && transfers.size < 1) {
        transfer = {
          parts: [],
          total: total!,
          length: 0,
          timer: setTimeout(() => {
            transfers.delete(transferId);
            rejectPending('The shared chat transfer timed out.');
            recoverStream();
          }, 30_000),
        };
        transfers.set(transferId, transfer);
      }
      if (!transfer || transfer.total !== total || transfer.parts.length !== index) {
        rejectPending('The shared chat transfer was interrupted.');
        clearTransfers();
        recoverStream();
        return;
      }
      transfer.length += data.length;
      if (transfer.length > 64 * 1024 * 1024) {
        rejectPending('The shared chat transfer is too large.');
        clearTransfers();
        recoverStream();
        return;
      }
      transfer.parts.push(data);
      if (transfer.parts.length === transfer.total) {
        clearTimeout(transfer.timer);
        transfers.delete(transferId);
        try {
          receive(JSON.parse(transfer.parts.join('')) as Message);
        } catch {
          rejectPending('Invalid shared chat transfer.');
          recoverStream();
        }
      }
      return;
    }
    if (message.kind === 'reset') {
      clearTransfers();
      confirmed = false;
      streamRevision++;
      rejectPending('The shared chat service reconnected.');
      if (listeners.size) subscribe();
      return;
    }
    if (message.kind === 'event' && message.event) {
      const event = message.event;
      if (event.type === 'sessionChatSnapshot' || event.type === 'sessionChatReplaced') {
        if (
          confirmed &&
          serverId === event.serverId &&
          snapshot &&
          (event.epoch < snapshot.epoch || (event.epoch === snapshot.epoch && event.seq < snapshot.seq))
        )
          return;
        snapshot = foldSessionChatState(serverId && serverId !== event.serverId ? undefined : snapshot, event);
        serverId = event.serverId;
        confirmed = true;
      } else {
        if (
          !confirmed ||
          !snapshot ||
          event.serverId !== serverId ||
          event.epoch !== snapshot.epoch ||
          event.seq > snapshot.seq + 1
        ) {
          recoverStream();
          return;
        }
        if (event.seq <= snapshot.seq) return;
        snapshot =
          event.type === 'sessionChatAppended'
            ? foldSessionChatAppend(snapshot, event)
            : foldSessionChatState(snapshot, event);
      }
      streamRevision++;
      for (const listener of listeners) listener.onEvent(event);
      return;
    }
    const request = pending.get(message.requestId ?? '');
    if (!request) {
      if (message.error && !message.requestId) recoverStream();
      return;
    }
    pending.delete(message.requestId!);
    clearTimeout(request.timer);
    if (message.ok) {
      request.resolve(undefined);
      return;
    }
    if (message.error || !message.snapshot)
      request.reject(new Error(message.error ?? 'The shared chat service returned no conversation.'));
    else {
      if (request.revision !== streamRevision && request.beforeOffset === undefined && snapshot) {
        request.resolve(snapshot);
        return;
      }
      snapshot =
        request.beforeOffset !== undefined && snapshot && snapshot.epoch === message.snapshot.epoch
          ? {
              ...snapshot,
              messages: [...mergeSessionChatMessagesWith(message.snapshot.messages, snapshot.messages)],
              hasMore: message.snapshot.hasMore,
              hasMoreExact: message.snapshot.hasMoreExact,
              beforeOffset: message.snapshot.beforeOffset,
            }
          : message.snapshot;
      request.resolve(message.snapshot);
    }
  };
  namespace.onSessionChatRuntimeMessage = receive;
  const request = <T = GxserverReadSessionChatResult>(method: string, params: Record<string, unknown>): Promise<T> =>
    new Promise((resolve, reject) => {
      if (disposed) {
        reject(new Error('The chat view was released.'));
        return;
      }
      const requestId = `${host.generation}:${++sequence}`;
      const timer = setTimeout(() => {
        pending.delete(requestId);
        reject(new Error('The shared chat service did not answer.'));
      }, 30_000);
      pending.set(requestId, {
        resolve: (value) => resolve(value as T),
        reject,
        timer,
        revision: streamRevision,
        beforeOffset: typeof params.beforeOffset === 'number' ? params.beforeOffset : undefined,
      });
      send(method, params, requestId);
    });
  clients.set(host.generation, {
    machineId: identity.machineId,
    endpointChanged: () => send('endpoint'),
    adopt: (drafts) => request<void>('adoptDrafts', { drafts }),
    dispose: () => {
      if (disposed) return;
      send('unsubscribe');
      disposed = true;
      clearTransfers();
      rejectPending('The chat view was released.');
      listeners.clear();
      snapshot = undefined;
      if (namespace.onSessionChatRuntimeMessage === receive) delete namespace.onSessionChatRuntimeMessage;
    },
  });
  return {
    ...raw,
    getCachedSnapshot: () => snapshot,
    seed: (params) => (snapshot ? Promise.resolve(snapshot) : request('seed', params)),
    read: (params) => request('read', params),
    reconnect: () => {
      send('reconnect');
      if (listeners.size) subscribe();
    },
    subscribe: (listener) => {
      listeners.add(listener);
      subscribe();
      return () => {
        listeners.delete(listener);
        if (!listeners.size) send('unsubscribe');
      };
    },
  };
}
export function disposeSessionChatActivation(generation: string): void {
  clients.get(generation)?.dispose();
  clients.delete(generation);
}
export function disposeSessionChatRuntime(): void {
  for (const client of clients.values()) client.dispose();
  clients.clear();
  endpoints.clear();
}

export function adoptSessionChatDrafts(generation: string, drafts: PendingDraft[]): Promise<void> {
  const client = clients.get(generation);
  return client ? client.adopt(drafts) : Promise.reject(new Error('The chat view was released.'));
}
