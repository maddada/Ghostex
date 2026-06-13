import { createRoot } from "react-dom/client";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { Toaster, toast } from "sonner";
import { AddRepositoryModal } from "../../sidebar/add-repository-modal";
import { AgentConfigModal, type AgentConfigDraft } from "../../sidebar/agent-config-modal";
import { AgentsHubModal } from "../../sidebar/agents-hub-modal";
import { CommandPalette } from "../../sidebar/command-palette";
import { CommandConfigModal, type CommandConfigDraft } from "../../sidebar/command-config-modal";
import { DaemonSessionsModal } from "../../sidebar/daemon-sessions-modal";
import { DelayedSendModal } from "../../sidebar/delayed-send-modal";
import { FirstUserMessageModal } from "../../sidebar/first-user-message-modal";
import { PinnedPromptsModal } from "../../sidebar/pinned-prompts-modal";
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
import { FirstLaunchSetupModal } from "../../sidebar/first-launch-setup-modal";
import { GitFileDiffModal, type GitFileDiffModalDraft } from "../../sidebar/git-file-diff-modal";
import { GitCommitModal, type GitCommitModalDraft } from "../../sidebar/git-commit-modal";
import {
  WorktreeDeleteModal,
  type WorktreeDeleteModalDraft,
} from "../../sidebar/worktree-delete-modal";
import { WorktreeCreateModal } from "../../sidebar/worktree-create-modal";
import type { AppToastRequest } from "../../shared/app-toast-contract";
import type { SidebarActionType } from "../../shared/sidebar-commands";
import type { SidebarAgentButton } from "../../shared/sidebar-agents";
import type {
  ExtensionToSidebarMessage,
  SidebarAgentHookStatusMessage,
  SidebarGhostexCliStatusMessage,
  SidebarGhostexFolderStatsMessage,
  SidebarOSIntegrationStatusMessage,
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
import { trimPromptEditorTrailingSpaces } from "../../shared/prompt-editor-text";
import type { WebviewApi } from "../../sidebar/webview-api";
import "../../sidebar/styles.css";

type AppModalKind =
  | "addRepository"
  | "agentConfig"
  | "agentsHub"
  | "commandPalette"
  | "commandConfig"
  | "configureActions"
  | "configureAgents"
  | "daemonSessions"
  | "delayedSend"
  | "hotkeys"
  | "gitCommit"
  | "gitFileDiff"
  | "deleteWorktree"
  | "openTargets"
  | "pinnedPrompts"
  | "floatingPromptEditor"
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

type T3BrowserAccessMessage = Extract<ExtensionToSidebarMessage, { type: "showT3BrowserAccess" }>;
type AgentsHubCatalogMessage = Extract<ExtensionToSidebarMessage, { type: "agentsHubCatalog" }>;
type AgentsHubFileContentMessage = Extract<
  ExtensionToSidebarMessage,
  { type: "agentsHubFileContent" }
>;
type AgentHookStatusMessage = Extract<ExtensionToSidebarMessage, { type: "agentHookStatus" }>;
type GhostexCliStatusMessage = Extract<ExtensionToSidebarMessage, { type: "ghostexCliStatus" }>;
type OSIntegrationStatusMessage = Extract<ExtensionToSidebarMessage, { type: "osIntegrationStatus" }>;

type AppModalHostMessage =
  | {
      agentDraft?: AgentConfigDraft;
      access?: T3BrowserAccessMessage;
      commandDraft?: CommandConfigDraft;
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
      initialFrame?: FloatingPromptEditorFrame;
      initialRemoteMachineId?: string;
      initialSection?: MainSettingsInitialSectionId;
      initialSearchQuery?: string;
      initialTab?: SettingsModalTab;
      initialText?: string;
      lockedActionType?: SidebarActionType;
      language?: string;
      modal: AppModalKind;
      nativeOpenStartedAtMs?: number;
      prewarm?: boolean;
      requestId?: string;
      sessionId?: string;
      statusFile?: string;
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
      error?: string;
      ok: boolean;
      requestId: string;
      type: "projectWorktreesResult";
      worktrees?: unknown;
    }
  | { details?: string; event: string; type: "debugLog" }
  | { requestId: string; type: "floatingPromptEditorCloseAndSave" }
  | {
      frame: FloatingPromptEditorFrame;
      imagePreviewOpen?: boolean;
      requestId: string;
      type: "floatingPromptEditorHitRegion";
    }
  | {
      error?: string;
      imagePath?: string;
      pasteRequestId: string;
      requestId: string;
      type: "floatingPromptEditorImagePasteResult";
    }
  | {
      dataUrl?: string;
      error?: string;
      path: string;
      previewRequestId: string;
      requestId: string;
      type: "floatingPromptEditorImagePreviewResult";
    }
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

type FloatingPromptEditorFrame = {
  height: number;
  left: number;
  top: number;
  width: number;
};

type FloatingPromptEditorState = {
  filePath: string;
  initialFrame?: FloatingPromptEditorFrame;
  initialText: string;
  isPrewarm?: boolean;
  language: string;
  requestId: string;
  statusFile?: string;
  title: string;
};

type FloatingPromptEditorDragMode = "move" | "resize";

type FloatingPromptEditorImagePreview = {
  endOffset: number;
  id: string;
  markdown: string;
  path: string;
  startOffset: number;
};

const floatingPromptEditorFrameMargin = 16;
const floatingPromptEditorDefaultHeight = 320;
const floatingPromptEditorDefaultWidth = 400;
const floatingPromptEditorMaximumWidth = 700;
/**
 * CDXC:AppToasts 2026-06-03-16:12:
 * macOS and crossplatform app-modal toasts should sit 23px higher than
 * Sonner's 24px bottom default, so progress notices stay clear of lower app
 * chrome while preserving the bottom-center stack behavior.
 */
const APP_MODAL_TOAST_BOTTOM_OFFSET_PX = 47;
/**
 * CDXC:PromptEditor 2026-05-14-09:55:
 * Users can shrink the floating prompt editor after expanding it. The minimum
 * width must still leave room for both titlebar actions anchored to the live
 * right edge, while the title and shortcut text truncate first.
 *
 * CDXC:PromptEditor 2026-05-15-12:42:
 * The editor must keep shrinking cleanly after it has been widened. Use a
 * narrower floor and let the title/shortcut disappear before action buttons
 * or Monaco content can hold the panel at a stale wider layout.
 */
const floatingPromptEditorMinimumWidth = 180;

/**
 * CDXC:PromptEditor 2026-05-19-10:05:
 * The modal host still recognizes the historical hidden prompt-editor prewarm
 * request id so older bridge messages close cleanly after Monaco initializes.
 *
 * CDXC:PromptEditor 2026-06-11-22:51:
 * The active rich prompt editor now opens in a native child window, so native
 * no longer creates a hidden full-workspace overlay editor just to prewarm the
 * user-facing child-window instance.
 */
export const FLOATING_PROMPT_EDITOR_PREWARM_REQUEST_ID = "ghostex-floating-prompt-editor-prewarm";

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

const APP_MODAL_CONTEXT_MENU_EDITABLE_SELECTOR =
  "input, textarea, select, [contenteditable='true'], [role='textbox'], .monaco-editor";

function isEditableAppModalContextMenuTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) {
    return false;
  }

  return target.closest(APP_MODAL_CONTEXT_MENU_EDITABLE_SELECTOR) !== null;
}

type ConfigModalState = {
  agentDraft?: AgentConfigDraft;
  commandDraft?: CommandConfigDraft;
  lockedActionType?: SidebarActionType;
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
  }
}

type MonacoEditorInstance = {
  dispose: () => void;
  executeEdits: (source: string, edits: MonacoEdit[]) => boolean;
  focus?: () => void;
  getModel: () => MonacoTextModel | null;
  getPosition: () => MonacoPosition | null;
  getSelection: () => MonacoRange | null;
  getValue: () => string;
  layout: () => void;
  onDidChangeModelContent: (listener: () => void) => { dispose: () => void };
  pushUndoStop?: () => boolean;
  revealPositionInCenterIfOutsideViewport?: (position: MonacoPosition) => void;
  setPosition?: (position: MonacoPosition) => void;
  setValue: (value: string) => void;
};

type MonacoPosition = {
  column: number;
  lineNumber: number;
};

type MonacoRange = {
  endColumn: number;
  endLineNumber: number;
  startColumn: number;
  startLineNumber: number;
};

type MonacoEdit = {
  forceMoveMarkers?: boolean;
  range: MonacoRange;
  text: string;
};

type MonacoTextModel = {
  getPositionAt: (offset: number) => MonacoPosition;
};

type MonacoAmdRequire = {
  (dependencies: string[], onLoad: () => void, onError?: (error: unknown) => void): void;
  config?: (config: Record<string, unknown>) => void;
};

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

function isAppModalDebugLoggingEnabled(): boolean {
  return useSidebarStore.getState().hud.debuggingMode;
}

/**
 * CDXC:PromptEditor 2026-05-19-11:20:
 * Prompt-editor repro logs must land in ~/.ghostex/logs/native-prompt-editor-debug.log
 * only while Settings debugging mode is enabled. Native owns the file; React posts
 * structured events across the modal-host bridge so Monaco, hit regions, and focus
 * can be correlated on one timeline.
 */
function appendPromptEditorDebugLog(
  event: string,
  details: Record<string, string | number | boolean | null | undefined> = {},
) {
  if (!isAppModalDebugLoggingEnabled()) {
    return;
  }
  const payload = {
    performanceNow: performance.now(),
    ...details,
  };
  console.debug("[ghostex-prompt-editor]", event, payload);
  postAppModalHostMessage(
    {
      details: JSON.stringify(payload),
      event,
      type: "promptEditorDebugLog",
    },
    "PromptEditor:debug",
  );
}

function notifyNativeModalClosed() {
  postAppModalHostMessage({ type: "close" }, "AppModals:close");
}

let modalHostMonacoLoadPromise: Promise<void> | undefined;

function getModalHostMonacoRequire(): MonacoAmdRequire | undefined {
  return (window as unknown as { require?: MonacoAmdRequire }).require;
}

function loadModalHostMonaco(): Promise<void> {
  if (window.monaco) {
    return Promise.resolve();
  }
  if (modalHostMonacoLoadPromise) {
    return modalHostMonacoLoadPromise;
  }
  modalHostMonacoLoadPromise = new Promise((resolve, reject) => {
    window.MonacoEnvironment = {
      getWorkerUrl: () => "./monaco/vs/base/worker/workerMain.js",
    };
    const configureRequire = () => {
      const amdRequire = getModalHostMonacoRequire();
      if (!amdRequire) {
        reject(new Error("Monaco AMD loader did not initialize."));
        return;
      }
      amdRequire.config?.({ paths: { vs: "./monaco/vs" } });
      amdRequire(["vs/editor/editor.main"], resolve, reject);
    };
    const existingLoader = document.querySelector<HTMLScriptElement>(
      'script[data-ghostex-monaco-loader="true"]',
    );
    if (existingLoader) {
      existingLoader.addEventListener("load", configureRequire, { once: true });
      existingLoader.addEventListener("error", () => reject(new Error("Monaco loader failed.")), {
        once: true,
      });
      if (getModalHostMonacoRequire()) {
        configureRequire();
      }
      return;
    }
    const script = document.createElement("script");
    script.dataset.ghostexMonacoLoader = "true";
    script.src = "./monaco/vs/loader.js";
    script.addEventListener("load", configureRequire, { once: true });
    script.addEventListener("error", () => reject(new Error("Monaco loader failed.")), {
      once: true,
    });
    document.head.appendChild(script);
  });
  return modalHostMonacoLoadPromise;
}

function moveMonacoCaretToEnd(monacoEditor: MonacoEditorInstance, text: string) {
  const model = monacoEditor.getModel();
  if (!model) {
    return;
  }
  /**
   * CDXC:PromptEditor 2026-05-19-10:05:
   * Ctrl+G prompt editing should open with the caret at the end of the loaded
   * buffer so users can append to an existing prompt immediately.
   */
  const endPosition = model.getPositionAt(text.length);
  monacoEditor.setPosition?.(endPosition);
  monacoEditor.revealPositionInCenterIfOutsideViewport?.(endPosition);
}

function getNextPromptEditorImageIndex(text: string): number {
  const imageLabelPattern = /\[Image #(\d+)\]\(/g;
  let highestIndex = 0;
  for (const match of text.matchAll(imageLabelPattern)) {
    const index = Number.parseInt(match[1] ?? "", 10);
    if (Number.isFinite(index)) {
      highestIndex = Math.max(highestIndex, index);
    }
  }
  return highestIndex + 1;
}

function hasImagePastePayload(event: ClipboardEvent): boolean {
  const clipboardData = event.clipboardData;
  if (!clipboardData) {
    return false;
  }

  const files = Array.from(clipboardData.files);
  if (
    files.some((file) => {
      const type = file.type.toLowerCase();
      return type.startsWith("image/") || /\.(avif|gif|heic|heif|jpe?g|png|svg|tiff?|webp)$/iu.test(file.name);
    })
  ) {
    return true;
  }

  const items = Array.from(clipboardData.items);
  if (
    items.some((item) => item.kind === "file" && item.type.toLowerCase().startsWith("image/"))
  ) {
    return true;
  }

  const types = Array.from(clipboardData.types).map((type) => type.toLowerCase());
  if (
    types.some(
      (type) =>
        type === "files" ||
        type === "public.file-url" ||
        type.startsWith("image/") ||
        type.startsWith("public.image"),
    )
  ) {
    return true;
  }

  return (
    types.includes("text/uri-list") &&
    clipboardData.getData("text/uri-list").trim().toLowerCase().startsWith("file:")
  );
}

function rangeFromPosition(position: MonacoPosition): MonacoRange {
  return {
    endColumn: position.column,
    endLineNumber: position.lineNumber,
    startColumn: position.column,
    startLineNumber: position.lineNumber,
  };
}

function endPositionAfterInsertedText(start: MonacoPosition, text: string): MonacoPosition {
  const lines = text.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
  if (lines.length === 1) {
    return {
      column: start.column + text.length,
      lineNumber: start.lineNumber,
    };
  }
  return {
    column: lines[lines.length - 1].length + 1,
    lineNumber: start.lineNumber + lines.length - 1,
  };
}

function PromptEditorCloseIcon() {
  return (
    <svg aria-hidden="true" fill="none" viewBox="0 0 24 24">
      <path
        d="M18 6 6 18M6 6l12 12"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2.4"
      />
    </svg>
  );
}

function parsePromptEditorImagePreviews(text: string): FloatingPromptEditorImagePreview[] {
  const markdownLinkPattern = /!?\[[^\]\n]*\]\(([^)\n]+)\)/g;
  const previews: FloatingPromptEditorImagePreview[] = [];
  for (const match of text.matchAll(markdownLinkPattern)) {
    const markdown = match[0];
    const rawPath = match[1]?.trim() ?? "";
    const startOffset = match.index ?? 0;
    if (!isPromptEditorImagePath(rawPath)) {
      continue;
    }
    previews.push({
      endOffset: startOffset + markdown.length,
      id: `${startOffset}:${rawPath}:${markdown.length}`,
      markdown,
      path: rawPath,
      startOffset,
    });
  }
  return previews;
}

function isPromptEditorImagePath(path: string): boolean {
  const normalizedPath = path.split(/[?#]/u)[0].toLowerCase();
  return (
    normalizedPath.startsWith("~/.ghostex/i/") ||
    /\.(avif|gif|heic|heif|jpe?g|png|svg|tiff?|webp)$/iu.test(normalizedPath)
  );
}

function clampFloatingPromptEditorFrame(frame: FloatingPromptEditorFrame): FloatingPromptEditorFrame {
  const margin = floatingPromptEditorFrameMargin;
  const availableWidth = Math.max(240, window.innerWidth - margin * 2);
  const maxWidth = Math.min(floatingPromptEditorMaximumWidth, availableWidth);
  const minWidth = Math.min(floatingPromptEditorMinimumWidth, maxWidth);
  const minHeight = Math.min(260, Math.max(180, window.innerHeight - margin * 2));
  const width = Math.min(Math.max(frame.width, minWidth), maxWidth);
  const height = Math.min(
    Math.max(frame.height, minHeight),
    Math.max(minHeight, window.innerHeight - margin * 2),
  );
  return {
    height,
    left: Math.min(Math.max(margin, frame.left), Math.max(margin, window.innerWidth - width - margin)),
    top: Math.min(Math.max(margin, frame.top), Math.max(margin, window.innerHeight - height - margin)),
    width,
  };
}

function defaultFloatingPromptEditorFrame(): FloatingPromptEditorFrame {
  const availableWidth = Math.max(240, window.innerWidth - floatingPromptEditorFrameMargin * 2);
  const defaultHeight = Math.min(
    floatingPromptEditorDefaultHeight,
    Math.max(180, window.innerHeight - floatingPromptEditorFrameMargin * 2),
  );
  const defaultWidth = Math.min(
    floatingPromptEditorDefaultWidth,
    Math.max(floatingPromptEditorMinimumWidth, availableWidth),
  );
  return clampFloatingPromptEditorFrame({
    height: defaultHeight,
    left: Math.max(floatingPromptEditorFrameMargin, (window.innerWidth - defaultWidth) / 2),
    top: Math.max(floatingPromptEditorFrameMargin, window.innerHeight - defaultHeight - floatingPromptEditorFrameMargin),
    width: defaultWidth,
  });
}

function FloatingPromptEditorModal({
  closeAndSaveRequestId,
  editor,
  isOpen,
}: {
  closeAndSaveRequestId?: string;
  editor?: FloatingPromptEditorState;
  isOpen: boolean;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const editorRef = useRef<MonacoEditorInstance | null>(null);
  const [frame, setFrame] = useState<FloatingPromptEditorFrame>(() => defaultFloatingPromptEditorFrame());
  const [dragMode, setDragMode] = useState<FloatingPromptEditorDragMode>();
  const [imagePreviewDataUrls, setImagePreviewDataUrls] = useState<Record<string, string>>({});
  const [imagePreviews, setImagePreviews] = useState<FloatingPromptEditorImagePreview[]>([]);
  const [openImagePreview, setOpenImagePreview] = useState<FloatingPromptEditorImagePreview>();
  const [isCancelConfirming, setIsCancelConfirming] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const activePointerDragCleanupRef = useRef<(() => void) | undefined>(undefined);
  const cancelConfirmTimeoutRef = useRef<number | undefined>(undefined);
  const editorContentListenerRef = useRef<{ dispose: () => void } | undefined>(undefined);
  const imagePasteRequestCounterRef = useRef(0);
  const imagePreviewLoadRequestCounterRef = useRef(0);
  const failedImagePreviewPathsRef = useRef<Set<string>>(new Set());
  const pendingImagePreviewPathRequestsRef = useRef<Set<string>>(new Set());
  const pendingImagePreviewRequestIdsRef = useRef<Map<string, string>>(new Map());
  const pendingImagePasteRequestIdsRef = useRef<Set<string>>(new Set());
  const savedCloseAndSaveRequestIdRef = useRef<string | undefined>(undefined);
  const isNativeWindowSurface = window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow";

  useEffect(() => {
    if (!isOpen || !editor) {
      appendPromptEditorDebugLog("react.lifecycle.closed", {
        hadEditorRef: editorRef.current !== null,
        requestId: editor?.requestId ?? null,
      });
      editorContentListenerRef.current?.dispose();
      editorContentListenerRef.current = undefined;
      editorRef.current?.dispose();
      editorRef.current = null;
      window.clearTimeout(cancelConfirmTimeoutRef.current);
      cancelConfirmTimeoutRef.current = undefined;
      activePointerDragCleanupRef.current?.();
      activePointerDragCleanupRef.current = undefined;
      setDragMode(undefined);
      setImagePreviewDataUrls({});
      setImagePreviews([]);
      setOpenImagePreview(undefined);
      setIsCancelConfirming(false);
      setIsSaving(false);
      failedImagePreviewPathsRef.current.clear();
      pendingImagePreviewPathRequestsRef.current.clear();
      pendingImagePreviewRequestIdsRef.current.clear();
      pendingImagePasteRequestIdsRef.current.clear();
      return;
    }
    setFrame(clampFloatingPromptEditorFrame(editor.initialFrame ?? defaultFloatingPromptEditorFrame()));
    setIsCancelConfirming(false);
    setImagePreviewDataUrls({});
    setImagePreviews(parsePromptEditorImagePreviews(editor.initialText));
    setOpenImagePreview(undefined);
    failedImagePreviewPathsRef.current.clear();
    pendingImagePreviewPathRequestsRef.current.clear();
    pendingImagePreviewRequestIdsRef.current.clear();
    appendPromptEditorDebugLog("react.lifecycle.opened", {
      hasInitialFrame: editor.initialFrame !== undefined,
      initialTextLength: editor.initialText.length,
      isPrewarm: editor.isPrewarm === true,
      requestId: editor.requestId,
    });
  }, [editor?.requestId, isOpen]);

  useEffect(() => {
    if (!isOpen || !editor || !containerRef.current) {
      if (isOpen && editor && !containerRef.current) {
        appendPromptEditorDebugLog("react.monaco.waitingForContainer", {
          requestId: editor.requestId,
        });
      }
      return;
    }
    let disposed = false;
    /**
     * CDXC:PromptEditor 2026-06-12-04:37:
     * First-open speed work needs privacy-safe timing across native, React,
     * and Monaco. Log only request ids, durations, booleans, and text lengths
     * so support can identify whether Ctrl+G latency is WebKit load, Monaco
     * load, or editor creation without recording prompt content.
     */
    const loadStartedAt = performance.now();
    appendPromptEditorDebugLog("react.monaco.loadStart", {
      hasExistingMonaco: Boolean(window.monaco),
      requestId: editor.requestId,
    });
    loadModalHostMonaco()
      .then(() => {
        const loadDurationMs = Math.round(performance.now() - loadStartedAt);
        appendPromptEditorDebugLog("react.monaco.loadReady", {
          hasExistingMonaco: Boolean(window.monaco),
          loadDurationMs,
          requestId: editor.requestId,
        });
        if (disposed || !containerRef.current || !window.monaco) {
          appendPromptEditorDebugLog("react.monaco.createSkipped", {
            disposed,
            hasContainer: containerRef.current !== null,
            hasMonacoGlobal: Boolean(window.monaco),
            requestId: editor.requestId,
          });
          return;
        }
        editorContentListenerRef.current?.dispose();
        editorContentListenerRef.current = undefined;
        editorRef.current?.dispose();
        const createStartedAt = performance.now();
        /**
         * CDXC:PromptEditor 2026-05-13-09:48
         * Ctrl+G prompt editing is plain text entry, not code authoring.
         * Disable Monaco's word/spelling-style suggestions, trigger
         * completions, snippets, and parameter hints; force Markdown with
         * wrapping because prompt text should read naturally in the default
         * writing pane.
         *
         * CDXC:PromptEditor 2026-05-15-20:09:
         * Prompt writing should not behave like code navigation. Disable
         * Monaco occurrence highlights so moving the caret onto a word does not
         * mark every matching word in the rich prompt editor.
         *
         * CDXC:PromptEditor 2026-05-15-20:36:
         * Keep the rich prompt editor visually quiet for prose editing. Disable
         * selection match highlights, active-line highlighting, and rich text
         * clipboard metadata so copied prompt text remains plain.
         */
        const monacoEditor = window.monaco.editor.create(containerRef.current, {
          acceptSuggestionOnEnter: "off",
          automaticLayout: true,
          copyWithSyntaxHighlighting: false,
          cursorBlinking: "smooth",
          fontFamily:
            "JetBrains Mono, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', monospace",
          fontLigatures: true,
          fontSize: 14,
          language: "markdown",
          lineNumbersMinChars: 3,
          minimap: { enabled: false },
          occurrencesHighlight: "off",
          padding: { bottom: 48, top: 12 },
          parameterHints: { enabled: false },
          quickSuggestions: false,
          renderLineHighlight: "none",
          scrollBeyondLastLine: false,
          /*
           * CDXC:PromptEditor 2026-06-04-19:29:
           * The floating prompt editor uses Monaco's internal scrollbar; keep its rail as thin as the Agents Hub sidebar scrollbar so native overlays share one visual density.
           *
           * CDXC:PromptEditor 2026-06-04-19:48:
           * The Monaco rail should be 7px wide after visual review showed 10px still felt too heavy for the macOS editor chrome.
           */
          scrollbar: {
            horizontalScrollbarSize: 7,
            verticalScrollbarSize: 7,
          },
          selectionHighlight: false,
          snippetSuggestions: "none",
          suggestOnTriggerCharacters: false,
          tabCompletion: "off",
          theme: "vs-dark",
          value: editor.initialText,
          wordBasedSuggestions: "off",
          wordWrap: "on",
        }) as MonacoEditorInstance;
        editorRef.current = monacoEditor;
        moveMonacoCaretToEnd(monacoEditor, editor.initialText);
        const createDurationMs = Math.round(performance.now() - createStartedAt);
        const caretPosition = monacoEditor.getPosition();
        setImagePreviews(parsePromptEditorImagePreviews(monacoEditor.getValue()));
        editorContentListenerRef.current = monacoEditor.onDidChangeModelContent(() => {
          setImagePreviews(parsePromptEditorImagePreviews(monacoEditor.getValue()));
        });
        if (editor.isPrewarm) {
          appendPromptEditorDebugLog("react.monaco.prewarmReady", {
            createDurationMs,
            loadDurationMs,
            requestId: editor.requestId,
            textLength: monacoEditor.getValue().length,
          });
          postAppModalHostMessage(
            {
              requestId: editor.requestId,
              type: "floatingPromptEditorPrewarmReady",
            },
            "PromptEditor:prewarm",
          );
          return;
        }
        monacoEditor.focus?.();
        appendPromptEditorDebugLog("react.monaco.createdAndFocused", {
          caretColumn: caretPosition?.column ?? null,
          caretLine: caretPosition?.lineNumber ?? null,
          createDurationMs,
          documentHasFocus: document.hasFocus(),
          loadDurationMs,
          requestId: editor.requestId,
          textLength: monacoEditor.getValue().length,
        });
      })
      .catch((error) => {
        postAppModalHostMessage(
          {
            area: "PromptEditor:monaco",
            message: error instanceof Error ? error.message : String(error),
            type: "logError",
          },
          "PromptEditor:monaco",
        );
      });
    return () => {
      disposed = true;
      appendPromptEditorDebugLog("react.monaco.effectCleanup", {
        hadEditorRef: editorRef.current !== null,
        requestId: editor?.requestId ?? null,
      });
      editorContentListenerRef.current?.dispose();
      editorContentListenerRef.current = undefined;
      editorRef.current?.dispose();
      editorRef.current = null;
    };
  }, [editor?.requestId, isOpen]);

  useEffect(() => {
    editorRef.current?.layout();
  }, [frame.height, frame.width, imagePreviews.length]);

  useEffect(() => {
    if (!openImagePreview || imagePreviews.some((preview) => preview.id === openImagePreview.id)) {
      return;
    }
    setOpenImagePreview(undefined);
  }, [imagePreviews, openImagePreview]);

  useEffect(() => {
    if (!isOpen || !editor || imagePreviews.length === 0) {
      return;
    }

    for (const preview of imagePreviews) {
      if (
        imagePreviewDataUrls[preview.path] ||
        failedImagePreviewPathsRef.current.has(preview.path) ||
        pendingImagePreviewPathRequestsRef.current.has(preview.path)
      ) {
        continue;
      }
      const previewRequestId = `${editor.requestId}:image-preview:${++imagePreviewLoadRequestCounterRef.current}`;
      pendingImagePreviewPathRequestsRef.current.add(preview.path);
      pendingImagePreviewRequestIdsRef.current.set(previewRequestId, preview.path);
      postAppModalHostMessage(
        {
          path: preview.path,
          previewRequestId,
          requestId: editor.requestId,
          type: "floatingPromptEditorLoadImagePreview",
        },
        "PromptEditor:imagePreview",
      );
    }
  }, [editor?.requestId, imagePreviewDataUrls, imagePreviews, isOpen]);

  useEffect(() => {
    if (!isOpen || !editor) {
      return;
    }

    const insertTextIntoEditor = (source: string, text: string) => {
      const monacoEditor = editorRef.current;
      const position = monacoEditor?.getPosition();
      if (!monacoEditor || !position) {
        return false;
      }
      const range = monacoEditor.getSelection() ?? rangeFromPosition(position);
      const startPosition = {
        column: range.startColumn,
        lineNumber: range.startLineNumber,
      };
      const endPosition = endPositionAfterInsertedText(startPosition, text);
      monacoEditor.pushUndoStop?.();
      const didApplyEdit = monacoEditor.executeEdits(source, [
        {
          forceMoveMarkers: true,
          range,
          text,
        },
      ]);
      if (!didApplyEdit) {
        return false;
      }
      monacoEditor.setPosition?.(endPosition);
      monacoEditor.revealPositionInCenterIfOutsideViewport?.(endPosition);
      monacoEditor.pushUndoStop?.();
      monacoEditor.focus?.();
      return true;
    };

    const insertImageMarkdown = (imagePath: string) => {
      const monacoEditor = editorRef.current;
      if (!monacoEditor) {
        return;
      }
      const markdown = `[Image #${getNextPromptEditorImageIndex(monacoEditor.getValue())}](${imagePath})`;
      /**
       * CDXC:PromptEditor 2026-05-16-21:21:
       * Pasting an image into the rich prompt editor should insert a Markdown
       * file reference, not binary image content. Native owns path resolution
       * so clipboard images become durable local files before insertion.
       *
       * CDXC:PromptEditor 2026-05-16-22:56:
       * Insert the short native-returned tilde path for every pasted image.
       * Native always copies or saves image data under ~/.ghostex/i first so
       * long source paths do not wrap across multiple prompt-editor lines.
       */
      insertTextIntoEditor("ghostex-image-paste", markdown);
    };

    const handlePaste = (event: ClipboardEvent) => {
      if (
        event.defaultPrevented ||
        !containerRef.current ||
        !(event.target instanceof Node) ||
        !containerRef.current.contains(event.target)
      ) {
        return;
      }
      if (!hasImagePastePayload(event)) {
        const pastedText = event.clipboardData?.getData("text/plain") ?? "";
        const trimmedText = trimPromptEditorTrailingSpaces(pastedText);
        if (!pastedText || trimmedText === pastedText) {
          return;
        }
        if (insertTextIntoEditor("ghostex-trimmed-text-paste", trimmedText)) {
          event.preventDefault();
          event.stopPropagation();
        }
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      const pasteRequestId = `${editor.requestId}:image-paste:${++imagePasteRequestCounterRef.current}`;
      pendingImagePasteRequestIdsRef.current.add(pasteRequestId);
      postAppModalHostMessage(
        {
          pasteRequestId,
          requestId: editor.requestId,
          type: "floatingPromptEditorPasteImage",
        },
        "PromptEditor:imagePaste",
      );
    };

    const handleNativeMessage = (event: Event) => {
      const message = (event as CustomEvent<AppModalHostMessage>).detail;
      if (
        message &&
        typeof message === "object" &&
        message.type === "floatingPromptEditorImagePreviewResult" &&
        message.requestId === editor.requestId &&
        pendingImagePreviewRequestIdsRef.current.has(message.previewRequestId)
      ) {
        const requestedPath = pendingImagePreviewRequestIdsRef.current.get(message.previewRequestId);
        pendingImagePreviewRequestIdsRef.current.delete(message.previewRequestId);
        if (requestedPath) {
          pendingImagePreviewPathRequestsRef.current.delete(requestedPath);
        }
        if (typeof message.dataUrl === "string" && message.dataUrl.startsWith("data:image/")) {
          setImagePreviewDataUrls((previous) => ({
            ...previous,
            [message.path]: message.dataUrl ?? "",
          }));
          return;
        }
        failedImagePreviewPathsRef.current.add(message.path);
        postAppModalHostMessage(
          {
            area: "PromptEditor:imagePreview",
            message: message.error || `Native image preview load failed for ${message.path}.`,
            type: "logError",
          },
          "PromptEditor:imagePreview",
        );
        return;
      }

      if (
        !message ||
        typeof message !== "object" ||
        message.type !== "floatingPromptEditorImagePasteResult" ||
        message.requestId !== editor.requestId ||
        !pendingImagePasteRequestIdsRef.current.has(message.pasteRequestId)
      ) {
        return;
      }
      pendingImagePasteRequestIdsRef.current.delete(message.pasteRequestId);
      if (typeof message.imagePath === "string" && message.imagePath.trim()) {
        insertImageMarkdown(message.imagePath.trim());
        return;
      }
      postAppModalHostMessage(
        {
          area: "PromptEditor:imagePaste",
          message: message.error || "Native clipboard did not provide an image path.",
          type: "logError",
        },
        "PromptEditor:imagePaste",
      );
    };

    window.addEventListener("paste", handlePaste, { capture: true });
    window.addEventListener("ghostex-app-modal-host-message", handleNativeMessage);
    return () => {
      window.removeEventListener("paste", handlePaste, { capture: true });
      window.removeEventListener("ghostex-app-modal-host-message", handleNativeMessage);
    };
  }, [editor?.requestId, isOpen]);

  useLayoutEffect(() => {
    if (!isOpen || !editor) {
      return;
    }
    /**
     * CDXC:PromptEditor 2026-05-15-12:42:
     * The floating prompt editor should only intercept native AppKit events
     * over the visible editor panel. Publish the live panel rectangle after
     * each move or resize so terminal panes and pins behind the transparent
     * modal WKWebView remain clickable and scrollable outside that rectangle.
     * Image preview is the exception: its backdrop and close button are outside
     * the panel, so native must block the full modal-host surface while it is
     * open.
     *
     * CDXC:PromptEditor 2026-06-11-22:51:
     * In the native prompt-editor child window, AppKit owns the exact window
     * hit area, movement, and resizing. Do not publish full-window overlay hit
     * regions from React when the editor is already inside its own native
     * window.
     */
    if (isNativeWindowSurface) {
      appendPromptEditorDebugLog("react.hitRegion.nativeWindowSkipped", {
        hasEditorRef: editorRef.current !== null,
        requestId: editor.requestId,
      });
      return;
    }
    const imagePreviewOpen = openImagePreview !== undefined;
    appendPromptEditorDebugLog("react.hitRegion.publish", {
      frameHeight: frame.height,
      frameLeft: frame.left,
      frameTop: frame.top,
      frameWidth: frame.width,
      hasEditorRef: editorRef.current !== null,
      imagePreviewOpen,
      innerHeight: window.innerHeight,
      innerWidth: window.innerWidth,
      requestId: editor.requestId,
    });
    postAppModalHostMessage(
      {
        frame,
        imagePreviewOpen,
        requestId: editor.requestId,
        type: "floatingPromptEditorHitRegion",
      },
      "PromptEditor:hitRegion",
    );
  }, [
    editor?.requestId,
    frame.height,
    frame.left,
    frame.top,
    frame.width,
    isNativeWindowSurface,
    isOpen,
    openImagePreview,
  ]);

  useEffect(() => {
    return () => {
      activePointerDragCleanupRef.current?.();
      activePointerDragCleanupRef.current = undefined;
    };
  }, []);

  const save = () => {
    if (!editor || isSaving) {
      return;
    }
    setIsSaving(true);
    postAppModalHostMessage(
      {
        filePath: editor.filePath,
        requestId: editor.requestId,
        statusFile: editor.statusFile,
        text: editorRef.current?.getValue() ?? editor.initialText,
        type: "floatingPromptEditorSave",
      },
      "PromptEditor:save",
    );
  };

  const requestCancel = () => {
    if (!editor) {
      return;
    }
    if (!isCancelConfirming) {
      setIsCancelConfirming(true);
      window.clearTimeout(cancelConfirmTimeoutRef.current);
      cancelConfirmTimeoutRef.current = window.setTimeout(() => {
        setIsCancelConfirming(false);
        cancelConfirmTimeoutRef.current = undefined;
      }, 3000);
      return;
    }
    window.clearTimeout(cancelConfirmTimeoutRef.current);
    cancelConfirmTimeoutRef.current = undefined;
    postAppModalHostMessage(
      {
        requestId: editor.requestId,
        statusFile: editor.statusFile,
        type: "floatingPromptEditorCancel",
      },
      "PromptEditor:cancel",
    );
  };

  useEffect(() => {
    if (!isOpen || !editor) {
      return;
    }
    /**
     * CDXC:PromptEditor 2026-05-13-15:53
     * Inside the Ctrl+G floating prompt editor, Ctrl+G must save the live Monaco text instead of only opening the editor from the terminal. Escape mirrors the Cancel button: the first press turns Cancel into Confirm, and a second press within three seconds cancels the editor.
     * Save and Cancel tooltips must name those keyboard paths so hover help matches the behavior.
     *
     * CDXC:PromptEditor 2026-05-17-01:41:
     * The Confirm cancel state should stay visible for three seconds so accidental discard intent clears sooner after the user hesitates.
     *
     * CDXC:PromptEditor 2026-05-16-23:23:
     * Escape should close an open image preview popup without counting as the
     * first or second Escape for prompt-editor cancellation. The popup is a
     * transient inspection layer above the editor, not an editor discard intent.
     *
     * CDXC:PromptEditor 2026-05-19-10:05:
     * Cmd+S must save the prompt editor the same way Ctrl+G does so macOS users
     * can use the standard save shortcut inside the floating writing pane.
     */
    const handleKeyDown = (event: KeyboardEvent) => {
      const key = event.key.toLowerCase();
      if (!event.ctrlKey && !event.altKey && !event.metaKey && key === "escape" && openImagePreview) {
        event.preventDefault();
        event.stopPropagation();
        setOpenImagePreview(undefined);
        return;
      }
      if (
        (event.ctrlKey && !event.altKey && !event.metaKey && key === "g") ||
        (event.metaKey && !event.ctrlKey && !event.altKey && key === "s")
      ) {
        event.preventDefault();
        event.stopPropagation();
        save();
        return;
      }
      if (!event.ctrlKey && !event.altKey && !event.metaKey && key === "escape") {
        event.preventDefault();
        event.stopPropagation();
        requestCancel();
      }
    };
    window.addEventListener("keydown", handleKeyDown, { capture: true });
    return () => {
      window.removeEventListener("keydown", handleKeyDown, { capture: true });
    };
  }, [editor?.requestId, isCancelConfirming, isOpen, isSaving, openImagePreview]);

  useEffect(() => {
    if (
      !isOpen ||
      !editor ||
      !closeAndSaveRequestId ||
      closeAndSaveRequestId !== editor.requestId ||
      savedCloseAndSaveRequestIdRef.current === closeAndSaveRequestId
    ) {
      return;
    }
    savedCloseAndSaveRequestIdRef.current = closeAndSaveRequestId;
    save();
  }, [closeAndSaveRequestId, editor?.requestId, isOpen]);

  if (!isOpen || !editor) {
    return null;
  }

  const beginPanelPointerDrag = (
    event: ReactPointerEvent<HTMLElement>,
    mode: FloatingPromptEditorDragMode,
    getNextFrame: (
      moveEvent: PointerEvent,
      startFrame: FloatingPromptEditorFrame,
      startX: number,
      startY: number,
    ) => FloatingPromptEditorFrame,
    options: {
      preventInitialDefault?: boolean;
      refocusEditorAfterDrag?: boolean;
    } = {},
  ) => {
    if (isNativeWindowSurface) {
      return;
    }
    if (event.button !== 0) {
      return;
    }

    /**
     * CDXC:PromptEditor 2026-05-13-23:22:
     * Floating prompt editor resize/move drags must not text-select Monaco
     * gutter, editor rows, or empty editor chrome. Capture the pointer and
     * suppress document selection until the drag finishes.
     *
     * CDXC:PromptEditor 2026-06-09-19:47:
     * Resizing must leave macOS WebKit keyboard input routed to Monaco. The
     * resize handle's initial pointer-down is a trusted native focus event, so
     * that path must not call preventDefault before WebKit updates its input
     * context; refocus Monaco after drag cleanup when resize CSS is gone.
     */
    if (options.preventInitialDefault !== false) {
      event.preventDefault();
    }
    event.stopPropagation();
    activePointerDragCleanupRef.current?.();
    const dragTarget = event.currentTarget;
    dragTarget.setPointerCapture(event.pointerId);
    const startX = event.clientX;
    const startY = event.clientY;
    const startFrame = frame;
    const preventSelection = (selectionEvent: Event) => {
      selectionEvent.preventDefault();
    };
    const handleMove = (moveEvent: PointerEvent) => {
      moveEvent.preventDefault();
      window.getSelection()?.removeAllRanges();
      setFrame(clampFloatingPromptEditorFrame(getNextFrame(moveEvent, startFrame, startX, startY)));
    };
    const cleanupDrag = () => {
      window.removeEventListener("pointermove", handleMove);
      window.removeEventListener("pointerup", cleanupDrag);
      window.removeEventListener("pointercancel", cleanupDrag);
      document.removeEventListener("selectstart", preventSelection, true);
      document.removeEventListener("dragstart", preventSelection, true);
      if (dragTarget.hasPointerCapture(event.pointerId)) {
        dragTarget.releasePointerCapture(event.pointerId);
      }
      window.getSelection()?.removeAllRanges();
      activePointerDragCleanupRef.current = undefined;
      setDragMode(undefined);
      if (options.refocusEditorAfterDrag) {
        window.requestAnimationFrame(() => {
          editorRef.current?.focus?.();
        });
      }
    };
    activePointerDragCleanupRef.current = cleanupDrag;
    setDragMode(mode);
    window.getSelection()?.removeAllRanges();
    document.addEventListener("selectstart", preventSelection, true);
    document.addEventListener("dragstart", preventSelection, true);
    window.addEventListener("pointermove", handleMove, { passive: false });
    window.addEventListener("pointerup", cleanupDrag, { once: true });
    window.addEventListener("pointercancel", cleanupDrag, { once: true });
  };

  const startMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    beginPanelPointerDrag(event, "move", (moveEvent, startFrame, startX, startY) => ({
      ...startFrame,
      left: startFrame.left + moveEvent.clientX - startX,
      top: startFrame.top + moveEvent.clientY - startY,
    }));
  };

  const startResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    beginPanelPointerDrag(
      event,
      "resize",
      (moveEvent, startFrame, startX, startY) => ({
        ...startFrame,
        height: startFrame.height + moveEvent.clientY - startY,
        width: startFrame.width + moveEvent.clientX - startX,
      }),
      {
        preventInitialDefault: false,
        refocusEditorAfterDrag: true,
      },
    );
  };

  const removeImagePreview = (preview: FloatingPromptEditorImagePreview) => {
    const monacoEditor = editorRef.current;
    const model = monacoEditor?.getModel();
    if (!monacoEditor || !model) {
      return;
    }

    const currentText = monacoEditor.getValue();
    let startOffset =
      currentText.slice(preview.startOffset, preview.endOffset) === preview.markdown
        ? preview.startOffset
        : currentText.indexOf(preview.markdown);
    if (startOffset < 0) {
      return;
    }
    let endOffset = startOffset + preview.markdown.length;
    if (currentText[startOffset - 1] === "\n" && currentText[endOffset] === "\n") {
      endOffset += 1;
    } else if (currentText[endOffset] === "\n") {
      endOffset += 1;
    } else if (currentText[startOffset - 1] === "\n") {
      startOffset -= 1;
    }
    const startPosition = model.getPositionAt(startOffset);
    const endPosition = model.getPositionAt(endOffset);
    monacoEditor.pushUndoStop?.();
    monacoEditor.executeEdits("ghostex-image-preview-remove", [
      {
        forceMoveMarkers: true,
        range: {
          endColumn: endPosition.column,
          endLineNumber: endPosition.lineNumber,
          startColumn: startPosition.column,
          startLineNumber: startPosition.lineNumber,
        },
        text: "",
      },
    ]);
    monacoEditor.pushUndoStop?.();
    monacoEditor.focus?.();
    setOpenImagePreview((current) => (current?.id === preview.id ? undefined : current));
  };

  const closeImagePreviewFromBackdrop = (event: ReactPointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    if (event.target === event.currentTarget) {
      /**
       * CDXC:PromptEditor 2026-05-16-23:37:
       * The dimmed image-preview backdrop is part of the preview dismissal
       * target. Close on direct backdrop pointer-down while keeping clicks on
       * the image itself inside the popup.
       */
      setOpenImagePreview(undefined);
    }
  };

  const focusEditorFromPanelPointerDown = (event: ReactPointerEvent<HTMLElement>) => {
    const target = event.target;
    const isMonacoPointer = target instanceof Element && target.closest(".floating-prompt-editor-monaco");
    const monacoEditor = editorRef.current;
    /**
     * CDXC:PromptEditor 2026-05-17-02:15:
     * Clicking blank prompt-editor chrome should focus Monaco just like clicking
     * text. Browser default pointer handling blurs the hidden Monaco textarea
     * after React's panel handler on non-editable targets, so prevent that blur
     * outside Monaco internals and refocus after the default phase.
     *
     * CDXC:PromptEditor 2026-06-09-19:58:
     * Monaco owns its text-surface pointer events, including right-click context
     * menus. Do not run the panel-level delayed refocus for Monaco targets,
     * because that blur/focus cycle can immediately dismiss Monaco's menu.
     */
    appendPromptEditorDebugLog("react.panelPointerDown", {
      documentHasFocus: document.hasFocus(),
      hasEditorRef: monacoEditor !== null,
      isMonacoPointer: Boolean(isMonacoPointer),
      pointerType: event.pointerType,
      requestId: editor.requestId,
      targetClass:
        target instanceof Element && typeof target.className === "string" ? target.className : null,
    });
    if (isMonacoPointer) {
      return;
    }

    event.preventDefault();
    monacoEditor?.focus?.();
    window.setTimeout(() => {
      const refocusedEditor = editorRef.current;
      refocusedEditor?.focus?.();
      appendPromptEditorDebugLog("react.panelPointerDown.refocus", {
        documentHasFocus: document.hasFocus(),
        hasEditorRef: refocusedEditor !== null,
        requestId: editor.requestId,
      });
    }, 0);
  };

  return (
    <div
      className="floating-prompt-editor-root"
      data-drag-mode={dragMode}
      data-native-window={isNativeWindowSurface ? "true" : undefined}
      data-prewarm={editor.isPrewarm ? "true" : undefined}
    >
      <section
        aria-label="Prompt Editor"
        className="floating-prompt-editor-panel"
        onPointerDown={focusEditorFromPanelPointerDown}
        style={
          isNativeWindowSurface
            ? undefined
            : {
                height: `${frame.height}px`,
                left: `${frame.left}px`,
                top: `${frame.top}px`,
                width: `${frame.width}px`,
              }
        }
      >
        <div
          className="floating-prompt-editor-titlebar"
          onPointerDown={isNativeWindowSurface ? undefined : startMove}
        >
          <div className="floating-prompt-editor-title">
            {/*
             * CDXC:PromptEditor 2026-05-14-09:55:
             * The prompt editor titlebar must show the save shortcut beside
             * the title while keeping actions permanently anchored to the
             * current right edge during resize.
             */}
            <span className="floating-prompt-editor-title-text">{editor.title}</span>
            <span className="floating-prompt-editor-title-shortcut">(Save with ^G or ⌘S)</span>
          </div>
          <div className="floating-prompt-editor-actions">
            <button
              aria-label={isCancelConfirming ? "Confirm cancel prompt editor" : "Cancel prompt editor"}
              aria-keyshortcuts="Escape"
              className="floating-prompt-editor-cancel"
              data-confirming={isCancelConfirming ? "true" : undefined}
              onClick={requestCancel}
              onPointerDown={(event) => event.stopPropagation()}
              title="press escape to cancel"
              type="button"
            >
              {isCancelConfirming ? "Confirm" : "Cancel"}
            </button>
            <button
              aria-keyshortcuts="Control+G Meta+S"
              aria-label="Save prompt editor"
              className="floating-prompt-editor-save"
              disabled={isSaving}
              onClick={save}
              onPointerDown={(event) => event.stopPropagation()}
              title="press ctrl+g or cmd+s to save"
              type="button"
            >
              {isSaving ? "Saving" : "Save"}
            </button>
          </div>
        </div>
        <div className="floating-prompt-editor-monaco" ref={containerRef} spellCheck={false} />
        {imagePreviews.length > 0 ? (
          <div className="floating-prompt-editor-image-strip" onPointerDown={(event) => event.stopPropagation()}>
            {imagePreviews.map((preview) => {
              const dataUrl = imagePreviewDataUrls[preview.path];
              return (
                <div
                  aria-label={`Open image preview ${preview.path}`}
                  className="floating-prompt-editor-image-thumb"
                  key={preview.id}
                  onClick={() => setOpenImagePreview(preview)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      setOpenImagePreview(preview);
                    }
                  }}
                  role="button"
                  tabIndex={0}
                  title={preview.path}
                >
                  {dataUrl ? <img alt="" src={dataUrl} /> : <span aria-hidden="true" />}
                  <button
                    aria-label={`Remove image ${preview.path}`}
                    className="floating-prompt-editor-image-remove"
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      removeImagePreview(preview);
                    }}
                    type="button"
                  >
                    <PromptEditorCloseIcon />
                  </button>
                </div>
              );
            })}
          </div>
        ) : null}
        {isNativeWindowSurface ? null : (
          <div
            aria-label="Resize prompt editor"
            className="floating-prompt-editor-resize"
            onPointerDown={startResize}
            role="separator"
          />
        )}
      </section>
      {openImagePreview && imagePreviewDataUrls[openImagePreview.path] ? (
        <div
          className="floating-prompt-editor-image-popup"
          onPointerDown={closeImagePreviewFromBackdrop}
          role="presentation"
        >
          <button
            aria-label="Close image preview"
            className="floating-prompt-editor-image-popup-close"
            onClick={() => setOpenImagePreview(undefined)}
            type="button"
          >
            <PromptEditorCloseIcon />
          </button>
          <img
            alt=""
            onClick={(event) => {
              event.stopPropagation();
              setOpenImagePreview(undefined);
            }}
            onPointerDown={(event) => event.stopPropagation()}
            src={imagePreviewDataUrls[openImagePreview.path]}
          />
        </div>
      ) : null}
    </div>
  );
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
    addRepository,
    agentsHubCatalog,
    agentsHubFileContent,
    config,
    delayedSend,
    firstUserMessage,
    gitCommit,
    gitFileDiff,
    worktreeDelete,
    floatingPromptEditor,
    floatingPromptEditorCloseAndSaveRequestId,
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
    settingsInitialSection,
    settingsInitialRemoteMachineId,
    settingsInitialSearchQuery,
    settingsInitialTabOverride,
  } = useModalStateFromNative();
  const [agentHookStatusLoading, setAgentHookStatusLoading] = useState(false);
  const [ghostexCliStatusLoading, setGhostexCliStatusLoading] = useState(false);
  const [ghostexFolderStatsLoading, setGhostexFolderStatsLoading] = useState(false);
  const [osIntegrationStatusLoading, setOSIntegrationStatusLoading] = useState(false);
  const [isPreviousSessionsInitialLoadReady, setIsPreviousSessionsInitialLoadReady] = useState(false);
  const settings = useSidebarStore((state) => state.hud.settings);
  const agents = useSidebarStore((state) => state.hud.agents);
  const commands = useSidebarStore((state) => state.hud.commands);
  const projectSettingsProjects = useSidebarStore(
    (state) => state.hud.projectSettingsProjects ?? [],
  );
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
  const isSettingsRenderable = isSettingsModalKind(activeModal) && settings !== undefined;
  const settingsInitialTab = settingsInitialTabOverride ?? getSettingsInitialTab(activeModal);
  const isBaseActiveModalRenderable = isModalRenderable({
    activeModal,
    config,
    delayedSend,
    firstUserMessage,
    gitCommit,
    gitFileDiff,
    worktreeDelete,
    floatingPromptEditor,
    remoteGxserverInstall,
    remoteProjectPicker,
    renameSession,
    settings,
    t3BrowserAccess,
    t3ThreadId,
    worktree,
  });
  /*
  CDXC:PreviousSessions 2026-06-02-20:39:
  The native app-modal host is hidden until React posts `presented`. Previous Sessions must delay that presented signal until its first gxserver history query resolves, proves empty, or hits the two-second cap, otherwise the user sees the empty short modal before loaded rows expand it.
  */
  const isActiveModalRenderable =
    isBaseActiveModalRenderable &&
    (activeModal !== "previousSessions" || isPreviousSessionsInitialLoadReady);

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

  /**
   * CDXC:AppModals 2026-05-08-09:00
   * Native should unhide the transparent modal webview only after the requested
   * modal has enough state to render. This prevents a blank overlay flash while
   * sidebar state is still syncing into the app-modal host.
   */
  useLayoutEffect(() => {
    if (!activeModal || !isActiveModalRenderable) {
      return;
    }
    if (activeModal === "floatingPromptEditor" && floatingPromptEditor) {
      appendPromptEditorDebugLog("react.presented", {
        documentHasFocus: document.hasFocus(),
        requestId: floatingPromptEditor.requestId,
      });
    }
    const presentedMessage: { modal: AppModalKind; requestId?: string; type: "presented" } = {
      modal: activeModal,
      type: "presented",
    };
    if (activeModal === "floatingPromptEditor" && floatingPromptEditor) {
      presentedMessage.requestId = floatingPromptEditor.requestId;
    }
    postAppModalHostMessage(presentedMessage, "AppModals:presented");
  }, [activeModal, floatingPromptEditor?.requestId, isActiveModalRenderable]);

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
       * default while a modal is active, but keep text fields and Monaco editor
       * surfaces eligible for their normal editing context menus.
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
      <FloatingPromptEditorModal
        closeAndSaveRequestId={floatingPromptEditorCloseAndSaveRequestId}
        editor={floatingPromptEditor}
        isOpen={activeModal === "floatingPromptEditor" && floatingPromptEditor !== undefined}
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
            return;
          }
          vscode.postMessage({
            installApproved: true,
            remoteMachineId: remoteGxserverInstall.remoteMachineId,
            type: "reconnectRemoteMachine",
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
       * CDXC:CommandPalette 2026-05-16-20:51:
       * Cmd+K must render in the same full-window app-modal host as Settings,
       * not inside the sidebar webview. The palette reads mirrored sidebar
       * state here so its command list remains current while the dialog is
       * centered over the whole Ghostex window.
       */}
      <CommandPalette
        commands={commands}
        hotkeys={settings?.hotkeys}
        isOpen={activeModal === "commandPalette"}
        onOpenChange={(isOpen) => {
          if (!isOpen) {
            closeModal();
          }
        }}
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
      />
      {/*
       * CDXC:Worktrees 2026-06-02-13:41:
       * Creating a project worktree is a full-window modal flow because macOS
       * owns the agent, first prompt, and image attachment drafts before submit,
       * while gxserver owns the branch/worktree mutation and returned project.
       *
       * CDXC:WorktreeProjectRegistration 2026-06-02-12:53:
       * Open Existing mode in this same modal is selection-only UI. It sends
       * only the selected worktree path through the native sidebar to gxserver
       * and must not expose agent, first-prompt, image attachment, or prompt
       * helper controls.
       */}
      <WorktreeCreateModal
        agents={agents}
        defaultAgentId={settings?.defaultPromptAgentId}
        isOpen={activeModal === "worktree" && worktree !== undefined}
        onCancel={closeModal}
        onConfirm={(draft) => {
          vscode.postMessage({
            agentId: draft.mode === "create" ? draft.agentId : undefined,
            existingWorktreePath:
              draft.mode === "openExisting" ? draft.existingWorktreePath : undefined,
            mode: draft.mode,
            projectId: worktree?.projectId,
            projectPath: worktree?.projectPath,
            prompt: draft.mode === "create" ? draft.prompt : undefined,
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
        initialSection={settingsInitialSection}
        initialRemoteMachineId={settingsInitialRemoteMachineId}
        initialSearchQuery={settingsInitialSearchQuery}
        initialTab={settingsInitialTab}
        isOpen={isSettingsRenderable}
        onChange={(nextSettings) => {
          vscode.postMessage({
            settings: nextSettings,
            type: "updateSettings",
          });
        }}
        onGhosttySettingsAction={(action) => {
          vscode.postMessage({ type: action });
        }}
        onInstallGte={() => {
          vscode.postMessage({ type: "installGte" });
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
        onInstallGenerateTitleSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installGenerateTitleSkill" });
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
        onOpenFirstLaunchSetup={() => {
          vscode.postMessage({ type: "openWorkspaceWelcome" });
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
        onTestAgentTaskCompletion={() => {
          vscode.postMessage({ type: "testAgentTaskCompletion" });
        }}
        onClose={closeModal}
        projects={projectSettingsProjects}
        settings={settings}
        vscode={vscode}
        ghostexCliStatus={ghostexCliStatus}
        ghostexCliStatusLoading={ghostexCliStatusLoading}
        ghostexFolderStats={ghostexFolderStats}
        ghostexFolderStatsLoading={ghostexFolderStatsLoading}
        osIntegrationStatus={osIntegrationStatus}
        osIntegrationStatusLoading={osIntegrationStatusLoading}
      />
      <FirstLaunchSetupModal
        agentHookStatus={agentHookStatus}
        agentHookStatusLoading={agentHookStatusLoading}
        ghostexCliStatus={ghostexCliStatus}
        ghostexCliStatusLoading={ghostexCliStatusLoading}
        isOpen={activeModal === "firstLaunchSetup" || activeModal === "tipsAndTricks"}
        onChange={(nextSettings) => {
          vscode.postMessage({
            settings: nextSettings,
            type: "updateSettings",
          });
        }}
        onClose={closeModal}
        onInstallAgentHooks={() => {
          setAgentHookStatusLoading(true);
          vscode.postMessage({ type: "installAgentHooks" });
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
        onInstallGenerateTitleSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: "installGenerateTitleSkill" });
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
        onRequestAgentHookStatus={() => {
          setAgentHookStatusLoading(true);
          vscode.postMessage({ type: "requestAgentHookStatus" });
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
      <CommandConfigModal
        draft={config.commandDraft ?? createEmptyCommandDraft()}
        isOpen={activeModal === "commandConfig" && config.commandDraft !== undefined}
        lockedActionType={config.lockedActionType}
        onCancel={closeModal}
        onSave={(draft) => {
          vscode.postMessage({
            actionType: draft.actionType,
            closeTerminalOnExit: draft.closeTerminalOnExit,
            command: draft.command,
            commandId: draft.commandId,
            icon: draft.icon,
            iconColor: draft.iconColor,
            name: draft.name,
            playCompletionSound: draft.playCompletionSound,
            type: "saveSidebarCommand",
            url: draft.url,
          });
          closeModal();
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
      />
      {/*
       * CDXC:AppToasts 2026-05-21-12:21:
       * Native/sidebar status feedback should appear as dark Ghostex toasts,
       * not Sonner's bright default surface, so non-blocking Delayed Send and
       * worktree/git notices stay visually consistent with the dark app chrome.
       *
       * CDXC:AppModals 2026-05-28-13:52:
       * Toast overlay chrome should use the same #0e0e0e background as modal
       * and menu overlays instead of the older #181818 surface.
       */}
      <Toaster
        offset={{ bottom: APP_MODAL_TOAST_BOTTOM_OFFSET_PX }}
        position="bottom-center"
        richColors
        theme="dark"
        toastOptions={{
          style: {
            background: "#0e0e0e",
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
  const [floatingPromptEditor, setFloatingPromptEditor] = useState<FloatingPromptEditorState>();
  const [floatingPromptEditorCloseAndSaveRequestId, setFloatingPromptEditorCloseAndSaveRequestId] =
    useState<string>();
  const [remoteGxserverInstall, setRemoteGxserverInstall] =
    useState<RemoteGxserverInstallState>();
  const [remoteProjectPicker, setRemoteProjectPicker] = useState<RemoteProjectPickerState>();
  const [renameSession, setRenameSession] = useState<RenameSessionModalState>();
  const [t3BrowserAccess, setT3BrowserAccess] = useState<T3BrowserAccessMessage>();
  const [t3ThreadId, setT3ThreadId] = useState<T3ThreadIdModalState>();
  const [worktree, setWorktree] = useState<WorktreeModalState>();
  const [agentHookStatus, setAgentHookStatus] = useState<AgentHookStatusMessage>();
  const [ghostexCliStatus, setGhostexCliStatus] = useState<GhostexCliStatusMessage>();
  const [ghostexFolderStats, setGhostexFolderStats] = useState<SidebarGhostexFolderStatsMessage>();
  const [osIntegrationStatus, setOSIntegrationStatus] = useState<OSIntegrationStatusMessage>();
  const [settingsInitialSection, setSettingsInitialSection] =
    useState<MainSettingsInitialSectionId>();
  const [settingsInitialRemoteMachineId, setSettingsInitialRemoteMachineId] = useState<string>();
  const [settingsInitialSearchQuery, setSettingsInitialSearchQuery] = useState<string>();
  const [settingsInitialTabOverride, setSettingsInitialTabOverride] = useState<SettingsModalTab>();
  const activeModalRef = useRef<AppModalKind | undefined>(activeModal);
  const toastTokenRef = useRef(0);

  const clearActiveModalState = useCallback(() => {
    setActiveModal(undefined);
    setAddRepository({});
    setConfig({});
    setDelayedSend(undefined);
    setFirstUserMessage(undefined);
    setGitCommit(undefined);
    setGitFileDiff(undefined);
    setWorktreeDelete(undefined);
    setFloatingPromptEditor(undefined);
    setRemoteGxserverInstall(undefined);
    setRemoteProjectPicker(undefined);
    setRenameSession(undefined);
    setT3BrowserAccess(undefined);
    setT3ThreadId(undefined);
    setWorktree(undefined);
    setGhostexFolderStats(undefined);
    setOSIntegrationStatus(undefined);
    setAgentsHubCatalog(undefined);
    setAgentsHubFileContent(undefined);
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
          if (isAppModalDebugLoggingEnabled()) {
            postAppModalHostMessage(
              {
                details: JSON.stringify({
                  hasSettings: useSidebarStore.getState().hud.settings !== undefined,
                  modal: message.modal,
                  performanceNow: performance.now(),
                }),
                event: "modalHost.open.received",
                type: "debugLog",
              },
              "AppModals:debug",
            );
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
            setFloatingPromptEditor(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
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
            setFloatingPromptEditor(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
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
            setRemoteGxserverInstall({
              remoteMachineId: message.remoteMachineId,
              remoteMachineName: message.remoteMachineName,
            });
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setFloatingPromptEditor(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
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
            setFloatingPromptEditor(undefined);
            setRemoteGxserverInstall(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
            setWorktreeDelete(undefined);
          } else if (message.modal === "floatingPromptEditor") {
            if (
              typeof message.requestId !== "string" ||
              typeof message.filePath !== "string" ||
              typeof message.initialText !== "string"
            ) {
              throw new Error("Floating prompt editor request is missing required state.");
            }
            const initialText = trimPromptEditorTrailingSpaces(message.initialText);
            /**
             * CDXC:PromptEditor 2026-05-13-09:48
             * Ctrl+G Monaco prompt editing is rendered by the app-modal host
             * component tree while native owns the file/status bridge contract.
             *
             * CDXC:PromptEditor 2026-05-13-10:22
             * Prompt editor buffers are always Markdown, regardless of caller
             * hints, so wrapped prose and Markdown tokenization stay consistent
             * in the floating pane.
             *
             * CDXC:PromptEditor 2026-05-28-07:47:
             * The editor should open with trailing line spaces already removed,
             * matching paste sanitization so users do not inherit invisible
             * whitespace from the captured prompt buffer.
             *
             * CDXC:PromptEditor 2026-06-11-22:51:
             * The rich prompt editor now runs in a native child-window modal
             * host instead of the full-workspace overlay, so React renders the
             * same Monaco surface without owning workspace hit testing, window
             * movement, or window resizing.
             */
            appendPromptEditorDebugLog("react.openMessage", {
              hasInitialFrame: message.initialFrame !== undefined,
              initialTextLength: initialText.length,
              isPrewarm: message.prewarm === true,
              nativeOpenStartedAtMs:
                typeof message.nativeOpenStartedAtMs === "number"
                  ? message.nativeOpenStartedAtMs
                  : null,
              nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow",
              requestId: message.requestId,
            });
            setFloatingPromptEditor({
              filePath: message.filePath,
              initialFrame: message.initialFrame,
              initialText,
              isPrewarm: message.prewarm === true,
              language: "markdown",
              requestId: message.requestId,
              statusFile: message.statusFile,
              title: message.title || "Prompt Editor",
            });
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
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
            setFloatingPromptEditor(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
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
            setFloatingPromptEditor(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
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
            setFloatingPromptEditor(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setWorktree(undefined);
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
            setFloatingPromptEditor(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
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
            setFloatingPromptEditor(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
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
            setFloatingPromptEditor(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
            setWorktreeDelete(undefined);
          } else if (message.modal === "gitFileDiff") {
            if (!message.gitFileDiff) {
              throw new Error("Git file diff modal request is missing gitFileDiff.");
            }
            setGitFileDiff(message.gitFileDiff);
            return;
          } else if (message.modal === "commandConfig") {
            if (!message.commandDraft) {
              throw new Error("Command config modal request is missing commandDraft.");
            }
            setConfig({
              commandDraft: message.commandDraft,
              lockedActionType: message.lockedActionType,
            });
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setFloatingPromptEditor(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
            setWorktreeDelete(undefined);
          } else if (message.modal === "agentConfig") {
            if (!message.agentDraft) {
              throw new Error("Agent config modal request is missing agentDraft.");
            }
            setConfig({ agentDraft: message.agentDraft });
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setFloatingPromptEditor(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
            setWorktreeDelete(undefined);
          } else {
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setFloatingPromptEditor(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setT3BrowserAccess(undefined);
            setT3ThreadId(undefined);
            setWorktree(undefined);
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
          setActiveModal(message.modal);
          return;
        }

        if (message.type === "close") {
          if (activeModalRef.current === "floatingPromptEditor") {
            appendPromptEditorDebugLog("react.closeMessage", {
              activeModal: activeModalRef.current,
            });
          }
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
            description: message.description,
            duration: isPersistent ? Number.POSITIVE_INFINITY : undefined,
            id: message.toastId,
            style:
              message.level === "error"
                ? {
                    background: "linear-gradient(0deg, rgba(95, 24, 31, 0.28), rgba(95, 24, 31, 0.28)), #0e0e0e",
                    border: "1px solid rgba(248, 113, 113, 0.32)",
                    color: "#fff1f2",
                  }
                : message.level === "success"
                  ? {
                      background: "linear-gradient(0deg, rgba(22, 101, 52, 0.24), rgba(22, 101, 52, 0.24)), #0e0e0e",
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

        if (message.type === "floatingPromptEditorCloseAndSave") {
          setFloatingPromptEditorCloseAndSaveRequestId(message.requestId);
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
    appendPromptEditorDebugLog("react.modalHost.ready", {
      nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === "nativeWindow",
    });
    postAppModalHostMessage({ type: "ready" }, "AppModals:ready");
    /*
     * CDXC:AppModals 2026-06-11-19:46:
     * Native child windows reuse modal-host.html for the app modal family.
     *
     * CDXC:PromptEditor 2026-06-11-22:51:
     * The rich prompt editor also opens in a native child window now. Keep the
     * generic child-window host light on first paint and let the prompt editor
     * load Monaco only when its own open message arrives.
     */
    if (window.__ghostex_APP_MODAL_HOST_SURFACE__ !== "nativeWindow") {
      loadModalHostMonaco().catch((error) => {
        logAppModalError("PromptEditor:prewarmMonacoLoad", error);
      });
    }
    return () => {
      window.removeEventListener("ghostex-app-modal-host-message", handleMessage);
    };
  }, []);

  return {
    activeModal,
    addRepository,
    agentsHubCatalog,
    agentsHubFileContent,
    config,
    delayedSend,
    firstUserMessage,
    gitCommit,
    gitFileDiff,
    worktreeDelete,
    floatingPromptEditor,
    floatingPromptEditorCloseAndSaveRequestId,
    closeGitFileDiff,
    closeModal,
    remoteProjectPicker,
    renameSession,
    remoteGxserverInstall,
    t3BrowserAccess,
    t3ThreadId,
    worktree,
    agentHookStatus,
    ghostexCliStatus,
    ghostexFolderStats,
    osIntegrationStatus,
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

function createEmptyCommandDraft(): CommandConfigDraft {
  return {
    actionType: "terminal",
    closeTerminalOnExit: false,
    name: "",
    playCompletionSound: false,
  };
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
  floatingPromptEditor,
  remoteProjectPicker,
  remoteGxserverInstall,
  renameSession,
  settings,
  t3BrowserAccess,
  t3ThreadId,
  worktree,
}: {
  activeModal: AppModalKind | undefined;
  config: ConfigModalState;
  delayedSend: DelayedSendModalState | undefined;
  firstUserMessage: FirstUserMessageModalState | undefined;
  gitCommit: GitCommitModalDraft | undefined;
  gitFileDiff: GitFileDiffModalDraft | undefined;
  worktreeDelete: WorktreeDeleteModalDraft | undefined;
  floatingPromptEditor: FloatingPromptEditorState | undefined;
  remoteProjectPicker: RemoteProjectPickerState | undefined;
  remoteGxserverInstall: RemoteGxserverInstallState | undefined;
  renameSession: RenameSessionModalState | undefined;
  settings: unknown;
  t3BrowserAccess: T3BrowserAccessMessage | undefined;
  t3ThreadId: T3ThreadIdModalState | undefined;
  worktree: WorktreeModalState | undefined;
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
    case "commandConfig":
      return config.commandDraft !== undefined;
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
    case "floatingPromptEditor":
      return floatingPromptEditor !== undefined;
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
    case "daemonSessions":
    case "pinnedPrompts":
    case "previousSessions":
    case "scratchPad":
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
