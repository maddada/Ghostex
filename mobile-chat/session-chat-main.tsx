import { useSyncExternalStore } from "react";
import { createRoot } from "react-dom/client";
import "./session-chat.css";
import {
  normalizeSessionChatTheme,
  resolveSessionChatTranscriptAgent,
  type GxserverReadSessionChatFilesResult,
  type GxserverReadSessionChatImageResult,
  type GxserverReadSessionChatResult,
  type GxserverReadSessionChatSkillsResult,
  type GxserverSaveSessionChatAttachmentResult,
  type GxserverSaveSessionChatImageResult,
  type GxserverSessionChatSnapshotEvent,
  type SessionChatTheme,
} from "../shared/session-chat";
import { GXSERVER_PROTOCOL_VERSION } from "../shared/gxserver-protocol";
import {
  SessionChatView,
  type SessionChatHostComposerBridge,
  type SessionChatHostSearchBridge,
} from "../sidebar/chat/session-chat-view";
import type { SessionChatTransport } from "../sidebar/chat/session-chat-transport";

/*
CDXC:SessionChatMobileWebview 2026-07-31:
Session Chat page for the React Native app, bundled by
scripts/build-mobile-chat.mjs into one self-contained HTML string the app
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
        | "saveImage" | "saveAttachment" | "loadImage",
      params }))
- RN → page: window.ghostexMobileChatDeliver({ id, ok, result?, error? })
- RN config (injected before content loads):
  window.__ghostexMobileChatConfig = {
    agentId?, sessionKey?, theme?, fontFamily?, transcriptWidthPercent?, verboseMode?
  }
- RN presentation updates (pushed when mobile settings change):
  window.ghostexMobileChatSetPresentation({
    theme?, fontFamily?, transcriptWidthPercent?, verboseMode?
  })
- RN host state (pushed on every change, may arrive before or after mount):
  window.ghostexMobileChatSetHostState({ working?, canSend? })
- RN terminal-draft transfer (pushed when the user switches into chat and the
  agent CLI's composer held text): window.ghostexMobileChatInsertDraft(content)
- RN transcript search (the phone's entry point is the terminal header's
  overflow menu, not a button on this page):
  window.ghostexMobileChatOpenSearch()
*/

interface MobileChatConfig {
  agentId?: string;
  fontFamily?: string;
  sessionKey?: string;
  theme?: SessionChatTheme;
  transcriptWidthPercent?: number;
  verboseMode?: boolean;
}

interface MobileChatPresentation {
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
Live session state the page cannot see for itself: the RN app polls the
machine inventory (`ghostex sessions --mobile-summary`) and pushes the
resulting working / can-send signals in, the same two values the desktop and
web hosts read straight off their workspace session record.
*/
interface MobileChatHostState {
  working: boolean;
  canSend: boolean;
}

type BridgeOp =
  | "read"
  | "readSkills"
  | "readFiles"
  | "send"
  | "sendKey"
  | "switchToTerminalForAgentPicker"
  | "answerPrompt"
  | "interrupt"
  | "saveImage"
  | "saveAttachment"
  | "loadImage";

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
    ghostexMobileChatOpenSearch?: () => void;
    __ghostexMobileChatConfig?: MobileChatConfig;
  }
}

let hostState: MobileChatHostState = { canSend: true, working: false };
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
let pendingComposerDraft = "";

window.ghostexMobileChatInsertDraft = (content) => {
  if (typeof content !== "string" || content.length === 0) {
    return;
  }
  if (insertDraftIntoComposer?.(content) === true) {
    return;
  }
  pendingComposerDraft = content;
};

/*
The mobile host registers composer actions but cannot stash: gxserver's stash
endpoints have no CLI verb, and this page reaches the machine only through
SSH-exec'd verbs. Omitting `stashPrompt` keeps the composer's Stash control
unrendered instead of offering a button that would always fail.
*/
const mobileComposerBridge: SessionChatHostComposerBridge = {
  register(actions) {
    insertDraftIntoComposer = actions.insertPrompt;
    if (pendingComposerDraft.length > 0 && actions.insertPrompt(pendingComposerDraft)) {
      pendingComposerDraft = "";
    }
    return () => {
      if (insertDraftIntoComposer === actions.insertPrompt) {
        insertDraftIntoComposer = null;
      }
    };
  },
};

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

window.ghostexMobileChatSetHostState = (state) => {
  const next: MobileChatHostState = {
    canSend: typeof state?.canSend === "boolean" ? state.canSend : hostState.canSend,
    working: typeof state?.working === "boolean" ? state.working : hostState.working,
  };
  if (next.canSend === hostState.canSend && next.working === hostState.working) {
    return;
  }
  hostState = next;
  for (const listener of hostStateListeners) {
    listener();
  }
};

function clampTranscriptWidthPercent(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_TRANSCRIPT_WIDTH_PERCENT;
  const clamped = Math.min(
    MAX_TRANSCRIPT_WIDTH_PERCENT,
    Math.max(MIN_TRANSCRIPT_WIDTH_PERCENT, value),
  );
  return (
    Math.round(clamped / TRANSCRIPT_WIDTH_PERCENT_STEP) * TRANSCRIPT_WIDTH_PERCENT_STEP
  );
}

let presentationState: MobileChatPresentation = {
  fontFamily: "",
  theme: "dark",
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
  const background = presentation.theme === "light" ? "#fdfdfd" : "#111111";
  document.documentElement.style.colorScheme = presentation.theme;
  document.documentElement.style.backgroundColor = background;
  document.documentElement.style.setProperty(
    "--ghostex-session-chat-font-family",
    presentation.fontFamily ||
      "var(--vscode-font-family, ui-sans-serif, system-ui, sans-serif)",
  );
  document.documentElement.style.setProperty(
    "--ghostex-session-chat-transcript-width-percent",
    String(presentation.transcriptWidthPercent),
  );
  document.body.style.backgroundColor = background;
  window.dispatchEvent(new Event("ghostex-session-chat-font-family-changed"));
}

window.ghostexMobileChatSetPresentation = (state) => {
  const next: MobileChatPresentation = {
    fontFamily:
      typeof state?.fontFamily === "string"
        ? state.fontFamily.trim()
        : presentationState.fontFamily,
    theme:
      state?.theme === "dark" || state?.theme === "light"
        ? state.theme
        : presentationState.theme,
    transcriptWidthPercent:
      typeof state?.transcriptWidthPercent === "number"
        ? clampTranscriptWidthPercent(state.transcriptWidthPercent)
        : presentationState.transcriptWidthPercent,
    verboseMode:
      typeof state?.verboseMode === "boolean"
        ? state.verboseMode
        : presentationState.verboseMode,
  };
  if (
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
    entry.reject(new Error(response.error || "The Ghostex bridge call failed."));
  }
};

function bridgeCall<TResult>(op: BridgeOp, params?: Record<string, unknown>): Promise<TResult> {
  return new Promise<TResult>((resolve, reject) => {
    const host = window.ReactNativeWebView;
    if (!host) {
      reject(new Error("This chat page is not hosted by the Ghostex app."));
      return;
    }
    const id = nextCallId;
    nextCallId += 1;
    const timer = window.setTimeout(() => {
      pendingCalls.delete(id);
      reject(new Error("The Ghostex bridge call timed out."));
    }, BRIDGE_CALL_TIMEOUT_MS);
    pendingCalls.set(id, {
      reject,
      resolve: (result) => resolve(result as TResult),
      timer,
    });
    host.postMessage(JSON.stringify({ id, op, params: params ?? {} }));
  });
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function snapshotEventFromRead(
  result: GxserverReadSessionChatResult,
): GxserverSessionChatSnapshotEvent {
  return {
    beforeOffset: result.beforeOffset,
    epoch: result.epoch,
    hasMore: result.hasMore,
    // The RN host scopes the whole bridge to one session, so identity fields
    // are placeholders the (already pre-filtered) transport never checks.
    projectId: "",
    sessionId: "",
    protocolVersion: GXSERVER_PROTOCOL_VERSION,
    seq: result.seq,
    serverId: "",
    type: "sessionChatSnapshot",
    messages: result.messages,
    ...(result.lifecycle !== undefined ? { lifecycle: result.lifecycle } : {}),
    ...(result.prompt !== undefined ? { prompt: result.prompt } : {}),
    ...(result.agentSessionId !== undefined
      ? { agentSessionId: result.agentSessionId }
      : {}),
    // Detected model/effort: this host's only live channel is the synthesized
    // snapshot, so dropping it here would hide the pills' real values.
    ...(result.selectedOptions !== undefined
      ? { selectedOptions: result.selectedOptions }
      : {}),
    // CDXC:SessionChatTerminalNotices 2026-08-19: terminal-screen state the
    // transcript can never show. Omitted means cleared, and this synthesized
    // snapshot is the host's only frame, so the omission has to survive too.
    ...(result.terminalNotice !== undefined
      ? { terminalNotice: result.terminalNotice }
      : {}),
    status: result.status,
  };
}

function createMobileSessionChatTransport(): SessionChatTransport {
  return {
    async answerPrompt(params) {
      await bridgeCall("answerPrompt", params as unknown as Record<string, unknown>);
    },
    async interrupt() {
      await bridgeCall("interrupt");
    },
    read(params) {
      return bridgeCall<GxserverReadSessionChatResult>("read", {
        ...(params.limit !== undefined ? { limit: params.limit } : {}),
        ...(params.beforeOffset !== undefined ? { beforeOffset: params.beforeOffset } : {}),
      });
    },
    readSkills() {
      return bridgeCall<GxserverReadSessionChatSkillsResult>("readSkills");
    },
    readFiles() {
      return bridgeCall<GxserverReadSessionChatFilesResult>("readFiles");
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
      return bridgeCall<GxserverSaveSessionChatImageResult>("saveImage", {
        base64Data: params.base64Data,
        ...(params.suggestedName !== undefined ? { suggestedName: params.suggestedName } : {}),
      });
    },
    // Non-image attachments ride the same SFTP staging route into
    // ~/.ghostex/f; the returned machine path becomes "[File #N](path)".
    saveAttachment(params) {
      return bridgeCall<GxserverSaveSessionChatAttachmentResult>("saveAttachment", {
        base64Data: params.base64Data,
        ...(params.suggestedName !== undefined ? { suggestedName: params.suggestedName } : {}),
      });
    },
    // Machine-path image bytes for the chat-log overlay viewer (RN reads the
    // file over the machine's SSH channel).
    loadImage(params) {
      return bridgeCall<GxserverReadSessionChatImageResult>("loadImage", {
        path: params.path,
      });
    },
    async send(text, imagePaths) {
      await bridgeCall("send", {
        text,
        ...(imagePaths && imagePaths.length > 0 ? { imagePaths } : {}),
      });
    },
    async sendKey(key) {
      await bridgeCall("sendKey", { key });
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
            result = await bridgeCall<GxserverReadSessionChatResult>("read", {
              ...(typeof limit === "number" && limit > 0 ? { limit } : {}),
              ...(fingerprint !== undefined
                ? { fingerprint, waitMs: LONG_POLL_WAIT_MS }
                : {}),
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
      if (config && typeof config === "object") {
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

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Ghostex session chat root element was not found.");
}
document.body.dataset.sidebarTheme = "plain-dark";
document.body.classList.add("vscode-dark", "native-sidebar-body");
/*
The document has to carry the page's starting presentation before any host
push arrives: the setter below short-circuits when nothing changed, so a host
config that happens to match these defaults (100% transcript width, dark, app
font) would otherwise leave the CSS custom properties unset and the stylesheet
fallbacks — notably the desktop's 75% transcript width — in charge.
*/
applyDocumentPresentation(presentationState);

function MobileSessionChat({
  agentLabel,
  sessionKey,
  transport,
}: {
  agentLabel: string | null;
  sessionKey: string | undefined;
  transport: SessionChatTransport;
}) {
  const { canSend, working } = useSyncExternalStore(
    subscribeHostState,
    readHostState,
    readHostState,
  );
  const { theme, verboseMode } = useSyncExternalStore(
    subscribePresentation,
    readPresentation,
    readPresentation,
  );
  return (
    <div className="native-sidebar-shell gpui-session-chat">
      <SessionChatView
        agentLabel={agentLabel}
        canSend={canSend}
        className="gpui-session-chat-view"
        hostComposerBridge={mobileComposerBridge}
        hostSearchBridge={mobileSearchBridge}
        onSwitchToTerminalForAgentPicker={() => {
          void bridgeCall("switchToTerminalForAgentPicker");
        }}
        sendOnEnter={false}
        sessionKey={sessionKey}
        showComposerAgentName={false}
        showNewSessionWelcomeTitle={false}
        searchLayout="overlay"
        showVerbosePill={false}
        theme={theme}
        transport={transport}
        verboseMode={verboseMode}
        working={working}
      />
    </div>
  );
}

const root = createRoot(rootElement);
void waitForConfig().then((config) => {
  const agentId = config.agentId?.trim() ?? "";
  const agentLabel = agentId ? resolveSessionChatTranscriptAgent(agentId) ?? agentId : null;
  const sessionKey = config.sessionKey?.trim() || undefined;
  window.ghostexMobileChatSetPresentation?.({
    fontFamily: config.fontFamily,
    theme: normalizeSessionChatTheme(config.theme),
    transcriptWidthPercent: config.transcriptWidthPercent,
    verboseMode: config.verboseMode,
  });
  root.render(
    <MobileSessionChat
      agentLabel={agentLabel}
      sessionKey={sessionKey}
      transport={createMobileSessionChatTransport()}
    />,
  );
});
