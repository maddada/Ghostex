import type { AgentConfigDraft } from "./agent-config-modal";
import { logAppModalError } from "./app-modal-error-log";
import type { CommandConfigDraft } from "./command-config-modal";
import type { SettingsModalTab } from "./settings-modal";
import type { SidebarActionType } from "../shared/sidebar-commands";
import type { ExtensionToSidebarMessage } from "../shared/session-grid-contract";

type T3BrowserAccessMessage = Extract<ExtensionToSidebarMessage, { type: "showT3BrowserAccess" }>;

export type AppModalKind =
  | "addRepository"
  | "agentConfig"
  | "agentsHub"
  | "commandPalette"
  | "commandConfig"
  | "configureActions"
  | "configureAgents"
  | "daemonSessions"
  | "floatingPromptEditor"
  | "gitFileDiff"
  | "deleteWorktree"
  | "hotkeys"
  | "openTargets"
  | "pinnedPrompts"
  | "previousSessions"
  | "firstUserMessage"
  | "remoteGxserverInstall"
  | "remoteProjectPicker"
  | "delayedSend"
  | "renameSession"
  | "scratchPad"
  | "settings"
  | "t3BrowserAccess"
  | "t3ThreadId"
  | "worktree"
  | "tipsAndTricks"
  | "firstLaunchSetup";

export type OpenAppModalMessage =
  | {
      modal: Exclude<
        AppModalKind,
        | "addRepository"
        | "agentConfig"
        | "commandConfig"
        | "delayedSend"
        | "firstUserMessage"
        | "floatingPromptEditor"
        | "gitFileDiff"
        | "deleteWorktree"
        | "remoteGxserverInstall"
        | "renameSession"
        | "remoteProjectPicker"
        | "t3BrowserAccess"
        | "t3ThreadId"
        | "worktree"
      >;
      type: "open";
    }
  | {
      modal: "addRepository";
      remoteMachineId?: string;
      remoteMachineName?: string;
      type: "open";
    }
  | {
      modal: "remoteGxserverInstall";
      remoteMachineId: string;
      remoteMachineName: string;
      type: "open";
    }
  | {
      initialQuery?: string;
      modal: "remoteProjectPicker";
      remoteMachineId: string;
      remoteMachineName: string;
      type: "open";
    }
  | {
      initialSearchQuery?: string;
      initialRemoteMachineId?: string;
      initialTab?: SettingsModalTab;
      modal: "settings";
      type: "open";
    }
  | { access: T3BrowserAccessMessage; modal: "t3BrowserAccess"; type: "open" }
  | { modal: "t3ThreadId"; sessionId: string; threadId: string; type: "open" }
  | { agentDraft: AgentConfigDraft; modal: "agentConfig"; type: "open" }
  | {
      commandDraft: CommandConfigDraft;
      lockedActionType?: SidebarActionType;
      modal: "commandConfig";
      type: "open";
    }
  | {
      message: string;
      modal: "firstUserMessage";
      title?: string;
      type: "open";
    }
  | {
      /**
       * CDXC:DelayedSend 2026-05-17-03:14
       * Opening the Delayed Send modal for an active timer must prefill the
       * current remaining duration and offer cancellation instead of acting as
       * a blind new-schedule dialog.
       */
      delayedSendDeadlineAt?: string;
      delayedSendRemainingLabel?: string;
      modal: "delayedSend";
      sessionId: string;
      title?: string;
      type: "open";
    }
  | { initialTitle: string; modal: "renameSession"; sessionId: string; type: "open" }
  | {
      modal: "worktree";
      projectId?: string;
      projectName?: string;
      projectPath?: string;
      remoteMachineId?: string;
      remoteMachineName?: string;
      type: "open";
    };

declare global {
  interface Window {
    webkit?: {
      messageHandlers?: {
        ghostexAppModalHost?: {
          postMessage: (message: unknown) => void;
        };
        ghostexNativeHost?: {
          postMessage: (message: unknown) => void;
        };
        ghostexNativeHostDiagnostics?: {
          postMessage: (message: unknown) => void;
        };
      };
    };
    __ghostex_APP_MODAL_HOST_SURFACE__?: "main" | "nativeWindow";
  }
}

/**
 * CDXC:AppModals 2026-04-27-14:25
 * Modal launchers must never fall back to sidebar-local dialogs. If the native
 * full-window modal host is unavailable, persist the error and throw so the
 * broken bridge is visible instead of silently showing a squeezed modal.
 */
export function openAppModal(message: OpenAppModalMessage): void {
  postAppModalHostMessage(message, `AppModals:${message.modal}`);
}

export function closeAppModal(area = "AppModals:close"): void {
  postAppModalHostMessage({ type: "close" }, area);
}

export function postAppModalHostMessage(message: unknown, area: string): void {
  const modalHost = window.webkit?.messageHandlers?.ghostexAppModalHost;
  if (!modalHost) {
    const error = new Error("Native full-window modal host is unavailable.");
    logAppModalError(area, error);
    throw error;
  }

  try {
    /*
     * CDXC:AppModals 2026-06-11-19:46:
     * Settings, Agents Hub, Previous Sessions, and the other non-prompt app modals now render in native child windows that reuse this web bridge. Mark messages with the modal-host surface when native injected one, so AppKit can route close/presented/result messages to the window host without guessing from modal kind.
     */
    modalHost.postMessage(withModalHostSurface(message));
  } catch (error) {
    logAppModalError(area, error);
    throw error;
  }
}

function withModalHostSurface(message: unknown): unknown {
  const surface = window.__ghostex_APP_MODAL_HOST_SURFACE__;
  if (
    !surface ||
    !message ||
    typeof message !== "object" ||
    Array.isArray(message) ||
    "surface" in message
  ) {
    return message;
  }
  return {
    ...(message as Record<string, unknown>),
    surface,
  };
}
