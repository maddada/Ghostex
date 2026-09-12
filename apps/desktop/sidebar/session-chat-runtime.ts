import type { SessionChatTransport } from '@/packages/core-ui/chat/session-chat-transport';
import { SESSION_CHAT_INITIAL_LIMIT, SESSION_CHAT_MAX_LIMIT } from '@/packages/core-ui/chat/session-chat-pagination';
import { mergeSessionChatMessagesWith } from '@/packages/core-ui/chat/session-chat-merge';
import { GXSERVER_PROTOCOL_VERSION } from '@/packages/shared/gxserver-protocol';
import { gxserverRpcErrorFromResponseBody } from '@/packages/shared/gxserver-rpc-error';
import type { GxserverReadSessionChatResult, GxserverSessionChatEvent } from '@/packages/shared/session-chat';
import { foldSessionChatAppend, foldSessionChatState } from './session-chat-runtime/fold';
import { persistSessionChat, readPersistedSessionChat } from './session-chat-runtime/persistence';
import { SessionChatSocket, type SessionChatRuntimeEndpoint } from './session-chat-runtime/socket';

export interface SessionChatRuntimeIdentity {
  machineId: string;
  projectId: string;
  sessionId: string;
}

type Listener = Parameters<SessionChatTransport['subscribe']>[0];
type RawTransport = Omit<SessionChatTransport, 'subscribe'>;
const MAX_RETAINED_SESSIONS = 12;
const IDLE_RETENTION_MS = 5 * 60 * 1_000;
const MAX_RETAINED_MESSAGES = 1_200;
const MAX_RETAINED_BYTES = 4 * 1024 * 1024;
const entries = new Map<string, RetainedSession>();
const servers = new Map<string, SessionChatSocket>();
const endpoints = new Map<string, SessionChatRuntimeEndpoint>();

/**
 * CDXC:SessionChat 2026-09-12 WHY:
 * Draft outbox writers survive their mounted chat, so every activation on one machine shares its mutable RPC endpoint while native callbacks retain their original activation generation.
 * Socket endpoints remain immutable connection snapshots, allowing reads from replaced credentials or tunnels to be rejected.
 */
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
  servers.get(machineId)?.update(endpoint);
  return endpoint;
}

function serverFor(machineId: string, endpoint: SessionChatRuntimeEndpoint): SessionChatSocket {
  let server = servers.get(machineId);
  if (!server) {
    server = new SessionChatSocket(endpoint);
    servers.set(machineId, server);
  } else server.update(endpoint);
  return server;
}

function prune(): void {
  const idle = [...entries.values()].filter((entry) => !entry.listeners.size).sort((a, b) => a.touchedAt - b.touchedAt);
  while (entries.size > MAX_RETAINED_SESSIONS && idle.length) idle.shift()!.dispose();
}

/**
 * CDXC:SessionChat 2026-09-12 DECISION:
 * User approved retaining chat data and live subscriptions independently of mounted views for fast session switching.
 * The server subscribes snapshot-first and has no afterSeq replay contract; short switches retain their subscription, while reconnects replace from an authoritative snapshot without clearing the cached view.
 */
class RetainedSession {
  readonly key: string;
  readonly streamKey: string;
  readonly transport: SessionChatTransport;
  readonly listeners = new Set<Listener>();
  touchedAt = Date.now();
  private snapshot?: GxserverReadSessionChatResult;
  private confirmed = false;
  private serverId?: string;
  private disposed = false;
  private unfollow?: () => void;
  private expiry?: ReturnType<typeof setTimeout>;
  private persistTimer?: ReturnType<typeof setTimeout>;
  private resyncTimer?: ReturnType<typeof setTimeout>;
  private resyncAttempt = 0;
  private revision = 0;
  private connectionGeneration = 0;
  private readFlight?: Promise<GxserverReadSessionChatResult>;
  private readFlightLimit = 0;
  private requestedWindow = SESSION_CHAT_INITIAL_LIMIT;
  private readAt = 0;
  private updatedAt = 0;
  private byteSizeCheckedAt = 0;
  private commandKeys = new Set<string>();
  private hydration: Promise<void>;

  constructor(
    readonly identity: SessionChatRuntimeIdentity,
    raw: RawTransport,
    readonly server: SessionChatSocket
  ) {
    this.key = JSON.stringify([identity.machineId, identity.projectId, identity.sessionId]);
    this.streamKey = JSON.stringify([identity.projectId, identity.sessionId]);
    this.transport = {
      ...raw,
      read: (params) => this.read(params),
      seed: async (params) => {
        await this.hydration;
        const cached = this.snapshot;
        if (!cached || cached.status === 'starting' || cached.status === 'loading' || cached.status === 'error')
          return this.read(params);
        if (Date.now() - this.readAt > 30_000) void this.read(params).catch(() => this.requestResync());
        return cached;
      },
      getCachedSnapshot: () => this.snapshot,
      subscribe: (listener) => this.subscribe(listener),
      reconnect: () => this.server.refresh(this.streamKey),
    };
    this.hydration = readPersistedSessionChat(this.key).then((stored) => {
      if (!this.disposed && !this.snapshot && stored) {
        this.snapshot = stored.snapshot;
        this.updatedAt = stored.savedAt;
        this.requestedWindow = Math.max(
          SESSION_CHAT_INITIAL_LIMIT,
          stored.requestedWindow ?? stored.snapshot.messages.length
        );
        this.emitSnapshot();
      }
    });
    this.update(raw);
    this.scheduleExpiry();
  }

  update(raw: RawTransport): void {
    // Activation callbacks belong to that activation. Future commands use the
    // new delegates; promises already started keep their captured callbacks.
    const { read: _read, seed: _seed, getCachedSnapshot: _cached, reconnect: _reconnect, ...commands } = raw;
    for (const key of this.commandKeys) {
      if (!(key in commands)) Reflect.deleteProperty(this.transport, key);
    }
    this.commandKeys = new Set(Object.keys(commands));
    Object.assign(this.transport, commands);
    this.touchedAt = Date.now();
  }

  private limit(): number {
    let limit = Math.max(this.requestedWindow, this.snapshot?.messages.length ?? 0);
    for (const listener of this.listeners) limit = Math.max(limit, listener.currentLimit?.() ?? 0);
    return Math.min(limit, SESSION_CHAT_MAX_LIMIT);
  }

  private subscribe(listener: Listener): () => void {
    if (this.disposed) throw new Error('This chat runtime has been disposed.');
    if (this.expiry) clearTimeout(this.expiry);
    this.expiry = undefined;
    this.touchedAt = Date.now();
    this.listeners.add(listener);
    if (this.snapshot) listener.onEvent(this.snapshotEvent());
    if (!this.unfollow) {
      this.unfollow = this.server.follow(this.streamKey, {
        ...this.identity,
        limit: () => this.limit(),
        receive: (event) => this.receive(event),
        reconnect: () => {
          this.confirmed = false;
          this.connectionGeneration += 1;
        },
      });
    }
    prune();
    return () => {
      this.listeners.delete(listener);
      this.touchedAt = Date.now();
      if (!this.listeners.size) {
        this.scheduleExpiry();
        this.compactIfNeeded();
        prune();
      }
    };
  }

  private snapshotEvent(): Extract<GxserverSessionChatEvent, { type: 'sessionChatSnapshot' }> {
    return {
      ...this.snapshot!,
      // Own-properties also clear the read-only draft metadata on promotion.
      availableAgents: this.snapshot!.availableAgents,
      switchableAgents: this.snapshot!.switchableAgents,
      sessionAgentId: this.snapshot!.sessionAgentId,
      type: 'sessionChatSnapshot',
      projectId: this.identity.projectId,
      sessionId: this.identity.sessionId,
      serverId: this.serverId ?? '',
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
    };
  }

  private emitSnapshot(): void {
    if (!this.snapshot) return;
    const event = this.snapshotEvent();
    for (const listener of this.listeners) listener.onEvent(event);
  }

  private receive(event: GxserverSessionChatEvent): void {
    if (this.disposed) return;
    if (event.type === 'sessionChatSnapshot' || event.type === 'sessionChatReplaced') {
      if (
        this.confirmed &&
        event.serverId === this.serverId &&
        this.snapshot &&
        (event.epoch < this.snapshot.epoch || (event.epoch === this.snapshot.epoch && event.seq < this.snapshot.seq))
      )
        return;
      const previous = this.serverId && this.serverId !== event.serverId ? undefined : this.snapshot;
      this.snapshot = foldSessionChatState(previous, event);
      this.serverId = event.serverId;
      this.confirmed = true;
    } else {
      const previous = this.snapshot;
      if (
        this.confirmed &&
        previous &&
        event.serverId === this.serverId &&
        event.epoch === previous.epoch &&
        event.seq <= previous.seq
      )
        return;
      if (
        !this.confirmed ||
        !previous ||
        event.serverId !== this.serverId ||
        event.epoch !== previous.epoch ||
        event.seq !== previous.seq + 1
      ) {
        this.requestResync();
        return;
      }
      this.snapshot =
        event.type === 'sessionChatAppended'
          ? foldSessionChatAppend(previous, event)
          : foldSessionChatState(previous, event);
    }
    this.revision += 1;
    this.resyncAttempt = 0;
    for (const listener of this.listeners) listener.onEvent(event);
    this.schedulePersistence();
    this.compactIfNeeded();
  }

  private async fetchRead(params: { limit?: number; beforeOffset?: number }): Promise<GxserverReadSessionChatResult> {
    const endpoint = this.server.endpoint;
    if (!endpoint.baseUrl || !endpoint.authToken) throw new Error('The chat server connection is unavailable.');
    const path = '/api/readSessionChat';
    const response = await fetch(`${endpoint.baseUrl}${path}`, {
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
          ...params,
          projectId: this.identity.projectId,
          sessionId: this.identity.sessionId,
        },
      }),
    });
    const body: unknown = await response.json();
    const envelope = body as { ok?: boolean; result?: GxserverReadSessionChatResult };
    if (!response.ok || envelope.ok !== true || !envelope.result) {
      throw gxserverRpcErrorFromResponseBody(path, body) ?? new Error(`Conversation read failed (${response.status}).`);
    }
    return envelope.result;
  }

  private read(params: { limit?: number; beforeOffset?: number }): Promise<GxserverReadSessionChatResult> {
    if (params.beforeOffset !== undefined) {
      const endpoint = this.server.endpoint;
      const generation = this.connectionGeneration;
      return this.fetchRead(params).then((result) => {
        const previous = this.snapshot;
        if (
          endpoint !== this.server.endpoint ||
          generation !== this.connectionGeneration ||
          (previous && previous.epoch !== result.epoch)
        ) {
          throw new Error('The conversation changed while loading earlier history.');
        }
        if (
          !this.disposed &&
          endpoint === this.server.endpoint &&
          generation === this.connectionGeneration &&
          previous?.epoch === result.epoch &&
          previous.beforeOffset === params.beforeOffset
        ) {
          // Filtered or duplicate-only pages still consume history. Raise the
          // requested window before refreshing, even when row count is unchanged.
          this.requestedWindow = Math.min(
            SESSION_CHAT_MAX_LIMIT,
            this.limit() + (params.limit ?? SESSION_CHAT_INITIAL_LIMIT)
          );
          this.snapshot = {
            ...previous,
            messages: mergeSessionChatMessagesWith(
              result.messages,
              previous.messages
            ) as GxserverReadSessionChatResult['messages'],
            beforeOffset: result.beforeOffset,
            hasMore: result.hasMore,
            hasMoreExact: result.hasMoreExact,
          };
          this.server.refresh(this.streamKey);
          this.schedulePersistence();
        }
        return result;
      });
    }
    const requestedLimit = Math.max(params.limit ?? SESSION_CHAT_INITIAL_LIMIT, this.requestedWindow);
    this.requestedWindow = Math.min(SESSION_CHAT_MAX_LIMIT, Math.max(this.requestedWindow, requestedLimit));
    if (this.readFlight) {
      if (requestedLimit <= this.readFlightLimit) return this.readFlight;
      return this.readFlight.then(() => this.read(params));
    }
    this.readFlightLimit = requestedLimit;
    const revision = this.revision;
    const generation = this.connectionGeneration;
    const endpoint = this.server.endpoint;
    const flight = this.fetchRead({ ...params, limit: requestedLimit })
      .then((result) => {
        if (this.disposed) return result;
        if (endpoint !== this.server.endpoint || generation !== this.connectionGeneration) {
          this.requestResync();
          return this.snapshot ?? result;
        }
        const previous = this.snapshot;
        const stale =
          previous &&
          this.confirmed &&
          (result.epoch < previous.epoch || (result.epoch === previous.epoch && result.seq < previous.seq));
        if (stale || (revision !== this.revision && !this.confirmed)) {
          // A read never rolls a newer live tail back. Read-only metadata is
          // accepted only for the generation that still owns the conversation.
          if (previous && result.epoch === previous.epoch) {
            this.snapshot = foldSessionChatState(previous, {
              ...result,
              ...previous,
              agent: result.agent ?? previous.agent,
              selectedOptions: result.selectedOptions,
              availableAgents: result.availableAgents,
              switchableAgents: result.switchableAgents,
              sessionAgentId: result.sessionAgentId,
            });
            this.readAt = Date.now();
            this.emitSnapshot();
            this.schedulePersistence();
          }
          return this.snapshot!;
        }
        this.snapshot = foldSessionChatState(previous, result);
        this.readAt = Date.now();
        this.snapshot.availableAgents = result.availableAgents;
        this.snapshot.switchableAgents = result.switchableAgents;
        this.snapshot.sessionAgentId = result.sessionAgentId;
        this.revision += 1;
        this.emitSnapshot();
        this.schedulePersistence();
        return this.snapshot;
      })
      .finally(() => {
        if (this.readFlight === flight) this.readFlight = undefined;
      });
    this.readFlight = flight;
    return flight;
  }

  private requestResync(): void {
    if (this.resyncTimer || this.disposed) return;
    this.resyncTimer = setTimeout(
      () => {
        this.resyncTimer = undefined;
        this.server.refresh(this.streamKey);
        void this.read({ limit: this.limit() }).catch(() => {
          this.resyncAttempt += 1;
          this.requestResync();
        });
      },
      Math.min(250 * 2 ** this.resyncAttempt, 10_000)
    );
  }

  private compactIfNeeded(): void {
    if (this.listeners.size || !this.snapshot) return;
    const checkBytes = Date.now() - this.byteSizeCheckedAt > 5_000;
    if (checkBytes) this.byteSizeCheckedAt = Date.now();
    if (
      this.snapshot.messages.length > MAX_RETAINED_MESSAGES ||
      (checkBytes && JSON.stringify(this.snapshot).length * 2 > MAX_RETAINED_BYTES)
    ) {
      // Evict oversized tails instead of slicing them with an invalid cursor.
      // A later activation restores its persisted tail and requests a fresh window.
      this.dispose();
    }
  }

  private schedulePersistence(): void {
    this.updatedAt = Date.now();
    if (this.persistTimer) return;
    this.persistTimer = setTimeout(() => {
      this.persistTimer = undefined;
      if (this.snapshot) void persistSessionChat(this.key, this.snapshot, this.updatedAt, this.requestedWindow);
    }, 1_000);
  }

  private scheduleExpiry(): void {
    if (this.expiry) clearTimeout(this.expiry);
    this.expiry = setTimeout(() => this.dispose(), IDLE_RETENTION_MS);
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    if (this.snapshot) void persistSessionChat(this.key, this.snapshot, this.updatedAt, this.requestedWindow);
    this.unfollow?.();
    if (this.expiry) clearTimeout(this.expiry);
    if (this.persistTimer) clearTimeout(this.persistTimer);
    if (this.resyncTimer) clearTimeout(this.resyncTimer);
    this.listeners.clear();
    entries.delete(this.key);
    this.snapshot = undefined;
  }
}

export function retainSessionChatTransport(
  identity: SessionChatRuntimeIdentity,
  raw: RawTransport,
  endpoint: SessionChatRuntimeEndpoint
): SessionChatTransport {
  const server = serverFor(identity.machineId, endpoint);
  const key = JSON.stringify([identity.machineId, identity.projectId, identity.sessionId]);
  let entry = entries.get(key);
  if (entry) entry.update(raw);
  else {
    entry = new RetainedSession(identity, raw, server);
    entries.set(key, entry);
  }
  return entry.transport;
}

export function updateSessionChatRuntimeEndpoint(machineId: string, endpoint?: SessionChatRuntimeEndpoint): void {
  retainSessionChatRuntimeEndpoint(machineId, endpoint ?? { baseUrl: '', authToken: '' });
}

export function disposeSessionChatRuntime(): void {
  for (const entry of entries.values()) entry.dispose();
  for (const server of servers.values()) server.dispose();
  servers.clear();
  endpoints.clear();
}
