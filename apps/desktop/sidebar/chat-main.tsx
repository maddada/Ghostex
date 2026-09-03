import { createRoot } from 'react-dom/client';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import '@/packages/core-ui/styles.css';
import {
  isSessionChatEventType,
  normalizeSessionChatTheme,
  resolveSessionChatDisplayAgent,
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
} from '@/packages/shared/session-chat';
import {
  GXSERVER_PROTOCOL_VERSION,
  type GxserverListStashedPromptsResult,
  type GxserverPresentationProject,
  type GxserverPresentationSession,
  type GxserverPresentationSnapshot,
  type GxserverReadSessionAgentNoteResult,
  type GxserverReadSessionTerminalTailResult,
  type GxserverRewindSessionChatResult,
  type GxserverSelectSessionChatModelResult,
  type GxserverSessionForkBranch,
  type GxserverSessionForkBranchesResult,
} from '@/packages/shared/gxserver-protocol';
import { gxserverRpcErrorFromResponseBody } from '@/packages/shared/gxserver-rpc-error';
import { listProjectMarkdownDocumentPaths, saveProjectMarkdownDocument } from '@/packages/shared/project-docs';
import type {
  GhostexBridgeError,
  GhostexExecChunk,
  GhostexExtensionContext,
} from '@/packages/shared/ghostex-extension-sdk';
import {
  GHOSTEX_CHAT_BAR_CONTEXT_CHANGED_EVENT,
  GHOSTEX_CHAT_BAR_PANEL_STORAGE_KEY,
  type GhostexChatBarBridgeRequestMessage,
  type GhostexChatBarPanelSessionState,
  type GhostexChatBarPanelSessions,
  type GhostexChatBarPanelToggleMessage,
  type GhostexExtensionLaunchContext,
  type GhostexExtensionRuntimeResult,
  type GhostexInstalledExtension,
  type GhostexListExtensionsResult,
  type GhostexSetExtensionStateResult,
} from '@/packages/shared/ghostex-extensions';
import {
  clampSessionChatTranscriptWidthPercent,
  DEFAULT_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
} from '@/packages/shared/ghostex-settings';
import { normalizeghostexHotkeySettings } from '@/packages/shared/ghostex-hotkeys';
import { formatSidebarHotkeyLabel } from '@/packages/core-ui/hotkey-label';
import {
  SessionChatView,
  type SessionChatHostActions,
  type SessionChatHostComposerBridge,
  type SessionChatHostLinks,
} from '@/packages/core-ui/chat/session-chat-view';
import type { SessionChatBarExtension } from '@/packages/core-ui/chat/session-chat-extension-panel';
import type { SessionChatTransport } from '@/packages/core-ui/chat/session-chat-transport';

/*
CDXC:SessionChat 2026-07-31:
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
    ghostexSetSessionChatCustomTranscriptWidthEnabled?: (enabled: unknown) => void;
    ghostexSetSessionChatFontFamily?: (fontFamily: unknown) => void;
    ghostexSetSessionChatTheme?: (theme: unknown) => void;
    ghostexSetSessionChatTranscriptWidthPercent?: (widthPercent: unknown) => void;
    ghostexSetSessionChatVerboseMode?: (verboseMode: unknown) => void;
  }
}

interface ChatBridgeNamespace {
  gxserverBootstrap?: ChatGxserverBootstrap;
  /**
   * Absolute paths of the OS drag currently over this page, written by the
   * Rust shell's CEF drag handler at drag-enter (and cleared by non-file
   * drags). Chromium never exposes `File.path` to the page, so this is the
   * only way a drop resolves to real local paths.
   */
  sessionChatDropPaths?: unknown;
  onGxserverBootstrapChanged?: (bootstrap: ChatGxserverBootstrap) => void;
  onSessionChatFocusComposerRequested?: () => void;
  onSessionChatHandoffToTerminalRequested?: () => void;
  onSessionChatInsertPromptRequested?: (payload: { content?: unknown }) => void;
  onSessionChatStashPromptRequested?: () => void;
  onSessionChatExtensionRequested?: (payload: GhostexChatBarPanelToggleMessage) => void;
  onSessionChatExtensionBridgeMessage?: (payload: unknown) => void;
  onSessionChatExtensionContextChanged?: (context: GhostexExtensionContext) => void;
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
  candidate: ChatGxserverBootstrap | undefined
): { authToken: string; baseUrl: string } | undefined {
  if (!candidate) {
    return undefined;
  }
  if (candidate.protocolVersion !== undefined && candidate.protocolVersion !== GXSERVER_PROTOCOL_VERSION) {
    return undefined;
  }
  const baseUrl = typeof candidate.baseUrl === 'string' ? candidate.baseUrl.trim() : '';
  const authToken = typeof candidate.authToken === 'string' ? candidate.authToken : '';
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
        reject(new Error('The Ghostex server bootstrap did not arrive.'));
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
  params: Record<string, unknown>
): Promise<TResult> {
  const response = await fetch(`${bootstrap.baseUrl}${path}`, {
    body: JSON.stringify({
      params,
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
    }),
    headers: {
      authorization: `Bearer ${bootstrap.authToken}`,
      'content-type': 'application/json',
      'x-gxserver-protocol-version': String(GXSERVER_PROTOCOL_VERSION),
    },
    method: 'POST',
  });
  let body: unknown;
  try {
    body = await response.json();
  } catch {
    body = undefined;
  }
  const envelope = body as { ok?: boolean; result?: TResult } | undefined;
  if (!response.ok || !envelope || envelope.ok !== true) {
    /*
    CDXC:SessionChat 2026-08-26:
    A gxserver refusal is `{ ok: false, error: <code>, message }` — the code is
    a string on the envelope, not a nested object. Rethrowing it as the typed
    GxserverRpcError is what lets the shared chat composer tell
    `composerNotReady` apart from every other send failure, and it also carries
    the daemon's own sentence instead of an HTTP status.
    */
    const rpcError = gxserverRpcErrorFromResponseBody(path, body);
    if (rpcError) {
      throw rpcError;
    }
    throw new Error(`gxserver rejected ${path} (${response.status > 0 ? response.status : 'no response'}).`);
  }
  return envelope.result as TResult;
}

interface GxserverReadPresentationSnapshotResult {
  snapshot: GxserverPresentationSnapshot;
}

function recordText(value: unknown, key: string): string | undefined {
  if (!value || typeof value !== 'object') {
    return undefined;
  }
  const field = (value as Record<string, unknown>)[key];
  return typeof field === 'string' && field.trim() ? field : undefined;
}

function isChatBarExtension(extension: GhostexInstalledExtension): boolean {
  return (
    extension.state.enabled &&
    extension.manifest.placements?.includes('chat-bar') === true &&
    (extension.state.placement ?? extension.manifest.defaultPlacement) === 'chat-bar'
  );
}

function readChatBarPanelSessions(value: unknown): GhostexChatBarPanelSessions {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return {};
  }
  const sessions: GhostexChatBarPanelSessions = {};
  for (const [sessionKey, candidate] of Object.entries(value)) {
    if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
      continue;
    }
    const state = candidate as Partial<GhostexChatBarPanelSessionState>;
    if (typeof state.open !== 'boolean' || typeof state.minimized !== 'boolean') {
      continue;
    }
    sessions[sessionKey] = {
      open: state.open,
      minimized: state.minimized,
      ...(typeof state.activeExtensionId === 'string' && state.activeExtensionId
        ? { activeExtensionId: state.activeExtensionId }
        : {}),
    };
  }
  return sessions;
}

function extensionIconUrl(extension: GhostexInstalledExtension): string | undefined {
  const icon = extension.manifest.icon.trim();
  if (!icon) {
    return undefined;
  }
  if (icon.startsWith('data:') || icon.startsWith('http://') || icon.startsWith('https://')) {
    return icon;
  }
  const runtimeUrl = extension.runtime.url;
  if (!runtimeUrl) {
    return undefined;
  }
  try {
    return new URL(icon, runtimeUrl).toString();
  } catch {
    return undefined;
  }
}

function extensionLaunchContext(
  project: GxserverPresentationProject,
  session: GxserverPresentationSession
): GhostexExtensionLaunchContext {
  return {
    sessionId: session.sessionId,
    projectName: project.title,
    ...(project.path ? { projectPath: project.path } : {}),
    worktree: Boolean(project.worktree),
    ...(recordText(project.worktree, 'branch') ? { worktreeBranch: recordText(project.worktree, 'branch') } : {}),
  };
}

function createGpuiSessionChatTransport(
  bootstrap: { authToken: string; baseUrl: string },
  projectId: string,
  sessionId: string,
  remote: boolean
): SessionChatTransport {
  return {
    async answerPrompt(params) {
      await rpc(bootstrap, '/api/answerSessionChatPrompt', {
        ...params,
        projectId,
        sessionId,
      });
    },
    async interrupt() {
      await rpc(bootstrap, '/api/interruptSessionChat', {
        projectId,
        sessionId,
      });
    },
    read(params) {
      return rpc<GxserverReadSessionChatResult>(bootstrap, '/api/readSessionChat', {
        projectId,
        sessionId,
        ...(params.limit !== undefined ? { limit: params.limit } : {}),
        ...(params.beforeOffset !== undefined ? { beforeOffset: params.beforeOffset } : {}),
      });
    },
    readSkills() {
      return rpc<GxserverReadSessionChatSkillsResult>(bootstrap, '/api/readSessionChatSkills', {
        projectId,
        sessionId,
      });
    },
    /*
    CDXC:SessionFork 2026-08-28:
    The chat's branch switcher reads the fork family from the session's OWN
    daemon on the same bootstrap as the transcript, so a remote session's
    branches are derived by the registry on the machine that ran them. It is
    not a sidebar-bridge call, so it needs nothing from the remote sidebar
    allowlist.
    */
    forkBranches() {
      return rpc<GxserverSessionForkBranchesResult>(bootstrap, '/api/sessionForkBranches', {
        projectId,
        sessionId,
      });
    },
    /*
    CDXC:SessionChat 2026-09-02:
    Rewinding drives the agent's own `/rewind` dialog inside the session's
    terminal, so it lands on the daemon that owns that pane through the same
    bootstrap as the transcript. The daemon re-snapshots the chat stream after
    it succeeds, so this page has nothing to prune.
    */
    rewindSessionChat(params) {
      return rpc<GxserverRewindSessionChatResult>(bootstrap, '/api/rewindSessionChat', {
        messageId: params.messageId,
        projectId,
        sessionId,
      });
    },
    selectSessionChatModel(params) {
      return rpc<GxserverSelectSessionChatModelResult>(bootstrap, '/api/selectSessionChatModel', {
        effort: params.effort,
        model: params.model,
        projectId,
        sessionId,
      });
    },
    readFiles() {
      return rpc<GxserverReadSessionChatFilesResult>(bootstrap, '/api/readSessionChatFiles', {
        projectId,
        sessionId,
      });
    },
    async send(text, imagePaths) {
      await rpc(bootstrap, '/api/sendSessionChatMessage', {
        projectId,
        sessionId,
        text,
        ...(imagePaths && imagePaths.length > 0 ? { imagePaths } : {}),
      });
    },
    // Raw keystroke (Claude's Shift+Tab mode cycle): same endpoint, `key`
    // instead of a body, so the server writes the bytes verbatim.
    async sendKey(key) {
      await rpc(bootstrap, '/api/sendSessionChatMessage', {
        key,
        projectId,
        sessionId,
      });
    },
    saveImage(params) {
      return rpc<GxserverSaveSessionChatImageResult>(bootstrap, '/api/saveSessionChatImage', {
        projectId,
        sessionId,
        base64Data: params.base64Data,
        ...(params.suggestedName ? { suggestedName: params.suggestedName } : {}),
      });
    },
    saveAttachment(params) {
      return rpc<GxserverSaveSessionChatAttachmentResult>(bootstrap, '/api/saveSessionChatAttachment', {
        projectId,
        sessionId,
        base64Data: params.base64Data,
        ...(params.directory !== undefined ? { directory: params.directory } : {}),
        ...(params.uploadId ? { uploadId: params.uploadId } : {}),
        ...(params.relativePath ? { relativePath: params.relativePath } : {}),
        ...(params.suggestedName ? { suggestedName: params.suggestedName } : {}),
      });
    },
    loadImage(params) {
      return rpc<GxserverReadSessionChatImageResult>(bootstrap, '/api/readSessionChatImage', {
        path: params.path,
      });
    },
    /*
    CDXC:SessionChat 2026-08-26:
    The evidence read behind a `composerNotReady` refusal. It lands on the
    session's own machine like every other call here, so a remote session's
    excerpt is the remote terminal's screen.
    */
    readTerminalTail() {
      return rpc<GxserverReadSessionTerminalTailResult>(bootstrap, '/api/readSessionTerminalTail', {
        projectId,
        sessionId,
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
          readDropPaths() {
            const paths = chatBridgeNamespace().sessionChatDropPaths;
            return Array.isArray(paths) ? paths.filter((path): path is string => typeof path === 'string') : [];
          },
        }),
    // The save panel writes to this Mac, so it is offered for every session:
    // the bytes travel with the request and never touch the session's machine.
    saveImageAs(params) {
      return requestNativeImageSave(params.base64Data, params.suggestedName);
    },
    listMessageMarkdownPaths() {
      return listProjectMarkdownDocumentPaths(projectId, (endpoint, request) => rpc(bootstrap, endpoint, request));
    },
    saveMessageMarkdown(params) {
      return saveProjectMarkdownDocument({ ...params, projectId }, (endpoint, request) =>
        rpc(bootstrap, endpoint, request)
      );
    },
    /*
    CDXC:SessionChat 2026-08-21:
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
      return rpc<GxserverQueueSessionChatPromptResult>(bootstrap, '/api/queueSessionChatPrompt', {
        projectId,
        sessionId,
        text: params.text,
      });
    },
    updateQueuedPrompt(params) {
      return rpc<GxserverSessionChatQueueResult>(bootstrap, '/api/updateSessionChatQueuedPrompt', {
        projectId,
        promptId: params.promptId,
        sessionId,
        ...(params.text !== undefined ? { text: params.text } : {}),
        ...(params.retry !== undefined ? { retry: params.retry } : {}),
      });
    },
    removeQueuedPrompt(params) {
      return rpc<GxserverSessionChatRemoveQueuedPromptResult>(bootstrap, '/api/removeSessionChatQueuedPrompt', {
        projectId,
        promptId: params.promptId,
        sessionId,
      });
    },
    reorderQueue(params) {
      return rpc<GxserverSessionChatQueueResult>(bootstrap, '/api/reorderSessionChatQueue', {
        projectId,
        promptIds: params.promptIds,
        sessionId,
      });
    },
    sendQueuedPrompt(params) {
      return rpc<GxserverSendSessionChatQueuedPromptResult>(bootstrap, '/api/sendSessionChatQueuedPrompt', {
        projectId,
        promptId: params.promptId,
        sessionId,
      });
    },
    /*
    CDXC:SessionNotes 2026-08-24:
    The session note is a plain gxserver round trip on the same bootstrap as
    every other chat call, so a remote session's note is stored by the daemon
    that owns the conversation. gxserver resolves the provider conversation id
    from (projectId, sessionId) itself — the page never handles that key.
    */
    readSessionNote() {
      return rpc<GxserverReadSessionAgentNoteResult>(bootstrap, '/api/readSessionAgentNote', {
        projectId,
        sessionId,
      });
    },
    async saveSessionNote(note) {
      await rpc(bootstrap, '/api/saveSessionAgentNote', {
        note,
        projectId,
        sessionId,
      });
    },
    /*
    CDXC:Drafts 2026-08-28:
    Switching a draft's agent is a plain gxserver round trip on the same
    bootstrap as every other chat call, so a remote draft is re-launched by the
    daemon that owns it. Unconditional here for the same reason the queue calls
    are: the daemon's `availableAgents` field is the capability probe the shared
    UI gates the "Agents" section on, so a daemon predating drafts hides the
    section without this host guessing at versions.
    */
    async switchDraftAgent(params) {
      await rpc(bootstrap, '/api/switchDraftAgent', {
        agentId: params.agentId,
        projectId,
        sessionId,
      });
    },
    // `clientId` is minted and persisted by the shared chat hook. Forward it
    // verbatim: a per-call or per-mount id would make this client's own draft
    // echo look like another device and pop the conflict bar for nothing.
    async setDraft(params) {
      await rpc(bootstrap, '/api/setSessionChatDraft', {
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
        url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
        url.searchParams.set('protocolVersion', String(GXSERVER_PROTOCOL_VERSION));
        url.searchParams.set('authToken', bootstrap.authToken);
        const nextSocket = new WebSocket(url.toString());
        socket = nextSocket;
        nextSocket.addEventListener('open', () => {
          reconnectAttempt = 0;
          const limit = currentLimit?.();
          nextSocket.send(
            JSON.stringify({
              projectId,
              sessionId,
              type: 'subscribeSessionChat',
              ...(typeof limit === 'number' && limit > 0 ? { limit } : {}),
            })
          );
        });
        nextSocket.addEventListener('message', (event) => {
          let parsed: unknown;
          try {
            parsed = JSON.parse(String(event.data));
          } catch {
            return;
          }
          if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
            return;
          }
          const frame = parsed as Record<string, unknown>;
          if (
            typeof frame.type !== 'string' ||
            !isSessionChatEventType(frame.type) ||
            frame.projectId !== projectId ||
            frame.sessionId !== sessionId ||
            typeof frame.epoch !== 'number' ||
            typeof frame.seq !== 'number' ||
            frame.protocolVersion !== GXSERVER_PROTOCOL_VERSION
          ) {
            return;
          }
          onEvent(frame as unknown as GxserverSessionChatEvent);
        });
        nextSocket.addEventListener('close', () => {
          if (closed || socket !== nextSocket) {
            return;
          }
          const delay = RECONNECT_DELAYS_MS[Math.min(reconnectAttempt, RECONNECT_DELAYS_MS.length - 1)];
          reconnectAttempt += 1;
          reconnectTimeoutId = window.setTimeout(connect, delay);
        });
        nextSocket.addEventListener('error', () => {
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
                type: 'unsubscribeSessionChat',
              })
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
CDXC:SessionChat 2026-07-31:
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

interface NativeExtensionBridgeMessage {
  requestId: string;
  ok?: boolean;
  result?: unknown;
  chunk?: GhostexExecChunk;
  error?: {
    code?: GhostexBridgeError['code'];
    message?: string;
    permission?: GhostexBridgeError['permission'];
  };
}

interface PendingNativeExtensionBridgeCall {
  onChunk: (chunk: GhostexExecChunk) => void;
  reject: (error: GhostexBridgeError) => void;
  resolve: (result: unknown) => void;
}

function nativeExtensionBridgeError(
  code: GhostexBridgeError['code'],
  message: string,
  permission?: GhostexBridgeError['permission']
): GhostexBridgeError {
  return Object.assign(new Error(message), { code, ...(permission ? { permission } : {}) });
}

function appModalHostMessageHandler(): AppModalHostMessageHandler | undefined {
  const target = window as unknown as {
    webkit?: {
      messageHandlers?: { ghostexAppModalHost?: AppModalHostMessageHandler };
    };
  };
  return target.webkit?.messageHandlers?.ghostexAppModalHost;
}

/*
CDXC:Diagnostics 2026-08-24:
Typing-focus-loss repro breadcrumbs. The chat page has no disk writer of its
own, so composer/prompt transitions ride the bridge into Rust, which appends
them to gpui-terminal-focus-debug.log only while the native.terminal.focus
scenario (plus Show debug UI controls) is enabled — same gate as the native
first-responder log they are meant to be correlated with.
*/
function postSessionChatDiagnosticLog(event: string, details?: Record<string, unknown>): void {
  postSessionChatHostAction('diagnosticLog', { details: details ?? {}, event });
}

function postSessionChatHostAction(action: string, fields?: Record<string, unknown>): boolean {
  const handler = appModalHostMessageHandler();
  if (!handler) {
    return false;
  }
  try {
    return (
      handler.postMessage(
        JSON.stringify({
          action,
          type: 'sessionChatHostAction',
          ...fields,
        })
      ) !== false
    );
  } catch {
    return false;
  }
}

function createGpuiSessionChatComposerBridge(
  bootstrap: { authToken: string; baseUrl: string },
  projectId: string,
  sessionId: string
): SessionChatHostComposerBridge {
  return {
    register(actions) {
      const namespace = chatBridgeNamespace();
      const insertPrompt = (payload: { content?: unknown }): void => {
        if (typeof payload?.content === 'string' && payload.content.length > 0) {
          actions.insertPrompt(payload.content);
        }
      };
      const requestFocus = (): void => actions.focus();
      const requestHandoffToTerminal = (): void => {
        void actions
          .handoffToTerminal()
          .then((handoff) => {
            postSessionChatHostAction('draftHandoffToTerminalComplete', {
              content: handoff.content,
              stashedPromptId: handoff.stashedPromptId ?? '',
            });
          })
          .catch(() => {
            postSessionChatHostAction('draftHandoffToTerminalFailed');
          });
      };
      const requestStash = (): void => actions.requestStash();
      namespace.onSessionChatFocusComposerRequested = requestFocus;
      namespace.onSessionChatHandoffToTerminalRequested = requestHandoffToTerminal;
      namespace.onSessionChatInsertPromptRequested = insertPrompt;
      namespace.onSessionChatStashPromptRequested = requestStash;
      postSessionChatHostAction('composerReady');
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
    CDXC:SessionChat 2026-08-24:
    The desktop shell destroys chat surfaces that have been hidden for a long
    time to reclaim their Chromium RAM, and rebuilds them on the next pane that
    shows them. This is how Rust learns the page is not holding an unsent draft
    or attached image, so an eviction can never destroy typed text. The payload
    is the boolean only.
    */
    reportDraftState({ empty }) {
      postSessionChatHostAction('composerDraftState', { empty });
    },
    /*
    CDXC:Drafts 2026-08-24:
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
      }>(bootstrap, '/api/saveStashedPrompt', {
        content,
        projectId,
        sessionId,
      });
      const promptId = result.prompt?.promptId;
      // Only a row this save created may ever be deleted again: `created:
      // false` means the text matched a prompt the user saved by hand.
      return options?.transient && result.created === true && promptId ? { promptId } : {};
    },
    /*
    CDXC:SavedPrompts 2026-08-24:
    "Stashed from this conversation" is two questions, because a stash outlives
    the session it was written from. The provider conversation id is the
    durable answer and survives a compaction-resume rewrite (gxserver re-keys
    the column); the raw sessionId still matches legacy rows and rows stashed
    before this session had a conversation id. Both ids compared here are RAW
    gxserver ids — gxserver normalizes away the sidebar's `combined-session:`
    keys as of migration 0026 — so no decoding is needed on this side.
    */
    async countSessionStashedPrompts(agentSessionId) {
      const result = await rpc<GxserverListStashedPromptsResult>(bootstrap, '/api/listStashedPrompts', {
        projectId,
      });
      return result.prompts.filter(
        (prompt) =>
          (agentSessionId !== null && prompt.agentSessionId === agentSessionId) || prompt.sessionId === sessionId
      ).length;
    },
    showStashedPrompts() {
      // The existing host action already opens Saved Prompts with this
      // session's context attached, so the modal can default its own scope.
      postSessionChatHostAction('stashedPrompts');
    },
  };
}

/*
CDXC:SessionChat 2026-08-02:
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
    const requestId = typeof payload?.requestId === 'string' ? payload.requestId : '';
    const resolve = pendingAttachmentPicks.get(requestId);
    if (!resolve) {
      return;
    }
    pendingAttachmentPicks.delete(requestId);
    const paths = Array.isArray(payload.paths)
      ? payload.paths.filter((path): path is string => typeof path === 'string')
      : [];
    resolve(paths);
  };
}

/*
CDXC:SessionChat 2026-08-19:
"Save image" in the chat image overlay cannot be a browser download: gpui
installs no CEF download handler, so a <a download> click is cancelled without
a trace. Image bytes can exceed the bridge's one-message limit, so the page
transfers them in bounded chunks. Rust writes the assembled file into Downloads,
then answers through the fixed window.ghostexGpui.onSessionChatImageSaved
callback with {requestId, error}; no error means the file landed in Downloads.
*/
const IMAGE_SAVE_TIMEOUT_MS = 180_000;
const IMAGE_SAVE_CHUNK_CHARS = 256 * 1024;

interface ChatImageSaveNamespace {
  onSessionChatImageSaved?: (payload: { requestId?: string; error?: unknown }) => void;
}

let imageSaveSequence = 0;
const pendingImageSaves = new Map<string, (error: string | null) => void>();

function installImageSaveCallback(): void {
  const namespace = chatBridgeNamespace() as ChatBridgeNamespace & ChatImageSaveNamespace;
  namespace.onSessionChatImageSaved = (payload) => {
    const requestId = typeof payload?.requestId === 'string' ? payload.requestId : '';
    const settle = pendingImageSaves.get(requestId);
    if (!settle) {
      return;
    }
    pendingImageSaves.delete(requestId);
    settle(typeof payload.error === 'string' && payload.error !== '' ? payload.error : null);
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
    // Reclaim both sides if the host never answers (for example, if the pane
    // was torn down during the transfer). A timeout is a failure, never a
    // successful download.
    window.setTimeout(() => {
      if (pendingImageSaves.delete(requestId)) {
        postSessionChatHostAction('saveImageCancel', { requestId });
        reject(new Error('The image save did not complete.'));
      }
    }, IMAGE_SAVE_TIMEOUT_MS);
    const rejectBridgeTransfer = (): void => {
      pendingImageSaves.delete(requestId);
      postSessionChatHostAction('saveImageCancel', { requestId });
      reject(new Error('The native image save bridge rejected the transfer.'));
    };
    if (!postSessionChatHostAction('saveImageStart', { requestId, suggestedName })) {
      rejectBridgeTransfer();
      return;
    }
    let chunkIndex = 0;
    for (let offset = 0; offset < base64Data.length; offset += IMAGE_SAVE_CHUNK_CHARS) {
      if (
        !postSessionChatHostAction('saveImageChunk', {
          base64Chunk: base64Data.slice(offset, offset + IMAGE_SAVE_CHUNK_CHARS),
          chunkIndex,
          requestId,
        })
      ) {
        rejectBridgeTransfer();
        return;
      }
      chunkIndex += 1;
    }
    if (!postSessionChatHostAction('saveImageFinish', { requestId })) {
      rejectBridgeTransfer();
    }
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
    postSessionChatHostAction('pickAttachments', { requestId });
  });
}

/*
CDXC:SessionChat 2026-08-03:
Links in the conversation belong to the app, not to this page: a web URL opens
in Ghostex's own Browser view (Shift+click asks for the OS browser instead),
and a file path opens in the project's Docs view when Docs can show it, else in
the Code view. Both ride the same host-action bridge as the button cluster; the
page never navigates itself, since chat.html has nowhere to navigate to.

CDXC:SessionChat 2026-08-18:
Where a web URL actually lands is the host's call, not this page's: it reads the
"Open links in embedded browser" Browser setting, the same one Command-clicked
terminal links use, and hands the URL to the system default browser when that
setting is off.
*/
const GPUI_SESSION_CHAT_HOST_LINKS: SessionChatHostLinks = {
  openUrl: (url, { external, forceEmbedded }) =>
    postSessionChatHostAction('openLink', { external, forceEmbedded: forceEmbedded === true, url }),
  openFile: (path, position) =>
    postSessionChatHostAction('openFile', {
      path,
      ...(position ? { line: position.line, ...(position.column ? { column: position.column } : {}) } : {}),
    }),
  locateFile: (path) => postSessionChatHostAction('locateFile', { path }),
};

function createGpuiSessionChatHostActions(hotkeysValue: unknown): SessionChatHostActions {
  const hotkeys = normalizeghostexHotkeySettings(hotkeysValue);
  const shortcut = (id: keyof typeof hotkeys): string | undefined => {
    const value = hotkeys[id];
    return value ? formatSidebarHotkeyLabel(value) : undefined;
  };
  return {
    onPasteIntoComposer: () => postSessionChatHostAction('pasteIntoComposer'),
    onSwitchToTerminal: () => postSessionChatHostAction('terminalView'),
    onSwitchToTerminalForAgentPicker: () => postSessionChatHostAction('agentPickerTerminalView'),
    moreActionsShortcut: shortcut('toggleAgentActions'),
    sessionNoteShortcut: shortcut('sessionNote'),
    switchViewShortcut: shortcut('toggleChatView'),
    actions: [
      {
        id: 'rename',
        label: 'Rename',
        shortcut: shortcut('renameActiveSession'),
      },
      {
        id: 'sleep',
        label: 'Sleep',
        shortcut: shortcut('sleepFocusedSession'),
      },
      /*
      Sentence case, matching the desktop terminal's native agent action bar
      (apps/desktop/src/app/render/terminal_agent_action_bar.rs). The two
      surfaces show the same menu, so their rows may not read as two different
      products; the labels the host supplies are the only copy the chat menu has.
      */
      {
        id: 'delayedActions',
        label: 'Delayed actions',
        shortcut: shortcut('delayedSend'),
      },
      {
        id: 'closeAfterDone',
        label: 'Close After Done',
        shortcut: shortcut('closeAfterDone'),
      },
      { id: 'fork', label: 'Fork Session', shortcut: shortcut('forkSession') },
      {
        id: 'fullReload',
        label: 'Full reload',
        shortcut: shortcut('reloadSession'),
      },
      /*
      CDXC:AgentProviders 2026-09-03:
      Listed without rows: the shared chat view fills them from the daemon's
      `switchableAgents` on the read state and hides the row when there are
      none. The pick comes back as the value and rides to Rust with the id.
      */
      { id: 'switchAccount', label: 'Switch Account' },
      {
        id: 'promptEditor',
        label: 'Prompt editor',
        shortcut: shortcut('promptEditor'),
      },
      {
        id: 'stashPrompt',
        label: 'Stash prompt',
        shortcut: shortcut('stashPrompt'),
      },
      {
        id: 'stashedPrompts',
        label: 'Saved prompts',
        shortcut: shortcut('stashedPrompts'),
      },
      {
        id: 'attachPath',
        label: 'Attach a file or folder',
        shortcut: shortcut('attachFileOrFolder'),
      },
      {
        id: 'exportTranscript',
        label: 'Handoff / Export',
        shortcut: shortcut('exportTranscript'),
      },
    ],
    onAction: (id, value) =>
      postSessionChatHostAction(id, id === 'switchAccount' && value ? { agentId: value } : undefined),
  };
}

interface GpuiSessionChatPageProps {
  agentLabel: string | null;
  bootstrap: { authToken: string; baseUrl: string };
  composerBridge: SessionChatHostComposerBridge;
  projectId: string;
  sessionId: string;
  theme: SessionChatTheme;
  transport: SessionChatTransport;
}

function GpuiSessionChatPage({
  agentLabel,
  bootstrap,
  composerBridge,
  projectId,
  sessionId,
  theme,
  transport,
}: GpuiSessionChatPageProps) {
  const sessionKey = `${projectId}:${sessionId}`;
  const [sessionTitle, setSessionTitle] = useState('');
  const [extensions, setExtensions] = useState<GhostexInstalledExtension[]>([]);
  const extensionsRef = useRef<GhostexInstalledExtension[]>([]);
  const [panelState, setPanelState] = useState<GhostexChatBarPanelSessionState>({
    open: false,
    minimized: false,
  });
  const panelStateRef = useRef(panelState);
  const panelSessionsRef = useRef<GhostexChatBarPanelSessions>({});
  const ownerExtensionIdRef = useRef<string | undefined>(undefined);
  const extensionContextRef = useRef<{
    project: GxserverPresentationProject;
    session: GxserverPresentationSession;
  } | null>(null);
  const persistenceQueueRef = useRef<Promise<void>>(Promise.resolve());
  const nativeBridgeSequenceRef = useRef(0);
  const pendingNativeBridgeCallsRef = useRef(new Map<string, PendingNativeExtensionBridgeCall>());

  useEffect(() => {
    const namespace = chatBridgeNamespace();
    const previousMessageHandler = namespace.onSessionChatExtensionBridgeMessage;
    const previousContextHandler = namespace.onSessionChatExtensionContextChanged;
    const handleMessage = (payload: unknown): void => {
      if (!payload || typeof payload !== 'object') {
        return;
      }
      const message = payload as Partial<NativeExtensionBridgeMessage>;
      if (typeof message.requestId !== 'string') {
        return;
      }
      const pending = pendingNativeBridgeCallsRef.current.get(message.requestId);
      if (!pending) {
        return;
      }
      if (
        message.chunk &&
        (message.chunk.stream === 'stdout' || message.chunk.stream === 'stderr') &&
        typeof message.chunk.text === 'string'
      ) {
        pending.onChunk(message.chunk);
        return;
      }
      pendingNativeBridgeCallsRef.current.delete(message.requestId);
      if (message.ok === true) {
        pending.resolve(message.result);
        return;
      }
      const error = message.error;
      pending.reject(
        nativeExtensionBridgeError(
          error?.code === 'invalidRequest' ||
            error?.code === 'notFound' ||
            error?.code === 'permissionDenied' ||
            error?.code === 'operationFailed'
            ? error.code
            : 'operationFailed',
          typeof error?.message === 'string' ? error.message : 'The extension call failed.',
          error?.permission
        )
      );
    };
    const handleContextChanged = (context: GhostexExtensionContext): void => {
      if (context.activeSession?.title) {
        setSessionTitle(context.activeSession.title);
      }
      window.dispatchEvent(new CustomEvent(GHOSTEX_CHAT_BAR_CONTEXT_CHANGED_EVENT, { detail: context }));
    };
    namespace.onSessionChatExtensionBridgeMessage = handleMessage;
    namespace.onSessionChatExtensionContextChanged = handleContextChanged;
    const pendingCalls = pendingNativeBridgeCallsRef.current;
    return () => {
      if (namespace.onSessionChatExtensionBridgeMessage === handleMessage) {
        namespace.onSessionChatExtensionBridgeMessage = previousMessageHandler;
      }
      if (namespace.onSessionChatExtensionContextChanged === handleContextChanged) {
        namespace.onSessionChatExtensionContextChanged = previousContextHandler;
      }
      for (const pending of pendingCalls.values()) {
        pending.reject(nativeExtensionBridgeError('operationFailed', 'The chat-bar extension host closed.'));
      }
      pendingCalls.clear();
    };
  }, []);

  const publishExtensions = useCallback((next: GhostexInstalledExtension[]): void => {
    extensionsRef.current = next;
    setExtensions(next);
  }, []);

  const replaceExtension = useCallback(
    (replacement: GhostexInstalledExtension): void => {
      publishExtensions(
        extensionsRef.current.map((extension) => (extension.id === replacement.id ? replacement : extension))
      );
    },
    [publishExtensions]
  );

  const startExtension = useCallback(
    async (extensionId: string): Promise<void> => {
      const extension = extensionsRef.current.find((candidate) => candidate.id === extensionId);
      const context = extensionContextRef.current;
      if (!extension || !context || (extension.runtime.state === 'ready' && extension.runtime.url)) {
        return;
      }
      replaceExtension({
        ...extension,
        runtime: { state: 'starting' },
      });
      try {
        const result = await rpc<GhostexExtensionRuntimeResult>(bootstrap, '/api/startExtension', {
          id: extensionId,
          context: extensionLaunchContext(context.project, context.session),
        });
        const current = extensionsRef.current.find((candidate) => candidate.id === extensionId);
        if (current) {
          replaceExtension({ ...current, runtime: result.status });
        }
      } catch (error) {
        const current = extensionsRef.current.find((candidate) => candidate.id === extensionId);
        if (current) {
          replaceExtension({
            ...current,
            runtime: {
              state: 'failed',
              error: error instanceof Error ? error.message : 'The extension could not be started.',
            },
          });
        }
      }
    },
    [bootstrap, replaceExtension]
  );

  useEffect(() => {
    let active = true;
    void Promise.all([
      rpc<GhostexListExtensionsResult>(bootstrap, '/api/listExtensions', {}),
      rpc<GxserverReadPresentationSnapshotResult>(bootstrap, '/api/readPresentationSnapshot', {}),
    ])
      .then(([extensionResult, presentationResult]) => {
        if (!active) {
          return;
        }
        const project = presentationResult.snapshot.projects.find((candidate) => candidate.projectId === projectId);
        const session = presentationResult.snapshot.sessions.find(
          (candidate) => candidate.projectId === projectId && candidate.sessionId === sessionId
        );
        if (!project || !session) {
          throw new Error(`Session ${sessionId} was not found in project ${projectId}.`);
        }
        setSessionTitle(session.displayTitle ?? session.primaryTitle ?? session.title);
        const chatBarExtensions = extensionResult.extensions
          .filter(isChatBarExtension)
          .sort((a, b) => a.id.localeCompare(b.id));
        const owner = chatBarExtensions[0];
        const autoOpenExtension = chatBarExtensions.find((extension) => extension.state.chatBarAutoOpen);
        const storedSessions = readChatBarPanelSessions(owner?.state.storage[GHOSTEX_CHAT_BAR_PANEL_STORAGE_KEY]);
        const storedState = storedSessions[sessionKey];
        const requestedActiveId = storedState?.activeExtensionId;
        const activeExtensionId = chatBarExtensions.some((extension) => extension.id === requestedActiveId)
          ? requestedActiveId
          : (autoOpenExtension?.id ?? chatBarExtensions[0]?.id);
        const initialState: GhostexChatBarPanelSessionState = {
          open: storedState?.open ?? Boolean(autoOpenExtension),
          minimized: storedState?.minimized ?? false,
          ...(activeExtensionId ? { activeExtensionId } : {}),
        };
        extensionContextRef.current = { project, session };
        ownerExtensionIdRef.current = owner?.id;
        panelSessionsRef.current = storedSessions;
        panelStateRef.current = initialState;
        publishExtensions(chatBarExtensions);
        setPanelState(initialState);
        if (initialState.open && activeExtensionId) {
          void startExtension(activeExtensionId);
        }
      })
      .catch((error: unknown) => {
        if (active) {
          console.error('Could not initialize chat-bar extensions.', error);
        }
      });
    return () => {
      active = false;
    };
  }, [bootstrap, projectId, publishExtensions, sessionId, sessionKey, startExtension]);

  const persistPanelState = useCallback(
    (next: GhostexChatBarPanelSessionState): void => {
      const ownerExtensionId = ownerExtensionIdRef.current;
      if (!ownerExtensionId) {
        return;
      }
      const nextSessions = { ...panelSessionsRef.current, [sessionKey]: next };
      panelSessionsRef.current = nextSessions;
      persistenceQueueRef.current = persistenceQueueRef.current
        .catch(() => undefined)
        .then(async () => {
          const result = await rpc<GhostexSetExtensionStateResult>(bootstrap, '/api/updateExtensionState', {
            id: ownerExtensionId,
            patch: {
              storage: { [GHOSTEX_CHAT_BAR_PANEL_STORAGE_KEY]: nextSessions },
            },
          });
          replaceExtension(result.extension);
        })
        .catch((error: unknown) => {
          console.error('Could not persist chat-bar panel state.', error);
        });
    },
    [bootstrap, replaceExtension, sessionKey]
  );

  const updatePanelState = useCallback(
    (patch: { activeExtensionId?: string; minimized?: boolean; open?: boolean }): void => {
      const next: GhostexChatBarPanelSessionState = {
        ...panelStateRef.current,
        ...patch,
      };
      panelStateRef.current = next;
      setPanelState(next);
      persistPanelState(next);
      if (next.open && patch.activeExtensionId) {
        void startExtension(patch.activeExtensionId);
      }
    },
    [persistPanelState, startExtension]
  );

  useEffect(() => {
    if (extensions.length === 0) {
      return;
    }
    const namespace = chatBridgeNamespace();
    const previous = namespace.onSessionChatExtensionRequested;
    const handleExtensionRequest = (payload: GhostexChatBarPanelToggleMessage): void => {
      const extensionId =
        payload?.type === 'ghostexChatBarPanelToggle' && typeof payload.extensionId === 'string'
          ? payload.extensionId
          : '';
      if (!extensionsRef.current.some((extension) => extension.id === extensionId)) {
        return;
      }
      updatePanelState({
        activeExtensionId: extensionId,
        minimized: false,
        open: !panelStateRef.current.open,
      });
    };
    namespace.onSessionChatExtensionRequested = handleExtensionRequest;
    return () => {
      if (namespace.onSessionChatExtensionRequested === handleExtensionRequest) {
        namespace.onSessionChatExtensionRequested = previous;
      }
    };
  }, [extensions.length, updatePanelState]);

  /*
  CDXC:SessionFork 2026-08-28:
  Switching branches is a workspace focus change, and this page is bound to one
  session, so it asks the daemon to do it: `/api/focusSession` dispatches the
  renderer command the sidebar page already answers with its normal focusSession
  routing (pane selection, terminal materialization, presentation focus). No new
  native message shape is involved.

  Remote chats deliberately get NO switch: their bootstrap points at the remote
  machine's own daemon through the SSH tunnel, and that daemon has no renderer
  attached from this Mac, so the command would only sit until it timed out. The
  switcher then lists the family read-only, which is the honest state.

  CDXC:SessionFork 2026-09-03:
  A fork's ancestor is usually STOPPED: forking kills the source's provider and
  Previous Sessions hides the row. A stopped row is not in the live
  presentation, so `/api/focusSession` alone answers "No matching session was
  found" and the click did nothing. Wake it first: `/api/wakeSession` has no
  lifecycle guard, respawns the provider with the row's saved agent resume
  command, marks the SAME registry row running, and broadcasts its presentation
  delta before it answers, so the follow-up focus resolves. Reviving in place
  keeps the family edge intact; a Previous-Sessions-style restore would create a
  new row and remove the parent both leaves point at.
  */
  const focusForkBranch = useCallback(
    (branch: GxserverSessionForkBranch): void => {
      const target = { projectId: branch.projectId, sessionId: branch.sessionId };
      const wake =
        branch.lifecycleState === 'stopped'
          ? rpc(bootstrap, '/api/wakeSession', target).then(() => undefined)
          : Promise.resolve();
      void wake
        .then(() => rpc(bootstrap, '/api/focusSession', target))
        .catch((error: unknown) => {
          postSessionChatDiagnosticLog('sessionChat.forkBranchSwitchFailed', {
            lifecycleState: branch.lifecycleState,
            message: error instanceof Error ? error.message : String(error),
          });
        });
    },
    [bootstrap]
  );

  const handleBridgeRequest = useCallback(
    async (
      extensionId: string,
      request: GhostexChatBarBridgeRequestMessage,
      onChunk: (chunk: GhostexExecChunk) => void
    ): Promise<unknown> => {
      const extension = extensionsRef.current.find((candidate) => candidate.id === extensionId);
      if (!extension) {
        throw nativeExtensionBridgeError('notFound', 'The chat-bar extension is not available.');
      }
      const handler = appModalHostMessageHandler();
      if (!handler) {
        throw nativeExtensionBridgeError('operationFailed', 'The native extension bridge is unavailable.');
      }
      const nativeRequestId = `chat-bar-${Date.now().toString(36)}-${(++nativeBridgeSequenceRef.current).toString(36)}`;
      return new Promise((resolve, reject) => {
        pendingNativeBridgeCallsRef.current.set(nativeRequestId, { onChunk, reject, resolve });
        try {
          const accepted = handler.postMessage(
            JSON.stringify({
              type: 'sessionChatExtensionBridgeRequest',
              extensionId,
              request: {
                requestId: nativeRequestId,
                method: request.method,
                params: request.params ?? {},
              },
            })
          );
          if (accepted === false) {
            pendingNativeBridgeCallsRef.current.delete(nativeRequestId);
            reject(nativeExtensionBridgeError('operationFailed', 'Ghostex rejected the extension call.'));
          }
        } catch {
          pendingNativeBridgeCallsRef.current.delete(nativeRequestId);
          reject(nativeExtensionBridgeError('operationFailed', 'Ghostex could not send the extension call.'));
        }
      });
    },
    []
  );

  const chatBarExtensions = useMemo<SessionChatBarExtension[]>(
    () =>
      extensions.map((extension) => ({
        id: extension.id,
        title: extension.manifest.title,
        ...(extensionIconUrl(extension) ? { iconUrl: extensionIconUrl(extension) } : {}),
        ...(extension.runtime.url ? { url: extension.runtime.url } : {}),
        ...(extension.runtime.error ? { error: extension.runtime.error } : {}),
      })),
    [extensions]
  );

  return (
    <div className='native-sidebar-shell gpui-session-chat'>
      <SessionChatView
        agentLabel={agentLabel}
        chatBarExtensions={chatBarExtensions}
        chatBarPanelState={panelState}
        className='gpui-session-chat-view'
        customTranscriptWidthEnabled={chatCustomTranscriptWidthEnabled}
        diagnosticLog={postSessionChatDiagnosticLog}
        hostActions={GPUI_SESSION_CHAT_HOST_ACTIONS}
        hostComposerBridge={composerBridge}
        hostLinks={GPUI_SESSION_CHAT_HOST_LINKS}
        monacoVsBaseUrl='./monaco/vs'
        onChatBarBridgeRequest={handleBridgeRequest}
        onChatBarPanelStateChange={updatePanelState}
        onDelayedActions={() => postSessionChatHostAction('delayedActions')}
        {...(remote ? {} : { onSelectForkBranch: focusForkBranch })}
        sessionKey={sessionKey}
        sessionTitle={sessionTitle}
        theme={theme}
        transport={transport}
        verboseMode={chatVerboseMode}
      />
    </div>
  );
}

function renderFailure(root: ReturnType<typeof createRoot>, message: string, theme: SessionChatTheme): void {
  root.render(
    <div className='native-sidebar-shell gpui-session-chat'>
      <div className='ghostex-session-chat-scope ghostex-chat-empty-state' data-chat-theme={theme}>
        <div className='ghostex-chat-empty-title'>Chat unavailable</div>
        <div className='ghostex-chat-empty-detail'>{message}</div>
      </div>
    </div>
  );
}

const rootElement = document.getElementById('root');
if (!rootElement) {
  throw new Error('Ghostex session chat root element was not found.');
}
const root = createRoot(rootElement);
const searchParams = new URLSearchParams(window.location.search);
const projectId = searchParams.get('projectId')?.trim() ?? '';
const sessionId = searchParams.get('sessionId')?.trim() ?? '';
const agentId = searchParams.get('agentId')?.trim() ?? '';
const remote = searchParams.get('remote') === 'true';
let hotkeysValue: unknown;
try {
  hotkeysValue = JSON.parse(searchParams.get('hotkeys') ?? '{}');
} catch {
  hotkeysValue = {};
}
const GPUI_SESSION_CHAT_HOST_ACTIONS = createGpuiSessionChatHostActions(hotkeysValue);
let chatTheme = normalizeSessionChatTheme(searchParams.get('theme'));
let chatFontFamily = searchParams.get('fontFamily')?.trim() ?? '';
let chatCustomTranscriptWidthEnabled = searchParams.get('customTranscriptWidthEnabled') === 'true';
let chatTranscriptWidthPercent = clampSessionChatTranscriptWidthPercent(
  Number(searchParams.get('transcriptWidthPercent')) || DEFAULT_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT
);
let chatVerboseMode = searchParams.get('verboseMode') === 'true';
let renderReadyChat: ((theme: SessionChatTheme) => void) | null = null;

function applyDocumentChatTheme(theme: SessionChatTheme): void {
  document.documentElement.style.colorScheme = theme;
  /* Keep the document backing identical to the chat surface and native host. */
  document.documentElement.style.backgroundColor = theme === 'light' ? '#fdfdfd' : '#0d0d0d';
  document.body.style.backgroundColor = theme === 'light' ? '#fdfdfd' : '#0d0d0d';
}

/*
CDXC:SessionChat 2026-08-22:
An empty setting REMOVES the property rather than writing a fallback chain into
it. The stylesheet already declares what the transcript falls back to, and the
custom property's own fallback only applies while the property is unset — so
writing "no choice" as a value here silently overrode the sheet's default and
made the chat's typeface impossible to change from CSS.
*/
function applyDocumentChatFontFamily(fontFamily: string): void {
  const normalized = fontFamily.trim();
  if (normalized) {
    document.documentElement.style.setProperty('--ghostex-session-chat-font-family', normalized);
  } else {
    document.documentElement.style.removeProperty('--ghostex-session-chat-font-family');
  }
  window.dispatchEvent(new Event('ghostex-session-chat-font-family-changed'));
}

function applyDocumentChatTranscriptWidthPercent(widthPercent: number): void {
  document.documentElement.style.setProperty(
    '--ghostex-session-chat-transcript-width-percent',
    String(clampSessionChatTranscriptWidthPercent(widthPercent))
  );
}

document.body.dataset.sidebarTheme = 'plain-dark';
document.body.classList.add('vscode-dark', 'native-sidebar-body');
applyDocumentChatTheme(chatTheme);
applyDocumentChatFontFamily(chatFontFamily);
applyDocumentChatTranscriptWidthPercent(chatTranscriptWidthPercent);
window.ghostexSetSessionChatTheme = (value) => {
  chatTheme = normalizeSessionChatTheme(value);
  applyDocumentChatTheme(chatTheme);
  renderReadyChat?.(chatTheme);
};
window.ghostexSetSessionChatFontFamily = (value) => {
  chatFontFamily = typeof value === 'string' ? value : '';
  applyDocumentChatFontFamily(chatFontFamily);
};
window.ghostexSetSessionChatCustomTranscriptWidthEnabled = (value) => {
  chatCustomTranscriptWidthEnabled = value === true;
  renderReadyChat?.(chatTheme);
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
  renderFailure(root, 'This chat surface was opened without a session identity.', chatTheme);
} else {
  waitForBootstrap()
    .then((bootstrap) => {
      const transport = createGpuiSessionChatTransport(bootstrap, projectId, sessionId, remote);
      const composerBridge = createGpuiSessionChatComposerBridge(bootstrap, projectId, sessionId);
      /*
      CDXC:Drafts 2026-08-28:
      A SEED, not the truth: the URL parameter is whatever agent the session had
      when this page was opened, and a draft's agent can be switched from the
      composer without the page reloading. SessionChatView follows the chat read
      state once it lands and falls back to this only until then.
      */
      const agentLabel = agentId ? (resolveSessionChatDisplayAgent(agentId) ?? agentId) : null;
      renderReadyChat = (theme) => {
        root.render(
          <GpuiSessionChatPage
            agentLabel={agentLabel}
            bootstrap={bootstrap}
            composerBridge={composerBridge}
            projectId={projectId}
            sessionId={sessionId}
            theme={theme}
            transport={transport}
          />
        );
      };
      renderReadyChat(chatTheme);
    })
    .catch(() => {
      renderFailure(
        root,
        "The session's Ghostex server is not reachable from this window. Toggle back to the terminal and try again.",
        chatTheme
      );
    });
}
