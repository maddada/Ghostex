import { reduceGxserverPresentationDelta } from '@/packages/shared/gxserver-presentation-cache';
import { createGxserverClient, type PresentationSubscription, type SessionChatEventHandler } from './gxserver-client';
import type { GhostexWebMachine, MachineConnectionState } from './types';

const RECONNECT_DELAYS_MS = [1_000, 2_000, 4_000, 8_000, 16_000] as const;
const DEBUG_CONNECTIONS_STORAGE_KEY = 'ghostexWeb.debugConnections';

interface SessionChatSubscriptionEntry {
  projectId: string;
  sessionId: string;
  onEvent: SessionChatEventHandler;
  /** Follower tail window to request, re-read on every (re)subscribe. */
  currentLimit?: () => number;
  /** Detach from the currently live events socket, if any. */
  detach?: () => void;
}

export class GxserverConnection {
  readonly machine: GhostexWebMachine;

  private readonly client;
  private readonly listeners = new Set<() => void>();
  private reconnectAttempt = 0;
  private reconnectTimer: number | undefined;
  private running = false;
  private subscription: PresentationSubscription | undefined;
  private generation = 0;
  private state: MachineConnectionState;
  /** Active session-chat subscriptions; survives reconnects and re-subscribes. */
  private readonly chatSubscriptions = new Set<SessionChatSubscriptionEntry>();

  constructor(machine: GhostexWebMachine) {
    this.machine = machine;
    this.client = createGxserverClient(machine);
    this.state = { machine, status: 'disconnected' };
  }

  getState = (): MachineConnectionState => this.state;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  start(): void {
    if (this.running) {
      return;
    }
    this.running = true;
    window.addEventListener('online', this.handleOnline);
    window.addEventListener('offline', this.handleOffline);
    if (navigator.onLine) {
      void this.connect();
    } else {
      this.updateState({ error: 'Browser is offline.', status: 'disconnected' });
    }
  }

  stop(): void {
    this.running = false;
    this.generation += 1;
    window.removeEventListener('online', this.handleOnline);
    window.removeEventListener('offline', this.handleOffline);
    this.clearReconnectTimer();
    this.closeSubscription();
  }

  rpc<TResult>(path: Parameters<typeof this.client.rpc<TResult>>[0], params?: Record<string, unknown>) {
    return this.client.rpc<TResult>(path, params);
  }

  /**
   * Subscribe to session-chat frames for one (projectId, sessionId). The
   * subscription rides the presentation events socket and is automatically
   * re-established after every reconnect; the chat hook's epoch logic handles
   * resync from the fresh server snapshot.
   */
  subscribeSessionChat(
    projectId: string,
    sessionId: string,
    onEvent: SessionChatEventHandler,
    currentLimit?: () => number
  ): () => void {
    const entry: SessionChatSubscriptionEntry = {
      onEvent,
      projectId,
      sessionId,
      ...(currentLimit ? { currentLimit } : {}),
    };
    this.chatSubscriptions.add(entry);
    if (this.subscription) {
      entry.detach = this.subscription.subscribeSessionChat(projectId, sessionId, onEvent, currentLimit);
    }
    return () => {
      if (!this.chatSubscriptions.delete(entry)) {
        return;
      }
      entry.detach?.();
      entry.detach = undefined;
    };
  }

  private readonly handleOnline = () => {
    if (!this.running) {
      return;
    }
    this.reconnectAttempt = 0;
    this.clearReconnectTimer();
    void this.connect();
  };

  private readonly handleOffline = () => {
    this.generation += 1;
    this.clearReconnectTimer();
    this.closeSubscription();
    this.updateState({
      error: 'Browser is offline.',
      reconnectAt: undefined,
      status: 'disconnected',
    });
  };

  private async connect(): Promise<void> {
    if (!this.running || !navigator.onLine) {
      return;
    }
    const generation = ++this.generation;
    this.clearReconnectTimer();
    this.closeSubscription();
    this.updateState({ error: undefined, reconnectAt: undefined, status: 'connecting' });

    try {
      const presentation = await this.client.fetchPresentationSnapshot();
      if (!this.running || generation !== this.generation) {
        return;
      }
      this.updateState({ presentation });
      logPresentationSnapshot(this.machine, presentation.revision, presentation.projects.length);

      this.subscription = this.client.subscribePresentation(
        `ghostex-web-${this.machine.machineId}`,
        {
          onClose: () => {
            if (generation === this.generation) {
              this.disconnectAndRetry('Presentation stream closed.');
            }
          },
          onDelta: (delta, revision) => {
            if (generation !== this.generation) {
              return;
            }
            const current = this.state.presentation;
            if (current) {
              this.updateState({
                presentation: reduceGxserverPresentationDelta(current, delta, revision),
              });
            }
          },
          onError: () => {
            if (generation === this.generation) {
              this.disconnectAndRetry('Presentation stream connection failed.');
            }
          },
          onOpen: () => {
            if (generation !== this.generation) {
              return;
            }
            this.reconnectAttempt = 0;
            this.updateState({ error: undefined, reconnectAt: undefined, status: 'connected' });
          },
          onSidebarProjectCollections: (sidebarProjectCollections, revision) => {
            if (generation !== this.generation) {
              return;
            }
            const current = this.state.presentation;
            if (current) {
              this.updateState({
                presentation: { ...current, revision, sidebarProjectCollections },
              });
            }
          },
          onSnapshot: (snapshot) => {
            if (generation !== this.generation) {
              return;
            }
            this.updateState({ presentation: snapshot });
            logPresentationSnapshot(this.machine, snapshot.revision, snapshot.projects.length);
          },
          onWorkspaceGroups: (workspaceGroups, revision) => {
            if (generation !== this.generation) {
              return;
            }
            const current = this.state.presentation;
            if (current) {
              this.updateState({
                presentation: { ...current, revision, workspaceGroups },
              });
            }
          },
        },
        presentation.revision
      );
      for (const entry of this.chatSubscriptions) {
        entry.detach = this.subscription.subscribeSessionChat(
          entry.projectId,
          entry.sessionId,
          entry.onEvent,
          entry.currentLimit
        );
      }
    } catch (error) {
      if (this.running && generation === this.generation) {
        this.disconnectAndRetry(errorMessage(error));
      }
    }
  }

  private disconnectAndRetry(error: string): void {
    if (!this.running) {
      return;
    }
    this.generation += 1;
    this.closeSubscription();
    if (!navigator.onLine) {
      this.updateState({ error: 'Browser is offline.', status: 'disconnected' });
      return;
    }
    if (this.reconnectTimer !== undefined) {
      return;
    }
    const delay = RECONNECT_DELAYS_MS[Math.min(this.reconnectAttempt, RECONNECT_DELAYS_MS.length - 1)];
    this.reconnectAttempt += 1;
    const reconnectAt = Date.now() + delay;
    this.updateState({ error, reconnectAt, status: 'disconnected' });
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = undefined;
      void this.connect();
    }, delay);
  }

  private closeSubscription(): void {
    const subscription = this.subscription;
    this.subscription = undefined;
    // The socket is going away with the subscription; per-socket detach
    // functions are moot, but the registry entries stay for the reconnect.
    for (const entry of this.chatSubscriptions) {
      entry.detach = undefined;
    }
    subscription?.close();
  }

  private clearReconnectTimer(): void {
    if (this.reconnectTimer !== undefined) {
      window.clearTimeout(this.reconnectTimer);
      this.reconnectTimer = undefined;
    }
  }

  private updateState(update: Partial<MachineConnectionState>): void {
    this.state = { ...this.state, ...update };
    for (const listener of this.listeners) {
      listener();
    }
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function logPresentationSnapshot(machine: GhostexWebMachine, revision: number, projectCount: number): void {
  if (window.localStorage.getItem(DEBUG_CONNECTIONS_STORAGE_KEY) === '1') {
    console.info(
      `[ghostex-web] presentation snapshot for ${machine.machineId}: revision=${revision}, projects=${projectCount}`
    );
  }
}
