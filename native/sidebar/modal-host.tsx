import { createRoot } from "react-dom/client";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { Toaster, toast } from "sonner";
import { AddRepositoryModal } from "../../sidebar/add-repository-modal";
import { AgentConfigModal, type AgentConfigDraft } from "../../sidebar/agent-config-modal";
import { AgentsHubModal } from "../../sidebar/agents-hub-modal";
import { CommandPalette } from "../../sidebar/command-palette";
import { DaemonSessionsModal } from "../../sidebar/daemon-sessions-modal";
import { DelayedSendModal } from "../../sidebar/delayed-send-modal";
import { DiscoverGhostexModal } from "../../sidebar/discover-ghostex-modal";
import { FirstUserMessageModal } from "../../sidebar/first-user-message-modal";
import { PinnedPromptsModal } from "../../sidebar/pinned-prompts-modal";
import {
  PortlessSetupModal,
  type PortlessSetupModalMode,
} from "../../sidebar/portless-setup-modal";
import { PreviousSessionsModal } from "../../sidebar/previous-sessions-modal";
import { RemoteGxserverInstallModal } from "../../sidebar/remote-gxserver-install-modal";
import { RemoteProjectPickerModal } from "../../sidebar/remote-project-picker/remote-project-picker-modal";
import type { T3FilesystemBrowseResult } from "../../sidebar/remote-project-picker/t3-filesystem";
import { ScratchPadModal } from "../../sidebar/scratch-pad-modal";
import {
  SettingsModal,
  type MainSettingsInitialSectionId,
  type SettingsModalTab,
} from "../../sidebar/settings-modal";
import { SessionRenameModal } from "../../sidebar/session-rename-modal";
import { T3BrowserAccessModal } from "../../sidebar/t3-browser-access-modal";
import { T3ThreadIdModal } from "../../sidebar/t3-thread-id-modal";
import { WatchGhostexVideoModal } from "../../sidebar/watch-ghostex-video-modal";
import { FirstLaunchSetupModal } from "../../sidebar/first-launch-setup-modal";
import { GitFileDiffModal, type GitFileDiffModalDraft } from "../../sidebar/git-file-diff-modal";
import { GitCommitModal, type GitCommitModalDraft } from "../../sidebar/git-commit-modal";
import {
  WorktreeDeleteModal,
  type WorktreeDeleteModalDraft,
} from "../../sidebar/worktree-delete-modal";
import { WorktreeCreateModal } from "../../sidebar/worktree-create-modal";
import {
  normalizeAppToastDescription,
  type AppToastRequest,
} from "../../shared/app-toast-contract";
import type { SidebarAgentButton } from "../../shared/sidebar-agents";
import type {
  ExtensionToSidebarMessage,
  SidebarAgentHookStatusMessage,
  SidebarDoctorCheck,
  SidebarDoctorChecksResultMessage,
  SidebarDoctorFixResultMessage,
  SidebarDiagnosticsExportResultMessage,
  SidebarGhostexCliStatusMessage,
  SidebarGhostexFolderStatsMessage,
  SidebarOSIntegrationStatusMessage,
  // CDXC:AppIconPicker 2026-06-25-21:50: App Icon state flows to Settings through the modal-state relay.
  SidebarAppIconStateMessage,
  SidebarToExtensionMessage,
} from "../../shared/session-grid-contract";
import {
  getWorkspaceThemeForeground,
  normalizeWorkspaceThemeColor,
} from "../../shared/workspace-project-appearance";
import {
  installAppModalGlobalErrorLogging,
  logAppModalError,
} from "../../sidebar/app-modal-error-log";
import { postAppModalHostMessage } from "../../sidebar/app-modal-host-bridge";
import { useSidebarStore } from "../../sidebar/sidebar-store";
import {
  DEFAULT_ghostex_SETTINGS,
  isDiagnosticLoggingScenarioEnabled,
  type DiagnosticLoggingScenarioId,
} from "../../shared/ghostex-settings";
import type { WebviewApi } from "../../sidebar/webview-api";
import "../../sidebar/styles.css";

type AppModalKind =
  | "addRepository"
  | "agentConfig"
  | "agentsHub"
  | "commandPalette"
  | "configureActions"
  | "configureAgents"
  | "daemonSessions"
  | "delayedSend"
  | "discoverGhostex"
  | "watchGhostexVideo"
  | "hotkeys"
  | "gitCommit"
  | "gitFileDiff"
  | "deleteWorktree"
  | "openTargets"
  | "pinnedPrompts"
  | "portlessSetup"
  | "previousSessions"
  | "firstUserMessage"
  | "remoteGxserverInstall"
  | "remoteProjectPicker"
  | "renameSession"
  | "scratchPad"
  | "settings"
  | "t3BrowserAccess"
  | "t3ThreadId"
  | "worktree"
  | "tipsAndTricks"
  | "firstLaunchSetup";

/*
 * CDXC:AppModals 2026-06-30-16:08:
 * Centered compact native child-window modals should size to their rendered
 * React dialog once, before native presents the panel. Keep Settings out of
 * this path because it remains a user-resizable fixed-size native window.
 */
const ONE_SHOT_NATIVE_FIT_HEIGHT_MODAL_SELECTORS: Partial<Record<AppModalKind, string>> = {
  addRepository: ".add-repository-modal-shadcn",
  agentConfig: ".agent-config-modal-shadcn",
  delayedSend: ".delayed-send-modal-shadcn",
  deleteWorktree: ".worktree-delete-modal-shadcn",
  firstUserMessage: ".first-user-message-modal",
  portlessSetup: ".portless-setup-modal-shadcn",
  remoteGxserverInstall: ".remote-gxserver-install-modal",
  remoteProjectPicker: ".remote-project-picker-dialog",
  renameSession: ".session-rename-modal-shadcn",
  t3BrowserAccess: ".t3-browser-access-modal",
  t3ThreadId: ".t3-thread-id-modal",
  worktree: ".worktree-create-modal-shadcn",
};

/*
 * CDXC:AppModals 2026-06-30-16:08:
 * Most measured dialogs are centered, so setting the native window to their
 * element height puts the React shell at y=0. Top-aligned modals keep an
 * intentional WebView inset, so include that inset in the one-shot height.
 */
const ONE_SHOT_NATIVE_FIT_HEIGHT_TOP_OFFSET_MODALS = new Set<AppModalKind>([
  "previousSessions",
  "remoteProjectPicker",
]);

function oneShotNativeFitHeightSelector(modal: AppModalKind): string | undefined {
  return ONE_SHOT_NATIVE_FIT_HEIGHT_MODAL_SELECTORS[modal];
}

function shouldUseOneShotNativeFitHeight(
  modal: AppModalKind | null | undefined,
): modal is AppModalKind {
  return Boolean(modal && oneShotNativeFitHeightSelector(modal));
}

function measureOneShotNativeFitHeight(modal: AppModalKind): number | undefined {
  const selector = oneShotNativeFitHeightSelector(modal);
  if (!selector) {
    return undefined;
  }
  const element = document.querySelector(selector);
  if (!(element instanceof HTMLElement)) {
    return undefined;
  }
  const rect = element.getBoundingClientRect();
  const topOffset = ONE_SHOT_NATIVE_FIT_HEIGHT_TOP_OFFSET_MODALS.has(modal)
    ? Math.max(0, rect.top)
    : 0;
  const height = Math.ceil(Math.max(rect.height, element.offsetHeight) + topOffset);
  return Number.isFinite(height) && height > 0 ? height : undefined;
}

type T3BrowserAccessMessage = Extract<ExtensionToSidebarMessage, { type: "showT3BrowserAccess" }>;
type AgentsHubCatalogMessage = Extract<ExtensionToSidebarMessage, { type: "agentsHubCatalog" }>;
type AgentsHubFileContentMessage = Extract<
  ExtensionToSidebarMessage,
  { type: "agentsHubFileContent" }
>;
type AgentHookStatusMessage = Extract<ExtensionToSidebarMessage, { type: "agentHookStatus" }>;
type GhostexCliStatusMessage = Extract<ExtensionToSidebarMessage, { type: "ghostexCliStatus" }>;
type OSIntegrationStatusMessage = Extract<ExtensionToSidebarMessage, { type: "osIntegrationStatus" }>;
// CDXC:AppIconPicker 2026-06-25-21:50: App Icon state message threaded through modal state into Settings.
type AppIconStateMessage = Extract<ExtensionToSidebarMessage, { type: "appIconState" }>;

type AppModalHostMessage =
  | {
      agentDraft?: AgentConfigDraft;
      access?: T3BrowserAccessMessage;
      collapsedGroupsById?: Record<string, true>;
      delayedSendDeadlineAt?: string;
      delayedSendRemainingLabel?: string;
      initialTitle?: string;
      initialQuery?: string;
      message?: string;
      projectId?: string;
      projectName?: string;
      projectPath?: string;
      remoteMachineId?: string;
      remoteMachineName?: string;
      filePath?: string;
      gitCommitDraft?: GitCommitModalDraft;
      gitFileDiff?: GitFileDiffModalDraft;
      worktreeDeleteDraft?: WorktreeDeleteModalDraft;
      initialRemoteMachineId?: string;
      initialSection?: MainSettingsInitialSectionId;
      initialSearchQuery?: string;
      initialTab?: SettingsModalTab;
      latestSidebarStateMessage?: unknown;
      modal: AppModalKind;
      mode?: PortlessSetupModalMode;
      prewarm?: boolean;
      protocol?: "https" | "http";
      requestId?: string;
      sessionId?: string;
      showFirstLaunchSetupOnClose?: boolean;
      threadId?: string;
      title?: string;
      type: "open";
    }
  | { type: "close" }
  | AppToastRequest
  | { keepOpen?: boolean; type: "toastDismissed" }
  | { initialPath?: string; type: "pickRepositoryFolder" }
  | { path: string; type: "repositoryFolderPicked" }
  | {
      error?: string;
      ok: boolean;
      projectPath?: string;
      requestId: string;
      type: "repositoryCloneResult";
    }
  | {
      error?: string;
      ok: boolean;
      preview?: unknown;
      requestId: string;
      type: "repositoryClonePreviewResult";
    }
  | {
      error?: string;
      ok: boolean;
      requestId: string;
      result?: T3FilesystemBrowseResult;
      type: "remoteProjectDirectoryBrowseResult";
    }
  | {
      error?: string;
      ok: boolean;
      projectPath?: string;
      requestId: string;
      type: "remoteProjectAddResult";
    }
  | { type: "pickWorktreeImages" }
  | { paths: string[]; type: "worktreeImageFilesPicked" }
  | {
      branches?: unknown;
      error?: string;
      ok: boolean;
      requestId: string;
      type: "projectWorktreesResult";
      worktrees?: unknown;
    }
  | { details?: string; event: string; type: "debugLog" }
  | { modal: AppModalKind; requestId?: string; type: "presented" }
  | { message: unknown; type: "sidebarState" };

type RenameSessionModalState = {
  initialTitle: string;
  sessionId: string;
};

type PromptAgentModalKey = "gitCommit" | "renameSession";

const PROMPT_AGENT_MODAL_STORAGE_KEYS: Record<PromptAgentModalKey, string> = {
  gitCommit: "ghostex.promptAgent.gitCommit",
  renameSession: "ghostex.promptAgent.renameSession",
};

type FirstUserMessageModalState = {
  message: string;
  title?: string;
};

type RemoteProjectPickerState = {
  initialQuery?: string;
  remoteMachineId: string;
  remoteMachineName: string;
};

type RemoteGxserverInstallState = {
  remoteMachineId: string;
  remoteMachineName: string;
};

type AddRepositoryModalState = {
  remoteMachineId?: string;
  remoteMachineName?: string;
};

type DelayedSendModalState = {
  delayedSendDeadlineAt?: string;
  delayedSendRemainingLabel?: string;
  sessionId: string;
  title?: string;
};

/**
 * CDXC:AppToasts 2026-06-03-16:12:
 * macOS and crossplatform app-modal toasts should sit 23px higher than
 * Sonner's 24px bottom default, so progress notices stay clear of lower app
 * chrome while preserving the bottom-center stack behavior.
 */
const APP_MODAL_TOAST_BOTTOM_OFFSET_PX = 47;
type T3ThreadIdModalState = {
  currentThreadId: string;
  sessionId: string;
};

type WorktreeModalState = {
  projectId?: string;
  projectName?: string;
  projectPath?: string;
  remoteMachineId?: string;
  remoteMachineName?: string;
};

type PortlessSetupModalState = {
  mode: PortlessSetupModalMode;
  protocol: "https" | "http";
};

const APP_MODAL_CONTEXT_MENU_EDITABLE_SELECTOR =
  "input, textarea, select, [contenteditable='true'], [role='textbox']";

function isEditableAppModalContextMenuTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) {
    return false;
  }

  return target.closest(APP_MODAL_CONTEXT_MENU_EDITABLE_SELECTOR) !== null;
}

type ConfigModalState = {
  agentDraft?: AgentConfigDraft;
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
    __ghostex_APP_MODAL_HOST_ID__?: string;
    __ghostex_APP_MODAL_HOST_SURFACE__?: "main" | "nativeWindow";
  }
}

const vscode: WebviewApi = {
  postMessage(message) {
    if (isAppModalDebugLoggingEnabled()) {
      console.debug("[ghostex-app-modal-host] sidebarCommand", redactAppModalDebugMessage(message));
    }
    /**
     * CDXC:AppModals 2026-06-13-01:09:
     * Previous Sessions no longer sends agent-prompt search commands, but modal
     * commands still cross this full-window host before native dispatch. Keep a
     * single debug boundary for restore, delete, and direct text-search commands.
     */
    postAppModalHostMessage({ message, type: "sidebarCommand" }, "AppModals:sidebarCommand");
  },
};

function redactAppModalDebugMessage(message: unknown): unknown {
  if (
    typeof message === "object" &&
    message !== null &&
    !Array.isArray(message) &&
    (message as { type?: unknown }).type === "saveRemoteMachinePassword"
  ) {
    /*
     * CDXC:RemoteMachines 2026-06-09-18:23:
     * SSH password saves are intentionally one-shot Keychain writes. Modal
     * debug logging must redact the transient password before it reaches the
     * console so diagnostics cannot capture user credentials.
     */
    return {
      ...(message as Record<string, unknown>),
      password: "[redacted]",
    };
  }
  return message;
}

function isDiagnosticLoggingEnabledForScenario(scenarioId: DiagnosticLoggingScenarioId): boolean {
  const settings = useSidebarStore.getState().hud.settings ?? DEFAULT_ghostex_SETTINGS;
  return isDiagnosticLoggingScenarioEnabled(settings.diagnosticLogging, scenarioId);
}

function isAppModalDebugLoggingEnabled(): boolean {
  return isDiagnosticLoggingEnabledForScenario("native.app.modal");
}

function isRemoteGxserverInstallDebugLoggingEnabled(): boolean {
  return isDiagnosticLoggingEnabledForScenario("native.remote.gxserver.install");
}

type AppModalDebugDetails = Record<string, string | number | boolean | null | undefined>;

function postAppModalDebugLog(event: string, details: AppModalDebugDetails) {
  if (!isAppModalDebugLoggingEnabled()) {
    return;
  }
  /*
   * CDXC:SettingsModalDiagnostics 2026-06-20-05:38:
   * Settings and setup modal diagnostics must stay limited to lifecycle
   * booleans, revisions, timings, modal ids, and safe enum-like metadata.
   */
  postAppModalHostMessage(
    {
      details: JSON.stringify({
        performanceNow: Math.round(performance.now()),
        ...details,
      }),
      event,
      type: "debugLog",
    },
    "AppModals:debug",
  );
}

function postSettingsModalDebugLog(event: string, details: AppModalDebugDetails) {
  postAppModalDebugLog(event, details);
}

function postRemoteGxserverInstallDebugLog(event: string, details: AppModalDebugDetails) {
  if (!isRemoteGxserverInstallDebugLoggingEnabled()) {
    return;
  }
  /*
   * CDXC:RemoteMachines 2026-06-30-03:05:
   * Persist remote install modal-host breadcrumbs under the dedicated scenario
   * without machine names, hosts, paths, URLs, command text, passwords, tokens,
   * or raw errors.
   */
  postAppModalHostMessage(
    {
      details: JSON.stringify({
        performanceNow: Math.round(performance.now()),
        ...details,
      }),
      event,
      type: "remoteGxserverInstallDebugLog",
    },
    "RemoteGxserverInstall:debug",
  );
}

function notifyNativeModalClosed() {
  postAppModalHostMessage({ type: "close" }, "AppModals:close");
}

function isSettingsModalKind(modal: AppModalKind | undefined): boolean {
  return (
    modal === "settings" ||
    modal === "configureAgents" ||
    modal === "configureActions" ||
    modal === "openTargets" ||
    modal === "hotkeys"
  );
}

function isFirstLaunchSetupModalKind(modal: AppModalKind | undefined): boolean {
  return modal === "firstLaunchSetup" || modal === "tipsAndTricks";
}

function shouldApplySidebarStateBeforeModalOpen(modal: AppModalKind | undefined): boolean {
  /*
   * CDXC:FirstLaunchSetup 2026-06-29-13:46:
   * First-launch setup reads the same hydrated Settings store as the Settings
   * modal. Apply the native sidebar snapshot before setting activeModal so the
   * child-window setup flow cannot stay blank behind its native backdrop while
   * React waits at revision 0.
   */
  return isSettingsModalKind(modal) || isFirstLaunchSetupModalKind(modal);
}

function getSettingsInitialTab(modal: AppModalKind | undefined): SettingsModalTab {
  /**
   * CDXC:UnifiedSettings 2026-05-09-15:30
   * Existing entry points still request their historic modal kind, but the
   * app-modal host now routes Settings, Agents, Actions, and Hotkeys into one
   * tabbed Settings dialog so users have a single configuration surface.
   */
  if (modal === "configureAgents") {
    return "agents";
  }
  if (modal === "configureActions") {
    return "actions";
  }
  if (modal === "hotkeys") {
    return "hotkeys";
  }
  if (modal === "openTargets") {
    return "openTargets";
  }
  return "settings";
}

function isSettingsModalTab(value: unknown): value is SettingsModalTab {
  return (
    value === "settings" ||
    value === "ghostty" ||
    value === "integrations" ||
    value === "osIntegration" ||
    value === "remote" ||
    value === "projects" ||
    value === "agents" ||
    value === "actions" ||
    value === "openTargets" ||
    value === "hotkeys"
  );
}

function normalizeCommandPaletteCollapsedGroupsById(candidate: unknown): Record<string, true> {
  const normalized: Record<string, true> = {};
  if (candidate === null || typeof candidate !== "object" || Array.isArray(candidate)) {
    return normalized;
  }

  for (const [groupId, isCollapsed] of Object.entries(candidate)) {
    if (groupId.trim().length > 0 && isCollapsed === true) {
      normalized[groupId] = true;
    }
  }
  return normalized;
}

function readPromptAgentModalOverride(modal: PromptAgentModalKey): string | undefined {
  const value = localStorage.getItem(PROMPT_AGENT_MODAL_STORAGE_KEYS[modal])?.trim();
  return value || undefined;
}

function writePromptAgentModalOverride(modal: PromptAgentModalKey, agentId: string): void {
  const normalizedAgentId = agentId.trim();
  if (!normalizedAgentId) {
    localStorage.removeItem(PROMPT_AGENT_MODAL_STORAGE_KEYS[modal]);
    return;
  }
  localStorage.setItem(PROMPT_AGENT_MODAL_STORAGE_KEYS[modal], normalizedAgentId);
}

function clearPromptAgentModalOverrides(): void {
  for (const key of Object.values(PROMPT_AGENT_MODAL_STORAGE_KEYS)) {
    localStorage.removeItem(key);
  }
}

function resolvePromptAgentModalSelection(
  agents: readonly SidebarAgentButton[],
  savedAgentId: string | undefined,
  defaultAgentId: string | undefined,
): string | undefined {
  const commandAgents = agents.filter((agent) => agent.agentId !== "t3" && agent.command?.trim());
  return (
    commandAgents.find((agent) => agent.agentId === savedAgentId)?.agentId ??
    commandAgents.find((agent) => agent.agentId === defaultAgentId)?.agentId ??
    commandAgents[0]?.agentId
  );
}

function createRemoteProjectRequestId(kind: "add" | "browse"): string {
  return `remote-project-${kind}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

function waitForRemoteProjectDirectoryBrowseResult(
  requestId: string,
): Promise<T3FilesystemBrowseResult> {
  return new Promise((resolve, reject) => {
    let timeoutId = 0;
    const handleMessage = (event: Event) => {
      const message = (event as CustomEvent<AppModalHostMessage>).detail;
      if (
        !message ||
        typeof message !== "object" ||
        message.type !== "remoteProjectDirectoryBrowseResult" ||
        message.requestId !== requestId
      ) {
        return;
      }
      window.clearTimeout(timeoutId);
      window.removeEventListener("ghostex-app-modal-host-message", handleMessage);
      if (!message.ok || !isT3FilesystemBrowseResult(message.result)) {
        reject(new Error(message.error || "Remote directory browse failed."));
        return;
      }
      resolve(message.result);
    };

    window.addEventListener("ghostex-app-modal-host-message", handleMessage);
    timeoutId = window.setTimeout(() => {
      window.removeEventListener("ghostex-app-modal-host-message", handleMessage);
      reject(new Error("Remote directory browse timed out."));
    }, 15_000);
  });
}

function waitForRemoteProjectAddResult(requestId: string): Promise<void> {
  return new Promise((resolve, reject) => {
    let timeoutId = 0;
    const handleMessage = (event: Event) => {
      const message = (event as CustomEvent<AppModalHostMessage>).detail;
      if (
        !message ||
        typeof message !== "object" ||
        message.type !== "remoteProjectAddResult" ||
        message.requestId !== requestId
      ) {
        return;
      }
      window.clearTimeout(timeoutId);
      window.removeEventListener("ghostex-app-modal-host-message", handleMessage);
      if (!message.ok) {
        reject(new Error(message.error || "Remote project add failed."));
        return;
      }
      resolve();
    };

    window.addEventListener("ghostex-app-modal-host-message", handleMessage);
    timeoutId = window.setTimeout(() => {
      window.removeEventListener("ghostex-app-modal-host-message", handleMessage);
      reject(new Error("Remote project add timed out."));
    }, 20_000);
  });
}

function isT3FilesystemBrowseResult(value: unknown): value is T3FilesystemBrowseResult {
  if (!value || typeof value !== "object") {
    return false;
  }
  const candidate = value as Partial<T3FilesystemBrowseResult>;
  return (
    typeof candidate.parentPath === "string" &&
    Array.isArray(candidate.entries) &&
    candidate.entries.every(
      (entry) =>
        Boolean(entry) &&
        typeof entry === "object" &&
        typeof (entry as { fullPath?: unknown }).fullPath === "string" &&
        typeof (entry as { name?: unknown }).name === "string",
    )
  );
}

function AppModalHost() {
  const {
    activeModal,
    activeModalRequestId,
    addRepository,
    agentsHubCatalog,
    agentsHubFileContent,
    config,
    delayedSend,
    firstUserMessage,
    gitCommit,
    gitFileDiff,
    worktreeDelete,
    commandPaletteCollapsedGroupsById,
    commandPaletteInitialQuery,
    commandPaletteOpenRequestSequence,
    isCommandPalettePrewarm,
    closeGitFileDiff,
    closeModal,
    remoteGxserverInstall,
    remoteProjectPicker,
    renameSession,
    t3BrowserAccess,
    t3ThreadId,
    worktree,
    agentHookStatus,
    ghostexCliStatus,
    ghostexFolderStats,
    osIntegrationStatus,
    // CDXC:AppIconPicker 2026-06-25-21:50: Pull relayed App Icon state for the Settings modal.
    appIconState,
    portlessSetup,
    settingsInitialSection,
    settingsInitialRemoteMachineId,
    settingsInitialSearchQuery,
    settingsInitialTabOverride,
  } = useModalStateFromNative();
  const [agentHookStatusLoading, setAgentHookStatusLoading] = useState(false);
  const [ghostexCliStatusLoading, setGhostexCliStatusLoading] = useState(false);
  const [ghostexFolderStatsLoading, setGhostexFolderStatsLoading] = useState(false);
  const [osIntegrationStatusLoading, setOSIntegrationStatusLoading] = useState(false);
  const [doctorChecks, setDoctorChecks] = useState<SidebarDoctorCheck[]>();
  const [doctorLoading, setDoctorLoading] = useState(false);
  const [diagnosticsJson, setDiagnosticsJson] = useState<string>();
  const [diagnosticsLoading, setDiagnosticsLoading] = useState(false);
  const [isPreviousSessionsInitialLoadReady, setIsPreviousSessionsInitialLoadReady] = useState(false);
  const sentNativeFitHeightMeasurementKeysRef = useRef<Set<string>>(new Set());
  const previousSettingsRenderStateLogRef = useRef("");
  const previousFirstLaunchSetupRenderStateLogRef = useRef("");
  const latestSettingsPresentedLogDetailsRef = useRef<
    Record<string, string | number | boolean | null | undefined>
  >({});
  const latestFirstLaunchSetupPresentedLogDetailsRef = useRef<
    Record<string, string | number | boolean | null | undefined>
  >({});
  const settings = useSidebarStore((state) => state.hud.settings);
  const appIconPickerUnavailable = useSidebarStore(
    (state) => state.hud.appIconPickerUnavailable === true,
  );
  const revision = useSidebarStore((state) => state.revision);
  const agents = useSidebarStore((state) => state.hud.agents);
  const commands = useSidebarStore((state) => state.hud.commands);
  const projectSettingsProjects = useSidebarStore(
    (state) => state.hud.projectSettingsProjects ?? [],
  );
  const portless = useSidebarStore((state) => state.hud.portless);
  const customThemeColor = useSidebarStore((state) => state.hud.customThemeColor);
  const theme = useSidebarStore((state) => state.hud.theme);
  const [gitCommitPromptAgentId, setGitCommitPromptAgentId] = useState(() =>
    readPromptAgentModalOverride("gitCommit"),
  );
  const [renamePromptAgentId, setRenamePromptAgentId] = useState(() =>
    readPromptAgentModalOverride("renameSession"),
  );
  const previousDefaultPromptAgentIdRef = useRef(settings?.defaultPromptAgentId);
  const resolvedGitCommitPromptAgentId = resolvePromptAgentModalSelection(
    agents,
    gitCommitPromptAgentId,
    settings?.defaultPromptAgentId,
  );
  const resolvedRenamePromptAgentId = resolvePromptAgentModalSelection(
    agents,
    renamePromptAgentId,
    settings?.defaultPromptAgentId,
  );
  /*
   * CDXC:GxserverAgentSettings 2026-06-19-08:58:
   * The modal store starts with DEFAULT_ghostex_SETTINGS before the native
   * hydrate arrives. Keep Settings and First Launch closed until revision > 0
   * so their full-setting save messages cannot seed gxserver-owned Default
   * Prompt Agent back to Codex from a pre-hydrate placeholder.
   */
  const hasNativeSettingsHydrated = revision > 0;
  const isSettingsModal = isSettingsModalKind(activeModal);
  const isSettingsRenderable = isSettingsModal && hasNativeSettingsHydrated;
  const isFirstLaunchSetupModal = isFirstLaunchSetupModalKind(activeModal);
  const isFirstLaunchSetupRenderable = isFirstLaunchSetupModal && hasNativeSettingsHydrated;
  const settingsInitialTab = settingsInitialTabOverride ?? getSettingsInitialTab(activeModal);
  const hasSettings = settings !== undefined;
  const hasSettingsInitialSection = settingsInitialSection !== undefined;
  const hasSettingsInitialRemoteMachineId = settingsInitialRemoteMachineId !== undefined;
  const hasSettingsInitialSearchQuery = settingsInitialSearchQuery !== undefined;
  const isBaseActiveModalRenderable = isModalRenderable({
    activeModal,
    config,
    delayedSend,
    firstUserMessage,
    gitCommit,
    gitFileDiff,
    worktreeDelete,
    remoteGxserverInstall,
    remoteProjectPicker,
    renameSession,
    settings,
    t3BrowserAccess,
    t3ThreadId,
    worktree,
    portlessSetup,
  });
  /*
  CDXC:PreviousSessions 2026-06-02-20:39:
  The native app-modal host is hidden until React posts `presented`. Previous Sessions must delay that presented signal until its first gxserver history query resolves, proves empty, or hits the two-second cap, otherwise the user sees the empty short modal before loaded rows expand it.
  */
  /*
   * CDXC:SettingsModalStuckBlank 2026-06-20-23:02:
   * Settings must not send native `presented` from the generic modal-ready path
   * while the actual Settings component is still closed on revision 0. Tie
   * Settings-family presentation to the same hydrated renderability condition
   * used by SettingsModal so native cannot believe Settings is open while
   * React is showing no Settings UI.
   */
  const isActiveModalRenderable =
    isBaseActiveModalRenderable &&
    (!isSettingsModal || isSettingsRenderable) &&
    (!isFirstLaunchSetupModal || isFirstLaunchSetupRenderable) &&
    (activeModal !== "previousSessions" || isPreviousSessionsInitialLoadReady);
  /*
   * CDXC:SettingsModalDiagnostics 2026-06-20-20:24:
   * Settings presented diagnostics must not add sidebar revision or hydration
   * fields to the `presented` effect dependencies, because that would re-send
   * native presented messages on ordinary sidebar updates. Keep the latest safe
   * diagnostic payload in a ref while preserving the original present trigger.
   */
  latestSettingsPresentedLogDetailsRef.current = {
    activeModal,
    hasNativeSettingsHydrated,
    hasSettings,
    hasSettingsInitialRemoteMachineId,
    hasSettingsInitialSearchQuery,
    hasSettingsInitialSection,
    isActiveModalRenderable,
    isBaseActiveModalRenderable,
    isSettingsRenderable,
    nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow",
    revision,
    settingsInitialTab,
  };
  latestFirstLaunchSetupPresentedLogDetailsRef.current = {
    activeModal,
    hasNativeSettingsHydrated,
    hasSettings,
    isActiveModalRenderable,
    isBaseActiveModalRenderable,
    isFirstLaunchSetupModal,
    isFirstLaunchSetupRenderable,
    nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow",
    revision,
  };

  useEffect(() => {
    if (!isSettingsModalKind(activeModal)) {
      previousSettingsRenderStateLogRef.current = "";
      return;
    }
    const signature = JSON.stringify({
      activeModal,
      hasNativeSettingsHydrated,
      hasSettings,
      hasSettingsInitialRemoteMachineId,
      hasSettingsInitialSearchQuery,
      hasSettingsInitialSection,
      isActiveModalRenderable,
      isBaseActiveModalRenderable,
      isSettingsRenderable,
      nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow",
      revision,
      settingsInitialTab,
    });
    if (previousSettingsRenderStateLogRef.current === signature) {
      return;
    }
    previousSettingsRenderStateLogRef.current = signature;
    postSettingsModalDebugLog("modalHost.settings.renderState", {
      activeModal,
      hasNativeSettingsHydrated,
      hasSettings,
      hasSettingsInitialRemoteMachineId,
      hasSettingsInitialSearchQuery,
      hasSettingsInitialSection,
      isActiveModalRenderable,
      isBaseActiveModalRenderable,
      isSettingsRenderable,
      nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow",
      revision,
      settingsInitialTab,
    });
  }, [
    activeModal,
    hasNativeSettingsHydrated,
    hasSettings,
    hasSettingsInitialRemoteMachineId,
    hasSettingsInitialSearchQuery,
    hasSettingsInitialSection,
    isActiveModalRenderable,
    isBaseActiveModalRenderable,
    isSettingsRenderable,
    revision,
    settingsInitialTab,
  ]);

  useEffect(() => {
    if (!isFirstLaunchSetupModalKind(activeModal)) {
      previousFirstLaunchSetupRenderStateLogRef.current = "";
      return;
    }
    /*
     * CDXC:FirstLaunchSetupDiagnostics 2026-06-29-22:08:
     * Setup can feel slow before it ever becomes visible because native waits
     * for React renderability before presenting the child NSPanel. Log each
     * distinct setup renderability state with no settings values or user text.
     */
    const signature = JSON.stringify({
      activeModal,
      hasNativeSettingsHydrated,
      hasSettings,
      isActiveModalRenderable,
      isBaseActiveModalRenderable,
      isFirstLaunchSetupRenderable,
      nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow",
      revision,
    });
    if (previousFirstLaunchSetupRenderStateLogRef.current === signature) {
      return;
    }
    previousFirstLaunchSetupRenderStateLogRef.current = signature;
    postAppModalDebugLog("modalHost.setup.renderState", {
      activeModal,
      hasNativeSettingsHydrated,
      hasSettings,
      isActiveModalRenderable,
      isBaseActiveModalRenderable,
      isFirstLaunchSetupRenderable,
      nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow",
      revision,
    });
  }, [
    activeModal,
    hasNativeSettingsHydrated,
    hasSettings,
    isActiveModalRenderable,
    isBaseActiveModalRenderable,
    isFirstLaunchSetupRenderable,
    revision,
  ]);

  useEffect(() => {
    if (activeModal !== "previousSessions") {
      setIsPreviousSessionsInitialLoadReady(false);
    }
  }, [activeModal]);

  const handlePreviousSessionsInitialLoadReady = useCallback(() => {
    setIsPreviousSessionsInitialLoadReady(true);
  }, []);

  useEffect(() => {
    const previousDefaultPromptAgentId = previousDefaultPromptAgentIdRef.current;
    const nextDefaultPromptAgentId = settings?.defaultPromptAgentId;
    previousDefaultPromptAgentIdRef.current = nextDefaultPromptAgentId;
    if (!previousDefaultPromptAgentId || previousDefaultPromptAgentId === nextDefaultPromptAgentId) {
      return;
    }

    /*
     * CDXC:PromptAgents 2026-05-29-10:53:
     * Per-modal prompt-agent choices are temporary overrides. When the global
     * Settings default prompt agent changes, clear every modal override so Git
     * commit review and Rename Generate Name immediately show the new default.
     */
    clearPromptAgentModalOverrides();
    setGitCommitPromptAgentId(undefined);
    setRenamePromptAgentId(undefined);
  }, [settings?.defaultPromptAgentId]);

  const updateGitCommitPromptAgentId = useCallback((agentId: string) => {
    writePromptAgentModalOverride("gitCommit", agentId);
    setGitCommitPromptAgentId(agentId);
  }, []);

  const updateRenamePromptAgentId = useCallback((agentId: string) => {
    writePromptAgentModalOverride("renameSession", agentId);
    setRenamePromptAgentId(agentId);
  }, []);

  useEffect(() => {
    if (!activeModal) {
      sentNativeFitHeightMeasurementKeysRef.current.clear();
    }
  }, [activeModal]);

  useLayoutEffect(() => {
    if (
      window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow" &&
      shouldUseOneShotNativeFitHeight(activeModal)
    ) {
      document.body.dataset.appModalFitHeight = "true";
    } else {
      delete document.body.dataset.appModalFitHeight;
    }
    return () => {
      delete document.body.dataset.appModalFitHeight;
    };
  }, [activeModal]);

  /**
   * CDXC:AppModals 2026-05-08-09:00
   * Native should unhide the transparent modal webview only after the requested
   * modal has enough state to render. This prevents a blank overlay flash while
   * sidebar state is still syncing into the app-modal host.
   *
   * CDXC:AppModals 2026-06-30-16:08:
   * Approved compact native-window modals send their fitted React dialog height
   * once before `presented`, so AppKit can resize the child window without
   * later height churn while the user interacts with the form.
   */
  useLayoutEffect(() => {
    if (!activeModal || !isActiveModalRenderable) {
      return;
    }
    const presentedMessage: { modal: AppModalKind; requestId?: string; type: "presented" } = {
      modal: activeModal,
      type: "presented",
    };
    if (activeModalRequestId) {
      presentedMessage.requestId = activeModalRequestId;
    }
    if (isSettingsModalKind(activeModal)) {
      postSettingsModalDebugLog(
        "modalHost.settings.presented.sent",
        latestSettingsPresentedLogDetailsRef.current,
      );
    }
    if (isFirstLaunchSetupModalKind(activeModal)) {
      postAppModalDebugLog(
        "modalHost.setup.presented.sent",
        latestFirstLaunchSetupPresentedLogDetailsRef.current,
      );
    }
    if (
      window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow" &&
      shouldUseOneShotNativeFitHeight(activeModal)
    ) {
      const measurementKey = `${activeModal}:${activeModalRequestId ?? "none"}`;
      if (!sentNativeFitHeightMeasurementKeysRef.current.has(measurementKey)) {
        const measuredHeight = measureOneShotNativeFitHeight(activeModal);
        if (measuredHeight) {
          sentNativeFitHeightMeasurementKeysRef.current.add(measurementKey);
          const contentHeightMeasuredMessage: {
            height: number;
            modal: AppModalKind;
            nativeWindowHostId?: string;
            requestId?: string;
            type: "contentHeightMeasured";
          } = {
            height: measuredHeight,
            modal: activeModal,
            type: "contentHeightMeasured",
          };
          if (window.__ghostex_APP_MODAL_HOST_ID__) {
            contentHeightMeasuredMessage.nativeWindowHostId = window.__ghostex_APP_MODAL_HOST_ID__;
          }
          if (activeModalRequestId) {
            contentHeightMeasuredMessage.requestId = activeModalRequestId;
          }
          postAppModalHostMessage(
            contentHeightMeasuredMessage,
            "AppModals:contentHeightMeasured",
          );
        }
      }
    }
    postAppModalHostMessage(presentedMessage, "AppModals:presented");
  }, [activeModal, activeModalRequestId, isActiveModalRenderable]);

  useEffect(() => {
    if (activeModal !== "settings") {
      setGhostexFolderStatsLoading(false);
    }
  }, [activeModal]);

  useEffect(() => {
    if (!activeModal) {
      return;
    }

    const suppressModalWebviewContextMenu = (event: MouseEvent) => {
      if (isEditableAppModalContextMenuTarget(event.target)) {
        return;
      }

      /**
       * CDXC:AppModalContextMenu 2026-05-15-18:15:
       * Right-clicking modal backdrops, blank modal chrome, or modal buttons
       * must not expose WKWebView's native Reload menu. Suppress the webview
       * default while a modal is active, but keep editable fields eligible for
       * their normal editing context menus.
       */
      event.preventDefault();
    };

    document.addEventListener("contextmenu", suppressModalWebviewContextMenu, true);
    return () => {
      document.removeEventListener("contextmenu", suppressModalWebviewContextMenu, true);
    };
  }, [activeModal]);

  useEffect(() => {
    if (ghostexFolderStats) {
      setGhostexFolderStatsLoading(false);
    }
  }, [ghostexFolderStats]);

  useEffect(() => {
    if (agentHookStatus) {
      setAgentHookStatusLoading(false);
    }
  }, [agentHookStatus]);

  useEffect(() => {
    if (ghostexCliStatus) {
      setGhostexCliStatusLoading(false);
    }
  }, [ghostexCliStatus]);

  useEffect(() => {
    if (osIntegrationStatus) {
      setOSIntegrationStatusLoading(false);
    }
  }, [osIntegrationStatus]);

  useEffect(() => {
    if (activeModal !== "firstLaunchSetup" && activeModal !== "tipsAndTricks") {
      setGhostexCliStatusLoading(false);
      return;
    }
    if (ghostexCliStatus || ghostexCliStatusLoading) {
      return;
    }
    /**
     * CDXC:FirstLaunchSetup 2026-05-26-17:12:
     * The production first-launch modal should reflect the app-bundled CLI that
     * native auto-links on startup. Request native PATH inspection when the setup
     * flow opens and render Storybook through the same status prop.
     *
     * CDXC:FirstLaunchSetup 2026-05-27-02:41:
     * Tips & Tricks now opens the first-launch modal, so the legacy modal id must
     * receive the same CLI status request while old menu messages are still in use.
     */
    setGhostexCliStatusLoading(true);
    vscode.postMessage({ type: "requestGhostexCliStatus" });
  }, [activeModal, ghostexCliStatus, ghostexCliStatusLoading]);

  useEffect(() => {
    document.body.dataset.sidebarTheme = theme;
    const normalizedThemeColor = normalizeWorkspaceThemeColor(customThemeColor);
    if (normalizedThemeColor) {
      document.body.dataset.sidebarCustomTheme = "true";
      document.body.style.setProperty("--workspace-sidebar-theme-color", normalizedThemeColor);
      document.body.style.setProperty(
        "--workspace-sidebar-theme-foreground",
        getWorkspaceThemeForeground(normalizedThemeColor),
      );
    } else {
      delete document.body.dataset.sidebarCustomTheme;
      document.body.style.removeProperty("--workspace-sidebar-theme-color");
      document.body.style.removeProperty("--workspace-sidebar-theme-foreground");
    }

    return () => {
      delete document.body.dataset.sidebarTheme;
      delete document.body.dataset.sidebarCustomTheme;
      document.body.style.removeProperty("--workspace-sidebar-theme-color");
      document.body.style.removeProperty("--workspace-sidebar-theme-foreground");
    };
  }, [customThemeColor, theme]);

  return (
    <>
      <PreviousSessionsModal
        isOpen={activeModal === "previousSessions"}
        onClose={closeModal}
        onInitialLoadReady={handlePreviousSessionsInitialLoadReady}
        vscode={vscode}
      />
      <PinnedPromptsModal
        isOpen={activeModal === "pinnedPrompts"}
        onClose={closeModal}
        vscode={vscode}
      />
      <FirstUserMessageModal
        isOpen={activeModal === "firstUserMessage" && firstUserMessage !== undefined}
        message={firstUserMessage?.message ?? ""}
        onClose={closeModal}
        title={firstUserMessage?.title}
      />
      <RemoteGxserverInstallModal
        isOpen={activeModal === "remoteGxserverInstall" && remoteGxserverInstall !== undefined}
        machineName={remoteGxserverInstall?.remoteMachineName ?? "Remote"}
        onApprove={() => {
          if (!remoteGxserverInstall) {
            postRemoteGxserverInstallDebugLog("remoteGxserverInstall.approve.missingState", {
              activeModal: activeModal ?? null,
              hasRemoteGxserverInstall: false,
              nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow",
            });
            return;
          }
          postRemoteGxserverInstallDebugLog("remoteGxserverInstall.approve.clicked", {
            activeModal: activeModal ?? null,
            hasRemoteGxserverInstall: true,
            installApproved: true,
            nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow",
            remoteMachineId: remoteGxserverInstall.remoteMachineId,
          });
          vscode.postMessage({
            installApproved: true,
            remoteMachineId: remoteGxserverInstall.remoteMachineId,
            type: "reconnectRemoteMachine",
          });
          postRemoteGxserverInstallDebugLog("remoteGxserverInstall.approve.commandPosted", {
            activeModal: activeModal ?? null,
            installApproved: true,
            remoteMachineId: remoteGxserverInstall.remoteMachineId,
          });
          closeModal();
        }}
        onCancel={closeModal}
      />
      <RemoteProjectPickerModal
        initialQuery={remoteProjectPicker?.initialQuery}
        isOpen={activeModal === "remoteProjectPicker" && remoteProjectPicker !== undefined}
        machineName={remoteProjectPicker?.remoteMachineName ?? "Remote"}
        onAddProject={async (path) => {
          if (!remoteProjectPicker) {
            return;
          }
          const requestId = createRemoteProjectRequestId("add");
          vscode.postMessage({
            path,
            remoteMachineId: remoteProjectPicker.remoteMachineId,
            requestId,
            type: "addRemoteProjectPath",
          });
          await waitForRemoteProjectAddResult(requestId);
        }}
        onBrowse={async (input) => {
          if (!remoteProjectPicker) {
            return null;
          }
          const requestId = createRemoteProjectRequestId("browse");
          vscode.postMessage({
            partialPath: input.partialPath,
            remoteMachineId: remoteProjectPicker.remoteMachineId,
            requestId,
            type: "browseRemoteProjectDirectories",
          });
          return waitForRemoteProjectDirectoryBrowseResult(requestId);
        }}
        onClose={closeModal}
      />
      <DaemonSessionsModal
        isOpen={activeModal === "daemonSessions"}
        onClose={closeModal}
        vscode={vscode}
      />
      <AgentsHubModal
        catalog={agentsHubCatalog}
        fileContent={agentsHubFileContent}
        isOpen={activeModal === "agentsHub"}
        onClose={closeModal}
        vscode={vscode}
      />
      {/*
       * CDXC:CommandPalette 2026-06-13-10:26:
       * The configured command-palette hotkey must render in the same
       * full-window app-modal host as Settings, not inside the sidebar webview.
       * The palette reads mirrored sidebar state here so its command list
       * remains current while the dialog is centered over the whole Ghostex
       * window.
      */}
      <CommandPalette
        collapsedGroupsById={commandPaletteCollapsedGroupsById}
        commands={commands}
        hotkeys={settings?.hotkeys}
        initialQuery={commandPaletteInitialQuery}
        isOpen={activeModal === "commandPalette"}
        isPrewarm={isCommandPalettePrewarm}
        onOpenChange={(isOpen) => {
          if (!isOpen) {
            closeModal();
          }
        }}
        openRequestSequence={commandPaletteOpenRequestSequence}
        openTargetSettings={settings}
        petOverlayEnabled={settings?.petOverlayEnabled}
        vscode={vscode}
      />
      <DelayedSendModal
        delayedSendDeadlineAt={delayedSend?.delayedSendDeadlineAt}
        delayedSendRemainingLabel={delayedSend?.delayedSendRemainingLabel}
        isOpen={activeModal === "delayedSend" && delayedSend !== undefined}
        onCancel={closeModal}
        onCancelTimer={() => {
          if (!delayedSend) {
            return;
          }
          vscode.postMessage({
            sessionId: delayedSend.sessionId,
            type: "cancelDelayedSend",
          });
          closeModal();
        }}
        onConfirm={(delayMs) => {
          if (!delayedSend) {
            return;
          }
          vscode.postMessage({
            delayMs,
            sessionId: delayedSend.sessionId,
            type: "scheduleDelayedSend",
          });
          closeModal();
        }}
        sessionTitle={delayedSend?.title}
      />
      <GitCommitModal
        agents={agents}
        draft={
          gitCommit ?? {
            confirmLabel: "Commit",
            description: "",
            changedFiles: [],
            requestId: "",
            showCommitMessage: true,
            suggestedBody: undefined,
            suggestedSubject: "",
          }
        }
        isOpen={activeModal === "gitCommit" && gitCommit !== undefined}
        fileDiffDraft={gitFileDiff}
        onCancel={(requestId) => {
          vscode.postMessage({ requestId, type: "cancelSidebarGitCommit" });
          closeModal();
        }}
        onConfirm={(requestId, message, options) => {
          vscode.postMessage({
            agentId: options.agentId,
            commitOnNewRef: options.commitOnNewRef,
            deleteWorktreeAfter: options.deleteWorktreeAfter,
            filePaths: options.filePaths,
            message,
            requestId,
            type: "confirmSidebarGitCommit",
          });
          closeModal();
        }}
        onDirectMerge={(requestId, message, options) => {
          vscode.postMessage({
            agentId: options.agentId,
            deleteWorktreeAfter: options.deleteWorktreeAfter,
            filePaths: options.filePaths,
            message,
            requestId,
            type: "confirmSidebarGitDirectMerge",
          });
          closeModal();
        }}
        onMultipleCommits={(requestId, agentId) => {
          vscode.postMessage({ agentId, requestId, type: "runSidebarGitMultipleCommits" });
          closeModal();
        }}
        onOpenFileDiff={(filePath, requestId) => {
          vscode.postMessage({ filePath, requestId, type: "openSidebarGitChangedFileDiff" });
        }}
        onPromptAgentIdChange={updateGitCommitPromptAgentId}
        promptAgentId={resolvedGitCommitPromptAgentId}
        theme={theme}
      />
      {activeModal === "gitCommit" ? null : (
        <GitFileDiffModal
          draft={
            gitFileDiff ?? {
              filePath: "",
              patch: "No diff is available for this file.",
            }
          }
          isOpen={gitFileDiff !== undefined}
          onClose={closeGitFileDiff}
          theme={theme}
        />
      )}
      <WorktreeDeleteModal
        draft={
          worktreeDelete ?? {
            branch: null,
            canDeleteLocalBranch: false,
            groupId: "",
            hasChanges: false,
            projectId: "",
            remoteBranchExists: false,
            statusSummary: "",
            worktreeName: "worktree",
          }
        }
        isOpen={activeModal === "deleteWorktree" && worktreeDelete !== undefined}
        onCancel={closeModal}
        onCommit={(groupId) => {
          vscode.postMessage({ groupId, type: "commitWorktreeBeforeDelete" });
          closeModal();
        }}
        onDelete={(projectId, options) => {
          vscode.postMessage({
            deleteLocalBranch: options.deleteLocalBranch,
            deleteRemoteBranch: options.deleteRemoteBranch,
            projectId,
            type: "confirmDeleteWorktree",
          });
          closeModal();
        }}
        theme={theme}
      />
      {/*
       * CDXC:Worktrees 2026-06-02-13:41:
       * Creating a project worktree is a full-window modal flow because macOS
       * owns the agent, first prompt, and image attachment drafts before submit,
       * while gxserver owns the branch/worktree mutation and returned project.
       *
       * CDXC:GPUIWorktrees 2026-06-24-14:06:
       * Open Existing mode shares the worktree first-prompt controls. Blank
       * prompt submits remain project-open-only; non-blank prompts carry the
       * user-selected agent and prompt alongside the selected worktree path so
       * native and GPUI receivers can start the actual agent session.
       *
       * CDXC:WorktreeBaseBranch 2026-06-24-11:32:
       * Create New mode must send the selected base branch through the sidebar
       * command so the worktree starts from the chosen branch instead of the
       * currently checked-out HEAD.
       */}
      <WorktreeCreateModal
        agents={agents}
        defaultAgentId={settings?.defaultPromptAgentId}
        isOpen={activeModal === "worktree" && worktree !== undefined}
        onCancel={closeModal}
        onConfirm={(draft) => {
          vscode.postMessage({
            agentId: draft.agentId,
            baseBranch: draft.mode === "create" ? draft.baseBranch : undefined,
            existingWorktreeKey:
              draft.mode === "openExisting" ? draft.existingWorktreeKey : undefined,
            existingWorktreePath:
              draft.mode === "openExisting" ? draft.existingWorktreePath : undefined,
            mode: draft.mode,
            projectId: worktree?.projectId,
            projectPath: worktree?.projectPath,
            prompt: draft.prompt,
            remoteMachineId: worktree?.remoteMachineId,
            type: "createProjectWorktree",
          } satisfies SidebarToExtensionMessage);
          closeModal();
        }}
        onRequestExistingWorktrees={(requestId) => {
          vscode.postMessage({
            projectId: worktree?.projectId,
            projectPath: worktree?.projectPath,
            remoteMachineId: worktree?.remoteMachineId,
            requestId,
            type: "requestProjectWorktrees",
          } satisfies SidebarToExtensionMessage);
        }}
        projectName={worktree?.projectName}
      />
      <PortlessSetupModal
        isOpen={activeModal === "portlessSetup" && portlessSetup !== undefined}
        mode={portlessSetup?.mode ?? "firstSetup"}
        onAdminAction={(action, protocol, requestId) => {
          vscode.postMessage({
            action,
            protocol,
            requestId,
            type: "runPortlessSetupPromptAdminAction",
          } satisfies SidebarToExtensionMessage);
          closeModal();
        }}
        onCancel={() => {
          vscode.postMessage({ type: "cancelPortlessSetupPrompt" } satisfies SidebarToExtensionMessage);
          closeModal();
        }}
        onDisable={() => {
          vscode.postMessage({
            enabled: false,
            type: "setPortlessEnabled",
          } satisfies SidebarToExtensionMessage);
          closeModal();
        }}
        onPostpone={() => {
          vscode.postMessage({ type: "postponePortlessSetupPrompt" } satisfies SidebarToExtensionMessage);
          closeModal();
        }}
        protocol={portlessSetup?.protocol ?? "https"}
      />
      <ScratchPadModal
        isOpen={activeModal === "scratchPad"}
        onClose={closeModal}
        onDebug={(event, details) => {
          /**
           * CDXC:ScratchPadFocus 2026-04-28-05:21
           * Scratch Pad focus repros run inside the full-window modal host, not
           * the narrow sidebar webview. Forward those modal-host events through
           * the normal sidebar command bridge so native logs can correlate
           * textarea blur/focus with terminal first-responder changes.
           */
          vscode.postMessage({
            details,
            event,
            type: "sidebarDebugLog",
          });
        }}
        onSave={(content) => {
          vscode.postMessage({
            content,
            type: "saveScratchPad",
          });
        }}
      />
      <SettingsModal
        agentHookStatus={agentHookStatus}
        agentHookStatusLoading={agentHookStatusLoading}
        appIconPickerUnavailable={appIconPickerUnavailable}
        initialSection={settingsInitialSection}
        initialRemoteMachineId={settingsInitialRemoteMachineId}
        initialSearchQuery={settingsInitialSearchQuery}
        initialTab={settingsInitialTab}
        isOpen={isSettingsRenderable}
        onChange={(nextSettings, source = "settings:bulk") => {
          vscode.postMessage({
            settings: nextSettings,
            source,
            type: "updateSettings",
          });
        }}
        onPatch={(patch, source) => {
          vscode.postMessage({
            baseRevision: revision,
            patch,
            source,
            type: "updateSettingsPatch",
          });
        }}
        onGhosttySettingsAction={(action) => {
          vscode.postMessage({ type: action });
        }}
        onInstallGhostexCli={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installGhostexCli" });
        }}
        onInstallBrowserControl={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installBrowserControl" });
        }}
        onInstallComputerUseSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installComputerUseSkill" });
        }}
        onInstallAgentOrchestrationSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installAgentOrchestrationSkill" });
        }}
        onInstallFable56OrchestrationSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installFable56OrchestrationSkill" });
        }}
        onInstallGenerateTitleSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installGenerateTitleSkill" });
        }}
        onInstallMoveCodexSessionSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installMoveCodexSessionSkill" });
        }}
        onInstallCuaDriver={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installCuaDriver" });
        }}
        onSetOSIntegrationDefaults={(target) => {
          setOSIntegrationStatusLoading(true);
          vscode.postMessage({ target, type: "setOSIntegrationDefaults" });
        }}
        onPlayCompletionSound={(sound) => {
          vscode.postMessage({ sound, type: "playCompletionSoundPreview" });
        }}
        onOpenAccessibilityPreferences={() => {
          /**
           * CDXC:AccessibilityPermissions 2026-05-27-07:24
           * The settings modal button should open macOS Accessibility settings
           * directly for desktop integrations without enabling any removed
           * IDE attachment behavior.
           */
          vscode.postMessage({ type: "openAccessibilityPreferences" });
        }}
        onOpenMacOSNotificationSettings={() => {
          vscode.postMessage({ type: "openMacOSNotificationSettings" });
        }}
        onOpenScreenRecordingPreferences={() => {
          vscode.postMessage({ type: "openScreenRecordingPreferences" });
        }}
        onOpenGhostexFolder={() => {
          vscode.postMessage({ type: "openGhostexFolder" });
        }}
        onRequestMacOSNotificationPermission={() => {
          vscode.postMessage({ type: "requestMacOSNotificationPermission" });
        }}
        onRequestGhostexFolderStats={() => {
          setGhostexFolderStatsLoading(true);
          vscode.postMessage({ type: "requestGhostexFolderStats" });
        }}
        onRequestAgentHookStatus={() => {
          setAgentHookStatusLoading(true);
          vscode.postMessage({ type: "requestAgentHookStatus" });
        }}
        onRequestGhostexCliStatus={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "requestGhostexCliStatus" });
        }}
        onRequestOSIntegrationStatus={() => {
          setOSIntegrationStatusLoading(true);
          vscode.postMessage({ type: "requestOSIntegrationStatus" });
        }}
        onInstallAgentHooks={() => {
          setAgentHookStatusLoading(true);
          vscode.postMessage({ type: "installAgentHooks" });
        }}
        onUninstallAgentHooks={() => {
          setAgentHookStatusLoading(true);
          vscode.postMessage({ type: "uninstallAgentHooks" });
        }}
        onUninstallBundledAgentSkills={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "uninstallBundledAgentSkills" });
        }}
        onTestAgentTaskCompletion={() => {
          vscode.postMessage({ type: "testAgentTaskCompletion" });
        }}
        onClose={closeModal}
        portless={portless}
        projects={projectSettingsProjects}
        settings={settings}
        vscode={vscode}
        ghostexCliStatus={ghostexCliStatus}
        ghostexCliStatusLoading={ghostexCliStatusLoading}
        ghostexFolderStats={ghostexFolderStats}
        ghostexFolderStatsLoading={ghostexFolderStatsLoading}
        osIntegrationStatus={osIntegrationStatus}
        osIntegrationStatusLoading={osIntegrationStatusLoading}
        // CDXC:AppIconPicker 2026-06-25-21:50: Prop-driven App Icon state for Settings (mirrors osIntegrationStatus).
        appIconState={appIconState}
        doctorChecks={doctorChecks}
        doctorLoading={doctorLoading}
        diagnosticsJson={diagnosticsJson}
        diagnosticsLoading={diagnosticsLoading}
        onRunDoctor={() => {
          setDoctorLoading(true);
          vscode.postMessage({ type: "runDoctor" });
        }}
        onApplyDoctorFix={(fixId, confirmationToken) => {
          vscode.postMessage({ confirmationToken, fixId, type: "applyDoctorFix" });
        }}
        onExportDiagnostics={() => {
          setDiagnosticsLoading(true);
          vscode.postMessage({ type: "exportDiagnostics" });
        }}
      />
      <DiscoverGhostexModal
        isOpen={activeModal === "discoverGhostex"}
        onClose={closeModal}
        theme={theme}
      />
      <WatchGhostexVideoModal
        isOpen={activeModal === "watchGhostexVideo"}
        onClose={closeModal}
        theme={theme}
      />
      <FirstLaunchSetupModal
        agentHookStatus={agentHookStatus}
        agentHookStatusLoading={agentHookStatusLoading}
        ghostexCliStatus={ghostexCliStatus}
        ghostexCliStatusLoading={ghostexCliStatusLoading}
        isOpen={isFirstLaunchSetupRenderable}
        onChange={(nextSettings) => {
          vscode.postMessage({
            settings: nextSettings,
            source: "firstLaunch:preferences",
            type: "updateSettings",
          });
        }}
        onClose={closeModal}
        onInstallAgentHooks={(agentIds) => {
          setAgentHookStatusLoading(true);
          vscode.postMessage({ agentIds, type: "installAgentHooks" });
        }}
        onInstallGhostexCli={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installGhostexCli" });
        }}
        onInstallBrowserControl={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installBrowserControl" });
        }}
        onInstallComputerUseSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installComputerUseSkill" });
        }}
        onInstallAgentOrchestrationSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installAgentOrchestrationSkill" });
        }}
        onInstallFable56OrchestrationSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installFable56OrchestrationSkill" });
        }}
        onInstallGenerateTitleSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installGenerateTitleSkill" });
        }}
        onInstallMoveCodexSessionSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installMoveCodexSessionSkill" });
        }}
        onInstallCuaDriver={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installCuaDriver" });
        }}
        onOpenAccessibilityPreferences={() => {
          vscode.postMessage({ type: "openAccessibilityPreferences" });
        }}
        onOpenScreenRecordingPreferences={() => {
          vscode.postMessage({ type: "openScreenRecordingPreferences" });
        }}
        onRequestAgentHookStatus={(agentIds) => {
          setAgentHookStatusLoading(true);
          vscode.postMessage({ agentIds, type: "requestAgentHookStatus" });
        }}
        onRequestGhostexCliStatus={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "requestGhostexCliStatus" });
        }}
        settings={settings}
        theme={theme}
        vscode={vscode}
      />
      <T3ThreadIdModal
        currentThreadId={t3ThreadId?.currentThreadId ?? ""}
        isOpen={activeModal === "t3ThreadId" && t3ThreadId !== undefined}
        onCancel={closeModal}
        onConfirm={(threadId) => {
          if (!t3ThreadId) {
            return;
          }
          vscode.postMessage({
            sessionId: t3ThreadId.sessionId,
            threadId,
            type: "setT3SessionThreadId",
          });
          closeModal();
        }}
      />
      <T3BrowserAccessModal
        access={t3BrowserAccess}
        isOpen={activeModal === "t3BrowserAccess" && t3BrowserAccess !== undefined}
        onClose={closeModal}
        onOpenLink={(url) => {
          vscode.postMessage({
            type: "openT3SessionBrowserAccessLink",
            url,
          });
        }}
      />
      <SessionRenameModal
        agents={agents}
        initialTitle={renameSession?.initialTitle ?? ""}
        isOpen={activeModal === "renameSession" && renameSession !== undefined}
        onCancel={closeModal}
        onConfirm={(title, options) => {
          if (!renameSession) {
            return;
          }
          vscode.postMessage({
            agentId: options?.agentId,
            sessionId: renameSession.sessionId,
            ...(options?.shouldGenerateTitle ? { shouldGenerateTitle: true } : {}),
            title,
            type: "renameSession",
          });
          closeModal();
        }}
        onPromptAgentIdChange={updateRenamePromptAgentId}
        promptAgentId={resolvedRenamePromptAgentId}
      />
      <AddRepositoryModal
        isOpen={activeModal === "addRepository"}
        remoteMachineId={addRepository.remoteMachineId}
        remoteMachineName={addRepository.remoteMachineName}
        onCancel={closeModal}
        onClone={(request) => {
          /*
           * CDXC:AddRepository 2026-06-01-10:33:
           * Clone & Add should leave the dialog immediately and move long-running
           * Git feedback into the app toast layer, including cancellation. Native
           * owns clone progress and final success/error toasts after this message.
           */
          vscode.postMessage({
            branchName: request.branchName,
            cloneMainOnly: request.cloneMainOnly,
            folderPath: request.folderPath,
            newFolderName: request.newFolderName,
            remoteMachineId: addRepository.remoteMachineId,
            repositoryInput: request.repositoryInput,
            requestId: request.requestId,
            shallowClone: request.shallowClone,
            type: "cloneRepository",
          });
          closeModal();
        }}
        onCloneSuccess={closeModal}
        onRemoteBrowse={
          addRepository.remoteMachineId
            ? async (input) => {
                if (!addRepository.remoteMachineId) {
                  return null;
                }
                const requestId = createRemoteProjectRequestId("browse");
                vscode.postMessage({
                  partialPath: input.partialPath,
                  remoteMachineId: addRepository.remoteMachineId,
                  requestId,
                  type: "browseRemoteProjectDirectories",
                });
                return waitForRemoteProjectDirectoryBrowseResult(requestId);
              }
            : undefined
        }
        onPreview={(request) => {
          vscode.postMessage({
            folderPath: request.folderPath,
            newFolderName: request.newFolderName,
            remoteMachineId: addRepository.remoteMachineId,
            repositoryInput: request.repositoryInput,
            requestId: request.requestId,
            type: "previewRepositoryClone",
          });
        }}
      />
      <AgentConfigModal
        draft={config.agentDraft ?? createEmptyAgentDraft()}
        isOpen={activeModal === "agentConfig" && config.agentDraft !== undefined}
        onCancel={closeModal}
        onSave={(draft) => {
          vscode.postMessage({
            acceptAllMode: draft.acceptAllMode,
            agentId: draft.agentId,
            command: draft.command,
            icon: draft.icon,
            name: draft.name,
            type: "saveSidebarAgent",
          });
          closeModal();
        }}
        theme={theme}
      />
      {/*
       * CDXC:AppToasts 2026-05-21-12:21:
       * Native/sidebar status feedback should appear as dark Ghostex toasts,
       * not Sonner's bright default surface, so non-blocking Delayed Send and
       * worktree/git notices stay visually consistent with the dark app chrome.
       *
       * CDXC:AppModals 2026-05-28-13:52:
       * Toast overlay chrome should use the same background family as modal
       * and menu overlays instead of the older #181818 surface.
       *
       * CDXC:SidebarTheme 2026-06-15-01:43:
       * Toasts inherit --app-modal-background so Dark 1, Dark 2, and Light
       * keep transient modal-host feedback on the selected app surface.
       */}
      <Toaster
        offset={{ bottom: APP_MODAL_TOAST_BOTTOM_OFFSET_PX }}
        position="bottom-center"
        richColors
        theme="dark"
        toastOptions={{
          style: {
            background: "var(--app-modal-background)",
            border: "1px solid rgba(255, 255, 255, 0.14)",
            color: "#f4f4f5",
          },
        }}
      />
    </>
  );
}

/**
 * CDXC:AppModals 2026-04-26-15:10
 * Sidebar-owned modals must render from a full-window host so settings and
 * other management dialogs center over the whole application instead of being
 * constrained by the narrow sidebar WKWebView.
 */
function useModalStateFromNative() {
  const [activeModal, setActiveModal] = useState<AppModalKind | undefined>();
  /*
   * CDXC:CommandPalette 2026-06-13-09:53:
   * Native command-palette prewarm opens the real modal host while hidden.
   * Preserve the request id through React state so the presented event lets
   * AppKit hide the warmed host instead of showing it to the user.
   */
  const [activeModalRequestId, setActiveModalRequestId] = useState<string>();
  const [addRepository, setAddRepository] = useState<AddRepositoryModalState>({});
  const [agentsHubCatalog, setAgentsHubCatalog] = useState<AgentsHubCatalogMessage>();
  const [agentsHubFileContent, setAgentsHubFileContent] =
    useState<AgentsHubFileContentMessage>();
  const [config, setConfig] = useState<ConfigModalState>({});
  const [delayedSend, setDelayedSend] = useState<DelayedSendModalState>();
  const [firstUserMessage, setFirstUserMessage] = useState<FirstUserMessageModalState>();
  const [gitCommit, setGitCommit] = useState<GitCommitModalDraft>();
  const [gitFileDiff, setGitFileDiff] = useState<GitFileDiffModalDraft>();
  const [worktreeDelete, setWorktreeDelete] = useState<WorktreeDeleteModalDraft>();
  const [remoteGxserverInstall, setRemoteGxserverInstall] =
    useState<RemoteGxserverInstallState>();
  const [remoteProjectPicker, setRemoteProjectPicker] = useState<RemoteProjectPickerState>();
  const [renameSession, setRenameSession] = useState<RenameSessionModalState>();
  const [t3BrowserAccess, setT3BrowserAccess] = useState<T3BrowserAccessMessage>();
  const [t3ThreadId, setT3ThreadId] = useState<T3ThreadIdModalState>();
  const [worktree, setWorktree] = useState<WorktreeModalState>();
  const [portlessSetup, setPortlessSetup] = useState<PortlessSetupModalState>();
  const [agentHookStatus, setAgentHookStatus] = useState<AgentHookStatusMessage>();
  const [commandPaletteCollapsedGroupsById, setCommandPaletteCollapsedGroupsById] = useState<
    Record<string, true>
  >({});
  const [commandPaletteInitialQuery, setCommandPaletteInitialQuery] = useState("");
  const [commandPaletteOpenRequestSequence, setCommandPaletteOpenRequestSequence] = useState(0);
  const [isCommandPalettePrewarm, setIsCommandPalettePrewarm] = useState(false);
  const [ghostexCliStatus, setGhostexCliStatus] = useState<GhostexCliStatusMessage>();
  const [ghostexFolderStats, setGhostexFolderStats] = useState<SidebarGhostexFolderStatsMessage>();
  const [osIntegrationStatus, setOSIntegrationStatus] = useState<OSIntegrationStatusMessage>();
  // CDXC:AppIconPicker 2026-06-25-21:50: Latest native App Icon state passed to Settings.
  const [appIconState, setAppIconState] = useState<AppIconStateMessage>();
  const [settingsInitialSection, setSettingsInitialSection] =
    useState<MainSettingsInitialSectionId>();
  const [settingsInitialRemoteMachineId, setSettingsInitialRemoteMachineId] = useState<string>();
  const [settingsInitialSearchQuery, setSettingsInitialSearchQuery] = useState<string>();
  const [settingsInitialTabOverride, setSettingsInitialTabOverride] = useState<SettingsModalTab>();
  const activeModalRef = useRef<AppModalKind | undefined>(activeModal);
  const toastTokenRef = useRef(0);

  const clearActiveModalState = useCallback(() => {
    setActiveModal(undefined);
    setActiveModalRequestId(undefined);
    setAddRepository({});
    setConfig({});
    setDelayedSend(undefined);
    setFirstUserMessage(undefined);
    setGitCommit(undefined);
    setGitFileDiff(undefined);
    setWorktreeDelete(undefined);
    setRemoteGxserverInstall(undefined);
    setRemoteProjectPicker(undefined);
    setRenameSession(undefined);
    setT3BrowserAccess(undefined);
    setT3ThreadId(undefined);
    setWorktree(undefined);
    setPortlessSetup(undefined);
    setGhostexFolderStats(undefined);
    setOSIntegrationStatus(undefined);
    // CDXC:AppIconPicker 2026-06-25-21:50: Drop stale App Icon state when the modal closes.
    setAppIconState(undefined);
    setAgentsHubCatalog(undefined);
    setAgentsHubFileContent(undefined);
    setCommandPaletteCollapsedGroupsById({});
    setCommandPaletteInitialQuery("");
    setCommandPaletteOpenRequestSequence(0);
    setIsCommandPalettePrewarm(false);
    setSettingsInitialSection(undefined);
    setSettingsInitialRemoteMachineId(undefined);
    setSettingsInitialSearchQuery(undefined);
    setSettingsInitialTabOverride(undefined);
  }, []);

  const closeModal = useCallback(() => {
    /**
     * CDXC:AppModals 2026-05-22-16:55:
     * Modal controls such as Previous Sessions Escape and the X button must
     * dismiss the React dialog immediately, then notify native to hide the
     * transparent modal-host WKWebView. Do not require the native echo before
     * clearing visible modal state.
     */
    clearActiveModalState();
    notifyNativeModalClosed();
  }, [clearActiveModalState]);

  const closeGitFileDiff = useCallback(() => {
    setGitFileDiff(undefined);
  }, []);

  useEffect(() => {
    activeModalRef.current = activeModal;
  }, [activeModal]);

  useEffect(() => {
    const handleMessage = (event: Event) => {
      try {
        const message = (event as CustomEvent<AppModalHostMessage>).detail;
        if (!message || typeof message !== "object") {
          return;
        }

        if (message.type === "open") {
          const hasInlineSidebarStateMessage = message.latestSidebarStateMessage !== undefined;
          const shouldApplyInlineSidebarState = shouldApplySidebarStateBeforeModalOpen(message.modal);
          if (shouldApplyInlineSidebarState && hasInlineSidebarStateMessage) {
            /*
             * CDXC:SettingsModalStuckBlank 2026-06-20-23:02:
             * Settings opens must apply the native window's latest sidebar
             * snapshot before setting activeModal. This keeps Debugging Mode,
             * revision, and settings data in the modal host before React decides
             * whether the Settings component can actually render.
             *
             * CDXC:FirstLaunchSetup 2026-06-29-13:46:
             * The first-launch setup modal uses the same hydrated settings store,
             * so it must receive the inline native snapshot before activeModal is
             * set and before native waits for the React presented acknowledgement.
            */
            applySidebarStateMessage(message.latestSidebarStateMessage);
          }
          const sidebarStateAtOpen = useSidebarStore.getState();
          if (isAppModalDebugLoggingEnabled()) {
            postAppModalHostMessage(
              {
                details: JSON.stringify({
                  hasSettings: sidebarStateAtOpen.hud.settings !== undefined,
                  inlineSidebarStateApplied:
                    shouldApplyInlineSidebarState && hasInlineSidebarStateMessage,
                  modal: message.modal,
                  performanceNow: performance.now(),
                }),
                event: "modalHost.open.received",
                type: "debugLog",
              },
              "AppModals:debug",
            );
          }
          if (isSettingsModalKind(message.modal)) {
            postSettingsModalDebugLog("modalHost.settings.open.received", {
              activeModalBeforeOpen: activeModalRef.current ?? null,
              hasInitialRemoteMachineId:
                typeof message.initialRemoteMachineId === "string" &&
                message.initialRemoteMachineId.trim().length > 0,
              hasInitialSearchQuery: typeof message.initialSearchQuery === "string",
              hasSettings: sidebarStateAtOpen.hud.settings !== undefined,
              hasInlineSidebarStateMessage: message.latestSidebarStateMessage !== undefined,
              initialSection:
                typeof message.initialSection === "string" ? message.initialSection : null,
              initialTab: isSettingsModalTab(message.initialTab) ? message.initialTab : null,
              modal: message.modal,
              nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow",
              revision: sidebarStateAtOpen.revision,
            });
          }
          if (isFirstLaunchSetupModalKind(message.modal)) {
            /*
             * CDXC:FirstLaunchSetupDiagnostics 2026-06-29-22:08:
             * Capture the setup open boundary after any inline sidebar-state
             * hydrate has applied so a slow repro can tell whether React already
             * has settings state before renderability waits begin.
             */
            postAppModalDebugLog("modalHost.setup.open.received", {
              activeModalBeforeOpen: activeModalRef.current ?? null,
              hasInlineSidebarStateMessage,
              hasNativeSettingsHydrated: sidebarStateAtOpen.revision > 0,
              hasSettings: sidebarStateAtOpen.hud.settings !== undefined,
              inlineSidebarStateApplied: shouldApplyInlineSidebarState && hasInlineSidebarStateMessage,
              modal: message.modal,
              nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow",
              revision: sidebarStateAtOpen.revision,
            });
          }
          if (message.modal === "renameSession") {
            if (!message.sessionId) {
              throw new Error("Rename modal request is missing sessionId.");
            }
            setRenameSession({
              initialTitle: message.initialTitle ?? "",
              sessionId: message.sessionId,
            });
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
                    setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
          } else if (message.modal === "firstUserMessage") {
            if (typeof message.message !== "string" || !message.message.trim()) {
              throw new Error("First message modal request is missing message text.");
            }
            setFirstUserMessage({
              message: message.message,
              title: typeof message.title === "string" ? message.title : undefined,
            });
            setConfig({});
            setDelayedSend(undefined);
                    setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
          } else if (message.modal === "remoteGxserverInstall") {
            if (
              typeof message.remoteMachineId !== "string" ||
              !message.remoteMachineId.trim() ||
              typeof message.remoteMachineName !== "string" ||
              !message.remoteMachineName.trim()
            ) {
              throw new Error("Remote gxserver install request is missing machine details.");
            }
            /*
             * CDXC:RemoteMachines 2026-06-23-08:30:
             * SSH-reachable Ubuntu and macOS machines that are missing gxserver
             * must keep install approval state populated so Remote Settings
             * shows the Install gxserver button instead of only the warning
             * toast that explains the missing daemon.
             */
            setRemoteGxserverInstall({
              remoteMachineId: message.remoteMachineId,
              remoteMachineName: message.remoteMachineName,
            });
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
                    setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
          } else if (message.modal === "remoteProjectPicker") {
            if (
              typeof message.remoteMachineId !== "string" ||
              !message.remoteMachineId.trim() ||
              typeof message.remoteMachineName !== "string" ||
              !message.remoteMachineName.trim()
            ) {
              throw new Error("Remote project picker request is missing machine details.");
            }
            /*
             * CDXC:RemoteProjectPicker 2026-06-03-00:18:
             * Remote machine Add Project opens in the full-window modal host
             * with the selected machine carried as immutable request state.
             * Directory browsing remains machine-scoped through native so the
             * picker cannot accidentally browse local folders.
             */
            setRemoteProjectPicker({
              initialQuery:
                typeof message.initialQuery === "string" ? message.initialQuery : undefined,
              remoteMachineId: message.remoteMachineId,
              remoteMachineName: message.remoteMachineName,
            });
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
                    setRemoteGxserverInstall(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
          } else if (message.modal === "delayedSend") {
            if (!message.sessionId) {
              throw new Error("Delayed Send modal request is missing sessionId.");
            }
            setDelayedSend({
              delayedSendDeadlineAt:
                typeof message.delayedSendDeadlineAt === "string"
                  ? message.delayedSendDeadlineAt
                  : undefined,
              delayedSendRemainingLabel:
                typeof message.delayedSendRemainingLabel === "string"
                  ? message.delayedSendRemainingLabel
                  : undefined,
              sessionId: message.sessionId,
              title: typeof message.title === "string" ? message.title : undefined,
            });
            setConfig({});
            setFirstUserMessage(undefined);
                    setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
          } else if (message.modal === "t3BrowserAccess") {
            if (!message.access) {
              throw new Error("T3 browser access modal request is missing access details.");
            }
            /**
             * CDXC:T3RemoteAccess 2026-05-02-00:57
             * The Remote Access QR dialog must be owned by the full-window app
             * modal host so the QR code centers over ghostex instead of rendering
             * inside the narrow sidebar webview.
             */
            setT3BrowserAccess(message.access);
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
                    setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
          } else if (message.modal === "t3ThreadId") {
            if (!message.sessionId || typeof message.threadId !== "string") {
              throw new Error("T3 thread id modal request is missing sessionId or threadId.");
            }
            setT3ThreadId({
              currentThreadId: message.threadId,
              sessionId: message.sessionId,
            });
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
                    setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
          } else if (message.modal === "worktree") {
            setWorktree({
              projectId: typeof message.projectId === "string" ? message.projectId : undefined,
              projectName: typeof message.projectName === "string" ? message.projectName : undefined,
              projectPath: typeof message.projectPath === "string" ? message.projectPath : undefined,
              remoteMachineId: typeof message.remoteMachineId === "string" ? message.remoteMachineId : undefined,
              remoteMachineName: typeof message.remoteMachineName === "string" ? message.remoteMachineName : undefined,
            });
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
                    setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setGitCommit(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
          } else if (message.modal === "portlessSetup") {
            if (
              message.mode !== "firstSetup" &&
              message.mode !== "standaloneReconfigure"
            ) {
              throw new Error("Portless setup modal request is missing setup mode.");
            }
            if (message.protocol !== "https" && message.protocol !== "http") {
              throw new Error("Portless setup modal request is missing protocol.");
            }
            setPortlessSetup({ mode: message.mode, protocol: message.protocol });
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
                    setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
            setGitCommit(undefined);
            setWorktreeDelete(undefined);
          } else if (message.modal === "deleteWorktree") {
            if (!message.worktreeDeleteDraft) {
              throw new Error("Delete worktree modal request is missing worktreeDeleteDraft.");
            }
            setWorktreeDelete(message.worktreeDeleteDraft);
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
                    setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setGitCommit(undefined);
          } else if (message.modal === "gitCommit") {
            if (!message.gitCommitDraft) {
              throw new Error("Git commit modal request is missing gitCommitDraft.");
            }
            setGitCommit(message.gitCommitDraft);
            setGitFileDiff(undefined);
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
                    setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
          } else if (message.modal === "gitFileDiff") {
            if (!message.gitFileDiff) {
              throw new Error("Git file diff modal request is missing gitFileDiff.");
            }
            setGitFileDiff(message.gitFileDiff);
            return;
          } else if (message.modal === "agentConfig") {
            if (!message.agentDraft) {
              throw new Error("Agent config modal request is missing agentDraft.");
            }
            setConfig({ agentDraft: message.agentDraft });
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
                    setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
          } else {
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
                    setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
          }
          if (message.modal === "settings") {
            setGhostexFolderStats(undefined);
            setSettingsInitialSection(
              typeof message.initialSection === "string" ? message.initialSection : undefined,
            );
            /**
             * CDXC:SessionPersistence 2026-06-04-02:52:
             * Titlebar Tips notices can open Settings directly to a searchable
             * tab and pre-fill the query with a setting name. Carry that state
             * through the full-window modal host instead of requiring titlebar
             * code to know the Settings DOM.
             */
            setSettingsInitialSearchQuery(
              typeof message.initialSearchQuery === "string"
                ? message.initialSearchQuery
                : undefined,
            );
            /**
             * CDXC:RemoteMachines 2026-06-10-09:54:
             * Sidebar Remote machine Edit opens Settings directly on the Remote
             * tab and carries the selected machine id so the modal can scroll to
             * and focus that machine's editable fields.
             */
            setSettingsInitialRemoteMachineId(
              typeof message.initialRemoteMachineId === "string" &&
                message.initialRemoteMachineId.trim()
                ? message.initialRemoteMachineId
                : undefined,
            );
            setSettingsInitialTabOverride(
              isSettingsModalTab(message.initialTab) ? message.initialTab : undefined,
            );
          } else {
            setSettingsInitialSection(undefined);
            setSettingsInitialRemoteMachineId(undefined);
            setSettingsInitialSearchQuery(undefined);
            setSettingsInitialTabOverride(undefined);
          }
          if (message.modal === "commandPalette") {
            /*
             * CDXC:CommandPalette 2026-06-13-22:18:
             * One modal kind owns both session search and command fuzzy finding.
             * Preserve the caller's initial input so Cmd+Shift+P can prefill
             * `>` while Cmd+P opens the same window with an empty query.
             *
             * CDXC:CommandPalette 2026-06-13-22:48:
             * The command palette lives in the native modal host, but project
             * collapse is sidebar-local UI state. Normalize the caller's map at
             * the host boundary so Collapsed Projects can be rendered without
             * querying DOM state from the separate modal window.
             *
             * CDXC:CommandPalette 2026-06-15-10:27:
             * Duplicate command-palette opens are normally no-ops, but Cmd+P
             * and Cmd+Shift+P must still switch an already-visible palette
             * between session and command modes. Increment a request sequence
             * for every open message so React can distinguish a repeat hotkey
             * from a normal prop re-render.
             */
            setCommandPaletteCollapsedGroupsById(
              normalizeCommandPaletteCollapsedGroupsById(message.collapsedGroupsById),
            );
            setCommandPaletteInitialQuery(
              typeof message.initialQuery === "string" ? message.initialQuery : "",
            );
            setCommandPaletteOpenRequestSequence((sequence) => sequence + 1);
            setIsCommandPalettePrewarm(message.prewarm === true);
          } else {
            setCommandPaletteCollapsedGroupsById({});
            setCommandPaletteInitialQuery("");
            setCommandPaletteOpenRequestSequence(0);
            setIsCommandPalettePrewarm(false);
          }
          if (message.modal !== "agentsHub") {
            setAgentsHubCatalog(undefined);
            setAgentsHubFileContent(undefined);
          }
          if (message.modal === "addRepository") {
            setAddRepository({
              remoteMachineId:
                typeof message.remoteMachineId === "string" && message.remoteMachineId.trim()
                  ? message.remoteMachineId
                  : undefined,
              remoteMachineName:
                typeof message.remoteMachineName === "string" && message.remoteMachineName.trim()
                  ? message.remoteMachineName
                  : undefined,
            });
          } else {
            setAddRepository({});
          }
          setActiveModalRequestId(
            typeof message.requestId === "string" ? message.requestId : undefined,
          );
          setActiveModal(message.modal);
          return;
        }

        if (message.type === "close") {
          if (isAppModalDebugLoggingEnabled()) {
            postAppModalHostMessage(
              {
                details: JSON.stringify({ performanceNow: performance.now() }),
                event: "modalHost.close.received",
                type: "debugLog",
              },
              "AppModals:debug",
            );
          }
          clearActiveModalState();
          return;
        }

        if (message.type === "toast") {
          /**
           * CDXC:Worktrees 2026-06-02-15:27:
           * Git and worktree command execution belongs to gxserver after the ownership split. The app-modal host owns only the visible toast surface, so gxserver-backed progress feedback appears over the full Ghostex window without stealing focus from terminal panes.
           *
           * CDXC:GitActionModel 2026-05-30-05:34:
           * Long-running Git actions and agent workflows need persistent status
           * toasts. Reuse Sonner ids so native can update a running toast to a
           * success or error state instead of stacking transient progress notices.
           *
           * CDXC:GitActionToasts 2026-05-30-06:39:
           * Persistent Git/worktree toasts need an explicit spinner, error
           * toasts need a red-tinted surface, and success toasts need a subtle
           * green tint so users can distinguish completion states even when the
           * toast copy is partially clipped.
           */
          toastTokenRef.current += 1;
          const toastToken = toastTokenRef.current;
          const isPersistent = message.persistent === true;
          const toastDescription = normalizeAppToastDescription(
            message.title,
            typeof message.description === "string" ? message.description : undefined,
          );
          const toastClassName = [
            "ghostex-app-toast",
            isPersistent ? "ghostex-app-toast-persistent" : "",
            message.level === "error" ? "ghostex-app-toast-error" : "",
            message.level === "success" ? "ghostex-app-toast-success" : "",
          ]
            .filter(Boolean)
            .join(" ");
          const toastOptions = {
            action: message.action
              ? {
                  label: message.action.label,
                  onClick: () => {
                    if (message.action) {
                      vscode.postMessage(message.action.sidebarMessage);
                    }
                  },
                }
              : undefined,
            className: toastClassName,
            description: toastDescription,
            duration: isPersistent ? Number.POSITIVE_INFINITY : undefined,
            id: message.toastId,
            style:
              message.level === "error"
                ? {
                    background:
                      "linear-gradient(0deg, rgba(95, 24, 31, 0.28), rgba(95, 24, 31, 0.28)), var(--app-modal-background)",
                    border: "1px solid rgba(248, 113, 113, 0.32)",
                    color: "#fff1f2",
                  }
                : message.level === "success"
                  ? {
                      background:
                        "linear-gradient(0deg, rgba(22, 101, 52, 0.24), rgba(22, 101, 52, 0.24)), var(--app-modal-background)",
                      border: "1px solid rgba(74, 222, 128, 0.3)",
                      color: "#f0fdf4",
                    }
                : undefined,
          };
          if (message.level === "error") {
            toast.error(message.title, toastOptions);
          } else if (message.level === "warning") {
            toast.warning(message.title, toastOptions);
          } else if (message.level === "success") {
            toast.success(message.title, toastOptions);
          } else {
            toast.message(message.title, toastOptions);
          }
          if (isPersistent) {
            return;
          }
          window.setTimeout(() => {
            if (toastToken !== toastTokenRef.current) {
              return;
            }
            postAppModalHostMessage(
              { keepOpen: activeModalRef.current !== undefined, type: "toastDismissed" },
              "AppModals:toastDismissed",
            );
          }, 4_200);
          return;
        }


        if (message.type === "sidebarState") {
          if (isAgentsHubCatalogMessage(message.message)) {
            setAgentsHubCatalog(message.message);
            setAgentsHubFileContent(undefined);
            return;
          }
          if (isAgentsHubFileContentMessage(message.message)) {
            setAgentsHubFileContent(message.message);
            return;
          }
          if (isGhostexFolderStatsMessage(message.message)) {
            setGhostexFolderStats(message.message);
            return;
          }
          if (isAgentHookStatusMessage(message.message)) {
            setAgentHookStatus(message.message);
            return;
          }
          if (isGhostexCliStatusMessage(message.message)) {
            setGhostexCliStatus(message.message);
            return;
          }
          if (isOSIntegrationStatusMessage(message.message)) {
            setOSIntegrationStatus(message.message);
            return;
          }
          // CDXC:AppIconPicker 2026-06-25-21:50: Route relayed App Icon state into Settings modal state.
          if (isAppIconStateMessage(message.message)) {
            setAppIconState(message.message);
            return;
          }
          if (isDoctorChecksResultMessage(message.message)) {
            setDoctorChecks(message.message.checks);
            setDoctorLoading(false);
            return;
          }
          if (isDoctorFixResultMessage(message.message)) {
            if (message.message.ok) {
              toast.success("Fix applied successfully");
              setDoctorLoading(true);
              postAppModalHostMessage({ type: "runDoctor" }, "AppModals:runDoctor");
            } else {
              toast.error(message.message.error ?? "Fix failed");
            }
            return;
          }
          if (isDiagnosticsExportResultMessage(message.message)) {
            if (message.message.ok && message.message.json) {
              setDiagnosticsJson(message.message.json);
              setDiagnosticsLoading(false);
              void navigator.clipboard.writeText(message.message.json).then(
                () => toast.success("Diagnostics copied to clipboard"),
                () => toast.error("Failed to copy diagnostics"),
              );
            } else {
              toast.error(message.message.error ?? "Diagnostics export failed");
              setDiagnosticsLoading(false);
            }
            return;
          }
          if (isPreviousSessionsResultMessage(message.message)) {
            window.postMessage(message.message, "*");
            return;
          }
          applySidebarStateMessage(message.message);
        }
      } catch (error) {
        logAppModalError("AppModals:hostMessage", error);
        throw error;
      }
    };

    window.addEventListener("ghostex-app-modal-host-message", handleMessage);
    postAppModalHostMessage(
      { nativeWindowHostId: window.__ghostex_APP_MODAL_HOST_ID__, type: "ready" },
      "AppModals:ready",
    );
    /*
     * CDXC:AppModals 2026-06-11-19:46:
     * Native child windows reuse modal-host.html for the app modal family.
     */
    return () => {
      window.removeEventListener("ghostex-app-modal-host-message", handleMessage);
    };
  }, []);

  return {
    activeModal,
    activeModalRequestId,
    addRepository,
    agentsHubCatalog,
    agentsHubFileContent,
    config,
    delayedSend,
    firstUserMessage,
    gitCommit,
    gitFileDiff,
    worktreeDelete,
    commandPaletteCollapsedGroupsById,
    commandPaletteInitialQuery,
    commandPaletteOpenRequestSequence,
    isCommandPalettePrewarm,
    closeGitFileDiff,
    closeModal,
    remoteProjectPicker,
    renameSession,
    remoteGxserverInstall,
    t3BrowserAccess,
    t3ThreadId,
    worktree,
    portlessSetup,
    agentHookStatus,
    ghostexCliStatus,
    ghostexFolderStats,
    osIntegrationStatus,
    // CDXC:AppIconPicker 2026-06-25-21:50: Expose App Icon state to the modal component.
    appIconState,
    settingsInitialSection,
    settingsInitialRemoteMachineId,
    settingsInitialSearchQuery,
    settingsInitialTabOverride,
  };
}

function isAgentHookStatusMessage(message: unknown): message is SidebarAgentHookStatusMessage {
  return Boolean(
    message &&
      typeof message === "object" &&
      "type" in message &&
      message.type === "agentHookStatus",
  );
}

function isGhostexCliStatusMessage(message: unknown): message is SidebarGhostexCliStatusMessage {
  return Boolean(
    message &&
      typeof message === "object" &&
      "type" in message &&
      message.type === "ghostexCliStatus",
  );
}

function isGhostexFolderStatsMessage(message: unknown): message is SidebarGhostexFolderStatsMessage {
  return Boolean(
    message &&
      typeof message === "object" &&
      "type" in message &&
      message.type === "ghostexFolderStats",
  );
}

function isOSIntegrationStatusMessage(message: unknown): message is SidebarOSIntegrationStatusMessage {
  return Boolean(
    message &&
      typeof message === "object" &&
      "type" in message &&
      message.type === "osIntegrationStatus",
  );
}

// CDXC:AppIconPicker 2026-06-25-21:50: Narrow relayed sidebarState payloads to the App Icon contract.
function isAppIconStateMessage(message: unknown): message is SidebarAppIconStateMessage {
  return Boolean(
    message &&
      typeof message === "object" &&
      "type" in message &&
      message.type === "appIconState",
  );
}

function isDoctorChecksResultMessage(message: unknown): message is SidebarDoctorChecksResultMessage {
  return Boolean(
    message &&
      typeof message === "object" &&
      "type" in message &&
      message.type === "doctorChecksResult",
  );
}

function isDoctorFixResultMessage(message: unknown): message is SidebarDoctorFixResultMessage {
  return Boolean(
    message &&
      typeof message === "object" &&
      "type" in message &&
      message.type === "doctorFixResult",
  );
}

function isDiagnosticsExportResultMessage(
  message: unknown,
): message is SidebarDiagnosticsExportResultMessage {
  return Boolean(
    message &&
      typeof message === "object" &&
      "type" in message &&
      message.type === "diagnosticsExportResult",
  );
}

function isPreviousSessionsResultMessage(
  message: unknown,
): message is Extract<ExtensionToSidebarMessage, { type: "previousSessionsResult" }> {
  /*
  CDXC:PreviousSessionsModal 2026-06-01-22:01:
  The full-window Previous Sessions modal lives in the app modal host WebView, while gxserver previous-session queries are requested through the native sidebar bridge. Forward the result as a normal window message so the shared modal component receives the same response path it uses inside the sidebar WebView.
  */
  return Boolean(
    message &&
      typeof message === "object" &&
      "type" in message &&
      message.type === "previousSessionsResult",
  );
}

function isAgentsHubCatalogMessage(message: unknown): message is AgentsHubCatalogMessage {
  return Boolean(
    message &&
      typeof message === "object" &&
      "type" in message &&
      message.type === "agentsHubCatalog",
  );
}

function isAgentsHubFileContentMessage(message: unknown): message is AgentsHubFileContentMessage {
  return Boolean(
    message &&
      typeof message === "object" &&
      "type" in message &&
      message.type === "agentsHubFileContent",
  );
}

function createEmptyAgentDraft(): AgentConfigDraft {
  return {
    command: "",
    name: "",
  };
}

function isModalRenderable({
  activeModal,
  config,
  delayedSend,
  firstUserMessage,
  gitCommit,
  gitFileDiff,
  worktreeDelete,
  remoteProjectPicker,
  remoteGxserverInstall,
  renameSession,
  settings,
  t3BrowserAccess,
  t3ThreadId,
  worktree,
  portlessSetup,
}: {
  activeModal: AppModalKind | undefined;
  config: ConfigModalState;
  delayedSend: DelayedSendModalState | undefined;
  firstUserMessage: FirstUserMessageModalState | undefined;
  gitCommit: GitCommitModalDraft | undefined;
  gitFileDiff: GitFileDiffModalDraft | undefined;
  worktreeDelete: WorktreeDeleteModalDraft | undefined;
  remoteProjectPicker: RemoteProjectPickerState | undefined;
  remoteGxserverInstall: RemoteGxserverInstallState | undefined;
  renameSession: RenameSessionModalState | undefined;
  settings: unknown;
  t3BrowserAccess: T3BrowserAccessMessage | undefined;
  t3ThreadId: T3ThreadIdModalState | undefined;
  worktree: WorktreeModalState | undefined;
  portlessSetup: PortlessSetupModalState | undefined;
}): boolean {
  switch (activeModal) {
    case undefined:
      return false;
    case "addRepository":
      return true;
    case "agentConfig":
      return config.agentDraft !== undefined;
    case "agentsHub":
    case "commandPalette":
      return true;
    case "delayedSend":
      return delayedSend !== undefined;
    case "firstUserMessage":
      return firstUserMessage !== undefined;
    case "gitCommit":
      return gitCommit !== undefined;
    case "gitFileDiff":
      return gitFileDiff !== undefined;
    case "deleteWorktree":
      return worktreeDelete !== undefined;
    case "remoteProjectPicker":
      return remoteProjectPicker !== undefined;
    case "remoteGxserverInstall":
      return remoteGxserverInstall !== undefined;
    case "renameSession":
      return renameSession !== undefined;
    case "settings":
    case "configureActions":
    case "configureAgents":
    case "hotkeys":
    case "openTargets":
      return settings !== undefined;
    case "t3BrowserAccess":
      return t3BrowserAccess !== undefined;
    case "t3ThreadId":
      return t3ThreadId !== undefined;
    case "worktree":
      return worktree !== undefined;
    case "portlessSetup":
      return portlessSetup !== undefined;
    case "daemonSessions":
    case "pinnedPrompts":
    case "previousSessions":
    case "scratchPad":
    case "discoverGhostex":
    case "watchGhostexVideo":
    case "tipsAndTricks":
    case "firstLaunchSetup":
      return true;
  }
}

function applySidebarStateMessage(message: unknown) {
  if (!message || typeof message !== "object" || !("type" in message)) {
    return;
  }

  if (message.type === "hydrate" || message.type === "sessionState") {
    useSidebarStore
      .getState()
      .applySidebarMessage(
        message as Parameters<
          ReturnType<typeof useSidebarStore.getState>["applySidebarMessage"]
        >[0],
      );
    return;
  }

  if (message.type === "daemonSessionsState") {
    useSidebarStore
      .getState()
      .setDaemonSessionsState(
        message as Parameters<
          ReturnType<typeof useSidebarStore.getState>["setDaemonSessionsState"]
        >[0],
      );
  }
}

document.body.classList.add("app-modal-host-body");
if (window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow") {
  document.documentElement.classList.add("app-modal-host-native-window-document");
  document.body.classList.add("app-modal-host-native-window-body");
}
installAppModalGlobalErrorLogging("AppModals:modalHost");
createRoot(document.getElementById("root")!).render(<AppModalHost />);
