import { useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import { createRoot } from 'react-dom/client';
import './session-chat.css';
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
  type GxserverSessionChatSnapshotEvent,
  type SessionChatQueuedPrompt,
  type SessionChatTheme,
} from '@/packages/shared/session-chat';
import { GXSERVER_PROTOCOL_VERSION } from '@/packages/shared/gxserver-protocol';
import type {
  GxserverListStashedPromptsResult,
  GxserverDeleteStashedPromptTagResult,
  GxserverSaveStashedPromptResult,
  GxserverSaveStashedPromptTagResult,
  GxserverSetStashedPromptTagsResult,
} from '@/packages/shared/gxserver-protocol';
import { createGxserverPresentationProjectSessionId } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import {
  SessionChatView,
  type SessionChatComposerHandoff,
  type SessionChatHostComposerBridge,
  type SessionChatHostSearchBridge,
  type SessionChatHostSessionNoteBridge,
} from '@/packages/core-ui/chat/session-chat-view';
import { clearStoredSessionChatDraftIfUnchanged } from '@/packages/core-ui/chat/session-chat-draft-storage';
import type { SessionChatTransport } from '@/packages/core-ui/chat/session-chat-transport';
import { StashedPromptsModal } from '@/packages/core-ui/stashed-prompts-modal';
import type { WebviewApi } from '@/packages/core-ui/webview-api';

/*
CDXC:SessionChatMobileWebview 2026-07-31:
Session Chat page for the React Native app, bundled by
tooling/build-mobile-chat.mjs into one self-contained HTML string the app
loads in a react-native-webview. It mounts the same shared SessionChatView as
gpui's chat.html and the web app; only the transport differs. The phone has
no HTTP path to gxserver (SSH only), so every transport call crosses a
postMessage bridge to React Native, which SSH-execs the matching `ghostex
session-chat` CLI verb on the machine. Live updates come from this page's own
long-poll loop (readSessionChat --wait-ms/--fingerprint) re-emitted as
synthetic sessionChatSnapshot frames; the RN side stays a dumb verb runner so
all chat behavior lives in shared code.

Bridge contract (mirrored by mobile/src/chat/session-chat-bridge.ts):
- page → RN: window.ReactNativeWebView.postMessage(JSON.stringify(
    { id, op: "read" | "readSkills" | "readFiles" | "send" | "sendKey"
        | "switchToTerminalForAgentPicker" | "answerPrompt" | "interrupt"
        | "saveImage" | "saveAttachment" | "loadImage"
        | "queuePrompt" | "updateQueuedPrompt"
        | "removeQueuedPrompt" | "reorderQueue" | "sendQueuedPrompt"
        | "setDraft" | "sessionNoteRead" | "sessionNoteSave"
        | "savedPrompts" | "jumpToSavedPromptSession",
      params }))
- page → RN notice (no id, no answer):
  { notice: "queueCount", count } — see reportQueueCount below
  { notice: "draftHandoffToTerminal", content, promptId? } — the answer to
    ghostexMobileChatHandoffToTerminal below
- RN → page: window.ghostexMobileChatDeliver({ id, ok, result?, error? })
- RN config (injected before content loads):
  window.__ghostexMobileChatConfig = {
    acknowledgedDraft?, agentId?, projectId?, sessionId?, sessionKey?, theme?, fontFamily?,
    customTranscriptWidthEnabled?, transcriptWidthPercent?, verboseMode?
  }
- RN presentation updates (pushed when mobile settings change):
  window.ghostexMobileChatSetPresentation({
    theme?, fontFamily?, customTranscriptWidthEnabled?, transcriptWidthPercent?, verboseMode?
  })
- RN host state (pushed on every change, may arrive before or after mount):
  window.ghostexMobileChatSetHostState({ working? })
- RN terminal-draft transfer (pushed when the user switches into chat and the
  agent CLI's composer held text): window.ghostexMobileChatInsertDraft(content)
- RN late send acknowledgment (pushed when this replacement page mounted before
  the previous page's send completed): window.ghostexMobileChatAcknowledgeDraft(content)
- RN chat-draft handoff (pushed when the user switches out to the terminal):
  window.ghostexMobileChatHandoffToTerminal()
- RN transcript search (the phone's entry point is the terminal header's
  overflow menu, not a button on this page):
  window.ghostexMobileChatOpenSearch()
- RN shared-action entry points:
  window.ghostexMobileChatOpenSessionNote()
  window.ghostexMobileChatOpenSavedPrompts()
*/

interface MobileChatConfig {
  /** A send that completed after the previous WebView had already unmounted. */
  acknowledgedDraft?: string;
  agentId?: string;
  customTranscriptWidthEnabled?: boolean;
  fontFamily?: string;
  projectId?: string;
  sessionId?: string;
  sessionKey?: string;
  theme?: SessionChatTheme;
  transcriptWidthPercent?: number;
  verboseMode?: boolean;
}

interface MobileChatPresentation {
  customTranscriptWidthEnabled: boolean;
  fontFamily: string;
  theme: SessionChatTheme;
  transcriptWidthPercent: number;
  verboseMode: boolean;
}

interface BridgeResponse {
  id: number;
  ok: boolean;
  result?: unknown;
  error?: string;
}

/*
Live working state the page cannot see for itself: the RN app polls the
machine inventory (`ghostex sessions --mobile-summary`) and pushes that signal
in. Input availability is deliberately not mirrored from the inventory: it is
a periodic snapshot and must not lock a usable composer while it is stale.
*/
interface MobileChatHostState {
  working: boolean;
}

type BridgeOp =
  | 'read'
  | 'readSkills'
  | 'readFiles'
  | 'send'
  | 'sendKey'
  | 'switchToTerminalForAgentPicker'
  | 'answerPrompt'
  | 'interrupt'
  | 'saveImage'
  | 'saveAttachment'
  | 'loadImage'
  | 'queuePrompt'
  | 'updateQueuedPrompt'
  | 'removeQueuedPrompt'
  | 'reorderQueue'
  | 'sendQueuedPrompt'
  | 'setDraft'
  | 'sessionNoteRead'
  | 'sessionNoteSave'
  | 'savedPrompts'
  | 'jumpToSavedPromptSession';

const CONFIG_RETRY_DELAY_MS = 100;
const CONFIG_MAX_ATTEMPTS = 100;
const BRIDGE_CALL_TIMEOUT_MS = 90_000;
const LONG_POLL_WAIT_MS = 20_000;
const SUBSCRIBE_ERROR_RETRY_MS = 3_000;
/*
A daemon older than the fingerprint long-poll answers reads immediately and
without a fingerprint. Pacing those iterations is a hot-loop guard for that
version skew, not a feature fallback: chat still works, at plain-poll latency,
until the machine's Ghostex is updated.
*/
const NO_FINGERPRINT_POLL_DELAY_MS = 3_000;
const MIN_TRANSCRIPT_WIDTH_PERCENT = 50;
const MAX_TRANSCRIPT_WIDTH_PERCENT = 100;
const TRANSCRIPT_WIDTH_PERCENT_STEP = 5;
const DEFAULT_TRANSCRIPT_WIDTH_PERCENT = 100;

declare global {
  interface Window {
    ReactNativeWebView?: { postMessage(message: string): void };
    ghostexMobileChatDeliver?: (response: BridgeResponse) => void;
    ghostexMobileChatSetPresentation?: (state: Partial<MobileChatPresentation>) => void;
    ghostexMobileChatSetHostState?: (state: Partial<MobileChatHostState>) => void;
    ghostexMobileChatInsertDraft?: (content: string) => void;
    ghostexMobileChatAcknowledgeDraft?: (content: string) => void;
    ghostexMobileChatHandoffToTerminal?: () => void;
    ghostexMobileChatOpenSearch?: () => void;
    ghostexMobileChatOpenSessionNote?: () => void;
    ghostexMobileChatOpenSavedPrompts?: () => void;
    __ghostexMobileChatConfig?: MobileChatConfig;
    __ghostexMobileChatPendingAcknowledgedDraft?: string;
  }
}

let hostState: MobileChatHostState = { working: false };
const hostStateListeners = new Set<() => void>();

function subscribeHostState(listener: () => void): () => void {
  hostStateListeners.add(listener);
  return () => {
    hostStateListeners.delete(listener);
  };
}

function readHostState(): MobileChatHostState {
  return hostState;
}

/*
CDXC:SessionChatDraftHandoff 2026-08-18:
Text the user typed into the agent CLI follows them into this composer when
they switch views. RN owns the capture (it is a slow SSH round trip through the
daemon's Ctrl+G handshake) and pushes the result here. The hook is installed at
module scope, before React mounts, with a pending box: a transfer that lands
before the composer registers is held rather than dropped.
*/
let insertDraftIntoComposer: ((content: string) => boolean) | null = null;
let pendingComposerDraft = '';
let clearAcknowledgedComposerDraft: ((expectedContent: string) => boolean) | null = null;
let pendingAcknowledgedComposerDraft = '';

window.ghostexMobileChatInsertDraft = (content) => {
  if (typeof content !== 'string' || content.length === 0) {
    return;
  }
  if (insertDraftIntoComposer?.(content) === true) {
    return;
  }
  pendingComposerDraft = content;
};

window.ghostexMobileChatAcknowledgeDraft = (content) => {
  if (typeof content !== 'string' || content.length === 0) {
    return;
  }
  if (clearAcknowledgedComposerDraft !== null) {
    clearAcknowledgedComposerDraft(content);
    return;
  }
  pendingAcknowledgedComposerDraft = content;
};

const earlyAcknowledgedDraft = window.__ghostexMobileChatPendingAcknowledgedDraft;
if (typeof earlyAcknowledgedDraft === 'string') {
  window.__ghostexMobileChatPendingAcknowledgedDraft = undefined;
  window.ghostexMobileChatAcknowledgeDraft(earlyAcknowledgedDraft);
}

/*
The same rule in the other direction. The shared composer parks the draft in
Saved Prompts before it lets go of it, so the notice below only ever describes
text that is already durable on the machine: RN types it into the agent CLI and
drops the parking row once the terminal has taken it. Nothing is posted when
there is no composer registered yet, when it held nothing, or when the handoff
failed — all three mean the terminal must be left alone, and the text (if any)
stays exactly where the user typed it.
*/
let handoffComposerDraftToTerminal: (() => Promise<SessionChatComposerHandoff>) | null = null;

window.ghostexMobileChatHandoffToTerminal = () => {
  void handoffComposerDraftToTerminal?.()
    .then((handoff) => {
      if (handoff.content.length === 0) {
        return;
      }
      window.ReactNativeWebView?.postMessage(
        JSON.stringify({
          content: handoff.content,
          notice: 'draftHandoffToTerminal',
          ...(handoff.stashedPromptId ? { promptId: handoff.stashedPromptId } : {}),
        })
      );
    })
    .catch(() => undefined);
};

function createMobileComposerBridge(
  projectId: string | undefined,
  sessionId: string | undefined,
  showStashedPrompts: () => void
): SessionChatHostComposerBridge {
  return {
    register(actions) {
      insertDraftIntoComposer = actions.insertPrompt;
      clearAcknowledgedComposerDraft = actions.clearDraft;
      handoffComposerDraftToTerminal = actions.handoffToTerminal;
      if (pendingComposerDraft.length > 0 && actions.insertPrompt(pendingComposerDraft)) {
        pendingComposerDraft = '';
      }
      if (pendingAcknowledgedComposerDraft.length > 0) {
        actions.clearDraft(pendingAcknowledgedComposerDraft);
        pendingAcknowledgedComposerDraft = '';
      }
      return () => {
        if (insertDraftIntoComposer === actions.insertPrompt) {
          insertDraftIntoComposer = null;
        }
        if (handoffComposerDraftToTerminal === actions.handoffToTerminal) {
          handoffComposerDraftToTerminal = null;
        }
        if (clearAcknowledgedComposerDraft === actions.clearDraft) {
          clearAcknowledgedComposerDraft = null;
        }
      };
    },
    async stashPrompt(content, options) {
      const result = await bridgeCall<GxserverSaveStashedPromptResult>('savedPrompts', {
        action: 'save',
        payload: {
          content,
          ...(projectId ? { projectId } : {}),
          ...(sessionId ? { sessionId } : {}),
        },
      });
      const promptId = result.prompt?.promptId;
      return options?.transient && result.created && promptId ? { promptId } : {};
    },
    async countSessionStashedPrompts(agentSessionId) {
      const result = await bridgeCall<GxserverListStashedPromptsResult>('savedPrompts', {
        action: 'list',
        payload: projectId ? { projectId } : {},
      });
      return result.prompts.filter(
        (prompt) =>
          (agentSessionId !== null && prompt.agentSessionId === agentSessionId) ||
          (sessionId !== undefined && prompt.sessionId === sessionId)
      ).length;
    },
    showStashedPrompts,
  };
}

/*
Transcript search is opened from the app's own chrome (the terminal header's
⋯ menu), so the chat page shows no search button of its own. Same pending-box
shape as the draft handoff above: a request that lands before the search box
has registered opens it as soon as it mounts instead of being dropped.
*/
let openChatSearch: (() => void) | null = null;
let pendingSearchOpen = false;

window.ghostexMobileChatOpenSearch = () => {
  if (openChatSearch === null) {
    pendingSearchOpen = true;
    return;
  }
  openChatSearch();
};

const mobileSearchBridge: SessionChatHostSearchBridge = {
  register(actions) {
    openChatSearch = actions.open;
    if (pendingSearchOpen) {
      pendingSearchOpen = false;
      actions.open();
    }
    return () => {
      if (openChatSearch === actions.open) {
        openChatSearch = null;
      }
    };
  },
};

let openSessionNote: (() => void) | null = null;
let pendingSessionNoteOpen = false;

window.ghostexMobileChatOpenSessionNote = () => {
  if (openSessionNote === null) {
    pendingSessionNoteOpen = true;
    return;
  }
  openSessionNote();
};

const mobileSessionNoteBridge: SessionChatHostSessionNoteBridge = {
  register(actions) {
    openSessionNote = actions.open;
    if (pendingSessionNoteOpen) {
      pendingSessionNoteOpen = false;
      actions.open();
    }
    return () => {
      if (openSessionNote === actions.open) {
        openSessionNote = null;
      }
    };
  },
};

let openSavedPrompts: (() => void) | null = null;
let pendingSavedPromptsOpen = false;

window.ghostexMobileChatOpenSavedPrompts = () => {
  if (openSavedPrompts === null) {
    pendingSavedPromptsOpen = true;
    return;
  }
  openSavedPrompts();
};

window.ghostexMobileChatSetHostState = (state) => {
  const next: MobileChatHostState = {
    working: typeof state?.working === 'boolean' ? state.working : hostState.working,
  };
  if (next.working === hostState.working) {
    return;
  }
  hostState = next;
  for (const listener of hostStateListeners) {
    listener();
  }
};

function clampTranscriptWidthPercent(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_TRANSCRIPT_WIDTH_PERCENT;
  const clamped = Math.min(MAX_TRANSCRIPT_WIDTH_PERCENT, Math.max(MIN_TRANSCRIPT_WIDTH_PERCENT, value));
  return Math.round(clamped / TRANSCRIPT_WIDTH_PERCENT_STEP) * TRANSCRIPT_WIDTH_PERCENT_STEP;
}

let presentationState: MobileChatPresentation = {
  customTranscriptWidthEnabled: false,
  fontFamily: '',
  theme: 'dark',
  transcriptWidthPercent: DEFAULT_TRANSCRIPT_WIDTH_PERCENT,
  verboseMode: false,
};
const presentationListeners = new Set<() => void>();

function subscribePresentation(listener: () => void): () => void {
  presentationListeners.add(listener);
  return () => {
    presentationListeners.delete(listener);
  };
}

function readPresentation(): MobileChatPresentation {
  return presentationState;
}

function applyDocumentPresentation(presentation: MobileChatPresentation): void {
  const background = presentation.theme === 'light' ? '#fdfdfd' : '#0d0d0d';
  document.documentElement.style.colorScheme = presentation.theme;
  document.documentElement.style.backgroundColor = background;
  // CDXC:SessionChatTypeScale 2026-08-22: unset, not a written-out fallback —
  // the shared sheet owns the default face (see chat-main.tsx).
  if (presentation.fontFamily) {
    document.documentElement.style.setProperty('--ghostex-session-chat-font-family', presentation.fontFamily);
  } else {
    document.documentElement.style.removeProperty('--ghostex-session-chat-font-family');
  }
  document.documentElement.style.setProperty(
    '--ghostex-session-chat-transcript-width-percent',
    String(presentation.transcriptWidthPercent)
  );
  document.body.style.backgroundColor = background;
  window.dispatchEvent(new Event('ghostex-session-chat-font-family-changed'));
}

window.ghostexMobileChatSetPresentation = (state) => {
  const next: MobileChatPresentation = {
    customTranscriptWidthEnabled:
      typeof state?.customTranscriptWidthEnabled === 'boolean'
        ? state.customTranscriptWidthEnabled
        : presentationState.customTranscriptWidthEnabled,
    fontFamily: typeof state?.fontFamily === 'string' ? state.fontFamily.trim() : presentationState.fontFamily,
    theme: state?.theme === 'dark' || state?.theme === 'light' ? state.theme : presentationState.theme,
    transcriptWidthPercent:
      typeof state?.transcriptWidthPercent === 'number'
        ? clampTranscriptWidthPercent(state.transcriptWidthPercent)
        : presentationState.transcriptWidthPercent,
    verboseMode: typeof state?.verboseMode === 'boolean' ? state.verboseMode : presentationState.verboseMode,
  };
  if (
    next.customTranscriptWidthEnabled === presentationState.customTranscriptWidthEnabled &&
    next.fontFamily === presentationState.fontFamily &&
    next.theme === presentationState.theme &&
    next.transcriptWidthPercent === presentationState.transcriptWidthPercent &&
    next.verboseMode === presentationState.verboseMode
  ) {
    return;
  }
  presentationState = next;
  applyDocumentPresentation(next);
  for (const listener of presentationListeners) {
    listener();
  }
};

const pendingCalls = new Map<
  number,
  { resolve: (result: unknown) => void; reject: (error: Error) => void; timer: number }
>();
let nextCallId = 1;

window.ghostexMobileChatDeliver = (response) => {
  const entry = pendingCalls.get(response?.id);
  if (!entry) {
    return;
  }
  pendingCalls.delete(response.id);
  window.clearTimeout(entry.timer);
  if (response.ok) {
    entry.resolve(response.result);
  } else {
    entry.reject(new Error(response.error || 'The Ghostex bridge call failed.'));
  }
};

function bridgeCall<TResult>(op: BridgeOp, params?: Record<string, unknown>): Promise<TResult> {
  return new Promise<TResult>((resolve, reject) => {
    const host = window.ReactNativeWebView;
    if (!host) {
      reject(new Error('This chat page is not hosted by the Ghostex app.'));
      return;
    }
    const id = nextCallId;
    nextCallId += 1;
    const timer = window.setTimeout(() => {
      pendingCalls.delete(id);
      reject(new Error('The Ghostex bridge call timed out.'));
    }, BRIDGE_CALL_TIMEOUT_MS);
    pendingCalls.set(id, {
      reject,
      resolve: (result) => resolve(result as TResult),
      timer,
    });
    host.postMessage(JSON.stringify({ id, op, params: params ?? {} }));
  });
}

function deliverSavedPromptsMessage(data: Record<string, unknown>): void {
  window.dispatchEvent(new MessageEvent('message', { data }));
}

function savedPromptsPayload(message: Record<string, unknown>): Record<string, unknown> {
  const { requestId: _requestId, type: _type, ...payload } = message;
  return payload;
}

/*
CDXC:MobileSavedPrompts 2026-08-26:
The desktop modal speaks the VS Code sidebar message contract. This adapter
translates only the host transport: requests become an allowlisted mobile
bridge call, then the daemon's answer is re-emitted as the same window message
the component receives on desktop. Prompt bodies never enter RN state or logs.
*/
function createMobileSavedPromptsApi(): WebviewApi {
  return {
    postMessage(message) {
      const record = message as unknown as Record<string, unknown>;
      switch (message.type) {
        case 'requestStashedPrompts':
          void bridgeCall<GxserverListStashedPromptsResult>('savedPrompts', {
            action: 'list',
            payload: {},
          }).then((result) => {
            deliverSavedPromptsMessage({
              prompts: [...result.prompts],
              requestId: message.requestId,
              tags: result.tags ? [...result.tags] : [],
              type: 'stashedPromptsResult',
            });
          });
          return;
        case 'saveStashedPrompt':
          void bridgeCall<GxserverSaveStashedPromptResult>('savedPrompts', {
            action: 'save',
            payload: savedPromptsPayload(record),
          })
            .then((result) => {
              deliverSavedPromptsMessage({
                ok: true,
                prompt: result.prompt,
                requestId: message.requestId,
                type: 'saveStashedPromptResult',
              });
            })
            .catch((error: unknown) => {
              deliverSavedPromptsMessage({
                error: error instanceof Error ? error.message : String(error),
                ok: false,
                requestId: message.requestId,
                type: 'saveStashedPromptResult',
              });
            });
          return;
        case 'deleteStashedPrompt':
          void bridgeCall('savedPrompts', {
            action: 'delete',
            payload: savedPromptsPayload(record),
          });
          return;
        case 'saveStashedPromptTag':
          void bridgeCall<GxserverSaveStashedPromptTagResult>('savedPrompts', {
            action: 'save-tag',
            payload: savedPromptsPayload(record),
          })
            .then((result) => {
              deliverSavedPromptsMessage({
                ok: true,
                requestId: message.requestId,
                tags: [...result.tags],
                type: 'stashedPromptTagsResult',
              });
            })
            .catch((error: unknown) => {
              deliverSavedPromptsMessage({
                error: error instanceof Error ? error.message : String(error),
                ok: false,
                requestId: message.requestId,
                tags: [],
                type: 'stashedPromptTagsResult',
              });
            });
          return;
        case 'deleteStashedPromptTag':
          void bridgeCall<GxserverDeleteStashedPromptTagResult>('savedPrompts', {
            action: 'delete-tag',
            payload: savedPromptsPayload(record),
          })
            .then((result) => {
              deliverSavedPromptsMessage({
                deletedTagId: message.tagId,
                ok: true,
                requestId: message.requestId,
                tags: [...result.tags],
                type: 'stashedPromptTagsResult',
              });
            })
            .catch((error: unknown) => {
              deliverSavedPromptsMessage({
                error: error instanceof Error ? error.message : String(error),
                ok: false,
                requestId: message.requestId,
                tags: [],
                type: 'stashedPromptTagsResult',
              });
            });
          return;
        case 'setStashedPromptTags':
          void bridgeCall<GxserverSetStashedPromptTagsResult>('savedPrompts', {
            action: 'set-tags',
            payload: savedPromptsPayload(record),
          })
            .then((result) => {
              deliverSavedPromptsMessage({
                ok: true,
                prompt: result.prompt,
                requestId: message.requestId,
                type: 'setStashedPromptTagsResult',
              });
            })
            .catch((error: unknown) => {
              deliverSavedPromptsMessage({
                error: error instanceof Error ? error.message : String(error),
                ok: false,
                requestId: message.requestId,
                type: 'setStashedPromptTagsResult',
              });
            });
          return;
        case 'insertStashedPrompt':
          window.ghostexMobileChatInsertDraft?.(message.content);
          return;
        case 'jumpToStashedPromptSession':
          void bridgeCall('jumpToSavedPromptSession', savedPromptsPayload(record));
          return;
        default:
          return;
      }
    },
  };
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

/*
CDXC:SessionChatPromptQueue 2026-08-21:
The app's terminal view carries a "Queued: N" button, and this page is the
only thing on the phone already watching that queue: every read — including
each long-poll iteration — and every queue mutation answers with the
authoritative list. Reporting the count from here keeps the app chrome live
without opening a second polling channel over SSH, and an id-less notice keeps
it out of the request/response map.

`failed` rows are excluded, mirroring the sidebar badge (presentation.rs): a
failed row is waiting on the user, not on the agent.

An ABSENT queue is a daemon that predates the feature, and reports nothing at
all: the button must stay hidden rather than claim a queue of zero.
*/
let reportedQueueCount: number | null = null;

function reportQueueCount(queue: readonly SessionChatQueuedPrompt[] | undefined): void {
  if (queue === undefined) {
    return;
  }
  const count = queue.filter((prompt) => prompt.state !== 'failed').length;
  if (count === reportedQueueCount) {
    return;
  }
  reportedQueueCount = count;
  window.ReactNativeWebView?.postMessage(JSON.stringify({ count, notice: 'queueCount' }));
}

/** Pass-through that reports whatever queue an answer happened to carry. */
function withQueueReport<TResult extends { queue?: readonly SessionChatQueuedPrompt[] }>(result: TResult): TResult {
  reportQueueCount(result.queue);
  return result;
}

function snapshotEventFromRead(result: GxserverReadSessionChatResult): GxserverSessionChatSnapshotEvent {
  return {
    beforeOffset: result.beforeOffset,
    epoch: result.epoch,
    hasMore: result.hasMore,
    // The RN host scopes the whole bridge to one session, so identity fields
    // are placeholders the (already pre-filtered) transport never checks.
    projectId: '',
    sessionId: '',
    protocolVersion: GXSERVER_PROTOCOL_VERSION,
    seq: result.seq,
    serverId: '',
    type: 'sessionChatSnapshot',
    messages: result.messages,
    ...(result.lifecycle !== undefined ? { lifecycle: result.lifecycle } : {}),
    ...(result.prompt !== undefined ? { prompt: result.prompt } : {}),
    ...(result.agentSessionId !== undefined ? { agentSessionId: result.agentSessionId } : {}),
    // Detected model/effort: this host's only live channel is the synthesized
    // snapshot, so dropping it here would hide the pills' real values.
    ...(result.selectedOptions !== undefined ? { selectedOptions: result.selectedOptions } : {}),
    // CDXC:SessionChatTerminalNotices 2026-08-19: terminal-screen state the
    // transcript can never show. Omitted means cleared, and this synthesized
    // snapshot is the host's only frame, so the omission has to survive too.
    ...(result.terminalNotice !== undefined ? { terminalNotice: result.terminalNotice } : {}),
    /*
    CDXC:SessionChatTerminalActivity 2026-08-22: the transcript's progress row,
    same carriage rule. The phone's long poll re-reads whenever the bar moves
    (the fingerprint hashes its numbers), so this is what makes a compaction
    tick on a device with no event socket at all.
    */
    ...(result.terminalActivity !== undefined ? { terminalActivity: result.terminalActivity } : {}),
    /*
    CDXC:SessionChatAgentFleet 2026-08-23: sub-agents, same carriage
    rule again. The read's fingerprint hashes the roster, so the phone's long
    poll wakes when an agent starts, finishes, or changes task.
    */
    ...(result.agentFleet !== undefined ? { agentFleet: result.agentFleet } : {}),
    /*
    CDXC:SessionChatAppCommands 2026-08-23: commands Ghostex typed into the
    agent. Same pass-through, and the phone needs it most — it is the client
    least likely to be watching the terminal when the session renames itself.
    */
    ...(result.appCommands !== undefined ? { appCommands: result.appCommands } : {}),
    /*
    CDXC:SessionChatPromptQueue 2026-08-21: the queue and the synced draft.
    Both keep the READ's semantics here, and both matter: an absent queue is
    the daemon capability probe that hides every queue control, and an absent
    draft means "unchanged", never "cleared". This synthesized snapshot is the
    host's only frame, so passing the presence/absence through verbatim is
    what makes those two rules survive on the phone.
    */
    ...(result.queue !== undefined ? { queue: result.queue } : {}),
    ...(result.draft !== undefined ? { draft: result.draft } : {}),
    status: result.status,
  };
}

function createMobileSessionChatTransport(): SessionChatTransport {
  return {
    async answerPrompt(params) {
      await bridgeCall('answerPrompt', params as unknown as Record<string, unknown>);
    },
    async interrupt() {
      await bridgeCall('interrupt');
    },
    read(params) {
      return bridgeCall<GxserverReadSessionChatResult>('read', {
        ...(params.limit !== undefined ? { limit: params.limit } : {}),
        ...(params.beforeOffset !== undefined ? { beforeOffset: params.beforeOffset } : {}),
      }).then(withQueueReport);
    },
    readSkills() {
      return bridgeCall<GxserverReadSessionChatSkillsResult>('readSkills');
    },
    readFiles() {
      return bridgeCall<GxserverReadSessionChatFilesResult>('readFiles');
    },
    /*
    Composer image paste. gxserver's saveSessionChatImage endpoint has no CLI
    verb (base64 bytes would blow past ARG_MAX on the SSH command line), so RN
    stages the bytes as a local cache file and SFTPs them into ~/.ghostex/i on
    the machine — the same directory that endpoint writes, so the path reads
    back identically to a desktop or web upload; the returned absolute path
    goes into the message as `[Image #N](path)`.
    */
    saveImage(params) {
      return bridgeCall<GxserverSaveSessionChatImageResult>('saveImage', {
        base64Data: params.base64Data,
        ...(params.suggestedName !== undefined ? { suggestedName: params.suggestedName } : {}),
      });
    },
    // Non-image attachments ride the same SFTP staging route into
    // ~/.ghostex/f; the returned machine path becomes "[File #N](path)".
    saveAttachment(params) {
      return bridgeCall<GxserverSaveSessionChatAttachmentResult>('saveAttachment', {
        base64Data: params.base64Data,
        ...(params.suggestedName !== undefined ? { suggestedName: params.suggestedName } : {}),
      });
    },
    // Machine-path image bytes for the chat-log overlay viewer (RN reads the
    // file over the machine's SSH channel).
    loadImage(params) {
      return bridgeCall<GxserverReadSessionChatImageResult>('loadImage', {
        path: params.path,
      });
    },
    async send(text, imagePaths) {
      await bridgeCall('send', {
        text,
        ...(imagePaths && imagePaths.length > 0 ? { imagePaths } : {}),
      });
    },
    async sendKey(key) {
      await bridgeCall('sendKey', { key });
    },
    /*
    Ghostex's prompt queue and the cross-client composer draft (plan 016).
    gxserver owns both and this host reaches them the same way it reaches
    everything else — one SSH-exec'd CLI verb each — so all six transport
    methods exist here and the shared UI shows every queue control. The
    daemon's own capability gate still applies: a machine whose Ghostex
    predates the feature omits `queue` from its reads, and the shared UI hides
    the strip no matter what this transport implements.
    */
    queuePrompt(params) {
      return bridgeCall<GxserverQueueSessionChatPromptResult>('queuePrompt', {
        text: params.text,
      }).then(withQueueReport);
    },
    updateQueuedPrompt(params) {
      return bridgeCall<GxserverSessionChatQueueResult>('updateQueuedPrompt', {
        promptId: params.promptId,
        ...(params.text !== undefined ? { text: params.text } : {}),
        ...(params.retry !== undefined ? { retry: params.retry } : {}),
      }).then(withQueueReport);
    },
    removeQueuedPrompt(params) {
      return bridgeCall<GxserverSessionChatRemoveQueuedPromptResult>('removeQueuedPrompt', {
        promptId: params.promptId,
      }).then(withQueueReport);
    },
    reorderQueue(params) {
      return bridgeCall<GxserverSessionChatQueueResult>('reorderQueue', {
        promptIds: params.promptIds,
      }).then(withQueueReport);
    },
    sendQueuedPrompt(params) {
      return bridgeCall<GxserverSendSessionChatQueuedPromptResult>('sendQueuedPrompt', {
        promptId: params.promptId,
      }).then(withQueueReport);
    },
    /*
    CDXC:SessionAgentNotes 2026-08-24:
    The session note is one more SSH-exec'd verb (`ghostex session-note
    read|save`), so it reaches the same store the desktop and web hosts write.
    The machine resolves the provider conversation id the note is filed under
    from the session selector RN already passes; the page sends only the body.
    */
    readSessionNote() {
      return bridgeCall<{ agentSessionId?: string; note?: string }>('sessionNoteRead');
    },
    async saveSessionNote(note) {
      await bridgeCall('sessionNoteSave', { note });
    },
    // The client id is minted and persisted by the shared hook; forwarding it
    // untouched is what keeps this device's own echo from reading as another
    // device and popping the conflict bar against itself.
    async setDraft(params) {
      await bridgeCall('setDraft', {
        clientId: params.clientId,
        content: params.content,
      });
    },
    subscribe({ currentLimit, onEvent }) {
      let stopped = false;
      void (async () => {
        let fingerprint: string | undefined;
        let emitted = false;
        while (!stopped) {
          let result: GxserverReadSessionChatResult;
          // This host synthesizes snapshot frames from long-poll reads, so
          // the window is a read `limit`: re-read every iteration so a long
          // live conversation is never answered with fewer rows than shown.
          const limit = currentLimit?.();
          try {
            result = await bridgeCall<GxserverReadSessionChatResult>('read', {
              ...(typeof limit === 'number' && limit > 0 ? { limit } : {}),
              ...(fingerprint !== undefined ? { fingerprint, waitMs: LONG_POLL_WAIT_MS } : {}),
            });
          } catch {
            if (!stopped) {
              await sleep(SUBSCRIBE_ERROR_RETRY_MS);
            }
            continue;
          }
          if (stopped) {
            return;
          }
          // The read fingerprint folds in the queue revision and the draft
          // stamp, so a queue change on another device wakes this long poll
          // and arrives as an ordinary snapshot — no second channel needed.
          reportQueueCount(result.queue);
          const changed = result.fingerprint === undefined || result.fingerprint !== fingerprint;
          fingerprint = result.fingerprint;
          if (changed || !emitted) {
            emitted = true;
            onEvent(snapshotEventFromRead(result));
          }
          if (result.fingerprint === undefined) {
            await sleep(NO_FINGERPRINT_POLL_DELAY_MS);
          }
        }
      })();
      return () => {
        stopped = true;
      };
    },
  };
}

function waitForConfig(): Promise<MobileChatConfig> {
  return new Promise((resolve) => {
    const poll = (attempt: number): void => {
      const config = window.__ghostexMobileChatConfig;
      if (config && typeof config === 'object') {
        resolve(config);
        return;
      }
      if (attempt >= CONFIG_MAX_ATTEMPTS) {
        resolve({});
        return;
      }
      window.setTimeout(() => poll(attempt + 1), CONFIG_RETRY_DELAY_MS);
    };
    poll(0);
  });
}

const rootElement = document.getElementById('root');
if (!rootElement) {
  throw new Error('Ghostex session chat root element was not found.');
}
document.body.dataset.sidebarTheme = 'plain-dark';
document.body.classList.add('vscode-dark', 'native-sidebar-body');
/*
The document has to carry the page's starting presentation before any host
push arrives: the setter below short-circuits when nothing changed, so a host
config that happens to match these defaults (100% transcript width, dark, app
font) would otherwise leave the CSS custom properties unset and the stylesheet
fallbacks — notably the desktop's 75% transcript width — in charge.
*/
applyDocumentPresentation(presentationState);

/*
CDXC:SessionChatMobileKeyboard 2026-08-21:
The page sizes itself off `height: 100%`, i.e. the LAYOUT viewport. A software
keyboard does not shrink that: iOS/WKWebView contracts only the VISUAL viewport,
so the page keeps its full height and the keyboard covers whatever is at the
bottom — which is exactly the interactive card's answer row, leaving "Send
answer" unreachable. The page also cannot scroll to it: the root is a fixed-
height non-scrolling grid, and the host disables webview bounce.

Track the visual viewport instead and give the shell a real height, so the
transcript gives up the space and the answer row lands directly above the
keyboard. `offsetTop` is included because iOS also scrolls the visual viewport
within the layout viewport while a focused field is being revealed. On Android
the host already resizes the webview around the IME, which leaves the two
viewports the same size and makes this a no-op rather than a double correction.
*/
function trackVisualViewportHeight(): void {
  const viewport = window.visualViewport;
  if (!viewport) {
    return;
  }
  const apply = (): void => {
    const height = Math.round(viewport.height + viewport.offsetTop);
    document.documentElement.style.setProperty('--ghostex-mobile-chat-viewport-height', `${height}px`);
  };
  viewport.addEventListener('resize', apply);
  viewport.addEventListener('scroll', apply);
  apply();
}
trackVisualViewportHeight();

function MobileSessionChat({
  agentLabel,
  projectId,
  sessionId,
  sessionKey,
  transport,
}: {
  agentLabel: string | null;
  projectId: string | undefined;
  sessionId: string | undefined;
  sessionKey: string | undefined;
  transport: SessionChatTransport;
}) {
  const { working } = useSyncExternalStore(subscribeHostState, readHostState, readHostState);
  const { customTranscriptWidthEnabled, theme, verboseMode } = useSyncExternalStore(
    subscribePresentation,
    readPresentation,
    readPresentation
  );
  const [savedPromptsOpen, setSavedPromptsOpen] = useState(false);
  const showSavedPrompts = useMemo(() => () => setSavedPromptsOpen(true), []);
  const composerBridge = useMemo(
    () => createMobileComposerBridge(projectId, sessionId, showSavedPrompts),
    [projectId, sessionId, showSavedPrompts]
  );
  const savedPromptsApi = useMemo(() => createMobileSavedPromptsApi(), []);
  const combinedSessionId = useMemo(
    () => (projectId && sessionId ? createGxserverPresentationProjectSessionId(projectId, sessionId) : undefined),
    [projectId, sessionId]
  );

  useEffect(() => {
    openSavedPrompts = showSavedPrompts;
    if (pendingSavedPromptsOpen) {
      pendingSavedPromptsOpen = false;
      showSavedPrompts();
    }
    return () => {
      if (openSavedPrompts === showSavedPrompts) {
        openSavedPrompts = null;
      }
    };
  }, [showSavedPrompts]);

  return (
    <div className='native-sidebar-shell gpui-session-chat'>
      <SessionChatView
        agentLabel={agentLabel}
        className='gpui-session-chat-view'
        hostComposerBridge={composerBridge}
        hostSearchBridge={mobileSearchBridge}
        hostSessionNoteBridge={mobileSessionNoteBridge}
        nativeSelectionMenus
        customTranscriptWidthEnabled={customTranscriptWidthEnabled}
        onSwitchToTerminalForAgentPicker={() => {
          void bridgeCall('switchToTerminalForAgentPicker');
        }}
        sendOnEnter={false}
        sessionKey={sessionKey}
        showNewSessionWelcomeTitle={false}
        searchLayout='overlay'
        theme={theme}
        transport={transport}
        verboseMode={verboseMode}
        working={working}
      />
      <StashedPromptsModal
        isOpen={savedPromptsOpen}
        onClose={() => {
          setSavedPromptsOpen(false);
          // Desktop's separate modal window naturally returns focus to chat,
          // which refreshes the stash badge. Mirror that lifecycle in this
          // same-document mobile host.
          window.dispatchEvent(new Event('focus'));
        }}
        projectId={projectId}
        sessionId={combinedSessionId}
        vscode={savedPromptsApi}
      />
    </div>
  );
}

const root = createRoot(rootElement);
void waitForConfig().then((config) => {
  const agentId = config.agentId?.trim() ?? '';
  const agentLabel = agentId ? (resolveSessionChatDisplayAgent(agentId) ?? agentId) : null;
  const projectId = config.projectId?.trim() || undefined;
  const sessionId = config.sessionId?.trim() || undefined;
  const sessionKey = config.sessionKey?.trim() || undefined;
  if (typeof config.acknowledgedDraft === 'string') {
    clearStoredSessionChatDraftIfUnchanged(sessionKey, config.acknowledgedDraft);
  }
  window.ghostexMobileChatSetPresentation?.({
    customTranscriptWidthEnabled: config.customTranscriptWidthEnabled,
    fontFamily: config.fontFamily,
    theme: normalizeSessionChatTheme(config.theme),
    transcriptWidthPercent: config.transcriptWidthPercent,
    verboseMode: config.verboseMode,
  });
  root.render(
    <MobileSessionChat
      agentLabel={agentLabel}
      projectId={projectId}
      sessionId={sessionId}
      sessionKey={sessionKey}
      transport={createMobileSessionChatTransport()}
    />
  );
});
