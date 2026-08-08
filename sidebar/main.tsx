import { createRoot } from "react-dom/client";
import { SidebarApp } from "./sidebar-app";
import "./styles.css";

declare global {
  function acquireVsCodeApi(): {
    postMessage: (message: unknown) => void;
  };
}

const SIDEBAR_BOOTSTRAP_REPRO_WINDOW_MS = 15_000;
const sidebarBootstrapStartedAt = Date.now();

type SidebarVsCodeApi = ReturnType<typeof acquireVsCodeApi>;

let vscode: SidebarVsCodeApi | undefined;

function getBootstrapElapsedMs(): number {
  return Date.now() - sidebarBootstrapStartedAt;
}

function postSidebarBootstrapReproLog(event: string, details: Record<string, unknown> = {}): void {
  if (!vscode) {
    return;
  }

  if (getBootstrapElapsedMs() > SIDEBAR_BOOTSTRAP_REPRO_WINDOW_MS) {
    return;
  }

  vscode.postMessage({
    details: {
      ...details,
      elapsedMs: getBootstrapElapsedMs(),
    },
    event: `repro.sidebarStartup.bootstrap.${event}`,
    scenarioId: "native.sidebar.refresh",
    type: "sidebarDebugLog",
  });
}

function describeBootstrapError(error: unknown): Record<string, unknown> {
  if (error instanceof Error) {
    return {
      message: error.message,
      name: error.name,
      stack: error.stack,
    };
  }

  return {
    value: String(error),
  };
}

function installBootstrapObservers(): void {
  window.addEventListener("error", (event) => {
    postSidebarBootstrapReproLog("windowError", {
      colno: event.colno,
      filename: event.filename,
      lineno: event.lineno,
      message: event.message,
    });
  });

  window.addEventListener("unhandledrejection", (event) => {
    postSidebarBootstrapReproLog("windowUnhandledRejection", {
      reason: describeBootstrapError(event.reason),
    });
  });

  window.addEventListener("message", (event) => {
    const message = event.data;
    if (!message || typeof message !== "object") {
      return;
    }

    const messageType = "type" in message ? message.type : undefined;
    if (messageType !== "hydrate" && messageType !== "sessionState") {
      return;
    }

    const groups = "groups" in message && Array.isArray(message.groups) ? message.groups : [];
    const sessionCount = groups.reduce((total: number, group: unknown) => {
      if (!group || typeof group !== "object" || !("sessions" in group)) {
        return total;
      }

      const sessions = Array.isArray(group.sessions) ? group.sessions : [];
      return total + sessions.length;
    }, 0);

    postSidebarBootstrapReproLog("windowMessageReceived", {
      groupCount: groups.length,
      messageType,
      revision: "revision" in message ? message.revision : undefined,
      sessionCount,
    });
  });

  window.addEventListener("focus", () => {
    postSidebarBootstrapReproLog("windowFocus", {
      visibilityState: document.visibilityState,
    });
  });

  window.addEventListener("blur", () => {
    postSidebarBootstrapReproLog("windowBlur", {
      visibilityState: document.visibilityState,
    });
  });

  window.addEventListener("pageshow", (event) => {
    postSidebarBootstrapReproLog("pageShow", {
      persisted: event.persisted,
      visibilityState: document.visibilityState,
    });
  });

  document.addEventListener("visibilitychange", () => {
    postSidebarBootstrapReproLog("visibilityChanged", {
      readyState: document.readyState,
      visibilityState: document.visibilityState,
    });
  });
}

try {
  vscode = acquireVsCodeApi();
} catch (error) {
  console.error("Failed to acquire VS Code API for sidebar bootstrap logging.", error);
  throw error;
}

postSidebarBootstrapReproLog("moduleStart", {
  readyState: document.readyState,
  visibilityState: document.visibilityState,
});
installBootstrapObservers();

const rootElement = document.getElementById("root");
postSidebarBootstrapReproLog("rootLookup", {
  hasRootElement: rootElement !== null,
});

if (!rootElement) {
  postSidebarBootstrapReproLog("rootMissing");
  throw new Error("Sidebar root element was not found.");
}

const root = createRoot(rootElement);
postSidebarBootstrapReproLog("reactRootCreated");

try {
  root.render(<SidebarApp vscode={vscode} />);
  postSidebarBootstrapReproLog("reactRenderCalled");
} catch (error) {
  postSidebarBootstrapReproLog("reactRenderFailed", {
    error: describeBootstrapError(error),
  });
  throw error;
}
