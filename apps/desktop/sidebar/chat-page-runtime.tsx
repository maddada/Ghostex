import { createSessionChatDiagnosticRecorder } from '@/packages/core-ui/chat/session-chat-diagnostics';
import { resolveSessionChatTheme, subscribeSystemChatTheme } from '@/packages/core-ui/chat/session-chat-theme';
import type { SessionChatTransport } from '@/packages/core-ui/chat/session-chat-transport';
import {
  type SessionChatHostActions,
  type SessionChatHostComposerBridge,
  type SessionChatHostLinks,
} from '@/packages/core-ui/chat/session-chat-view';
import { formatSidebarHotkeyLabel } from '@/packages/core-ui/hotkey-label';
import type { GhostexExtensionContext } from '@/packages/shared/ghostex-extension-sdk';
import { type GhostexChatBarPanelToggleMessage } from '@/packages/shared/ghostex-extensions';
import { normalizeghostexHotkeySettings } from '@/packages/shared/ghostex-hotkeys';
import {
  clampSessionChatTranscriptWidthPercent,
  DEFAULT_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
} from '@/packages/shared/ghostex-settings';
import {
  GXSERVER_PROTOCOL_VERSION,
  type GxserverListStashedPromptsResult,
  type GxserverPresentationSnapshot,
  type GxserverReadSessionAgentNoteResult,
  type GxserverReadSessionTerminalTailResult,
  type GxserverRewindSessionChatResult,
  type GxserverSelectSessionChatModelResult,
  type GxserverSessionForkBranchesResult,
} from '@/packages/shared/gxserver-protocol';
import { gxserverRpcErrorFromResponseBody } from '@/packages/shared/gxserver-rpc-error';
import { listProjectMarkdownDocumentPaths, saveProjectMarkdownDocument } from '@/packages/shared/project-docs';
import {
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
  type GxserverSessionChatQueueResult,
  type GxserverSessionChatRemoveQueuedPromptResult,
  type SessionChatTheme,
} from '@/packages/shared/session-chat';
import type { GxserverSetSessionChatDraftResult, SessionChatDraftHandoff } from '@/packages/shared/session-chat-queue';
import { createRoot } from 'react-dom/client';
import { createAccountSwitchTransport } from './account-switch';

import { createGpuiSessionChatPage } from './chat-page';
import { retainSessionChatRuntimeEndpoint, retainSessionChatTransport } from './session-chat-runtime';
export interface ChatGxserverBootstrap {
  authToken?: string;
  baseUrl?: string;
  clientId?: string;
  protocolVersion?: number;
}

declare global {
  interface Window {
    ghostexSetSessionChatCustomTranscriptWidthEnabled?: (enabled: unknown) => void;
    ghostexSetSessionChatFontFamily?: (fontFamily: unknown) => void;
    ghostexSetHideAccountEmails?: (hidden: unknown) => void;
    ghostexSetSessionChatTheme?: (theme: unknown) => void;
    ghostexSetSessionChatTranscriptWidthPercent?: (widthPercent: unknown) => void;
    ghostexSetSessionChatFileEditPreviews?: (enabled: unknown) => void;
    ghostexSetSessionChatHotkeys?: (hotkeys: unknown) => void;
    ghostexSetSessionChatVerboseMode?: (verboseMode: unknown) => void;
  }
}

export interface ChatBridgeNamespace {
  gxserverBootstrap?: ChatGxserverBootstrap;
  sessionChatActivation?: SessionChatPageActivation;
  onSessionChatActivate?: (activation: SessionChatPageActivation) => void;
  onSessionChatDeactivate?: (generation: string) => void;
  /**
   * Absolute paths of the OS drag currently over this page, written by the
   * Rust shell's CEF drag handler at drag-enter (and cleared by non-file
   * drags). Chromium never exposes `File.path` to the page, so this is the
   * only way a drop resolves to real local paths.
   */
  sessionChatDropPaths?: unknown;
  sessionChatPaneFocused?: boolean;
  onSessionChatPaneFocusChanged?: (focused: boolean) => void;
  onGxserverBootstrapChanged?: (bootstrap: ChatGxserverBootstrap) => void;
  onSessionChatFocusComposerRequested?: () => void;
  onSessionChatEvictionProbeRequested?: (nonce: string) => void;
  onSessionChatHandoffToTerminalRequested?: () => void;
  onSessionChatInsertPromptRequested?: (payload: SessionChatDraftHandoff) => void;
  onSessionChatStashPromptRequested?: () => void;
  onSessionChatExtensionRequested?: (payload: GhostexChatBarPanelToggleMessage) => void;
  onSessionChatExtensionBridgeMessage?: (payload: unknown) => void;
  onSessionChatExtensionContextChanged?: (context: GhostexExtensionContext) => void;
}

export interface SessionChatPageActivation {
  url: string;
  generation: string;
  bootstrap: ChatGxserverBootstrap;
}
export function chatBridgeNamespace(): ChatBridgeNamespace {
  const target = window as unknown as { ghostexGpui?: ChatBridgeNamespace };
  target.ghostexGpui = target.ghostexGpui ?? {};
  return target.ghostexGpui;
}

export function validatedBootstrap(
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

async function rpc<TResult>(
  bootstrap: { authToken: string; baseUrl: string },
  path: string,
  params: Record<string, unknown>,
  signal?: AbortSignal
): Promise<TResult> {
  const response = await fetch(`${bootstrap.baseUrl}${path}`, {
    ...(signal ? { signal } : {}),
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

/**
 * CDXC:SessionChat 2026-09-12 DECISION:
 * User approved reusing loaded chat pages across threads and projects while retaining each conversation independently.
 * Every activation captures its own native generation so delayed callbacks keep their original session owner after the page is reused.
 */
export function activateSessionChatPage(root: ReturnType<typeof createRoot>, activation: SessionChatPageActivation) {
  const candidateBootstrap = validatedBootstrap(activation.bootstrap);
  if (!candidateBootstrap) throw new Error('The chat activation has no valid server connection.');
  const searchParams = new URL(activation.url, window.location.href).searchParams;
  const machineId = searchParams.get('remoteMachineId') || 'local';
  const bootstrap = retainSessionChatRuntimeEndpoint(machineId, candidateBootstrap);
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
  let GPUI_SESSION_CHAT_HOST_ACTIONS = createGpuiSessionChatHostActions(hotkeysValue);
  let hideAccountEmails = searchParams.get('hideAccountEmails') === 'true';
  let chatThemeSetting = normalizeSessionChatTheme(searchParams.get('theme'));
  let chatTheme = resolveSessionChatTheme(chatThemeSetting);
  let chatFontFamily = searchParams.get('fontFamily')?.trim() ?? '';
  let chatCustomTranscriptWidthEnabled = searchParams.get('customTranscriptWidthEnabled') === 'true';
  let chatTranscriptWidthPercent = clampSessionChatTranscriptWidthPercent(
    Number(searchParams.get('transcriptWidthPercent')) || DEFAULT_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT
  );
  let chatFileEditPreviews = searchParams.get('fileEditPreviews') === 'true';
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
  window.ghostexSetHideAccountEmails = (value) => {
    hideAccountEmails = value === true;
    renderReadyChat?.(chatTheme);
  };
  window.ghostexSetSessionChatTheme = (value) => {
    chatThemeSetting = normalizeSessionChatTheme(value);
    chatTheme = resolveSessionChatTheme(chatThemeSetting);
    applyDocumentChatTheme(chatTheme);
    renderReadyChat?.(chatTheme);
  };
  const unsubscribeTheme = subscribeSystemChatTheme(() => {
    if (chatThemeSetting !== 'system') return;
    chatTheme = resolveSessionChatTheme(chatThemeSetting);
    applyDocumentChatTheme(chatTheme);
    if (renderReadyChat) renderReadyChat(chatTheme);
  });
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
  window.ghostexSetSessionChatFileEditPreviews = (value) => {
    chatFileEditPreviews = value === true;
    renderReadyChat?.(chatTheme);
  };
  window.ghostexSetSessionChatHotkeys = (value) => {
    hotkeysValue = value;
    GPUI_SESSION_CHAT_HOST_ACTIONS = createGpuiSessionChatHostActions(value);
    renderReadyChat?.(chatTheme);
  };
  window.ghostexSetSessionChatVerboseMode = (value) => {
    chatVerboseMode = value === true;
    renderReadyChat?.(chatTheme);
  };

  function createGpuiSessionChatTransport(
    bootstrap: { authToken: string; baseUrl: string },
    projectId: string,
    sessionId: string,
    remote: boolean
  ): Omit<SessionChatTransport, 'subscribe'> {
    return {
      accounts: createAccountSwitchTransport(
        (params) => rpc(bootstrap, '/api/agentAccounts', { ...params, projectId, sessionId }),
        (progress) => {
          postSessionChatHostAction('accountSwitchProgress', { progress });
        }
      ),
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
      readSubagent(params) {
        return rpc<GxserverReadSessionChatResult>(bootstrap, '/api/readSessionChat', {
          ...params,
          projectId,
          sessionId,
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
          defer: params.defer,
          options: params.options,
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
      async send(text, imagePaths, draftVersion) {
        return rpc<{ queuedPromptId?: string }>(bootstrap, '/api/sendSessionChatMessage', {
          projectId,
          sessionId,
          text,
          draftVersion,
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
          draftVersion: params.draftVersion,
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
        return rpc<GxserverSetSessionChatDraftResult>(bootstrap, '/api/setSessionChatDraft', {
          clientId: params.clientId,
          content: params.content,
          draftVersion: params.draftVersion,
          projectId,
          sessionId,
        });
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
  const postSessionChatDiagnosticLog = createSessionChatDiagnosticRecorder((event, details) => {
    postSessionChatHostAction('diagnosticLog', { details: details ?? {}, event });
  });

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
            pageGeneration: activation.generation,
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
      providesPaneFocus: true,
      register(actions) {
        const namespace = chatBridgeNamespace();
        const receiving = new Set<string>();
        const received = new Set<string>();
        const insertPrompt = (payload: SessionChatDraftHandoff): void => {
          if (payload.handoffId) {
            if (receiving.has(payload.handoffId)) return;
            const id = payload.handoffId;
            receiving.add(id);
            void (received.has(id) ? Promise.resolve() : actions.receiveDraftHandoff(payload))
              .then(async () => {
                received.add(id);
                await rpc(bootstrap, '/api/acknowledgeSessionChatDraftHandoff', {
                  projectId,
                  sessionId,
                  handoffId: payload.handoffId,
                });
                postSessionChatHostAction('draftHandoffToChatComplete', { handoffId: payload.handoffId });
              })
              .catch(() => {
                postSessionChatHostAction('draftHandoffToChatRetry', { handoffId: payload.handoffId });
              })
              .finally(() => receiving.delete(id));
            return;
          }
          if (typeof payload?.content === 'string' && payload.content.length > 0) {
            if (payload.append) actions.appendPrompt(payload.content);
            else actions.insertPrompt(payload.content);
          }
        };
        let registered = true;
        let evictionRequest: AbortController | undefined;
        /**
         * CDXC:SessionChat 2026-09-05 WHY:
         * An old empty report cannot authorize destroying a page after a final edit or pending attachment operation.
         * Read current provider activity through this page's own bootstrap, then sample the live composer after the await; unknown or unreachable state protects the page.
         */
        const requestEvictionProbe = (nonce: string): void => {
          evictionRequest?.abort();
          const reply = (allowed: boolean): void => {
            if (registered) {
              postSessionChatHostAction('composerEvictionState', { allowed, nonce });
            }
          };
          if (!actions.canRelease()) {
            reply(false);
            return;
          }
          const request = new AbortController();
          evictionRequest = request;
          const timeout = window.setTimeout(() => request.abort(), 2_500);
          void rpc<{ snapshot: GxserverPresentationSnapshot }>(
            bootstrap,
            '/api/readPresentationSnapshot',
            {},
            request.signal
          )
            .then(({ snapshot }) => {
              const session = snapshot.sessions.find(
                (candidate) => candidate.projectId === projectId && candidate.sessionId === sessionId
              );
              reply(
                session?.activity === 'idle' &&
                  session.delayedSendDeadlineAt === undefined &&
                  session.delayedSendRemainingMs === undefined &&
                  (session.queuedPromptCount ?? 0) === 0 &&
                  actions.canRelease()
              );
            })
            .catch(() => reply(false))
            .finally(() => {
              window.clearTimeout(timeout);
              if (evictionRequest === request) {
                evictionRequest = undefined;
              }
            });
        };
        const requestFocus = (): void => actions.focus();
        const updatePaneFocus = (focused: boolean): void => actions.setPaneFocused(focused);
        const requestHandoffToTerminal = (): void => {
          void actions
            .handoffToTerminal()
            .then((handoff) => {
              postSessionChatHostAction('draftHandoffToTerminalComplete', {
                content: handoff.content,
                handoffId: handoff.handoffId,
                draftVersion: handoff.draftVersion,
                stashedPromptId: handoff.stashedPromptId ?? '',
              });
            })
            .catch(() => {
              postSessionChatHostAction('draftHandoffToTerminalFailed');
            });
        };
        const requestStash = (): void => actions.requestStash();
        namespace.onSessionChatEvictionProbeRequested = requestEvictionProbe;
        namespace.onSessionChatFocusComposerRequested = requestFocus;
        namespace.onSessionChatPaneFocusChanged = updatePaneFocus;
        updatePaneFocus(namespace.sessionChatPaneFocused === true);
        namespace.onSessionChatHandoffToTerminalRequested = requestHandoffToTerminal;
        namespace.onSessionChatInsertPromptRequested = insertPrompt;
        namespace.onSessionChatStashPromptRequested = requestStash;
        postSessionChatHostAction('composerReady');
        return () => {
          registered = false;
          evictionRequest?.abort();
          if (namespace.onSessionChatEvictionProbeRequested === requestEvictionProbe) {
            delete namespace.onSessionChatEvictionProbeRequested;
          }
          if (namespace.onSessionChatFocusComposerRequested === requestFocus) {
            delete namespace.onSessionChatFocusComposerRequested;
          }
          if (namespace.onSessionChatPaneFocusChanged === updatePaneFocus) {
            delete namespace.onSessionChatPaneFocusChanged;
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
          includeRecovery: false,
          includeDelivered: false,
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
    const requestId = `image-save-${activation.generation}-${imageSaveSequence}`;
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
    const requestId = `attach-${activation.generation}-${attachmentPickSequence}`;
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
        { id: 'splitSessionRight', label: 'Split Right', shortcut: shortcut('splitSessionRight') },
        { id: 'fork', label: 'Fork Session', shortcut: shortcut('forkSession') },
        {
          id: 'fullReload',
          label: 'Full Reload',
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

  const GpuiSessionChatPage = createGpuiSessionChatPage({
    rpc,
    chatBridgeNamespace,
    appModalHostMessageHandler,
    postSessionChatHostAction,
    postSessionChatDiagnosticLog,
    pageGeneration: activation.generation,
    searchParams,
    getSettings: () => ({
      hideAccountEmails,
      chatCustomTranscriptWidthEnabled,
      hotkeysValue,
      GPUI_SESSION_CHAT_HOST_ACTIONS,
      GPUI_SESSION_CHAT_HOST_LINKS,
      chatVerboseMode,
      chatFileEditPreviews,
      remote,
    }),
  });
  const transport = retainSessionChatTransport(
    { machineId, projectId, sessionId },
    createGpuiSessionChatTransport(bootstrap, projectId, sessionId, remote),
    bootstrap
  );
  const composerBridge = createGpuiSessionChatComposerBridge(bootstrap, projectId, sessionId);
  const agentLabel = agentId ? (resolveSessionChatDisplayAgent(agentId) ?? agentId) : null;
  renderReadyChat = (theme) => {
    root.render(
      <GpuiSessionChatPage
        key={`${machineId}:${projectId}:${sessionId}:${activation.generation}`}
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
  return {
    bootstrap,
    dispose: () => {
      renderReadyChat = null;
      unsubscribeTheme();
    },
  };
}
