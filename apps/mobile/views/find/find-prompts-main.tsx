import { createRoot } from "react-dom/client";
import "./find-prompts.css";
import type {
  ReadAgentPromptTextParams,
  ReadAgentPromptTextResult,
  ResolveAgentPromptLaunchParams,
  ResolveAgentPromptLaunchResult,
  SearchAgentPromptsParams,
  SearchAgentPromptsResult,
  ToggleAgentPromptFavoriteParams,
  ToggleAgentPromptFavoriteResult,
} from "@/packages/shared/agent-prompt-search";
import { FindPromptsView } from "@/packages/core-ui/find/find-prompts-view";
import type { FindPromptsTransport } from "@/packages/core-ui/find/find-prompts-transport";

/*
CDXC:AgentHistorySearch 2026-08-20:
Find page for the React Native app — the GUI for `gx f` — bundled by
tooling/build-mobile-find.mjs into one self-contained HTML string the app loads
in a react-native-webview. It mounts the same shared FindPromptsView as gpui's
find.html and the web app; only the transport differs. The phone has no HTTP
path to gxserver (SSH only), so every transport call crosses a postMessage
bridge to React Native, which SSH-execs the matching `ghostex` CLI verb on the
machine. The RN side stays a dumb verb runner so all Find behavior lives in
shared code.

Bridge contract (mirrored by mobile/src/find/find-prompts-bridge.ts):
- page → RN: window.ReactNativeWebView.postMessage(JSON.stringify(
    { id, op: "search" | "readText" | "toggleFavorite" | "resolveLaunch"
        | "focusSession" | "launchSession" | "copyText" | "close",
      params }))
- RN → page: window.ghostexMobileFindDeliver({ id, ok, result?, error? })
- RN config (injected before content loads):
  window.__ghostexMobileFindConfig = { theme? }
*/

type BridgeOp =
  | "close"
  | "copyText"
  | "focusSession"
  | "launchSession"
  | "readText"
  | "resolveLaunch"
  | "search"
  | "toggleFavorite";

interface BridgeResponse {
  error?: string;
  id: number;
  ok: boolean;
  result?: unknown;
}

interface MobileFindConfig {
  theme?: "dark" | "light";
}

declare global {
  interface Window {
    ReactNativeWebView?: { postMessage: (payload: string) => void };
    ghostexMobileFindDeliver?: (response: BridgeResponse) => void;
    __ghostexMobileFindConfig?: MobileFindConfig;
  }
}

/*
Searching walks every agent history store on the machine, and the first call
after a cold daemon also rebuilds the Codex derived cache, so the phone's
timeout has to clear that rather than reporting a hang.
*/
const BRIDGE_CALL_TIMEOUT_MS = 60_000;

const pendingCalls = new Map<
  number,
  { reject: (error: Error) => void; resolve: (result: unknown) => void; timer: number }
>();
let nextCallId = 1;

window.ghostexMobileFindDeliver = (response) => {
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
      reject(new Error("This Find page is not hosted by the Ghostex app."));
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

const transport: FindPromptsTransport = {
  close() {
    void bridgeCall("close");
  },
  copyText(text) {
    return bridgeCall("copyText", { text });
  },
  focusSession(params) {
    return bridgeCall("focusSession", { ...params });
  },
  launchSession(plan) {
    return bridgeCall("launchSession", {
      agent: plan.agent,
      command: plan.commandLine,
      cwd: plan.cwd,
      cwdExists: plan.cwdExists,
      title: plan.title,
    });
  },
  readText(params: ReadAgentPromptTextParams) {
    return bridgeCall<ReadAgentPromptTextResult>("readText", { ...params });
  },
  resolveLaunch(params: ResolveAgentPromptLaunchParams) {
    return bridgeCall<ResolveAgentPromptLaunchResult>("resolveLaunch", { ...params });
  },
  search(params: SearchAgentPromptsParams) {
    return bridgeCall<SearchAgentPromptsResult>("search", { ...params });
  },
  toggleFavorite(params: ToggleAgentPromptFavoriteParams) {
    return bridgeCall<ToggleAgentPromptFavoriteResult>("toggleFavorite", { ...params });
  },
};

const config = window.__ghostexMobileFindConfig ?? {};
const theme = config.theme === "light" ? "light" : "dark";
document.body.dataset.sidebarTheme = theme === "light" ? "plain-light" : "plain-dark";
document.body.classList.add(theme === "light" ? "vscode-light" : "vscode-dark", "native-sidebar-body");
if (theme === "dark") {
  document.documentElement.classList.add("dark");
}
document.documentElement.style.colorScheme = theme;

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Ghostex find root element was not found.");
}
createRoot(rootElement).render(
  <div className="native-sidebar-shell gpui-find-prompts">
    <FindPromptsView transport={transport} />
  </div>,
);
