import {
  GXSERVER_PRODUCT,
  GXSERVER_PROTOCOL_VERSION,
  type GxserverEvent,
  type GxserverPresentationDelta,
  type GxserverPresentationSnapshot,
  type GxserverRpcEndpointPath,
  type GxserverServerHealthResponse,
} from "@/packages/shared/gxserver-protocol";
import {
  isSessionChatEventType,
  type GxserverSessionChatEvent,
} from "@/packages/shared/session-chat";
import type { GhostexWebMachine } from "./types";

export type SessionChatEventHandler = (event: GxserverSessionChatEvent) => void;

export type PresentationSubscription = {
  close(): void;
  /**
   * Attach a session-chat subscription to this events socket. Sends
   * subscribeSessionChat once per (projectId, sessionId) key (after
   * subscribePresentation when the socket opens) and unsubscribeSessionChat
   * when the last handler for the key detaches. Events are dispatched only to
   * handlers matching the frame's projectId/sessionId.
   */
  subscribeSessionChat(
    projectId: string,
    sessionId: string,
    onEvent: SessionChatEventHandler,
    /**
     * Read at every (re)subscribe: the follower's tail window must be at
     * least as large as what the client already displays, or a reconnect's
     * snapshot visibly shrinks a long conversation.
     */
    currentLimit?: () => number,
  ): () => void;
};

type PresentationSubscriptionHandlers = {
  onClose(): void;
  onDelta(delta: GxserverPresentationDelta, revision: number): void;
  onError(): void;
  onOpen(): void;
  onSnapshot(snapshot: GxserverPresentationSnapshot): void;
};

export function createGxserverClient(machine: GhostexWebMachine) {
  const createHeaders = (): Record<string, string> => ({
    authorization: `Bearer ${machine.authToken}`,
    "x-gxserver-protocol-version": String(GXSERVER_PROTOCOL_VERSION),
  });

  async function fetchHealth(): Promise<GxserverServerHealthResponse> {
    const response = await fetch(`${machine.baseUrl}/api/health/server`, {
      headers: createHeaders(),
      method: "GET",
    });
    const body = await readJson(response);
    if (!response.ok) {
      throw new Error(readErrorMessage(body, `gxserver health probe failed (${response.status}).`));
    }
    if (!isHealthResponse(body)) {
      throw new Error("The server did not return a compatible gxserver health response.");
    }
    return body;
  }

  async function rpc<TResult>(
    path: GxserverRpcEndpointPath,
    params: Record<string, unknown> = {},
  ): Promise<TResult> {
    const response = await fetch(`${machine.baseUrl}${path}`, {
      body: JSON.stringify({ params, protocolVersion: GXSERVER_PROTOCOL_VERSION }),
      headers: {
        ...createHeaders(),
        "content-type": "application/json",
      },
      method: "POST",
    });
    const body = await readJson(response);
    if (!response.ok || !isRpcResponse(body)) {
      throw new Error(readErrorMessage(body, `gxserver request failed (${response.status}).`));
    }
    if (body.protocolVersion !== GXSERVER_PROTOCOL_VERSION) {
      throw new Error(
        `gxserver protocol mismatch. Expected ${GXSERVER_PROTOCOL_VERSION}, got ${String(body.protocolVersion)}.`,
      );
    }
    return body.result as TResult;
  }

  async function fetchPresentationSnapshot(): Promise<GxserverPresentationSnapshot> {
    const { snapshot } = await rpc<{ snapshot: GxserverPresentationSnapshot }>(
      "/api/readPresentationSnapshot",
    );
    return snapshot;
  }

  function subscribePresentation(
    clientId: string,
    handlers: PresentationSubscriptionHandlers,
    lastRevision: number,
  ): PresentationSubscription {
    const url = new URL(`${machine.baseUrl}/api/events`);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    url.searchParams.set("authToken", machine.authToken);
    url.searchParams.set("protocolVersion", String(GXSERVER_PROTOCOL_VERSION));

    const socket = new WebSocket(url);
    let closedByClient = false;
    const chatHandlers = new Map<
      string,
      {
        projectId: string;
        sessionId: string;
        handlers: Set<SessionChatEventHandler>;
        currentLimit?: () => number;
      }
    >();
    const chatKey = (projectId: string, sessionId: string) =>
      JSON.stringify([projectId, sessionId]);
    const sendChatSubscribe = (entry: {
      projectId: string;
      sessionId: string;
      currentLimit?: () => number;
    }) => {
      if (socket.readyState === WebSocket.OPEN) {
        const limit = entry.currentLimit?.();
        socket.send(
          JSON.stringify({
            projectId: entry.projectId,
            sessionId: entry.sessionId,
            type: "subscribeSessionChat",
            ...(typeof limit === "number" && limit > 0 ? { limit } : {}),
          }),
        );
      }
    };
    socket.addEventListener("open", () => {
      socket.send(JSON.stringify({
        clientId,
        lastRevision,
        type: "subscribePresentation",
      }));
      // Chat subscriptions attached while the socket was still connecting are
      // flushed here, after subscribePresentation.
      for (const entry of chatHandlers.values()) {
        sendChatSubscribe(entry);
      }
      handlers.onOpen();
    });
    socket.addEventListener("message", (event) => {
      const parsed = parseGxserverEvent(event.data);
      if (parsed?.type === "presentationSnapshot") {
        handlers.onSnapshot(parsed.snapshot);
      } else if (parsed?.type === "presentationDelta") {
        handlers.onDelta(parsed.delta, parsed.revision);
      } else if (parsed && isSessionChatEventType(parsed.type)) {
        const chatEvent = parsed as GxserverSessionChatEvent;
        const entry = chatHandlers.get(chatKey(chatEvent.projectId, chatEvent.sessionId));
        if (entry) {
          for (const handler of entry.handlers) {
            handler(chatEvent);
          }
        }
      }
    });
    socket.addEventListener("error", () => handlers.onError());
    socket.addEventListener("close", () => {
      if (!closedByClient) {
        handlers.onClose();
      }
    });

    return {
      close() {
        closedByClient = true;
        socket.close();
      },
      subscribeSessionChat(projectId, sessionId, onEvent, currentLimit) {
        const key = chatKey(projectId, sessionId);
        let entry = chatHandlers.get(key);
        if (!entry) {
          entry = {
            handlers: new Set(),
            projectId,
            sessionId,
            ...(currentLimit ? { currentLimit } : {}),
          };
          chatHandlers.set(key, entry);
          sendChatSubscribe(entry);
        }
        entry.handlers.add(onEvent);
        return () => {
          const current = chatHandlers.get(key);
          if (!current?.handlers.delete(onEvent)) {
            return;
          }
          if (current.handlers.size === 0) {
            chatHandlers.delete(key);
            if (socket.readyState === WebSocket.OPEN) {
              socket.send(JSON.stringify({ projectId, sessionId, type: "unsubscribeSessionChat" }));
            }
          }
        };
      },
    };
  }

  return {
    fetchHealth,
    fetchPresentationSnapshot,
    rpc,
    subscribePresentation,
  };
}

async function readJson(response: Response): Promise<unknown> {
  const text = await response.text();
  return text.trim() ? JSON.parse(text) as unknown : undefined;
}

function isHealthResponse(value: unknown): value is GxserverServerHealthResponse {
  return isObject(value)
    && value.ok === true
    && value.product === GXSERVER_PRODUCT
    && value.protocolVersion === GXSERVER_PROTOCOL_VERSION;
}

function isRpcResponse(
  value: unknown,
): value is { ok: true; product: string; protocolVersion: number; result: unknown } {
  return isObject(value)
    && value.ok === true
    && value.product === GXSERVER_PRODUCT
    && "result" in value;
}

function readErrorMessage(value: unknown, defaultMessage: string): string {
  return isObject(value) && typeof value.message === "string" ? value.message : defaultMessage;
}

function parseGxserverEvent(value: unknown): GxserverEvent | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  try {
    const parsed = JSON.parse(value) as unknown;
    return isObject(parsed) && parsed.protocolVersion === GXSERVER_PROTOCOL_VERSION
      ? parsed as GxserverEvent
      : undefined;
  } catch {
    return undefined;
  }
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

