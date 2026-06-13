import { randomUUID } from "node:crypto";
import { WebSocket, WebSocketServer } from "ws";
import {
  GXSERVER_PROTOCOL_VERSION,
  type GxserverEvent,
  type GxserverPresentationRevision,
  type GxserverPresentationSnapshot,
  type GxserverRendererCommand,
  type GxserverRendererCommandAction,
  type GxserverServerId,
} from "../protocol/index.js";

export type GxserverPresentationSnapshotProvider = (input: {
  clientId?: string;
  lastRevision?: GxserverPresentationRevision;
}) => Promise<GxserverPresentationSnapshot> | GxserverPresentationSnapshot;

export class GxserverRendererCommandUnavailableError extends Error {
  constructor() {
    super("No macOS renderer is connected to gxserver for renderer-only commands.");
    this.name = "GxserverRendererCommandUnavailableError";
  }
}

export class GxserverRendererCommandTimeoutError extends Error {
  constructor(action: GxserverRendererCommandAction, timeoutMs: number) {
    super(`Timed out waiting ${timeoutMs}ms for macOS renderer command ${action}.`);
    this.name = "GxserverRendererCommandTimeoutError";
  }
}

type PendingRendererCommand = {
  resolve: (result: Record<string, unknown>) => void;
  timer: ReturnType<typeof setTimeout>;
};

export class GxserverEventHub {
  readonly server: WebSocketServer;
  readonly serverId: GxserverServerId;
  #presentationSnapshotProvider?: GxserverPresentationSnapshotProvider;
  #pendingRendererCommands = new Map<string, PendingRendererCommand>();
  #rendererCommandSockets = new Set<WebSocket>();

  constructor(serverId: GxserverServerId) {
    this.serverId = serverId;
    this.server = new WebSocketServer({ noServer: true });
    this.server.on("connection", (socket) => {
      this.send(socket, {
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
        serverId,
        type: "eventStreamReady",
      });
      socket.on("message", (message) => {
        void this.#handleMessage(socket, message);
      });
      socket.on("close", () => {
        this.#rendererCommandSockets.delete(socket);
      });
    });
  }

  setPresentationSnapshotProvider(provider: GxserverPresentationSnapshotProvider): void {
    this.#presentationSnapshotProvider = provider;
  }

  broadcast(event: GxserverEvent): void {
    const payload = `${JSON.stringify(event)}\n`;
    for (const client of this.server.clients) {
      if (client.readyState === WebSocket.OPEN) {
        client.send(payload);
      }
    }
  }

  close(): Promise<void> {
    for (const pending of this.#pendingRendererCommands.values()) {
      clearTimeout(pending.timer);
      pending.resolve({ error: "gxserver event stream closed before renderer command completed.", ok: false });
    }
    this.#pendingRendererCommands.clear();
    for (const client of this.server.clients) {
      client.terminate();
    }
    return new Promise((resolve, reject) => {
      this.server.close((error) => {
        if (error) {
          reject(error);
          return;
        }
        resolve();
      });
    });
  }

  dispatchRendererCommand(input: {
    action: GxserverRendererCommandAction;
    payload?: Record<string, unknown>;
    timeoutMs?: number;
  }): Promise<Record<string, unknown>> {
    const socket = this.#openRendererCommandSocket();
    if (!socket) {
      throw new GxserverRendererCommandUnavailableError();
    }
    const timeoutMs = normalizeRendererCommandTimeoutMs(input.timeoutMs);
    const command: GxserverRendererCommand = {
      action: input.action,
      commandId: `renderer-${randomUUID()}`,
      createdAt: new Date().toISOString(),
      payload: input.payload ?? {},
      timeoutMs,
    };
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.#pendingRendererCommands.delete(command.commandId);
        reject(new GxserverRendererCommandTimeoutError(command.action, timeoutMs));
      }, timeoutMs);
      this.#pendingRendererCommands.set(command.commandId, { resolve, timer });
      try {
        this.send(socket, {
          command,
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
          serverId: this.serverId,
          type: "rendererCommand",
        });
      } catch (error) {
        clearTimeout(timer);
        this.#pendingRendererCommands.delete(command.commandId);
        reject(error);
      }
    });
  }

  private send(socket: WebSocket, event: GxserverEvent): void {
    if (socket.readyState === WebSocket.OPEN) {
      socket.send(`${JSON.stringify(event)}\n`);
    }
  }

  async #handleMessage(socket: WebSocket, message: WebSocket.RawData): Promise<void> {
    const parsed = parseEventStreamMessage(message);
    if (!parsed) {
      return;
    }
    if (parsed.type === "rendererCommandResult") {
      this.#handleRendererCommandResult(parsed);
      return;
    }
    if (parsed.type !== "subscribePresentation") {
      return;
    }
    if (parsed.rendererCommands === true) {
      this.#rendererCommandSockets.add(socket);
    }
    if (!this.#presentationSnapshotProvider) {
      return;
    }
    /*
    CDXC:GxserverPresentationEvents 2026-06-01-15:08:
    Presentation WebSocket reconnects use fresh snapshots in this pass because gxserver does not persist a replay log yet. The event stream still carries revisions so clients can apply ordered deltas and replace local state whenever a gap cannot be proven safe.
    */
    const snapshot = await this.#presentationSnapshotProvider({
      clientId: typeof parsed.clientId === "string" ? parsed.clientId : undefined,
      lastRevision: typeof parsed.lastRevision === "number" ? parsed.lastRevision as GxserverPresentationRevision : undefined,
    });
    this.send(socket, {
      clientId: typeof parsed.clientId === "string" ? parsed.clientId : undefined,
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      revision: snapshot.revision,
      serverId: this.serverId,
      snapshot,
      type: "presentationSnapshot",
    });
  }

  #handleRendererCommandResult(parsed: Record<string, unknown>): void {
    const commandId = typeof parsed.commandId === "string" ? parsed.commandId : "";
    const pending = this.#pendingRendererCommands.get(commandId);
    if (!pending) {
      return;
    }
    clearTimeout(pending.timer);
    this.#pendingRendererCommands.delete(commandId);
    if (parsed.ok === false) {
      pending.resolve({
        error: typeof parsed.error === "string" ? parsed.error : "macOS renderer command failed.",
        ok: false,
      });
      return;
    }
    pending.resolve(isObjectRecord(parsed.result) ? parsed.result : { ok: true });
  }

  #openRendererCommandSocket(): WebSocket | undefined {
    for (const socket of this.#rendererCommandSockets) {
      if (socket.readyState === WebSocket.OPEN) {
        return socket;
      }
      this.#rendererCommandSockets.delete(socket);
    }
    return undefined;
  }
}

function parseEventStreamMessage(message: WebSocket.RawData): Record<string, unknown> | undefined {
  try {
    const text = Array.isArray(message) ? Buffer.concat(message).toString("utf8") : Buffer.from(message as Buffer).toString("utf8");
    const parsed = JSON.parse(text) as unknown;
    return typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)
      ? parsed as Record<string, unknown>
      : undefined;
  } catch {
    return undefined;
  }
}

function normalizeRendererCommandTimeoutMs(value: unknown): number {
  const timeoutMs = Number(value ?? 15_000);
  if (!Number.isFinite(timeoutMs)) {
    return 15_000;
  }
  return Math.min(60_000, Math.max(1_000, Math.round(timeoutMs)));
}

function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
