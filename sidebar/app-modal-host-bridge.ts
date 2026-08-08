import type { AgentConfigDraft } from "./agent-config-modal";
import { logAppModalError } from "./app-modal-error-log";
import type { GitCommitModalDraft } from "./git-commit-modal";
import type { SettingsModalTab } from "./settings-modal";
import type { ExtensionToSidebarMessage } from "../shared/session-grid-contract";
import type { SidebarAgentIcon } from "../shared/sidebar-agents";

type T3BrowserAccessMessage = Extract<ExtensionToSidebarMessage, { type: "showT3BrowserAccess" }>;

export type AppModalKind =
  | "addProject"
  | "addRepository"
  | "agentConfig"
  | "agentsHub"
  | "commandPalette"
  | "configureActions"
  | "configureAgents"
  | "daemonSessions"
  | "discoverGhostex"
  | "watchGhostexVideo"
  | "gitCommit"
  | "gitFileDiff"
  | "deleteWorktree"
  | "hotkeys"
  | "missingProjectFolder"
  | "openTargets"
  | "pinnedPrompts"
  | "portlessSetup"
  | "previousSessions"
  | "recentProjects"
  | "firstUserMessage"
  | "remoteGxserverInstall"
  | "remoteProjectPicker"
  | "delayedSend"
  | "renameSession"
  | "scratchPad"
  | "settings"
  | "stashedPrompts"
  | "t3BrowserAccess"
  | "t3ThreadId"
  | "worktree"
  | "tipsAndTricks"
  | "firstLaunchSetup";

export type OpenAppModalMessage =
  | {
      modal: Exclude<
        AppModalKind,
        | "addProject"
        | "addRepository"
        | "agentConfig"
        | "commandPalette"
        | "delayedSend"
        | "discoverGhostex"
        | "firstUserMessage"
        | "gitCommit"
        | "gitFileDiff"
        | "deleteWorktree"
        | "missingProjectFolder"
        | "portlessSetup"
        | "recentProjects"
        | "remoteGxserverInstall"
        | "renameSession"
        | "remoteProjectPicker"
        | "stashedPrompts"
        | "t3BrowserAccess"
        | "t3ThreadId"
        | "worktree"
      >;
      type: "open";
    }
  | {
      /**
       * CDXC:StashedPrompts 2026-07-29:
       * The session Prompts modal lists gxserver-stashed prompt-editor saves.
       * projectId scopes the default "this project and its worktrees" view and
       * sessionId names the terminal session the selected prompt is inserted
       * back into. Both are optional so the modal can still open (in
       * all-projects browse mode) when the launcher has no session mapping.
       */
      modal: "stashedPrompts";
      projectId?: string;
      sessionId?: string;
      type: "open";
    }
  | {
      /*
       * CDXC:FirstLaunchSetup 2026-06-16-07:58:
       * Automatic first-run onboarding should open the replayable Discover
       * Ghostex tour before firstLaunchSetup. Keep the follow-up flag scoped
       * to this modal open so manual overflow-menu Discover launches stay a
       * standalone tour.
       */
      modal: "discoverGhostex";
      showFirstLaunchSetupOnClose?: boolean;
      type: "open";
    }
  | {
      /**
       * CDXC:CommandPalette 2026-06-13-22:18:
       * The Commands tab accepts an optional initial search query. Quick Access
       * tab selection is carried by the modal id instead of encoding a mode in
       * the query text.
       */
      initialQuery?: string;
      modal: "commandPalette";
      type: "open";
    }
  | {
      /*
       * CDXC:PortlessSetupModal 2026-06-23-13:42:
       * Portless setup prompts render in the native app-modal child-window host
       * and carry only enum state needed to choose the exact handoff copy and
       * native admin protocol. Do not send settings or project/session data
       * through this modal-open boundary.
       */
      modal: "portlessSetup";
      mode: "firstSetup" | "standaloneReconfigure";
      protocol: "https" | "http";
      type: "open";
    }
  | {
      modal: "missingProjectFolder";
      projectId: string;
      projectName: string;
      projectPath: string;
      type: "open";
    }
  | {
      modal: "addRepository";
      remoteMachineId?: string;
      remoteMachineName?: string;
      type: "open";
    }
  | {
      /**
       * CDXC:AddProject 2026-07-30:
       * The add-project dialog is machine-agnostic: `machineId` only preselects
       * a machine and skips the dialog's machine step, which is what a remote
       * machine header wants. Omitting it opens the dialog on its machine step
       * whenever this host has more than one machine, and goes straight to the
       * sources step when it has one.
       */
      machineId?: string;
      modal: "addProject";
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
      machineId?: string;
      machineName?: string;
      modal: "recentProjects";
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
  | { gitCommitDraft: GitCommitModalDraft; modal: "gitCommit"; type: "open" }
  | { modal: "t3ThreadId"; sessionId: string; threadId: string; type: "open" }
  | { agentDraft: AgentConfigDraft; modal: "agentConfig"; type: "open" }
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
      agentIcon?: SidebarAgentIcon;
      closeAfterDoneActive?: boolean;
      delayedSendDeadlineAt?: string;
      delayedSendRemainingLabel?: string;
      modal: "delayedSend";
      sendWhenAllProjectSessionsStopActive?: boolean;
      sendWhenAgentStopsActive?: boolean;
      sessionId: string;
      supportsSendWhenAgentStops?: boolean;
      supportsSendWhenAllProjectSessionsStop?: boolean;
      title?: string;
      type: "open";
    }
  | {
      initialTitle: string;
      modal: "renameSession";
      /**
       * CDXC:SessionHistoryTitleSource 2026-07-29:
       * The rename modal enables empty-title Generate Name only for sessions
       * whose agent transcript gxserver can summarize, so the launcher passes
       * the session's agent icon identity through the modal bridge.
       */
      sessionAgentIcon?: string;
      sessionId: string;
      type: "open";
    }
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

export type QuickAccessPage =
  | "commands"
  | "recentProjects"
  | "recentSessions"
  | "savedPrompts";

type QuickAccessOpenOptions = {
  machineId?: string;
  machineName?: string;
};

/**
 * Open Ghostex Quick Access on one explicit page. Keep this mapping at the
 * modal-host boundary so shortcuts, sidebar buttons, titlebar actions, palette
 * commands, and the tabs themselves cannot drift back to query-driven routing.
 */
export function openQuickAccess(
  page: QuickAccessPage,
  options: QuickAccessOpenOptions = {},
): void {
  if (page === "recentProjects") {
    openAppModal({
      ...(options.machineId ? { machineId: options.machineId } : {}),
      ...(options.machineName ? { machineName: options.machineName } : {}),
      modal: "recentProjects",
      type: "open",
    });
    return;
  }
  if (page === "recentSessions") {
    openAppModal({ modal: "previousSessions", type: "open" });
    return;
  }
  if (page === "savedPrompts") {
    openAppModal({ modal: "stashedPrompts", type: "open" });
    return;
  }
  openAppModal({ initialQuery: "", modal: "commandPalette", type: "open" });
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
