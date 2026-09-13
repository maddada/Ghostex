import type { SessionChatTransport } from '@/packages/core-ui/chat/session-chat-transport';
import {
  registerDraftWriter,
  replayDraftSaves,
  queueDraftSave,
  persistDraftsForRelease,
  type PendingDraft,
} from '@/packages/core-ui/chat/session-chat-draft-outbox';
import { GXSERVER_PROTOCOL_VERSION } from '@/packages/shared/gxserver-protocol';
import { disposeSessionChatRuntime, retainSessionChatRuntimeEndpoint, retainSessionChatTransport } from './store';
import type { SessionChatRuntimeIdentity } from './store';
import type { SessionChatRuntimeEndpoint } from './socket';

type Request = {
  epoch: string;
  generation: string;
  method:
    | 'read'
    | 'seed'
    | 'subscribe'
    | 'unsubscribe'
    | 'reconnect'
    | 'release'
    | 'endpoint'
    | 'adoptDrafts'
    | 'machineEndpoint';
  requestId?: string;
  clientId?: string;
  machineId?: string;
  identity?: SessionChatRuntimeIdentity;
  endpoint?: SessionChatRuntimeEndpoint;
  params?: { limit?: number; beforeOffset?: number; drafts?: PendingDraft[] };
};

/** The existing sidebar is the single app-wide owner of transcript caches, followers and draft retries. */
export function installSessionChatRuntimeBroker(): void {
  const epoch = crypto.randomUUID();
  const namespace = window.ghostexGpui as typeof window.ghostexGpui & {
    onSessionChatRuntimeRequest?: (request: Request) => void;
  };
  const replayedEndpoints = new Map<string, string>();
  const subscriptions = new Map<string, () => void>();
  const raw = {
    read: async () => {
      throw new Error('Shared reads are owned by the retained store.');
    },
    send: async () => {
      throw new Error('Commands belong to the chat activation.');
    },
    interrupt: async () => {
      throw new Error('Commands belong to the chat activation.');
    },
    answerPrompt: async () => {
      throw new Error('Commands belong to the chat activation.');
    },
  } satisfies Omit<SessionChatTransport, 'subscribe'>;
  let transferSequence = 0;
  const send = (payload: Record<string, unknown>): void => {
    window.webkit?.messageHandlers?.ghostexNativeHost?.postMessage({
      type: 'sessionChatRuntimeBroker',
      epoch,
      ...payload,
    });
  };
  const post = (payload: Record<string, unknown>): void => {
    const serialized = JSON.stringify(payload);
    const chunkSize = 96 * 1024;
    if (serialized.length <= chunkSize) {
      send(payload);
      return;
    }
    if (serialized.length > 64 * 1024 * 1024) {
      send({
        kind: 'response',
        generation: payload.generation,
        requestId: payload.requestId,
        error: 'This conversation exceeds the shared chat transfer limit.',
      });
      return;
    }
    const transferId = String(++transferSequence);
    const total = Math.ceil(serialized.length / chunkSize);
    for (let index = 0; index < total; index++)
      send({
        kind: 'chunk',
        generation: payload.generation,
        requestId: payload.requestId,
        transferId,
        index,
        total,
        data: serialized.slice(index * chunkSize, (index + 1) * chunkSize),
      });
  };
  const setupMachine = (machineId: string, candidate: SessionChatRuntimeEndpoint, originClientId?: string) => {
    const endpoint = retainSessionChatRuntimeEndpoint(machineId, candidate);
    const prefix = machineId === 'local' ? '' : `remote-${machineId}:`;
    const writeDraft = async (
      draft: { content: string; version: unknown; clientId?: string },
      projectId: string,
      sessionId: string
    ): Promise<void> => {
      if (!endpoint.baseUrl || !endpoint.authToken) throw new Error('The chat server connection is unavailable.');
      if (!draft.clientId && !originClientId)
        throw new Error('The original draft owner must reopen this conversation before retrying.');
      const response = await fetch(`${endpoint.baseUrl}/api/setSessionChatDraft`, {
        method: 'POST',
        redirect: 'error',
        signal: AbortSignal.timeout(25_000),
        headers: {
          authorization: `Bearer ${endpoint.authToken}`,
          'content-type': 'application/json',
          'x-gxserver-protocol-version': String(GXSERVER_PROTOCOL_VERSION),
        },
        body: JSON.stringify({
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
          params: {
            projectId,
            sessionId,
            content: draft.content,
            draftVersion: draft.version,
            clientId: draft.clientId ?? originClientId,
          },
        }),
      });
      const body = (await response.json()) as { ok?: boolean };
      if (!response.ok || !body.ok) throw new Error('The draft could not be saved.');
    };
    const endpointIdentity = JSON.stringify(candidate);
    if (replayedEndpoints.get(machineId) !== endpointIdentity) {
      replayedEndpoints.set(machineId, endpointIdentity);
      replayDraftSaves(prefix, writeDraft);
    }
    return { endpoint, prefix, writeDraft };
  };
  namespace.onSessionChatRuntimeRequest = (request) => {
    if (request.epoch !== epoch) return;
    if (request.method === 'machineEndpoint' && request.machineId && request.endpoint) {
      setupMachine(request.machineId, request.endpoint);
      return;
    }
    if (!/^\d+$/.test(request.generation)) return;
    if (request.method === 'release' || request.method === 'unsubscribe') {
      subscriptions.get(request.generation)?.();
      subscriptions.delete(request.generation);
      return;
    }
    if (!request.identity || !request.endpoint) return;
    const { identity } = request;
    const { endpoint, prefix, writeDraft } = setupMachine(identity.machineId, request.endpoint, request.clientId);
    registerDraftWriter(`${prefix}${identity.projectId}:${identity.sessionId}`, (draft) =>
      writeDraft(draft, identity.projectId, identity.sessionId)
    );
    if (request.method === 'adoptDrafts') {
      const sessionKey = `${prefix}${identity.projectId}:${identity.sessionId}`;
      for (const draft of request.params?.drafts ?? [])
        queueDraftSave({ ...draft, sessionKey, clientId: draft.clientId ?? request.clientId });
      void persistDraftsForRelease(sessionKey).then(
        () => post({ kind: 'response', generation: request.generation, requestId: request.requestId, ok: true }),
        (error: unknown) =>
          post({
            kind: 'response',
            generation: request.generation,
            requestId: request.requestId,
            error: error instanceof Error ? error.message : String(error),
          })
      );
      return;
    }
    if (request.method === 'endpoint') return;
    const transport = retainSessionChatTransport(identity, raw, endpoint);
    if (request.method === 'subscribe') {
      const unsubscribe = transport.subscribe({
        currentLimit: () => request.params?.limit ?? 120,
        onEvent: (event) => post({ kind: 'event', generation: request.generation, event }),
      });
      subscriptions.get(request.generation)?.();
      subscriptions.set(request.generation, unsubscribe);
      return;
    }
    if (request.method === 'reconnect') {
      transport.reconnect?.();
      return;
    }
    const operation =
      request.method === 'seed' ? transport.seed!(request.params ?? {}) : transport.read(request.params ?? {});
    void operation.then(
      (snapshot) =>
        post({
          kind: 'response',
          generation: request.generation,
          requestId: request.requestId,
          cacheable: request.params?.beforeOffset === undefined,
          snapshot,
        }),
      (error: unknown) =>
        post({
          kind: 'response',
          generation: request.generation,
          requestId: request.requestId,
          error: error instanceof Error ? error.message : String(error),
        })
    );
  };
  post({ kind: 'ready' });
  window.addEventListener('pagehide', () => {
    for (const unsubscribe of subscriptions.values()) unsubscribe();
    disposeSessionChatRuntime();
  });
}
