import { GXSERVER_PROTOCOL_VERSION } from '@/packages/shared/gxserver-protocol';
import type { GxserverSessionChatEvent } from '@/packages/shared/session-chat';

export interface SessionChatRuntimeEndpoint {
  baseUrl: string;
  authToken: string;
}

interface Follower {
  projectId: string;
  sessionId: string;
  limit: () => number;
  receive: (event: GxserverSessionChatEvent) => void;
  reconnect: () => void;
}

const RECONNECT_DELAYS = [500, 1_000, 2_000, 5_000, 10_000];
const FRAME_TYPES = new Set(['sessionChatSnapshot', 'sessionChatReplaced', 'sessionChatAppended', 'sessionChatState']);

/** One connection carries every retained conversation on this server in this renderer. */
export class SessionChatSocket {
  private followers = new Map<string, Follower>();
  private socket?: WebSocket;
  private retry?: ReturnType<typeof setTimeout>;
  private attempts = 0;
  private generation = 0;
  endpoint: SessionChatRuntimeEndpoint;

  constructor(endpoint: SessionChatRuntimeEndpoint) {
    this.endpoint = { ...endpoint };
  }

  update(endpoint: SessionChatRuntimeEndpoint): void {
    if (endpoint.baseUrl === this.endpoint.baseUrl && endpoint.authToken === this.endpoint.authToken) return;
    this.endpoint = { ...endpoint };
    this.disconnect();
    if (this.followers.size) this.connect();
  }

  follow(key: string, follower: Follower): () => void {
    this.followers.set(key, follower);
    if (this.socket?.readyState === WebSocket.OPEN) this.subscribe(follower);
    else if (!this.socket && !this.retry) this.connect();
    return () => {
      if (this.followers.get(key) !== follower) return;
      this.followers.delete(key);
      if (this.socket?.readyState === WebSocket.OPEN) {
        this.socket.send(
          JSON.stringify({
            type: 'unsubscribeSessionChat',
            projectId: follower.projectId,
            sessionId: follower.sessionId,
          })
        );
      }
      if (!this.followers.size) this.disconnect();
    };
  }

  refresh(key: string): void {
    const follower = this.followers.get(key);
    if (follower && this.socket?.readyState === WebSocket.OPEN) this.subscribe(follower);
  }

  dispose(): void {
    this.followers.clear();
    this.disconnect();
  }

  private subscribe(follower: Follower): void {
    this.socket?.send(
      JSON.stringify({
        type: 'subscribeSessionChat',
        projectId: follower.projectId,
        sessionId: follower.sessionId,
        limit: follower.limit(),
      })
    );
  }

  private disconnect(): void {
    this.generation += 1;
    if (this.retry) clearTimeout(this.retry);
    this.retry = undefined;
    const previous = this.socket;
    this.socket = undefined;
    previous?.close();
  }

  private connect(): void {
    if (!this.followers.size || !this.endpoint.baseUrl || !this.endpoint.authToken) return;
    const generation = ++this.generation;
    const url = new URL(`${this.endpoint.baseUrl}/api/events`);
    url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
    url.searchParams.set('protocolVersion', String(GXSERVER_PROTOCOL_VERSION));
    url.searchParams.set('authToken', this.endpoint.authToken);
    // Browser WebSocket handshakes reject redirects (WHATWG opening handshake, step 2).
    const socket = new WebSocket(url.toString());
    this.socket = socket;
    const current = () => this.generation === generation && this.socket === socket;
    socket.addEventListener('open', () => {
      if (!current()) return;
      this.attempts = 0;
      for (const follower of this.followers.values()) {
        follower.reconnect();
        this.subscribe(follower);
      }
    });
    socket.addEventListener('message', (event) => {
      if (!current()) return;
      let parsed: unknown;
      try {
        parsed = JSON.parse(String(event.data));
      } catch {
        return;
      }
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return;
      const frame = parsed as Record<string, unknown>;
      if (
        typeof frame.type !== 'string' ||
        !FRAME_TYPES.has(frame.type) ||
        typeof frame.projectId !== 'string' ||
        typeof frame.sessionId !== 'string' ||
        typeof frame.epoch !== 'number' ||
        typeof frame.seq !== 'number' ||
        typeof frame.serverId !== 'string' ||
        frame.protocolVersion !== GXSERVER_PROTOCOL_VERSION
      )
        return;
      const follower = this.followers.get(JSON.stringify([frame.projectId, frame.sessionId]));
      follower?.receive(frame as unknown as GxserverSessionChatEvent);
    });
    socket.addEventListener('close', () => {
      if (!current()) return;
      this.socket = undefined;
      if (!this.followers.size) return;
      const delay = RECONNECT_DELAYS[Math.min(this.attempts++, RECONNECT_DELAYS.length - 1)]!;
      this.retry = setTimeout(() => {
        this.retry = undefined;
        this.connect();
      }, delay);
    });
    socket.addEventListener('error', () => {
      if (current()) socket.close();
    });
  }
}
