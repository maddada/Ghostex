import { createRoot } from "react-dom/client";
import "@/packages/core-ui/styles.css";
import {
  isSessionChatEventType,
  normalizeSessionChatTheme,
  resolveSessionChatTranscriptAgent,
  type GxserverQueueSessionChatPromptResult,
  type GxserverReadSessionChatFilesResult,
  type GxserverReadSessionChatImageResult,
  type GxserverReadSessionChatResult,
  type GxserverReadSessionChatSkillsResult,
  type GxserverSaveSessionChatAttachmentResult,
  type GxserverSaveSessionChatImageResult,
  type GxserverSendSessionChatQueuedPromptResult,
  type GxserverSessionChatEvent,
  type GxserverSessionChatQueueResult,
  type GxserverSessionChatRemoveQueuedPromptResult,
  type SessionChatTheme,
} from "@/packages/shared/session-chat";
import { GXSERVER_PROTOCOL_VERSION } from "@/packages/shared/gxserver-protocol";
import {
  clampSessionChatTranscriptWidthPercent,
  DEFAULT_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
} from "@/packages/shared/ghostex-settings";
import { normalizeghostexHotkeySettings } from "@/packages/shared/ghostex-hotkeys";
import { formatSidebarHotkeyLabel } from "@/packages/core-ui/hotkey-label";
import {
  SessionChatView,
  type SessionChatHostActions,
  type SessionChatHostComposerBridge,
  type SessionChatHostLinks,
} from "@/packages/core-ui/chat/session-chat-view";
import type { SessionChatTransport } from "@/packages/core-ui/chat/session-chat-transport";

/*
CDXC:GPUISessionChatSurface 2026-07-31:
chat.html is the per-session Session Chat CEF surface that swaps with the
terminal pane body in the gpui Agents workspace. It follows the
kanban-main/manage-main minimalism: session identity arrives as URL query
params (projectId/sessionId/agentId), and the gxserver bootstrap
(baseUrl/token/protocolVersion) is installed by Rust on
window.ghostexGpui.gxserverBootstrap through the chat bootstrap process
message. The page owns its own /api/events websocket with
subscribeSessionChat and filters frames client-side, so the sidebar runtime
never proxies chat data. Remote sessions use the same transport through the
localhost port already owned by that machine's SSH tunnel.
*/

interface ChatGxserverBootstrap {
  authToken?: string;
  baseUrl?: string;
  clientId?: string;
  protocolVersion?: number;
}

declare global {
  interface Window {
    ghostexSetSessionChatFontFamily?: (fontFamily: unknown) => void;
    ghostexSetSessionChatTheme?: (theme: unknown) => void;
    ghostexSetSessionChatTranscriptWidthPercent?: (widthPercent: unknown) => void;
    ghostexSetSessionChatVerboseMode?: (verboseMode: unknown) => void;
  }
}

interface ChatBridgeNamespace {
  gxserverBootstrap?: ChatGxserverBootstrap;
  onGxserverBootstrapChanged?: (bootstrap: ChatGxserverBootstrap) => void;
  onSessionChatFocusComposerRequested?: () => void;
  onSessionChatHandoffToTerminalRequested?: () => void;
  onSessionChatInsertPromptRequested?: (payload: { content?: unknown }) => void;
  onSessionChatStashPromptRequested?: () => void;
}

const BOOTSTRAP_RETRY_DELAY_MS = 120;
const BOOTSTRAP_MAX_ATTEMPTS = 250;
const RECONNECT_DELAYS_MS = [1_000, 2_000, 4_000, 8_000, 16_000];

function chatBridgeNamespace(): ChatBridgeNamespace {
  const target = window as unknown as { ghostexGpui?: ChatBridgeNamespace };
  target.ghostexGpui = target.ghostexGpui ?? {};
  return target.ghostexGpui;
}

function validatedBootstrap(
  candidate: ChatGxserverBootstrap | undefined,
): { authToken: string; baseUrl: string } | undefined {
  if (!candidate) {
    return undefined;
  }
  if (
    candidate.protocolVersion !== undefined &&
    candidate.protocolVersion !== GXSERVER_PROTOCOL_VERSION
  ) {
    return undefined;
  }
  const baseUrl = typeof candidate.baseUrl === "string" ? candidate.baseUrl.trim() : "";
  const authToken = typeof candidate.authToken === "string" ? candidate.authToken : "";
  if (!baseUrl || !authToken) {
    return undefined;
  }
  return { authToken, baseUrl };
}

function waitForBootstrap(): Promise<{ authToken: string; baseUrl: string }> {
  return new Promise((resolve, reject) => {
    let settled = false;
    const namespace = chatBridgeNamespace();
    const settle = (bootstrap: { authToken: string; baseUrl: string }) => {
      if (!settled) {
        settled = true;
        resolve(bootstrap);
      }
    };
    namespace.onGxserverBootstrapChanged = (candidate) => {
      const validated = validatedBootstrap(candidate);
      if (validated) {
        settle(validated);
      }
    };
    const poll = (attempt: number): void => {
      if (settled) {
        return;
      }
      const validated = validatedBootstrap(chatBridgeNamespace().gxserverBootstrap);
      if (validated) {
        settle(validated);
        return;
      }
      if (attempt >= BOOTSTRAP_MAX_ATTEMPTS) {
        reject(new Error("The Ghostex server bootstrap did not arrive."));
        return;
      }
      window.setTimeout(() => poll(attempt + 1), BOOTSTRAP_RETRY_DELAY_MS);
    };
    poll(0);
  });
}

async function rpc<TResult>(
  bootstrap: { authToken: string; baseUrl: string },
  path: string,
  params: Record<string, unknown>,
): Promise<TResult> {
  const response = await fetch(`${bootstrap.baseUrl}${path}`, {
    body: JSON.stringify({
      params,
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
    }),
    headers: {
      authorization: `Bearer ${bootstrap.authToken}`,
      "content-type": "application/json",
      "x-gxserver-protocol-version": String(GXSERVER_PROTOCOL_VERSION),
    },
    method: "POST",
  });
  let body: unknown;
  try {
    body = await response.json();
  } catch {
    body = undefined;
  }
  const envelope = body as
    { error?: { message?: string }; ok?: boolean; result?: TResult } | undefined;
  if (!response.ok || !envelope || envelope.ok !== true) {
    const message =
      envelope && typeof envelope.error?.message === "string"
        ? envelope.error.message
        : `gxserver rejected ${path} (${response.status > 0 ? response.status : "no response"}).`;
    throw new Error(message);
  }
  return envelope.result as TResult;
}

function createGpuiSessionChatTransport(
  bootstrap: { authToken: string; baseUrl: string },
  projectId: string,
  sessionId: string,
  remote: boolean,
): SessionChatTransport {
  return {
    async answerPrompt(params) {
      await rpc(bootstrap, "/api/answerSessionChatPrompt", {
        ...params,
        projectId,
        sessionId,
      });
    },
    async interrupt() {
      await rpc(bootstrap, "/api/interruptSessionChat", {
        projectId,
        sessionId,
      });
    },
    read(params) {
      return rpc<GxserverReadSessionChatResult>(bootstrap, "/api/readSessionChat", {
        projectId,
        sessionId,
        ...(params.limit !== undefined ? { limit: params.limit } : {}),
        ...(params.beforeOffset !== undefined ? { beforeOffset: params.beforeOffset } : {}),
      });
    },
    readSkills() {
      return rpc<GxserverReadSessionChatSkillsResult>(bootstrap, "/api/readSessionChatSkills", {
        projectId,
        sessionId,
      });
    },
    readFiles() {
      return rpc<GxserverReadSessionChatFilesResult>(bootstrap, "/api/readSessionChatFiles", {
        projectId,
        sessionId,
      });
    },
    async send(text, imagePaths) {
      await rpc(bootstrap, "/api/sendSessionChatMessage", {
        projectId,
        sessionId,
        text,
        ...(imagePaths && imagePaths.length > 0 ? { imagePaths } : {}),
      });
    },
    // Raw keystroke (Claude's Shift+Tab mode cycle): same endpoint, `key`
    // instead of a body, so the server writes the bytes verbatim.
    async sendKey(key) {
      await rpc(bootstrap, "/api/sendSessionChatMessage", {
        key,
        projectId,
        sessionId,
      });
    },
    saveImage(params) {
      return rpc<GxserverSaveSessionChatImageResult>(bootstrap, "/api/saveSessionChatImage", {
        projectId,
        sessionId,
        base64Data: params.base64Data,
        ...(params.suggestedName ? { suggestedName: params.suggestedName } : {}),
      });
    },
    saveAttachment(params) {
      return rpc<GxserverSaveSessionChatAttachmentResult>(
        bootstrap,
        "/api/saveSessionChatAttachment",
        {
          projectId,
          sessionId,
          base64Data: params.base64Data,
          ...(params.suggestedName ? { suggestedName: params.suggestedName } : {}),
        },
      );
    },
    loadImage(params) {
      return rpc<GxserverReadSessionChatImageResult>(bootstrap, "/api/readSessionChatImage", {
        path: params.path,
      });
    },
    // Native picker paths are valid only for sessions on this Mac. Remote
    // chats omit this hook so the composer uses byte upload through the
    // remote gxserver tunnel and receives a path on the session's machine.
    ...(remote
      ? {}
      : {
          pickAttachmentPaths() {
            return requestNativeAttachmentPaths();
          },
        }),
    // The save panel writes to this Mac, so it is offered for every session:
    // the bytes travel with the request and never touch the session's machine.
    saveImageAs(params) {
      return requestNativeImageSave(params.base64Data, params.suggestedName);
    },
    /*
    CDXC:GPUISessionChatPromptQueue 2026-08-21:
    Ghostex's prompt queue and the synced composer draft are plain gxserver
    round trips on the same bootstrap as every other chat call, so a remote
    session's queue rides the machine's own SSH tunnel exactly like its
    transcript does — no bridge hop through Rust, which would only add a
    second identity vocabulary for data the page can already reach.

    These six are unconditional here: the daemon's `queue` field is the
    capability probe the shared UI gates on, so a daemon that predates the
    queue hides the controls without this host guessing at versions.
    */
    queuePrompt(params) {
      return rpc<GxserverQueueSessionChatPromptResult>(
        bootstrap,
        "/api/queueSessionChatPrompt",
        { projectId, sessionId, text: params.text },
      );
    },
    updateQueuedPrompt(params) {
      return rpc<GxserverSessionChatQueueResult>(
        bootstrap,
        "/api/updateSessionChatQueuedPrompt",
        {
          projectId,
          promptId: params.promptId,
          sessionId,
          ...(params.text !== undefined ? { text: params.text } : {}),
          ...(params.retry !== undefined ? { retry: params.retry } : {}),
        },
      );
    },
    removeQueuedPrompt(params) {
      return rpc<GxserverSessionChatRemoveQueuedPromptResult>(
        bootstrap,
        "/api/removeSessionChatQueuedPrompt",
        { projectId, promptId: params.promptId, sessionId },
      );
    },
    reorderQueue(params) {
      return rpc<GxserverSessionChatQueueResult>(bootstrap, "/api/reorderSessionChatQueue", {
        projectId,
        promptIds: params.promptIds,
        sessionId,
      });
    },
    sendQueuedPrompt(params) {
      return rpc<GxserverSendSessionChatQueuedPromptResult>(
        bootstrap,
        "/api/sendSessionChatQueuedPrompt",
        { projectId, promptId: params.promptId, sessionId },
      );
    },
    // `clientId` is minted and persisted by the shared chat hook. Forward it
    // verbatim: a per-call or per-mount id would make this client's own draft
    // echo look like another device and pop the conflict bar for nothing.
    async setDraft(params) {
      await rpc(bootstrap, "/api/setSessionChatDraft", {
        clientId: params.clientId,
        content: params.content,
        projectId,
        sessionId,
      });
    },
    subscribe({ currentLimit, onEvent }) {
      /*
      Own /api/events socket per subscription: send subscribeSessionChat on
      every open (the server replies with an authoritative snapshot frame
      first), filter broadcast frames client-side by session identity, and
      resubscribe after reconnects with the same snapshot-first contract the
      web connection uses. The requested window is re-read at every open, so a
      reconnect after a long live session cannot answer with fewer rows than
      the page already shows.
      */
      let closed = false;
      let socket: WebSocket | undefined;
      let reconnectAttempt = 0;
      let reconnectTimeoutId: number | undefined;

      const connect = (): void => {
        if (closed) {
          return;
        }
        const url = new URL(`${bootstrap.baseUrl}/api/events`);
        url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
        url.searchParams.set("protocolVersion", String(GXSERVER_PROTOCOL_VERSION));
        url.searchParams.set("authToken", bootstrap.authToken);
        const nextSocket = new WebSocket(url.toString());
        socket = nextSocket;
        nextSocket.addEventListener("open", () => {
          reconnectAttempt = 0;
          const limit = currentLimit?.();
          nextSocket.send(
            JSON.stringify({
              projectId,
              sessionId,
              type: "subscribeSessionChat",
              ...(typeof limit === "number" && limit > 0 ? { limit } : {}),
            }),
          );
        });
        nextSocket.addEventListener("message", (event) => {
          let parsed: unknown;
          try {
            parsed = JSON.parse(String(event.data));
          } catch {
            return;
          }
          if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
            return;
          }
          const frame = parsed as Record<string, unknown>;
          if (
            typeof frame.type !== "string" ||
            !isSessionChatEventType(frame.type) ||
            frame.projectId !== projectId ||
            frame.sessionId !== sessionId ||
            typeof frame.epoch !== "number" ||
            typeof frame.seq !== "number" ||
            frame.protocolVersion !== GXSERVER_PROTOCOL_VERSION
          ) {
            return;
          }
          onEvent(frame as unknown as GxserverSessionChatEvent);
        });
        nextSocket.addEventListener("close", () => {
          if (closed || socket !== nextSocket) {
            return;
          }
          const delay =
            RECONNECT_DELAYS_MS[Math.min(reconnectAttempt, RECONNECT_DELAYS_MS.length - 1)];
          reconnectAttempt += 1;
          reconnectTimeoutId = window.setTimeout(connect, delay);
        });
        nextSocket.addEventListener("error", () => {
          if (socket === nextSocket) {
            nextSocket.close();
          }
        });
      };
      connect();

      return () => {
        closed = true;
        if (reconnectTimeoutId !== undefined) {
          window.clearTimeout(reconnectTimeoutId);
          reconnectTimeoutId = undefined;
        }
        const activeSocket = socket;
        socket = undefined;
        if (activeSocket && activeSocket.readyState === WebSocket.OPEN) {
          try {
            activeSocket.send(
              JSON.stringify({
                projectId,
                sessionId,
                type: "unsubscribeSessionChat",
              }),
            );
          } catch {
            // Socket teardown races are fine; the server refcounts followers.
          }
        }
        activeSocket?.close();
      };
    },
  };
}

/*
CDXC:GPUISessionChatHostActions 2026-07-31:
gpui cannot paint above this native CEF view, so the chat page renders the
top-right [Terminal View][Agent Actions] cluster itself and posts clicks to
Rust over the app-modal-host bridge shim installed for chat.html. The
action ids and labels mirror the terminal overlay's expanded row; the
terminal-owned actions still reach Rust, while composer-owned actions return
through the bounded chat bridge below.
*/
interface AppModalHostMessageHandler {
  postMessage: (payload: string) => unknown;
}

function postSessionChatHostAction(action: string, fields?: Record<string, unknown>): void {
  const target = window as unknown as {
    webkit?: {
      messageHandlers?: { ghostexAppModalHost?: AppModalHostMessageHandler };
    };
  };
  target.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage(
    JSON.stringify({
      action,
      type: "sessionChatHostAction",
      ...fields,
    }),
  );
}

function createGpuiSessionChatComposerBridge(
  bootstrap: { authToken: string; baseUrl: string },
  projectId: string,
  sessionId: string,
): SessionChatHostComposerBridge {
  return {
    register(actions) {
      const namespace = chatBridgeNamespace();
      const insertPrompt = (payload: { content?: unknown }): void => {
        if (typeof payload?.content === "string" && payload.content.length > 0) {
          actions.insertPrompt(payload.content);
        }
      };
      const requestFocus = (): void => actions.focus();
      const requestHandoffToTerminal = (): void => {
        void actions
          .handoffToTerminal()
          .then((handoff) => {
            postSessionChatHostAction("draftHandoffToTerminalComplete", {
              content: handoff.content,
              stashedPromptId: handoff.stashedPromptId ?? "",
            });
          })
          .catch(() => {
            postSessionChatHostAction("draftHandoffToTerminalFailed");
          });
      };
      const requestStash = (): void => actions.requestStash();
      namespace.onSessionChatFocusComposerRequested = requestFocus;
      namespace.onSessionChatHandoffToTerminalRequested = requestHandoffToTerminal;
      namespace.onSessionChatInsertPromptRequested = insertPrompt;
      namespace.onSessionChatStashPromptRequested = requestStash;
      postSessionChatHostAction("composerReady");
      return () => {
        if (namespace.onSessionChatFocusComposerRequested === requestFocus) {
          delete namespace.onSessionChatFocusComposerRequested;
        }
        if (namespace.onSessionChatHandoffToTerminalRequested === requestHandoffToTerminal) {
          delete namespace.onSessionChatHandoffToTerminalRequested;
        }
        if (namespace.onSessionChatInsertPromptRequested === insertPrompt) {
          delete namespace.onSessionChatInsertPromptRequested;
        }
        if (namespace.onSessionChatStashPromptRequested === requestStash) {
          delete namespace.onSessionChatStashPromptRequested;
        }
      };
    },
    /*
    CDXC:SessionChatDraftHandoff 2026-08-24:
    A transient stash is the durable copy of a draft that is about to leave the
    composer for the terminal, so it must OUTLIVE the move. Deleting it here
    (which this used to do, immediately) left the text owned by nothing but a
    Rust HashMap, and every failure after that point — a torn-down chat
    surface, a paste the terminal refused, a session that never remounted —
    destroyed it. The row id rides back to Rust instead, and Rust deletes the
    row only once a terminal confirms it took the text.
    */
    async stashPrompt(content, options) {
      const result = await rpc<{
        created?: boolean;
        prompt?: { promptId?: string };
      }>(bootstrap, "/api/saveStashedPrompt", {
        content,
        projectId,
        sessionId,
      });
      const promptId = result.prompt?.promptId;
      // Only a row this save created may ever be deleted again: `created:
      // false` means the text matched a prompt the user saved by hand.
      return options?.transient && result.created === true && promptId
        ? { promptId }
        : {};
    },
  };
}

/*
CDXC:GPUISessionChatAttachPicker 2026-08-02:
The composer's attach button opens the same native macOS open panel the
terminal's "Attach File or Folder" action uses (files AND folders — a browser
file input cannot offer folders or absolute paths). The round trip rides the
existing bridge: the page posts a pickAttachments host action with a request
id, Rust runs the panel, then answers by executing the fixed
window.ghostexGpui.onSessionChatAttachmentsPicked callback in this page with
{requestId, paths} (empty paths on cancel, so the promise always settles).
*/
const ATTACHMENT_PICK_TIMEOUT_MS = 180_000;

interface ChatAttachmentPickNamespace {
  onSessionChatAttachmentsPicked?: (payload: { requestId?: string; paths?: unknown }) => void;
}

let attachmentPickSequence = 0;
const pendingAttachmentPicks = new Map<string, (paths: string[]) => void>();

function installAttachmentPickCallback(): void {
  const namespace = chatBridgeNamespace() as ChatBridgeNamespace & ChatAttachmentPickNamespace;
  namespace.onSessionChatAttachmentsPicked = (payload) => {
    const requestId = typeof payload?.requestId === "string" ? payload.requestId : "";
    const resolve = pendingAttachmentPicks.get(requestId);
    if (!resolve) {
      return;
    }
    pendingAttachmentPicks.delete(requestId);
    const paths = Array.isArray(payload.paths)
      ? payload.paths.filter((path): path is string => typeof path === "string")
      : [];
    resolve(paths);
  };
}

/*
CDXC:GPUISessionChatImageSave 2026-08-19:
"Save image" in the chat image overlay cannot be a browser download: gpui
installs no CEF download handler, so a <a download> click is cancelled without
a trace. The page posts a saveImage host action carrying the bytes and a
suggested name, Rust runs the native save panel and writes the file, then
answers through the fixed window.ghostexGpui.onSessionChatImageSaved callback
with {requestId, error} — no error means saved or cancelled, both of which the
panel already told the user about.
*/
const IMAGE_SAVE_TIMEOUT_MS = 180_000;

interface ChatImageSaveNamespace {
  onSessionChatImageSaved?: (payload: { requestId?: string; error?: unknown }) => void;
}

let imageSaveSequence = 0;
const pendingImageSaves = new Map<string, (error: string | null) => void>();

function installImageSaveCallback(): void {
  const namespace = chatBridgeNamespace() as ChatBridgeNamespace & ChatImageSaveNamespace;
  namespace.onSessionChatImageSaved = (payload) => {
    const requestId = typeof payload?.requestId === "string" ? payload.requestId : "";
    const settle = pendingImageSaves.get(requestId);
    if (!settle) {
      return;
    }
    pendingImageSaves.delete(requestId);
    settle(typeof payload.error === "string" && payload.error !== "" ? payload.error : null);
  };
}

function requestNativeImageSave(base64Data: string, suggestedName: string): Promise<void> {
  installImageSaveCallback();
  imageSaveSequence += 1;
  const requestId = `image-save-${imageSaveSequence}`;
  return new Promise<void>((resolve, reject) => {
    pendingImageSaves.set(requestId, (error) => {
      if (error === null) {
        resolve();
      } else {
        reject(new Error(error));
      }
    });
    // The panel can sit open indefinitely; the timeout only reclaims the entry
    // if the host never answers at all (e.g. the pane was torn down).
    window.setTimeout(() => {
      if (pendingImageSaves.delete(requestId)) {
        resolve();
      }
    }, IMAGE_SAVE_TIMEOUT_MS);
    postSessionChatHostAction("saveImage", { base64Data, requestId, suggestedName });
  });
}

function requestNativeAttachmentPaths(): Promise<string[]> {
  installAttachmentPickCallback();
  attachmentPickSequence += 1;
  const requestId = `attach-${attachmentPickSequence}`;
  return new Promise<string[]>((resolve) => {
    pendingAttachmentPicks.set(requestId, resolve);
    // The panel can sit open indefinitely; the timeout only reclaims the
    // entry if the host never answers at all (e.g. the pane was torn down).
    window.setTimeout(() => {
      if (pendingAttachmentPicks.delete(requestId)) {
        resolve([]);
      }
    }, ATTACHMENT_PICK_TIMEOUT_MS);
    postSessionChatHostAction("pickAttachments", { requestId });
  });
}

/*
CDXC:GPUISessionChatLinks 2026-08-03:
Links in the conversation belong to the app, not to this page: a web URL opens
in Ghostex's own Browser view (Shift+click asks for the OS browser instead),
and a file path opens in the project's Docs view when Docs can show it, else in
the Code view. Both ride the same host-action bridge as the button cluster; the
page never navigates itself, since chat.html has nowhere to navigate to.

CDXC:GPUISessionChatLinks 2026-08-18:
Where a web URL actually lands is the host's call, not this page's: it reads the
"Open links in embedded browser" Browser setting, the same one Command-clicked
terminal links use, and hands the URL to the system default browser when that
setting is off.
*/
const GPUI_SESSION_CHAT_HOST_LINKS: SessionChatHostLinks = {
  openUrl: (url, { external }) => postSessionChatHostAction("openLink", { external, url }),
  openFile: (path) => postSessionChatHostAction("openFile", { path }),
};

function createGpuiSessionChatHostActions(hotkeysValue: unknown): SessionChatHostActions {
  const hotkeys = normalizeghostexHotkeySettings(hotkeysValue);
  const shortcut = (id: keyof typeof hotkeys): string | undefined => {
    const value = hotkeys[id];
    return value ? formatSidebarHotkeyLabel(value) : undefined;
  };
  return {
    onSwitchToTerminal: () => postSessionChatHostAction("terminalView"),
    onSwitchToTerminalForAgentPicker: () => postSessionChatHostAction("agentPickerTerminalView"),
    switchViewShortcut: shortcut("toggleChatView"),
    actions: [
      {
        id: "rename",
        label: "Rename",
        shortcut: shortcut("renameActiveSession"),
      },
      {
        id: "sleep",
        label: "Sleep",
        shortcut: shortcut("sleepFocusedSession"),
      },
      {
        id: "delayedActions",
        label: "Delayed Actions",
        shortcut: shortcut("delayedSend"),
      },
      { id: "fork", label: "Fork", shortcut: shortcut("forkSession") },
      {
        id: "fullReload",
        label: "Full Reload",
        shortcut: shortcut("reloadSession"),
      },
      {
        id: "promptEditor",
        label: "Prompt Editor",
        shortcut: shortcut("promptEditor"),
      },
      {
        id: "stashPrompt",
        label: "Stash Prompt",
        shortcut: shortcut("stashPrompt"),
      },
      {
        id: "stashedPrompts",
        label: "Prompts",
        shortcut: shortcut("stashedPrompts"),
      },
      {
        id: "attachPath",
        label: "Attach File or Folder",
        shortcut: shortcut("attachFileOrFolder"),
      },
      {
        id: "exportTranscript",
        label: "Export Transcript",
        shortcut: shortcut("exportTranscript"),
      },
    ],
    onAction: (id) => postSessionChatHostAction(id),
  };
}

function renderFailure(
  root: ReturnType<typeof createRoot>,
  message: string,
  theme: SessionChatTheme,
): void {
  root.render(
    <div className="native-sidebar-shell gpui-session-chat">
      <div className="ghostex-session-chat-scope ghostex-chat-empty-state" data-chat-theme={theme}>
        <div className="ghostex-chat-empty-title">Chat unavailable</div>
        <div className="ghostex-chat-empty-detail">{message}</div>
      </div>
    </div>,
  );
}

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Ghostex session chat root element was not found.");
}
const root = createRoot(rootElement);
const searchParams = new URLSearchParams(window.location.search);
const projectId = searchParams.get("projectId")?.trim() ?? "";
const sessionId = searchParams.get("sessionId")?.trim() ?? "";
const agentId = searchParams.get("agentId")?.trim() ?? "";
const remote = searchParams.get("remote") === "true";
let hotkeysValue: unknown;
try {
  hotkeysValue = JSON.parse(searchParams.get("hotkeys") ?? "{}");
} catch {
  hotkeysValue = {};
}
const GPUI_SESSION_CHAT_HOST_ACTIONS = createGpuiSessionChatHostActions(hotkeysValue);
let chatTheme = normalizeSessionChatTheme(searchParams.get("theme"));
let chatFontFamily = searchParams.get("fontFamily")?.trim() ?? "";
let chatTranscriptWidthPercent = clampSessionChatTranscriptWidthPercent(
  Number(searchParams.get("transcriptWidthPercent")) ||
    DEFAULT_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
);
let chatVerboseMode = searchParams.get("verboseMode") === "true";
let renderReadyChat: ((theme: SessionChatTheme) => void) | null = null;

function applyDocumentChatTheme(theme: SessionChatTheme): void {
  document.documentElement.style.colorScheme = theme;
  document.documentElement.style.backgroundColor = theme === "light" ? "#fdfdfd" : "#0a0a0a";
  document.body.style.backgroundColor = theme === "light" ? "#fdfdfd" : "#0a0a0a";
}

/*
CDXC:SessionChatTypeScale 2026-08-22:
An empty setting REMOVES the property rather than writing a fallback chain into
it. The stylesheet already declares what the transcript falls back to, and the
custom property's own fallback only applies while the property is unset — so
writing "no choice" as a value here silently overrode the sheet's default and
made the chat's typeface impossible to change from CSS.
*/
function applyDocumentChatFontFamily(fontFamily: string): void {
  const normalized = fontFamily.trim();
  if (normalized) {
    document.documentElement.style.setProperty(
      "--ghostex-session-chat-font-family",
      normalized,
    );
  } else {
    document.documentElement.style.removeProperty("--ghostex-session-chat-font-family");
  }
  window.dispatchEvent(new Event("ghostex-session-chat-font-family-changed"));
}

function applyDocumentChatTranscriptWidthPercent(widthPercent: number): void {
  document.documentElement.style.setProperty(
    "--ghostex-session-chat-transcript-width-percent",
    String(clampSessionChatTranscriptWidthPercent(widthPercent)),
  );
}

document.body.dataset.sidebarTheme = "plain-dark";
document.body.classList.add("vscode-dark", "native-sidebar-body");
applyDocumentChatTheme(chatTheme);
applyDocumentChatFontFamily(chatFontFamily);
applyDocumentChatTranscriptWidthPercent(chatTranscriptWidthPercent);
window.ghostexSetSessionChatTheme = (value) => {
  chatTheme = normalizeSessionChatTheme(value);
  applyDocumentChatTheme(chatTheme);
  renderReadyChat?.(chatTheme);
};
window.ghostexSetSessionChatFontFamily = (value) => {
  chatFontFamily = typeof value === "string" ? value : "";
  applyDocumentChatFontFamily(chatFontFamily);
};
window.ghostexSetSessionChatTranscriptWidthPercent = (value) => {
  chatTranscriptWidthPercent = clampSessionChatTranscriptWidthPercent(Number(value));
  applyDocumentChatTranscriptWidthPercent(chatTranscriptWidthPercent);
};
window.ghostexSetSessionChatVerboseMode = (value) => {
  chatVerboseMode = value === true;
  renderReadyChat?.(chatTheme);
};

if (!projectId || !sessionId) {
  renderFailure(root, "This chat surface was opened without a session identity.", chatTheme);
} else {
  waitForBootstrap()
    .then((bootstrap) => {
      const transport = createGpuiSessionChatTransport(bootstrap, projectId, sessionId, remote);
      const composerBridge = createGpuiSessionChatComposerBridge(bootstrap, projectId, sessionId);
      const agentLabel = agentId ? (resolveSessionChatTranscriptAgent(agentId) ?? agentId) : null;
      renderReadyChat = (theme) => {
        root.render(
          <div className="native-sidebar-shell gpui-session-chat">
            <SessionChatView
              agentLabel={agentLabel}
              className="gpui-session-chat-view"
              hostActions={GPUI_SESSION_CHAT_HOST_ACTIONS}
              hostComposerBridge={composerBridge}
              hostLinks={GPUI_SESSION_CHAT_HOST_LINKS}
              // Staged next to chat.html by apps/desktop/vite.config.ts (stageMonacoVs).
              monacoVsBaseUrl="./monaco/vs"
              sessionKey={`${projectId}:${sessionId}`}
              theme={theme}
              transport={transport}
              verboseMode={chatVerboseMode}
            />
          </div>,
        );
      };
      renderReadyChat(chatTheme);
    })
    .catch(() => {
      renderFailure(
        root,
        "The session's Ghostex server is not reachable from this window. Toggle back to the terminal and try again.",
        chatTheme,
      );
    });
}
