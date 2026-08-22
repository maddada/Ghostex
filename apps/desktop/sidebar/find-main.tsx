import { createRoot } from "react-dom/client";
import "@/sidebar/styles.css";
import { GXSERVER_PROTOCOL_VERSION } from "@/shared/gxserver-protocol";
import type {
  ReadAgentPromptTextParams,
  ReadAgentPromptTextResult,
  ResolveAgentPromptLaunchParams,
  ResolveAgentPromptLaunchResult,
  SearchAgentPromptsParams,
  SearchAgentPromptsResult,
  ToggleAgentPromptFavoriteParams,
  ToggleAgentPromptFavoriteResult,
} from "@/shared/agent-prompt-search";
import { FindPromptsView } from "@/sidebar/find/find-prompts-view";
import type { FindPromptsTransport } from "@/sidebar/find/find-prompts-transport";

/*
CDXC:AgentHistorySearch 2026-08-20:
find.html is the Find CEF surface — the GUI for `gx f` — that swaps with the
terminal pane body in the gpui Agents workspace. It follows chat.html exactly:
the gxserver bootstrap (baseUrl/token/protocolVersion) is installed by Rust on
window.ghostexGpui.gxserverBootstrap, the page owns its own RPCs, and workspace
actions (focus a live session, open a new one, close the surface) post back to
Rust over the app-modal-host bridge shim because only Rust can move panes.
*/

interface FindGxserverBootstrap {
  authToken?: string;
  baseUrl?: string;
  clientId?: string;
  protocolVersion?: number;
}

interface FindBridgeNamespace {
  gxserverBootstrap?: FindGxserverBootstrap;
  onGxserverBootstrapChanged?: (bootstrap: FindGxserverBootstrap) => void;
}

const BOOTSTRAP_RETRY_DELAY_MS = 120;
const BOOTSTRAP_MAX_ATTEMPTS = 250;

function findBridgeNamespace(): FindBridgeNamespace {
  const target = window as unknown as { ghostexGpui?: FindBridgeNamespace };
  target.ghostexGpui = target.ghostexGpui ?? {};
  return target.ghostexGpui;
}

function validatedBootstrap(
  candidate: FindGxserverBootstrap | undefined,
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
    const namespace = findBridgeNamespace();
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
      const validated = validatedBootstrap(findBridgeNamespace().gxserverBootstrap);
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
    body: JSON.stringify({ params, protocolVersion: GXSERVER_PROTOCOL_VERSION }),
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
    | { error?: { message?: string }; message?: string; ok?: boolean; result?: TResult }
    | undefined;
  if (!response.ok || !envelope || envelope.ok !== true) {
    const message =
      (envelope && typeof envelope.error?.message === "string" && envelope.error.message) ||
      (envelope && typeof envelope.message === "string" && envelope.message) ||
      `gxserver rejected ${path} (${response.status > 0 ? response.status : "no response"}).`;
    throw new Error(message);
  }
  return envelope.result as TResult;
}

interface AppModalHostMessageHandler {
  postMessage: (payload: string) => unknown;
}

function postFindHostAction(action: string, fields?: Record<string, unknown>): void {
  const target = window as unknown as {
    webkit?: { messageHandlers?: { ghostexAppModalHost?: AppModalHostMessageHandler } };
  };
  target.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage(
    JSON.stringify({ action, type: "findPromptsHostAction", ...fields }),
  );
}

function createGpuiFindPromptsTransport(bootstrap: {
  authToken: string;
  baseUrl: string;
}): FindPromptsTransport {
  return {
    close() {
      postFindHostAction("close");
    },
    async copyText(text) {
      await navigator.clipboard.writeText(text);
    },
    async focusSession(params) {
      postFindHostAction("focusSession", params);
    },
    async launchSession(plan) {
      postFindHostAction("launchSession", {
        agent: plan.agent,
        command: plan.commandLine,
        cwd: plan.cwd,
        cwdExists: plan.cwdExists,
        title: plan.title,
      });
    },
    readText(params: ReadAgentPromptTextParams) {
      return rpc<ReadAgentPromptTextResult>(bootstrap, "/api/readAgentPromptText", { ...params });
    },
    resolveLaunch(params: ResolveAgentPromptLaunchParams) {
      return rpc<ResolveAgentPromptLaunchResult>(bootstrap, "/api/resolveAgentPromptLaunch", {
        ...params,
      });
    },
    search(params: SearchAgentPromptsParams) {
      return rpc<SearchAgentPromptsResult>(bootstrap, "/api/searchAgentPrompts", { ...params });
    },
    toggleFavorite(params: ToggleAgentPromptFavoriteParams) {
      return rpc<ToggleAgentPromptFavoriteResult>(bootstrap, "/api/toggleAgentPromptFavorite", {
        ...params,
      });
    },
  };
}

function applyDocumentFindTheme(theme: "dark" | "light"): void {
  document.documentElement.style.colorScheme = theme;
  document.documentElement.style.backgroundColor = theme === "light" ? "#fdfdfd" : "#111111";
  document.body.style.backgroundColor = theme === "light" ? "#fdfdfd" : "#111111";
}

function applyDocumentFindFontFamily(fontFamily: string): void {
  const normalized = fontFamily.trim();
  if (normalized) {
    document.documentElement.style.setProperty("--ghostex-find-font-family", normalized);
  } else {
    document.documentElement.style.removeProperty("--ghostex-find-font-family");
  }
}

function renderFailure(root: ReturnType<typeof createRoot>, message: string): void {
  root.render(
    <div className="native-sidebar-shell gpui-find-prompts">
      <div className="ghostex-find-scope flex h-full flex-col items-center justify-center gap-1 px-6 text-center">
        <div className="text-sm font-medium text-foreground">Find unavailable</div>
        <div className="text-[13px] text-muted-foreground">{message}</div>
      </div>
    </div>,
  );
}

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Ghostex find root element was not found.");
}
const root = createRoot(rootElement);
const searchParams = new URLSearchParams(window.location.search);
const findTheme: "dark" | "light" = searchParams.get("theme") === "light" ? "light" : "dark";
/*
Accept All is a daemon-owned policy — gxserver applies the same setting `gx f`
reads — so this surface deliberately does not carry one. The query param stays
available only as an explicit override for a host that needs one.
*/
const acceptAllParam = searchParams.get("acceptAll");
const acceptAll = acceptAllParam === null ? undefined : acceptAllParam === "true";

document.body.dataset.sidebarTheme = findTheme === "light" ? "plain-light" : "plain-dark";
document.body.classList.add(findTheme === "light" ? "vscode-light" : "vscode-dark", "native-sidebar-body");
if (findTheme === "dark") {
  document.documentElement.classList.add("dark");
}
applyDocumentFindTheme(findTheme);
applyDocumentFindFontFamily(searchParams.get("fontFamily") ?? "");

waitForBootstrap()
  .then((bootstrap) => {
    root.render(
      <div className="native-sidebar-shell gpui-find-prompts">
        <FindPromptsView
          acceptAll={acceptAll}
          transport={createGpuiFindPromptsTransport(bootstrap)}
        />
      </div>,
    );
  })
  .catch(() => {
    renderFailure(
      root,
      "The Ghostex server is not reachable from this window. Switch back to the terminal and try again.",
    );
  });
