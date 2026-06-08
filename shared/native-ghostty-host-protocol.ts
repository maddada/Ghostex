export const NATIVE_GHOSTTY_HOST_PROTOCOL_VERSION = 1;

import type { SidebarProjectDiffStats } from "./project-diff-stats";
import type { SidebarCommandButton } from "./sidebar-commands";
import type {
  CustomWorkspaceOpenTarget,
  WorkspaceOpenTargetAvailability,
} from "./workspace-open-targets";

export type NativeTerminalLayout =
  | {
      kind: "leaf";
      sessionId: string;
    }
  | {
      activeSessionId?: string;
      kind: "tabs";
      sessionIds: string[];
    }
  | {
      children: NativeTerminalLayout[];
      direction: "horizontal" | "vertical";
      kind: "split";
      ratio?: number;
    };

export type NativeTerminalTitleBarAction =
  | "close"
  | "closeCommandsPanel"
  | "delayedSend"
  | "expandCommandsPanel"
  | "fork"
  | "mergeAllTabs"
  | "newTerminal"
  | "openBrowser"
  | "pinCommandsPanel"
  | "popOut"
  | "reload"
  | "rename"
  | "restorePopOut"
  | "rotatePanesClockwise"
  | "sleep"
  | "splitHorizontal"
  | "splitVertical"
  | "unpinCommandsPanel";

export type TitlebarResourceGroup = {
  groupId: string;
  isActive: boolean;
  projectId?: string;
  projectName: string;
  projectPath: string;
  sessions: TitlebarResourceSession[];
  title: string;
};

export type TitlebarResourceSession = {
  activity: "attention" | "idle" | "working";
  agentIcon?: string;
  /**
   * CDXC:DelayedSend 2026-05-17-03:14
   * React titlebar resources need Delayed Send state so any terminal picker or
   * context menu using this resource graph can expose the active countdown.
   */
  delayedSendDeadlineAt?: string;
  delayedSendRemainingLabel?: string;
  delayedSendRemainingMs?: number;
  /**
   * CDXC:SessionLifecycle 2026-05-29-09:20:
   * Titlebar resources consume the same explicit lifecycle resources as the
   * sidebar: native pane mount state, provider session existence, and derived
   * live state. Keep legacy booleans for older hosts while new code avoids
   * conflating pane sleep with zmx/tmux/zellij liveness.
   *
   * CDXC:SessionLifecycle 2026-05-29-06:29:
   * Persistence-disabled terminal sessions use `providerSessionState:
   * "persistence-disabled"` so resource panels do not present a providerless
   * backend as an unknown one.
   *
   * CDXC:SessionLifecycle 2026-05-29-07:19:
   * Use `persistence-disabled` instead of a generic `disabled` value so
   * titlebar/resource payloads state exactly which provider capability is off.
   */
  nativePaneState?: "mounted" | "mounting" | "unmounted";
  providerSessionState?: "exists" | "missing" | "persistence-disabled" | "unknown";
  isLive?: boolean;
  isRunning: boolean;
  isSleeping?: boolean;
  lastInteractionAt?: string;
  projectId?: string;
  sessionId: string;
  sessionKind?: "browser" | "terminal" | "t3";
  sessionPersistenceName?: string;
  sessionPersistenceProvider?: "tmux" | "zmx" | "zellij";
  terminalTitle?: string;
  title: string;
};

export type NativeGhosttyHostCommand =
  | {
      activateOnCreate?: boolean;
      cwd: string;
      env?: Record<string, string>;
      initialInput?: string;
      sessionId: string;
      sessionPersistenceName?: string;
      sessionPersistenceProvider?: "tmux" | "zmx" | "zellij";
      shellCommand?: string;
      title?: string;
      tmuxMode?: boolean;
      tmuxSessionName?: string;
      type: "createTerminal";
    }
  | {
      browserFeedbackTool?: "react-grab" | "agentation";
      cwd?: string;
      projectId?: string;
      sessionId: string;
      threadId?: string;
      title: string;
      type: "createWebPane";
      url: string;
    }
    | {
      command?: string;
      cwd?: string;
      editorKind?: "terminal" | "monaco";
      env?: Record<string, string>;
      filePath?: string;
      language?: string;
      originatingSessionId?: string;
      requestId?: string;
      statusFile?: string;
      title?: string;
      type: "openFloatingEditor";
    }
  | {
      preservePersistenceSession?: boolean;
      sessionId: string;
      type: "closeTerminal";
    }
  | {
      sessionId: string;
      type: "closeWebPane";
    }
  | {
      sessionId: string;
      type: "focusTerminal";
    }
  | {
      sessionId: string;
      type: "focusWebPane";
    }
  | {
      cwd: string;
      type: "startT3CodeRuntime";
    }
  | {
      /**
       * CDXC:T3Code 2026-06-06-05:13:
       * This message remains for protocol compatibility only. Native managed T3
       * web panes now own provider lifetime, so stale sidebar/gxserver projection
       * cannot stop t3code while an embedded T3 tab is still open.
       */
      runtimeCwd?: string;
      runningSessionIds: string[];
      type: "setT3CodeRuntimeSessionState";
    }
  | {
      type: "stopT3CodeRuntime";
    }
  | {
      /**
       * CDXC:EditorPanes 2026-05-06-14:21
       * Project editor buttons launch a shared embedded code-server runtime,
       * then native AppKit creates one persistent Chromium editor surface per
       * project. These commands stay separate from terminal/web-pane sessions
       * because editor panes must not participate in split layout.
       *
       * CDXC:EditorPanes 2026-05-06-15:00
       * The runtime command carries the VS Code user-config link setting so the
       * native launcher can pass code-server's CLI flags before the editor
       * process starts instead of mutating the embedded VS Code UI later.
       *
       * CDXC:EditorPanes 2026-06-06-23:50:
       * VS Code server startup failures must be tied back to the project editor
       * row so the sidebar can show the real launch error and toast immediately.
       */
      cwd: string;
      linkVscodeUserConfig?: boolean;
      projectId?: string;
      type: "startCodeServerRuntime";
      vscodeUserConfigDir?: string;
    }
  | {
      type: "stopCodeServerRuntime";
    }
  | {
      /**
       * CDXC:GitProjectTabs 2026-05-16-07:42:
       * Git mode needs visible project-scoped browser chrome: reuse the native
      * browser address toolbar and the main work-area tab strip for each open
      * project's Git view while leaving Code and Project editor panes plain.
       */
      browserFeedbackTool?: "react-grab" | "agentation";
      mode?: "code" | "git" | "tasks";
      companionPaneHidden?: boolean;
      projectId: string;
      projectTitle?: string;
      showsBrowserToolbar?: boolean;
      showsProjectTabs?: boolean;
      title: string;
      type: "createProjectEditorPane";
      url: string;
    }
  | {
      projectId: string;
      type: "focusProjectEditorPane";
    }
  | {
      projectId: string;
      type: "closeProjectEditorPane";
    }
  | {
      sessionId: string;
      text: string;
      type: "writeTerminalText";
    }
  | {
      sessionId: string;
      text: string;
      type: "writeTerminalScript";
    }
  | {
      provider: "tmux" | "zmx" | "zellij";
      requestId: string;
      sessionName: string;
      type: "checkPersistenceSession";
    }
  | {
      layout: NativeTerminalLayout;
      type: "setTerminalLayout";
    }
  | {
      activeProjectEditorId?: string;
      activeProjectDiffStats?: SidebarProjectDiffStats;
      activeProjectMode?: "agents" | "code" | "git" | "tasks";
      activeProjectEditorCompanionPaneHidden?: boolean;
      activeProjectEditorIsOpen?: boolean;
      activeProjectEditorIsSleeping?: boolean;
      activeProjectEditorStatus?: "idle" | "opening" | "running" | "error";
      activeProjectId?: string;
      activeProjectIconDataUrl?: string;
      activeProjectName?: string;
      activeProjectPath?: string;
      activeSessionIds: string[];
      commandsPanelActiveSessionIds?: string[];
      commandsPanelFocusedSessionId?: string;
      commandsPanelHeightRatio?: number;
      commandsPanelDefaultHeightPx?: number;
      commandsPanelIsVisible?: boolean;
      commandsPanelLayout?: NativeTerminalLayout;
      commandsPanelMode?: "floating" | "pinned";
      /**
       * CDXC:NativeWindowChrome 2026-05-10-14:19
       * Native host commands carry the outer app title separately from pane
       * titles so project switches can update macOS chrome without changing
       * individual terminal/browser title bars.
       */
      appTitle?: string;
      attentionSessionIds?: string[];
      backgroundColor?: string;
      debuggingMode?: boolean;
      focusRequestId?: number;
      focusedSessionId?: string;
      /**
       * CDXC:SessionFocusMode 2026-05-23-14:35:
       * The React titlebar needs to know when reversible pane-tab Focus mode is active so it can expose an explicit exit control beside the mode switcher.
       */
      isFocusModeActive?: boolean;
      /**
       * CDXC:SessionFocusMode 2026-05-28-12:52:
       * Native tab context menus need a per-session Focus availability list because AppKit cannot infer whether a tab belongs to a split pane or a single tabbed pane from tab count alone.
       *
       * CDXC:SessionFocusMode 2026-05-28-15:35:
       * Availability follows rendered awake pane owners, so a persisted split whose other owner is sleeping does not show Focus in the native tab context menu.
       */
      sessionFocusModeAvailableSessionIds?: string[];
      sleepingSessionIds?: string[];
      /**
       * CDXC:NativeGpu 2026-05-08-16:45
       * Sidebar status/title/icon updates must still reach native pane chrome,
       * but they must not be treated as geometry changes. This flag lets the
       * native host skip AppKit surface relayout when only metadata changed.
       *
       * CDXC:PaneTabs 2026-06-04-12:54:
       * Tab owner selection is not full split geometry. The host needs a
       * separate signal so it can surface the selected tab without reframing
       * adjacent CEF/editor panes.
       */
      layoutChanged?: boolean;
      paneOwnerSelectionChanged?: boolean;
      layout?: NativeTerminalLayout;
      paneGap?: number;
      /**
       * CDXC:PanePopOut 2026-05-11-09:35
       * Layout sync keeps popped-out sessions in the split/tab tree while
       * telling AppKit to render a placeholder in-app and move the live native
       * surface into a ghostex-owned window.
       */
      poppedOutSessionIds?: string[];
      sessionActivities?: Record<string, "attention" | "sleeping" | "working">;
      sessionAgentIconColors?: Record<string, string>;
      sessionAgentIconDataUrls?: Record<string, string>;
      /**
       * CDXC:DelayedSend 2026-05-17-03:14
       * Native tab strips and pane overlays are outside React, so layout sync
       * must carry the active Delayed Send countdown labels into AppKit.
      */
      sessionDelayedSendRemainingLabels?: Record<string, string>;
      sessionFaviconDataUrls?: Record<string, string>;
      sessionFirstPromptTitleGenerationSessionIds?: string[];
      sessionTitleBarActions?: Record<string, NativeTerminalTitleBarAction[]>;
      sessionTitles?: Record<string, string>;
      /**
       * CDXC:SessionPersistence 2026-05-23-00:50:
       * Native pane overlays are outside React, so Settings must send the
       * top-right provider/session visibility preference with layout sync.
       */
      showSessionIdInTerminalPanes?: boolean;
      showProjectEditorDiffFileCount?: boolean;
      sidebarActions?: {
        commands: SidebarCommandButton[];
      };
      titlebarResourceGroups?: TitlebarResourceGroup[];
      type: "setActiveTerminalSet";
      workspaceOpenTargets?: {
        availability: WorkspaceOpenTargetAvailability;
        customTargets: CustomWorkspaceOpenTarget[];
        hiddenTargetIds: string[];
      };
    }
  | {
      sessionId: string;
      type: "setTerminalVisibility";
      visible: boolean;
    }
  | {
      /**
       * CDXC:SessionAttentionNotifications 2026-05-11-01:14
       * Settings must be able to show the native notification permission prompt
       * and open macOS Notification Settings without faking an attention event.
       */
      type: "requestMacOSNotificationPermission" | "openMacOSNotificationSettings";
    }
  | {
      /**
       * CDXC:SessionAttentionNotifications 2026-05-10-16:46
       * The sidebar owns attention transitions and rate limits; the native host
       * only presents the macOS banner and reports a click for exact-session
       * focus routing.
       */
      body?: string;
      iconDataUrl?: string;
      sessionId: string;
      title: string;
      type: "showSessionAttentionNotification";
    };

export type NativeGhosttyHostEvent =
  | {
      foregroundPid?: number;
      persistenceSessionCreated?: boolean;
      sessionId: string;
      sessionPersistenceName?: string;
      tmuxSessionName?: string;
      ttyName?: string;
      type: "terminalReady";
    }
  | {
      error?: string;
      exists: boolean;
      provider: "tmux" | "zmx" | "zellij";
      requestId: string;
      sessionName: string;
      type: "persistenceSessionState";
    }
  | {
      sessionId: string;
      sessionPersistenceName?: string;
      title: string;
      tmuxSessionName?: string;
      type: "terminalTitleChanged";
    }
  | {
      faviconDataUrl?: string;
      sessionId: string;
      type: "browserFaviconChanged";
    }
  | {
      sessionId: string;
      type: "browserUrlChanged";
      url: string;
    }
  | {
      cwd: string;
      sessionId: string;
      type: "terminalCwdChanged";
    }
  | {
      exitCode?: number;
      sessionId: string;
      type: "terminalExited";
    }
  | {
      sessionId: string;
      type: "terminalFocused";
    }
  | {
      sessionId: string;
      type: "terminalBell";
    }
  | {
      /**
       * CDXC:SessionTitleSync 2026-05-30-05:44:
       * Native Ghostty panes own keyboard input while the title-generation
       * overlay is visible, so Escape cancellation is reported through the
       * host event stream instead of React DOM key handling.
       */
      sessionId: string;
      type: "firstPromptAutoRenameCancelled";
    }
  | {
      /**
       * CDXC:SessionSurfaceRecovery 2026-05-23-09:05:
       * AppKit reports this when an active/focused layout id has no native
       * terminal or web surface. The sidebar owns recovery because it can full
       * reload restorable agent sessions or replace non-restorable records with
       * a fresh terminal in the same slot.
       */
      sessionId: string;
      type: "nativeSessionSurfaceMissing";
    }
  | {
      /**
       * CDXC:SessionRestore 2026-05-28-16:13:
       * Native blocks provider-backed terminal restore before launch when the
       * saved cwd was deleted and the backend session must be recreated there.
       * The sidebar confirms removal instead of showing a pane that exits.
       */
      cwd: string;
      reason: "missingCwd" | string;
      sessionId: string;
      type: "terminalRestoreBlocked";
    }
  | {
      heightPx?: number;
      heightRatio: number;
      type: "commandsPanelHeightRatioChanged";
    }
  | {
      commandsPanelHeightPx: number;
      sidebarWidthPx: number;
      type: "nativeChromeLayoutChanged";
    }
  | {
      sessionId: string;
      type: "sessionAttentionNotificationClicked";
    }
  | {
      message: string;
      sessionId: string;
      type: "terminalError";
    }
  | {
      placement?: "bottom" | "center" | "left" | "right" | "top";
      sourceSessionId: string;
      targetSessionId: string;
      type: "paneReorderRequested";
    }
  | {
      sessionId: string;
      type: "paneTabSelected";
    }
  | {
      /**
       * CDXC:SessionFocusMode 2026-05-23-09:28:
       * Native tab Focus is separate from selection because it enters the
       * reversible session-focus mode and may temporarily switch the project
       * workarea back to Agents before restoring Code/Git/Project on unfocus.
       */
      sessionId: string;
      type: "paneTabFocusRequested";
    }
  | {
      /**
       * CDXC:PaneTabs 2026-05-11-01:43
       * Native tab-bar drags report before/after target placement so the
       * sidebar can reorder the containing paneLayout tab group without
       * interpreting the gesture as a pane split/drop.
       */
      position: "after" | "before";
      sourceSessionId: string;
      targetSessionId: string;
      type: "paneTabReorderRequested";
    }
  | {
      /**
       * CDXC:PaneTabs 2026-05-11-00:45
       * Native tab context menus report a clicked tab plus a scoped close
       * command. The sidebar resolves the tab group from paneLayout so bulk
       * close actions never apply to every visible tab or another group.
       */
      scope: "close" | "closeLeft" | "closeOthers" | "closeRight";
      sessionId: string;
      type: "paneTabCloseRequested";
    }
  | {
      /**
       * CDXC:PaneTabs 2026-05-11-02:16
       * Native tab sleep context-menu actions use tab-group scoped targets and
       * keep sessions restorable through the normal wake path.
       */
      scope: "sleep" | "sleepLeft" | "sleepOthers" | "sleepRight";
      sessionId: string;
      type: "paneTabSleepRequested";
    }
  | {
      /**
       * CDXC:ProjectEditorCompanion 2026-05-14-09:19:
       * The native embedded-editor companion Back button returns to the normal
       * agents workarea and reports that mode change to the sidebar so later
       * layout syncs do not reopen the project editor.
       */
      projectId: string;
      type: "projectEditorBackRequested";
    }
  | {
      /**
       * CDXC:ProjectEditorCompanion 2026-05-16-14:42:
       * Closing the agent side pane is project state shared by Code, Git, and
       * Project surfaces. Native reports the close so the sidebar can persist
       * the hidden preference across mode switches and app restarts.
       */
      hidden: boolean;
      projectId: string;
      type: "projectEditorCompanionPaneHiddenChanged";
    }
  | {
      /**
       * CDXC:GitProjectTabs 2026-05-16-09:50:
       * Native Git project tabs and toolbar buttons report the selected
       * project-editor id plus active tab URL so React can make Git mode the
       * authoritative active surface before the next layout sync. This prevents
       * browser toolbar actions like Back from resurrecting the same project's
       * Code CEF pane.
       */
      projectId: string;
      type: "projectEditorTabSelected";
      url?: string;
    }
  | {
      /**
       * CDXC:EditorPanes 2026-05-09-17:24
       * Native reports project editor load state separately from terminal
       * sessions so the sidebar can keep the VS Code row visible through
       * startup, success, and error states.
       */
      message?: string;
      projectId: string;
      status: "opening" | "running" | "error";
      type: "projectEditorLoadState";
    }
  | {
      /**
       * CDXC:EditorPanes 2026-06-06-23:50:
       * Native reports code-server startup failures before browser navigation so
       * the app can show a toast instead of waiting for the generic open timeout.
       */
      message: string;
      projectId?: string;
      type: "codeServerRuntimeStartFailed";
    }
  | {
      projectId: string;
      serverOrigin: string;
      sessionId: string;
      threadId: string;
      type: "t3ThreadReady";
      workspaceRoot: string;
    }
  | {
      protocolVersion: typeof NATIVE_GHOSTTY_HOST_PROTOCOL_VERSION;
      type: "hostReady";
    };
