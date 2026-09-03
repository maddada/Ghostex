type GhostexGpuiWorkareaApi = {
  manageDocsResourceBaseUrl?: string;
  postManageFilesRequest?: (payload: string) => boolean;
  postProjectBeadsRequest?: (payload: string) => boolean;
  postProjectBoardImageRequest?: (payload: string) => boolean;
  postProjectBoardRequest?: (payload: string) => boolean;
  supportsManageFileChangePolling?: boolean;
};

type GhostexGpuiWorkareaWindow = Window & {
  ghostexGpui?: GhostexGpuiWorkareaApi;
  webkit?: {
    messageHandlers?: Record<string, { postMessage: (message: unknown) => void }>;
  };
};

const BRIDGE_SEND_RETRY_DELAY_MS = 20;
const BRIDGE_SEND_MAX_ATTEMPTS = 250;

/*
 * CDXC:CefRuntime 2026-06-24-11:03:
 * Kanban and Manage reuse the first-party React pages inside CEF, so GPUI installs a narrow compatibility shim before those pages mount. The shim maps the existing page-owned WebKit message-handler calls to fixed CEF bridge functions, queues only bounded in-memory JSON payloads during renderer bridge installation, and never exposes generic IPC, paths, logs, persistence, fallback URLs, or a WKWebView/WebKit runtime.
 */
function postToCefBridge(functionName: keyof GhostexGpuiWorkareaApi, message: unknown): void {
  const payload = JSON.stringify(message);
  let attempt = 0;

  const send = () => {
    const api = (window as GhostexGpuiWorkareaWindow).ghostexGpui;
    const post = api?.[functionName];
    if (typeof post === 'function' && post(payload)) {
      return;
    }
    attempt += 1;
    if (attempt >= BRIDGE_SEND_MAX_ATTEMPTS) {
      return;
    }
    window.setTimeout(send, BRIDGE_SEND_RETRY_DELAY_MS);
  };

  send();
}

function installMessageHandler(name: string, functionName: keyof GhostexGpuiWorkareaApi): void {
  const target = window as GhostexGpuiWorkareaWindow;
  target.webkit = target.webkit ?? {};
  target.webkit.messageHandlers = target.webkit.messageHandlers ?? {};
  target.webkit.messageHandlers[name] = {
    postMessage(message: unknown) {
      postToCefBridge(functionName, message);
    },
  };
}

export function installKanbanCefBridge(): void {
  installMessageHandler('ghostexProjectBeads', 'postProjectBeadsRequest');
  installMessageHandler('ghostexProjectBoard', 'postProjectBoardRequest');
  installMessageHandler('ghostexProjectBoardImages', 'postProjectBoardImageRequest');
}

export function installManageCefBridge(): void {
  const target = window as GhostexGpuiWorkareaWindow;
  target.ghostexGpui = target.ghostexGpui ?? {};
  target.ghostexGpui.supportsManageFileChangePolling = true;
  installMessageHandler('ghostexManageFiles', 'postManageFilesRequest');
}
