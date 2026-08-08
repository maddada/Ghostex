import {
  IconAlertTriangle,
  IconArrowsDiagonal2,
  IconArrowsDiagonalMinimize,
  IconBook2,
  IconBox,
  IconCalendarTime,
  IconCheck,
  IconChevronDown,
  IconChecklist,
  IconCode,
  IconCoffee,
  IconCommand,
  IconCpu,
  IconDeviceDesktop,
  IconDownload,
  IconFolderOpen,
  IconFocus2,
  IconGitCompare,
  IconGitCommit,
  IconGitPullRequest,
  IconHistory,
  IconInfoCircle,
  IconLayoutSidebar,
  IconLayoutSidebarLeftCollapse,
  IconLayoutSidebarLeftExpand,
  IconLayoutSidebarRight,
  IconLoader2,
  IconKeyboard,
  IconMoon,
  IconPlayerPlay,
  IconRefresh,
  IconRocket,
  IconRobotFace,
  IconSearch,
  IconSettings,
  IconStarFilled,
  IconSquareMinus,
  IconStackPush,
  IconTerminal2,
  IconTool,
  IconUpload,
  IconUser,
  IconUsersGroup,
  IconWorld,
  IconX,
} from "@tabler/icons-react";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
  type ReactElement,
  type ReactNode,
} from "react";
import { createRoot } from "react-dom/client";
import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import { cn } from "@/lib/utils";
import { AppTooltip, TooltipProvider } from "../../sidebar/app-tooltip";
import { openQuickAccess } from "../../sidebar/app-modal-host-bridge";
import type { SidebarProjectDiffStats } from "../../shared/project-diff-stats";
import { createDefaultSidebarProjectDiffStats } from "../../shared/project-diff-stats";
import {
  getSidebarCommandPreviewLabel,
  isSidebarCommandConfigured,
  type SidebarCommandButton,
} from "../../shared/sidebar-commands";
import { AGENT_LOGO_COLORS, AGENT_LOGOS } from "../../sidebar/agent-logos";
import {
  getDefaultSidebarAgentById,
  getDefaultSidebarAgentByIcon,
  type SidebarAgentIcon,
} from "../../shared/sidebar-agents";
import type {
  SidebarAgentHookStatusMessage,
  SidebarGhostexCliStatusMessage,
  SidebarPortlessState,
} from "../../shared/session-grid-contract-sidebar";
import type { NativePortlessAdminInstallAction } from "../../shared/native-ghostty-host-protocol";
import { resolveSidebarTheme, type SidebarTheme } from "../../shared/session-grid-contract";
import {
  getSidebarTitlebarGradientColors,
  getSidebarTitlebarForegroundForBackground,
  isDiagnosticLoggingScenarioEnabled,
  KEEP_AWAKE_DURATION_OPTIONS,
  normalizeghostexSettings,
  type DiagnosticLoggingSettings,
  type KeepAwakeDurationMinutes,
  type SidebarSide,
  type SessionPersistenceProvider,
  type TerminalDevServerOpenTarget,
} from "../../shared/ghostex-settings";
import {
  normalizeghostexHotkeySettings,
  type ghostexHotkeySettings,
} from "../../shared/ghostex-hotkeys";
import {
  BUILT_IN_WORKSPACE_OPEN_TARGETS,
  type CustomWorkspaceOpenTarget,
  type WorkspaceIdeTargetApp,
  type WorkspaceOpenTargetAvailability,
  type WorkspaceOpenTargetDefinition,
} from "../../shared/workspace-open-targets";
import { EditorBrandIcon, getEditorBrandIconId } from "../../sidebar/brand-icons";
import { formatSidebarHotkeyLabel } from "../../sidebar/hotkey-label";
import { SidebarCommandIconGlyph } from "../../sidebar/sidebar-command-icon";
import {
  createCombinedProjectSessionId,
  parseCombinedProjectGroupId,
  parseCombinedProjectSessionId,
} from "./combined-sidebar-mode";
import "../../sidebar/styles.css";
import {
  buildSidebarGitMenuItems,
  createDefaultSidebarGitState,
  getSidebarGitDisabledReason,
  hasSidebarGitRemoteCommitDelta,
  resolveSidebarGitPrimaryActionState,
  type SidebarGitAction,
  type SidebarGitState,
} from "../../shared/sidebar-git";

type ProjectEditorLoadStatus = "idle" | "opening" | "running" | "error";
type TitlebarMode = "agents" | "code" | "git" | "automate" | "tasks" | "manage";
type TitlebarDropdownPanelKind =
  | "actions"
  | "git"
  | "keepAwake"
  | "mode"
  | "openIn"
  | "resources"
  | "settings"
  | "tips";
type TitlebarDropdownPanelSize = {
  height: number;
  width: number;
};

type NativeProcessResult = {
  exitCode: number;
  requestId: string;
  stderr: string;
  stdout: string;
  type: "processResult";
};

type NativeHostEvent = NativeProcessResult | { protocolVersion: 1; type: "hostReady" };

type TitlebarOpenTargetsSettings = {
  availability: WorkspaceOpenTargetAvailability;
  customTargets: CustomWorkspaceOpenTarget[];
  hiddenTargetIds: string[];
};

/*
 * CDXC:TipsAndTricks 2026-06-16-19:42:
 * The Tips & Tricks header needs a Changelog action that opens the full Ghostex GitHub releases page as an in-project browser session, keeping release history inside the current workspace instead of the system browser.
 *
 * CDXC:TipsAndTricks 2026-06-18-04:53:
 * The tips panel header should not repeat the Tips & Tricks label in text. Expose Docs as a first-row action and keep documentation inside the current workspace browser session.
 *
 * CDXC:TipsAndTricks 2026-06-28-08:00:
 * Third-party skill recommendations from Tips should open as current-project
 * browser panes so users can inspect the setup detail without leaving Ghostex.
 */
const GHOSTEX_CHANGELOG_URL = "https://github.com/maddada/ghostex/releases";
const GHOSTEX_DOCS_URL = "https://ghostex.dev/docs";
const GHOSTEX_DISCORD_URL = "https://discord.gg/df7b3G92CS";
const FASTER_CHROME_DEVTOOLS_SKILL_URL = "https://github.com/zeke/faster-chrome-devtools-skill";
const TITLEBAR_GRADIENT_BLEND_START_PERCENT = 40;
const DEFAULT_CODE_SERVER_RESOURCE_PORT = 3775;
/*
 * CDXC:AutoUpdate 2026-06-30-15:51:
 * The titlebar update button tooltip must recommend updating and explicitly
 * reassure users that terminals and agents keep running while Ghostex restarts
 * for the update.
 */
const TITLEBAR_UPDATE_AVAILABLE_TOOLTIP =
  "Update to Latest (Recommended)\n\nNote: All your terminals & agents will keep running even while the app restarts to update";

function codeServerResourcePort(): number {
  const port = window.__ghostex_NATIVE_HOST__?.codeServerRuntime?.port;
  return typeof port === "number" && Number.isInteger(port) && port > 0
    ? port
    : DEFAULT_CODE_SERVER_RESOURCE_PORT;
}

type TitlebarSidebarActionsSettings = {
  commands: SidebarCommandButton[];
};

type TitlebarKeepAwakeSettings = {
  activateOnExternalDisplay: boolean;
  activateOnLaunch: boolean;
  allowDisplaySleep: boolean;
  batteryThresholdPercent: number;
  deactivateBelowBatteryThreshold: boolean;
  deactivateOnLowPowerMode: boolean;
  deactivateOnUserSwitch: boolean;
  defaultDurationMinutes: KeepAwakeDurationMinutes;
  delayedSendSessionCount: number;
  featureEnabled: boolean;
  hideTitlebarControl: boolean;
  preventLidSleep: boolean;
  whileWorkingSessions: boolean;
  workingSessionCount: number;
};

type TitlebarResourceGroup = {
  groupId: string;
  isActive: boolean;
  projectId?: string;
  projectName: string;
  projectPath: string;
  sessions: TitlebarResourceSession[];
  title: string;
};

type TitlebarResourceSession = {
  activity: "attention" | "idle" | "working";
  agentIcon?: string;
  delayedSendDeadlineAt?: string;
  delayedSendRemainingLabel?: string;
  delayedSendRemainingMs?: number;
  isLive?: boolean;
  isRunning: boolean;
  isSleeping?: boolean;
  lastInteractionAt?: string;
  nativePaneState?: "mounted" | "mounting" | "unmounted";
  providerSessionState?: "exists" | "missing" | "persistence-disabled" | "unknown";
  projectId?: string;
  sessionId: string;
  sessionKind?: "browser" | "terminal" | "t3";
  sessionPersistenceName?: string;
  sessionPersistenceProvider?: SessionPersistenceProvider;
  terminalTitle?: string;
  title: string;
};

type TitlebarTipIcon =
  | "browser"
  | "command"
  | "moon"
  | "resources"
  | "search"
  | "sidebar"
  | "warning";

type TitlebarTip = {
  action?: TitlebarTipAction;
  body: string;
  icon: TitlebarTipIcon;
  id: string;
  title: string;
};

type TitlebarTipAction =
  | {
      settingsSearchQuery: string;
      type: "openSettings";
    }
  | {
      type: "openBrowserPane";
      url: string;
    };

type TitlebarNotice = {
  action?: "openSettings";
  body: string;
  icon: TitlebarTipIcon;
  id: string;
  settingsTarget: "agentHooks" | "debuggingMode" | "ghostexCli" | "sessionPersistence";
  title: string;
};

type TitlebarBrowserTabResource = {
  browserId: number;
  id: string;
  isActive?: boolean;
  kind: "browser" | "code" | "git" | "tasks" | "manage" | string;
  projectId?: string;
  sessionId?: string;
  title: string;
  url?: string;
};

type TitlebarGxserverDaemonStatus = {
  alwaysStart: boolean;
  message?: string;
  nodePath?: string;
  nodeVersion?: string;
  ok?: boolean;
  pid?: number;
  startedAt?: string;
  state: string;
  version?: string;
};

type TitlebarProjectState = {
  activeMode: TitlebarMode;
  browserTabs: TitlebarBrowserTabResource[];
  codeEditorProjectIds: string[];
  agentHookStatus?: SidebarAgentHookStatusMessage;
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  portless?: SidebarPortlessState;
  debuggingMode: boolean;
  diagnosticLogging: DiagnosticLoggingSettings;
  showBetaFeatures: boolean;
  diffStats: SidebarProjectDiffStats;
  editorIsOpen: boolean;
  editorIsSleeping: boolean;
  editorStatus: ProjectEditorLoadStatus;
  git: SidebarGitState;
  gxserverDaemon: TitlebarGxserverDaemonStatus;
  keepAwake: TitlebarKeepAwakeSettings;
  projectEditorCompanionPaneHidden: boolean;
  projectIconDataUrl?: string | null;
  projectId?: string;
  projectIsQuick: boolean;
  projectName: string;
  projectPath: string;
  petOverlayEnabled: boolean;
  resourceGroups: TitlebarResourceGroup[];
  sidebarTheme: SidebarTheme;
  customSidebarTitlebarColorsEnabled: boolean;
  customSidebarTitlebarForegroundColor: string;
  customSidebarTitlebarBackgroundColor: string;
  sidebarCollapsed: boolean;
  sidebarSide: SidebarSide;
  sidebarActions: TitlebarSidebarActionsSettings;
  hotkeys: ghostexHotkeySettings;
  showProjectEditorDiffFileCount: boolean;
  sessionPersistenceProvider: SessionPersistenceProvider;
  terminalDevServerOpenTarget: TerminalDevServerOpenTarget;
  toggleSidebarHotkeyLabel: string;
  workspaceOpenTargets: TitlebarOpenTargetsSettings;
  isFocusModeActive?: boolean;
  promptEditorOpen?: boolean;
  updateAvailable: boolean;
  updateDownloadProgress: number | null;
  updateDownloading: boolean;
};

type ResourceProcess = {
  command: string;
  cpu: number;
  pid: number;
  ppid: number;
  rssMb: number;
};

type ResourceListeningServer = {
  commandName: string;
  cwd?: string;
  host: string;
  pid: number;
  port: number;
  url: string;
};

type ResourcePortlessServerPresentation = {
  hostname: string;
  isSetupActive: boolean;
  protocol: SidebarPortlessState["health"]["protocol"];
  setupAction?: NativePortlessAdminInstallAction;
  setupActionLabel: string;
  setupStatusLabel: string;
};

type ResourceProcessBundle = {
  childProcesses: ResourceProcess[];
  cpu: number;
  key: string;
  label: string;
  memoryMb: number;
  pids: number[];
  portless?: ResourcePortlessServerPresentation;
  projectEditorIds?: string[];
  process?: ResourceProcess;
  browserTab?: TitlebarBrowserTabResource;
  server?: ResourceListeningServer;
  session?: TitlebarResourceSession;
  type: "browser" | "code" | "orphan" | "server" | "session";
};

type ResourceProcessTotals = {
  cpu: number;
  memoryMb: number;
  processCount: number;
};

type ResourceGroupView = {
  bundles: ResourceProcessBundle[];
  group: TitlebarResourceGroup;
};

type NativeTitlebarCommand =
  | { details?: string; event: string; force?: boolean; type: "appendModeSwitcherDebugLog" }
  | { details?: string; event: string; type: "appendNativeChromeResponsivenessDebugLog" }
  | { details?: string; event: string; force?: boolean; type: "appendSessionTitleDebugLog" }
  | { details?: string; event: string; force?: boolean; type: "appendTerminalFocusDebugLog" }
  | {
      args: string[];
      cwd?: string;
      env?: Record<string, string>;
      executable: string;
      requestId: string;
      type: "runProcess";
    }
  | {
      enabled: boolean;
      installIfNeeded?: boolean;
      requestId: string;
      type: "setKeepAwakeLidSleepPrevention";
    }
  | {
      runtime?: KeepAwakeRuntimeState | null;
      suppressAutoStart: boolean;
      type: "syncTitlebarKeepAwakeRuntime";
    }
  | { type: "openActiveProjectEditorFromTitlebar" }
  | { type: "toggleProjectEditorCompanionFromTitlebar" }
  | { type: "exitFocusModeFromTitlebar" }
  | { type: "bringPromptEditorToFrontFromTitlebar" }
  | { type: "openAgentsModeFromTitlebar" }
  | { type: "openGitHubProjectFromTitlebar" }
  | { type: "openAutomateFromTitlebar" }
  | { type: "openTasksPlaceholderFromTitlebar" }
  | { type: "openManageFromTitlebar" }
  | { type: "refreshWorkspaceOpenTargetAvailabilityFromTitlebar" }
  | { type: "toggleCommandsPanelFromTitlebar" }
  | { type: "togglePetOverlayFromTitlebar" }
  | { type: "toggleSidebarCollapsed" }
  | { type: "showUpdateDialogFromTitlebar" }
  | { type: "startGxserverFromTitlebar" }
  | { type: "stopGxserverFromTitlebar" }
  | { type: "restartGxserverFromTitlebar" }
  | { enabled: boolean; type: "setGxserverAlwaysStartFromTitlebar" }
  | { sessionId: string; type: "focusResourceSessionFromTitlebar" }
  | { sessionIds: string[]; type: "sleepInactiveSessionsFromTitlebar" }
  | { projectIds: string[]; sessionIds: string[]; type: "quitResourcesFromTitlebar" }
  | { commandId: string; type: "runSidebarCommandFromTitlebar" }
  | { action: SidebarGitAction; type: "runSidebarGitActionFromTitlebar" }
  | { type: "openExternalUrl"; url: string }
  | {
      anchorRect: { height: number; width: number; x: number; y: number };
      kind: TitlebarDropdownPanelKind;
      preferredSize: TitlebarDropdownPanelSize;
      type: "showTitlebarDropdownPanel";
    }
  | { type: "closeTitlebarDropdownPanel" }
  | { type: "titlebarBlankMouseDown" }
  | { kind: TitlebarDropdownPanelKind; type: "titlebarDropdownPanelReady" }
  | {
      height: number;
      kind: TitlebarDropdownPanelKind;
      type: "resizeTitlebarDropdownPanel";
      width: number;
    }
  | {
      targetApp: WorkspaceIdeTargetApp;
      type: "openWorkspaceInIde";
      workspacePath: string;
    }
  | { type: "openWorkspaceInFinder"; workspacePath: string }
  | {
      overlayOpen: boolean;
      type: "setReactTitlebarStripState";
    };

type ResolvedOpenTarget =
  | {
      definition: WorkspaceOpenTargetDefinition;
      id: string;
      kind: "built-in";
      label: string;
      resolvedAppName?: string;
      resolvedCommand?: string;
    }
  | {
      command: string;
      custom: CustomWorkspaceOpenTarget;
      id: string;
      kind: "custom";
      label: string;
      resolvedCommand?: string;
    };

declare global {
  interface Window {
    __ghostex_PENDING_TITLEBAR_UPDATE_AVAILABLE__?: boolean;
    __ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOAD_PROGRESS__?: number | null;
    __ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOADING__?: boolean;
    __ghostex_PENDING_TITLEBAR_WINDOW_FOCUSED__?: boolean;
    __ghostex_TITLEBAR_PANEL_KIND__?: string;
    __ghostex_PENDING_TITLEBAR_PROJECT_STATE__?: Partial<TitlebarProjectState>;
    __ghostex_TITLEBAR__?: {
      closeOpenDropdowns: () => void;
      setActiveProjectState: (state: Partial<TitlebarProjectState>) => void;
      setLastActionCommandId: (commandId: string) => void;
      setNativeDropdownOpen: (kind: TitlebarDropdownPanelKind | undefined) => void;
      setNativePointerInside: (isInside: boolean) => void;
      setWindowFocused: (isFocused: boolean) => void;
      runKeepAwakeCommand?: (command: TitlebarKeepAwakeCommand) => void;
      syncKeepAwakeRuntime: (syncState: KeepAwakeRuntimeSyncState) => void;
    };
  }
}

const LAST_OPEN_TARGET_STORAGE_KEY = "ghostex.titlebar.lastOpenTargetId";
const LAST_ACTION_COMMAND_STORAGE_PREFIX = "ghostex.titlebar.lastActionCommandByProject:";
const KEEP_AWAKE_RUNTIME_STORAGE_KEY = "ghostex.titlebar.keepAwakeRuntime";
const KEEP_AWAKE_RUNTIME_SYNC_STORAGE_KEY = "ghostex.titlebar.keepAwakeRuntimeSync";
const KEEP_AWAKE_RUNTIME_CHANGED_EVENT = "ghostex:titlebar-keep-awake-runtime-changed";
const KEEP_AWAKE_LID_SLEEP_STORAGE_KEY = "ghostex.titlebar.lidSleepPrevention";
const TITLEBAR_GIT_STATE_CACHE_STORAGE_PREFIX = "ghostex.titlebar.gitState.";
const TITLEBAR_TIPS_READ_STORAGE_KEY = "ghostex.titlebar.tips.readIds";
const KEEP_AWAKE_POWER_CHECK_INTERVAL_MS = 30_000;
const KEEP_AWAKE_WORKING_SESSION_GRACE_MS = 20 * 60_000;
const KEEP_AWAKE_ADMIN_PROCESS_TIMEOUT_MS = 120_000;
/**
 * CDXC:NativeWindowChrome 2026-05-25-07:16:
 * The macOS app titlebar should now be 35px tall, not the earlier 45px. Keep the React titlebar height in sync with Swift's native reservation so web controls and AppKit traffic-light centering share one chrome height.
 */
const TITLEBAR_HEIGHT = 35;
const TITLEBAR_CONTROL_HEIGHT = TITLEBAR_HEIGHT - 1;
/**
 * CDXC:ProjectEditorCompanion 2026-06-12-03:18:
 * Companion pane collapse/expand is one titlebar toggle immediately left of
 * Agents. Keep both state glyphs at the same footprint so the control
 * reads as part of the mode switcher rather than separate floating chrome.
 *
 * CDXC:ProjectEditorCompanion 2026-06-12-04:23:
 * The toggle icon needs a larger 17x17 footprint after visual review so the
 * anchored companion control has the same presence as the adjacent text tabs.
 */
const COMPANION_SIDEPANE_ICON_SIZE = 17;
/**
 * CDXC:NativeWindowChrome 2026-06-17-18:25:
 * The traffic-light-side titlebar cluster should sit 2px higher after visual
 * review. Keep the left project slot offset named so the sidebar toggle,
 * project identity, and adjacent left-cluster controls move together without
 * changing the 35px titlebar reservation.
 */
const TITLEBAR_PROJECT_CLUSTER_TOP = -1;
const TITLEBAR_CONTROL_TOP = 1;
const TITLEBAR_PROJECT_TOP = TITLEBAR_PROJECT_CLUSTER_TOP;
const TITLEBAR_CENTER_CONTROLS_TOP = TITLEBAR_CONTROL_TOP;
const TITLEBAR_RIGHT_CONTROLS_TOP = TITLEBAR_CONTROL_TOP;
const RESOURCE_POLL_INTERVAL_MS = 5_000;
const TITLEBAR_EVENT_LOOP_WATCHDOG_INTERVAL_MS = 2_000;
const TITLEBAR_EVENT_LOOP_STALL_THRESHOLD_MS = 1_000;
const TITLEBAR_EVENT_LOOP_STALL_LOG_THROTTLE_MS = 10_000;
/**
 * CDXC:ReactTitlebar 2026-06-11-13:22:
 * The titlebar document uses native child-window dropdown panels instead of
 * Radix portals in the full-window WKWebView, so the workspace never sits under
 * a titlebar-owned overlay during editor drag/drop.
 *
 * CDXC:ReactTitlebar 2026-06-11-15:58:
 * Native titlebar dropdown panels must load the real titlebar-host.html file URL
 * without query parameters. Swift injects the panel kind at document start so
 * WebKit does not treat a synthetic local-file URL as the document resource.
 */
const TITLEBAR_PANEL_QUERY_PARAM = "ghostexTitlebarPanel";
const TITLEBAR_DROPDOWN_COMPACT_PANEL_WIDTH = 240;
const TITLEBAR_DROPDOWN_RESOURCES_PANEL_WIDTH = 656;
/**
 * CDXC:TipsAndTricks 2026-06-12-08:56:
 * The macOS Tips & Tricks child panel should be 100px narrower than the shared
 * Resources reading panel while preserving the always-expanded section layout.
 */
const TITLEBAR_DROPDOWN_TIPS_PANEL_WIDTH = 556;
const TITLEBAR_DROPDOWN_READING_PANEL_HEIGHT = 650;
const TITLEBAR_DROPDOWN_MENU_CHROME_HEIGHT = 10;
const TITLEBAR_DROPDOWN_MENU_LABEL_HEIGHT = 22;
const TITLEBAR_DROPDOWN_MENU_ITEM_HEIGHT = 30;
const TITLEBAR_DROPDOWN_ACTION_ITEM_HEIGHT = 44;
const TITLEBAR_DROPDOWN_SEPARATOR_HEIGHT = 9;
const TITLEBAR_DROPDOWN_EMPTY_ROW_HEIGHT = 30;

function readTitlebarDropdownPanelKind(): TitlebarDropdownPanelKind | undefined {
  const injectedKind =
    typeof window.__ghostex_TITLEBAR_PANEL_KIND__ === "string"
      ? window.__ghostex_TITLEBAR_PANEL_KIND__
      : undefined;
  const rawKind = injectedKind ?? new URLSearchParams(window.location.search).get(TITLEBAR_PANEL_QUERY_PARAM);
  if (
    rawKind === "actions" ||
    rawKind === "git" ||
    rawKind === "keepAwake" ||
    rawKind === "mode" ||
    rawKind === "openIn" ||
    rawKind === "resources" ||
    rawKind === "settings" ||
    rawKind === "tips"
  ) {
    return rawKind;
  }
  return undefined;
}

function compactTitlebarDropdownPanelSize(height: number): TitlebarDropdownPanelSize {
  return {
    height: Math.ceil(height),
    width: TITLEBAR_DROPDOWN_COMPACT_PANEL_WIDTH,
  };
}

function titlebarMenuHeight(rowCount: number, options: {
  rowHeight?: number;
  separatorCount?: number;
} = {}): number {
  return TITLEBAR_DROPDOWN_MENU_CHROME_HEIGHT +
    Math.max(0, rowCount) * (options.rowHeight ?? TITLEBAR_DROPDOWN_MENU_ITEM_HEIGHT) +
    Math.max(0, options.separatorCount ?? 0) * TITLEBAR_DROPDOWN_SEPARATOR_HEIGHT;
}

function createTitlebarDropdownPanelPreferredSize(
  kind: TitlebarDropdownPanelKind,
  counts: {
    actionCount: number;
    gitItemCount: number;
    keepAwakeIsRunning: boolean;
    modeOptionCount: number;
    targetCount: number;
  },
): TitlebarDropdownPanelSize {
  /*
   * CDXC:ReactTitlebar 2026-06-12-02:50:
   * Compact native titlebar dropdown panels must be sized from the number and
   * type of rendered options before AppKit creates the child window. This keeps
   * short menus from clipping rows below the fold without reintroducing
   * post-open WebKit measurement feedback.
   */
  switch (kind) {
    case "resources":
      return {
        height: TITLEBAR_DROPDOWN_READING_PANEL_HEIGHT,
        width: TITLEBAR_DROPDOWN_RESOURCES_PANEL_WIDTH,
      };
    case "tips":
      return {
        height: TITLEBAR_DROPDOWN_READING_PANEL_HEIGHT,
        width: TITLEBAR_DROPDOWN_TIPS_PANEL_WIDTH,
      };
    case "actions": {
      const actionRows = Math.max(0, counts.actionCount);
      const actionRowsHeight = actionRows > 0
        ? actionRows * TITLEBAR_DROPDOWN_ACTION_ITEM_HEIGHT
        : TITLEBAR_DROPDOWN_EMPTY_ROW_HEIGHT;
      return compactTitlebarDropdownPanelSize(
        TITLEBAR_DROPDOWN_MENU_CHROME_HEIGHT +
          actionRowsHeight +
          TITLEBAR_DROPDOWN_SEPARATOR_HEIGHT +
          TITLEBAR_DROPDOWN_MENU_ITEM_HEIGHT,
      );
    }
    case "git":
      /*
       * CDXC:TitlebarGit 2026-06-16-15:15:
       * The Git dropdown separates repository status from runnable commands
       * with Status and Actions section labels. Include those fixed label rows
       * in the child-window height so the native dropdown does not clip actions.
       */
      return compactTitlebarDropdownPanelSize(
        titlebarMenuHeight(Math.max(1, counts.gitItemCount) + 3, { separatorCount: 1 }) +
          TITLEBAR_DROPDOWN_MENU_LABEL_HEIGHT * 2,
      );
    case "keepAwake":
      return compactTitlebarDropdownPanelSize(
        titlebarMenuHeight(
          KEEP_AWAKE_DURATION_OPTIONS.length + (counts.keepAwakeIsRunning ? 1 : 0) + 1,
          { separatorCount: 1 },
        ) + TITLEBAR_DROPDOWN_MENU_LABEL_HEIGHT,
      );
    case "mode":
      return compactTitlebarDropdownPanelSize(
        titlebarMenuHeight(Math.max(1, counts.modeOptionCount)),
      );
    case "openIn":
      return compactTitlebarDropdownPanelSize(
        titlebarMenuHeight(Math.max(0, counts.targetCount) + 1, { separatorCount: 1 }),
      );
    case "settings":
      return compactTitlebarDropdownPanelSize(
        titlebarMenuHeight(5, { separatorCount: 2 }),
      );
  }
}

/**
 * CDXC:ReactTitlebar 2026-06-11-17:16:
 * Native dropdown child windows reuse this titlebar bundle, but their document
 * must avoid inheriting the normal full-width titlebar viewport. Read the panel
 * kind once before React mounts so document, body, and root sizing can be set
 * before WebKit lays out content.
 *
 * CDXC:ReactTitlebar 2026-06-11-17:27:
 * Dynamic measurement still allowed WebKit/AppKit feedback to shrink panels
 * after opening. Native titlebar dropdowns now use fixed child-window sizes, so
 * panel documents fill the WebView and dropdown content scrolls internally.
 */
const initialTitlebarDropdownPanelKind = readTitlebarDropdownPanelKind();

/**
 * CDXC:TipsAndTricks 2026-05-30-08:31:
 * Tips are authored in code, not by end users in the dropdown. Keep this array
 * as the ordered source of truth so adding, removing, or reordering tips is a
 * normal code edit while read state survives app updates by stable tip id.
 *
 * CDXC:TipsAndTricks 2026-06-05-12:39:
 * The dropdown should teach users early that the sidebar is highly customizable.
 * Keep this as the second built-in tip so it appears immediately after the command-palette hint for users who have not marked it read.
 *
 * CDXC:TipsAndTricks 2026-06-13-10:26:
 * The first tip should introduce Cmd Shift P as the universal entry point for app actions, not only pane moves.
 *
 * CDXC:TipsAndTricks 2026-06-28-08:00:
 * Tips should actively teach the agent-facing Browser Use, Computer Use,
 * Generate Title, and personal Chrome DevTools skills. Ghostex-owned skills
 * deep-link to Settings > Integrations with the relevant row searched; the
 * external Chrome skill opens its repository in a project browser pane.
 */
const TITLEBAR_TIPS: TitlebarTip[] = [
  {
    body: "Search for project actions, pane splits and moves, session controls, settings shortcuts, and other Ghostex actions.",
    icon: "command",
    id: "command-palette-all-actions",
    title: "Press Cmd Shift P anywhere to open Ghostex Quick Access",
  },
  {
    body: "Open Settings to customize sidebar presets, visible details, agents, actions, project tools, and workspace open targets.",
    icon: "sidebar",
    id: "customize-sidebar-layout-and-tools",
    title: "Customize the sidebar",
  },
  {
    body: "The Resources menu can sleep inactive terminal sessions while keeping them restorable in the sidebar.",
    icon: "moon",
    id: "sleep-idle-sessions-from-resources",
    title: "Sleep idle sessions from Resources",
  },
  {
    body: "Use browser panes beside agents when the task needs screenshots, DOM inspection, or logged-in product state.",
    icon: "browser",
    id: "attach-browser-pane-to-task",
    title: "Attach a browser pane to a task",
  },
  {
    action: {
      settingsSearchQuery: "Ghostex Computer Use",
      type: "openSettings",
    },
    body: "Configure Ghostex Computer Use in Settings, then ask agents to use /ghostex-computer-use for native macOS app control.",
    icon: "resources",
    id: "use-ghostex-computer-use-skill",
    title: "Use /ghostex-computer-use for desktop control",
  },
  {
    action: {
      settingsSearchQuery: "Ghostex Browser Use",
      type: "openSettings",
    },
    body: "Configure Ghostex Browser Use in Settings, then ask agents to use /ghostex-browser-use for page inspection, console logs, screenshots, and clicks.",
    icon: "browser",
    id: "use-ghostex-browser-use-skill",
    title: "Use /ghostex-browser-use for browser panes",
  },
  {
    action: {
      settingsSearchQuery: "Ghostex Auto Rename Session",
      type: "openSettings",
    },
    body: "Configure Ghostex Auto Rename Session in Settings, then ask agents to use $ghostex-auto-rename-session to auto rename the current session from the work they just did.",
    icon: "command",
    id: "use-ghostex-auto-rename-session-skill",
    title: "Use $ghostex-auto-rename-session to auto rename sessions",
  },
  {
    action: {
      type: "openBrowserPane",
      url: FASTER_CHROME_DEVTOOLS_SKILL_URL,
    },
    body: "Install Faster Chrome DevTools Skill when agents need fast CLI-backed access to your own Chrome profile, tabs, cookies, and extensions.",
    icon: "command",
    id: "recommend-faster-chrome-devtools-skill",
    title: "Give agents fast access to your personal Chrome",
  },
  {
    body: 'Open the sidebar Search row, click "Search by Text", then type any words you remember from the prompt.',
    icon: "search",
    id: "find-session-by-prompt-text",
    title: "Find any session from prompt text",
  },
  {
    body: "Pin a session in the sidebar when you need it to stay at the top.",
    icon: "resources",
    id: "pin-important-workspaces",
    title: "Pin important sessions",
  },
  {
    body: "Then you can easily ask agents to \"work on beads with   high priority from the kanban board\"",
    icon: "command",
    id: "add-todos-to-kanban-page",
    title: "Add all your Todos in the Kanban page",
  },
];

/**
 * CDXC:SessionPersistence 2026-06-04-01:57:
 * When Session Persistence is Off, Android and iOS attach can reconnect to the
 * macOS native terminal instead of a durable zmx/tmux/zellij session. Surface
 * this as a non-dismissable Tips & Tricks notice, not a normal read tip, so it
 * stays visible until persistence is enabled again.
 */
const TITLEBAR_PERSISTENCE_OFF_NOTICE: TitlebarNotice = {
  body: "Android and iOS attach can have issues while Session Persistence is Off. Enable zmx persistence so mobile clients reconnect to durable terminal sessions.",
  icon: "warning",
  id: "session-persistence-off-mobile-attach",
  settingsTarget: "sessionPersistence",
  title: "Mobile attach needs persistence",
};

/**
 * CDXC:DiagnosticsSettings 2026-06-06-07:09:
 * Debugging Mode previously wrote detailed diagnostics to disk and could affect
 * performance.
 *
 * CDXC:DiagnosticsSettings 2026-06-27-22:07:
 * Debugging Mode now exposes debug UI only. Keep the notice, but point users to
 * scenario-specific disk logging so turning on debug controls does not imply
 * every routine support log is active.
 */
const TITLEBAR_DEBUGGING_MODE_NOTICE: TitlebarNotice = {
  body: "Ghostex is showing debug UI controls. Routine disk logging is controlled by Diagnostic disk logging scenarios in Settings.",
  icon: "warning",
  id: "debugging-mode-enabled",
  settingsTarget: "debuggingMode",
  title: "Debug mode is on",
};

function createTitlebarGhostexCliNotice(
  ghostexCliStatus: SidebarGhostexCliStatusMessage | undefined,
): TitlebarNotice | undefined {
  /**
   * CDXC:CliInstall 2026-06-07-15:26:
   * Tips & Tricks should warn when either public CLI command is not accessible
   * on PATH. Keep the description to three lines or less while naming concrete
   * benefits: terminal commands, mobile attach, and agent integration skills.
   */
  if (
    !ghostexCliStatus ||
    (ghostexCliStatus.installed === true && ghostexCliStatus.gxUsable === true)
  ) {
    return undefined;
  }
  return {
    body: "Install or repair the CLI to use ghostex/gx in any terminal, attach mobile clients, and install Browser/Computer/Orchestration agent skills.",
    icon: "warning",
    id: "ghostex-cli-not-accessible",
    settingsTarget: "ghostexCli",
    title: "Ghostex CLI is not accessible",
  };
}

function createTitlebarMissingAgentHooksNotice(
  resourceGroups: TitlebarResourceGroup[],
  agentHookStatus: SidebarAgentHookStatusMessage | undefined,
): TitlebarNotice | undefined {
  if (!agentHookStatus || agentHookStatus.errorMessage) {
    return undefined;
  }
  const hookStatusByAgentId = new Map(
    agentHookStatus.agents.map((status) => [status.agentId, status]),
  );
  const liveSupportedAgentIds = new Set<string>();
  for (const group of resourceGroups) {
    for (const session of group.sessions) {
      if (!isTitlebarLiveTerminalAgentSession(session)) {
        continue;
      }
      const agent = getDefaultSidebarAgentByIcon(session.agentIcon as SidebarAgentIcon | undefined);
      if (!agent || agent.agentId === "t3") {
        continue;
      }
      liveSupportedAgentIds.add(agent.agentId);
    }
  }
  const missingAgents = new Map<string, string>();
  const outdatedAgents = new Map<string, string>();
  for (const status of agentHookStatus.agents) {
    if (!status.cliInstalled || status.status === "installed" || status.status === "notRequired" || status.status === "cliMissing") {
      continue;
    }
    const agent = getDefaultSidebarAgentById(status.agentId);
    if (!agent || agent.agentId === "t3") {
      continue;
    }
    if (status.status === "updateRequired") {
      outdatedAgents.set(agent.agentId, agent.name);
    } else {
      missingAgents.set(agent.agentId, agent.name);
    }
  }
  const prioritizedAgentNames = [
    ...[...outdatedAgents].filter(([agentId]) => liveSupportedAgentIds.has(agentId)).map(([, name]) => name),
    ...[...missingAgents].filter(([agentId]) => liveSupportedAgentIds.has(agentId)).map(([, name]) => name),
    ...[...outdatedAgents].filter(([agentId]) => !liveSupportedAgentIds.has(agentId)).map(([, name]) => name),
    ...[...missingAgents].filter(([agentId]) => !liveSupportedAgentIds.has(agentId)).map(([, name]) => name),
  ];
  const agentNames = prioritizedAgentNames;
  if (agentNames.length === 0) {
    return undefined;
  }

  /**
   * CDXC:AgentHookSettings 2026-06-07-08:51:
   * Installed agent CLIs without current Ghostex hooks should surface in Tips
   * & Tricks as non-dismissable runtime notices. Hooks power reliable session
   * status, first-message naming, and sleep/resume identity; read-once tips are
   * the wrong model while the machine still has missing or stale hook setup.
   *
   * CDXC:AgentHooks 2026-06-07-11:05:
   * gxserver now distinguishes old Ghostex hooks from absent hooks. The
   * titlebar notice should ask users to update old hooks instead of saying they
   * are not installed, because the reliable fix is migration to the current
   * gxserver ingest hook rather than accepting stale native-era artifacts.
   *
   * CDXC:AgentHooks 2026-06-18-03:08:
   * The titlebar Tips dropdown must warn even before a live agent session is
   * running when installed CLIs are missing hooks. Copy should explicitly name
   * auto session naming, session status, and sleep/resume reliability.
   */
  const formattedAgents = formatTitlebarNoticeNameList(agentNames);
  const hasOutdatedHooks = outdatedAgents.size > 0;
  const hasMissingHooks = missingAgents.size > 0;
  const action = hasOutdatedHooks && hasMissingHooks ? "setup" : hasOutdatedHooks ? "update" : "install";
  const actionLabel = action === "setup" ? "install or update" : action;
  const actionVerb = action === "setup" ? "set up" : action === "update" ? "updated" : "installed";
  return {
    action: "openSettings",
    body: `Open Settings > Integrations to ${actionLabel} agent hooks for ${formattedAgents}. Automatic session renaming, In Progress/Needs Attention status, and sleeping or resuming agent sessions will not work correctly until hooks are ${action === "setup" ? "installed or updated" : actionVerb}.`,
    icon: "warning",
    id: `agent-hooks-${action}-${[...outdatedAgents.keys(), ...missingAgents.keys()].sort().join("-")}`,
    settingsTarget: "agentHooks",
    title: "Warning: Agent hooks aren't installed for agent CLIs",
  };
}

function isTitlebarLiveTerminalAgentSession(session: TitlebarResourceSession): boolean {
  return (
    session.sessionKind === "terminal" &&
    session.isRunning === true &&
    session.isSleeping !== true &&
    Boolean(session.agentIcon)
  );
}

function formatTitlebarNoticeNameList(names: string[]): string {
  if (names.length <= 1) {
    return names[0] ?? "";
  }
  if (names.length === 2) {
    return `${names[0]} and ${names[1]}`;
  }
  return `${names.slice(0, -1).join(", ")}, and ${names[names.length - 1]}`;
}

type KeepAwakeRuntimeState = {
  durationMinutes: KeepAwakeDurationMinutes;
  fireAtMs?: number;
  pid: number;
  source: "automatic" | "manual";
  startedAtMs: number;
};

type KeepAwakeRuntimeSyncState = {
  runtime?: KeepAwakeRuntimeState | null;
  suppressAutoStart: boolean;
};

type TitlebarKeepAwakeCommand =
  | { action: "start"; durationMinutes?: KeepAwakeDurationMinutes }
  | { action: "stop" };

const pendingProcessResults = new Map<
  string,
  {
    reject: (error: Error) => void;
    resolve: (result: NativeProcessResult) => void;
    timeout: number;
  }
>();

function postNative(command: NativeTitlebarCommand): void {
  window.webkit?.messageHandlers?.ghostexNativeHost?.postMessage(command);
}

function setTitlebarNativePointerInside(isInside: boolean): void {
  /*
   * CDXC:ReactTitlebar 2026-06-10-23:44:
   * AppKit owns the effective titlebar hit boundary because the WKWebView spans
   * the window for portals. Store native pointer ownership on the body for
   * bridge state only; this flag must not own titlebar hover visibility.
   *
   * CDXC:TooltipLifecycle 2026-06-13-02:30:
   * Do not use this flag as a titlebar tooltip or hover gate. AppKit can leave
   * the flag false until a titlebar click updates strip ownership, so titlebar
   * tooltips must rely on normal CSS hover and local tooltip state instead.
   */
  document.body.dataset.nativePointerInside = isInside ? "true" : "false";
}

function setTitlebarWindowFocused(isFocused: boolean): void {
  /*
   * CDXC:ReactTitlebar 2026-06-20-17:10:
   * AppKit owns the key-window state for titlebar chrome. React keeps the
   * existing body dataset bridge so titlebar CSS can track active/inactive
   * windows without adding new native hit-test or routing behavior.
   */
  window.__ghostex_PENDING_TITLEBAR_WINDOW_FOCUSED__ = isFocused;
  document.body.dataset.windowFocused = isFocused ? "true" : "false";
}

function suppressTitlebarTooltipsFromDom(): void {
  setTitlebarNativePointerInside(false);
}

function enableTitlebarTooltipsFromDom(): void {
  setTitlebarNativePointerInside(true);
}

/*
 * CDXC:TitlebarTooltips 2026-06-15-13:34:
 * Titlebar hover labels should close when the pointer leaves the trigger.
 * Hovering the floating label itself must not keep it open, so titlebar-owned
 * AppTooltip roots disable Base UI's hoverable popup behavior instead of adding
 * native hit-test routing or invisible hover surfaces.
 */
const TITLEBAR_TOOLTIP_ROOT_PROPS = {
  disableHoverablePopup: true,
} as const;

/*
 * CDXC:TitlebarTooltips 2026-06-15-16:40:
 * macOS titlebar button hover labels should sit higher than the default
 * centered side placement. Use Base UI's alignment offset so the popup
 * positioner owns the nudge and tooltip animations keep their normal transform.
 *
 * CDXC:TitlebarTooltips 2026-06-15-23:25:
 * Shift titlebar hover labels up another 2px so they clear the titlebar button
 * row with the tighter tooltip height.
 */
const TITLEBAR_TOOLTIP_ALIGN_OFFSET_PX = -4;

function TitlebarAppTooltip({
  alignOffset = TITLEBAR_TOOLTIP_ALIGN_OFFSET_PX,
  children,
  content,
  side = "left",
  sideOffset = 7,
}: {
  alignOffset?: number;
  children: ReactElement;
  content: ReactNode;
  side?: "bottom" | "left" | "right" | "top";
  sideOffset?: number;
}) {
  if (content === undefined || content === null || content === "") {
    return children;
  }
  /*
   * CDXC:TitlebarTooltips 2026-06-13-02:59:
   * Titlebar hover labels must use the same AppTooltip wrapper as sidebar
   * controls. Keep a titlebar-local wrapper only for placement/styling so the
   * titlebar does not reintroduce data-tooltip pseudo-elements.
   */
  return (
    <AppTooltip
      {...TITLEBAR_TOOLTIP_ROOT_PROPS}
      alignOffset={alignOffset}
      content={content}
      contentClassName="titlebar-app-tooltip"
      side={side}
      sideOffset={sideOffset}
    >
      {children}
    </AppTooltip>
  );
}

function TitlebarUpdateProgressRing({ progress }: { progress: number | null }) {
  const normalizedProgress = normalizeTitlebarUpdateDownloadProgress(progress);
  const progressOffset = normalizedProgress === null ? 1 : 1 - normalizedProgress;
  /*
   * CDXC:AutoUpdate 2026-06-30-22:18:
   * The active update button should show a circular fill instead of a spinner.
   * Use the real Sparkle progress ratio when native provides one; keep the
   * unknown-size state visually distinct without inventing a fake percent.
   */
  return (
    <span
      aria-hidden="true"
      className="titlebar-update-progress-ring"
      data-progress-known={normalizedProgress === null ? "false" : "true"}
      style={{ "--titlebar-update-progress-offset": progressOffset } as CSSProperties}
    >
      <svg focusable="false" viewBox="0 0 16 16">
        <circle
          className="titlebar-update-progress-track"
          cx="8"
          cy="8"
          pathLength={1}
          r="5.5"
        />
        <circle
          className="titlebar-update-progress-fill"
          cx="8"
          cy="8"
          pathLength={1}
          r="5.5"
        />
      </svg>
    </span>
  );
}

function normalizeTitlebarUpdateDownloadProgress(value: unknown): number | null {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return null;
  }
  return Math.min(Math.max(value, 0), 1);
}

function formatTitlebarUpdateDownloadPercent(progress: number | null): string | undefined {
  const normalizedProgress = normalizeTitlebarUpdateDownloadProgress(progress);
  if (normalizedProgress === null) {
    return undefined;
  }
  return `${Math.min(Math.max(Math.round(normalizedProgress * 100), 0), 100)}%`;
}

function formatTitlebarUpdateDownloadingTooltip(progress: number | null): string {
  const percent = formatTitlebarUpdateDownloadPercent(progress);
  /*
   * CDXC:AutoUpdate 2026-06-30-22:18:
   * While Sparkle downloads an accepted update, the titlebar hover label should
   * show the current percent whenever native has enough information to compute
   * it. Keep the no-percent label only for the unknown-length startup window.
   */
  return percent ? `Downloading... ${percent}` : "Downloading...";
}

function formatTitlebarUpdateDownloadingAriaLabel(progress: number | null): string {
  const percent = formatTitlebarUpdateDownloadPercent(progress);
  return percent ? `Downloading update ${percent}` : "Downloading update";
}

function postTitlebarSidebarCommand(
  message:
    | { type: "openBrowserPane"; url: string }
    | { type: "openGhostexTutorialVideo" }
    | { type: "openWorkspaceWelcome" }
    | { type: "requestAgentHookStatus" }
    | { type: "requestGhostexCliStatus" }
    | { type: "refreshDaemonSessions" }
    | { type: "refreshGitState" }
    | {
        action: NativePortlessAdminInstallAction;
        protocol: SidebarPortlessState["health"]["protocol"];
        requestId: string;
        type: "runPortlessSettingsAdminAction";
      },
): void {
  /*
  CDXC:AgentHooks 2026-06-07-11:05:
  Opening Tips & Tricks should refresh gxserver hook status instead of relying
  on the titlebar's cached layout snapshot. Route through the existing
  app-modal sidebarCommand bridge so the native sidebar remains the owner of
  authenticated gxserver requests and hook-status state publication.

  CDXC:CliInstall 2026-06-07-15:26:
  Tips & Tricks CLI notices must use the native sidebar's real PATH inspection
  instead of probing from the isolated titlebar webview.

  CDXC:TipsAndTricks 2026-06-16-08:17:
  Tips & Tricks header actions should launch Features and setup
  flow through the sidebar command bridge because the native sidebar owns app
  modal presentation.

  CDXC:GhostexTutorialVideo 2026-06-18-05:31:
  The Features action now opens the tutorial video modal through the sidebar
  command bridge, leaving the old Highlighted Features modal unused.

  CDXC:TipsAndTricks 2026-06-16-19:42:
  The Changelog header action should reuse the sidebar browser-pane command so
  the releases page opens in the current project as a new browser session.

  CDXC:TitlebarGit 2026-06-16-18:41:
  Opening the titlebar Git menu should request fresh Git stats through the
  sidebar-owned bridge before showing the dropdown, including right-click opens.
  */
  window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
    message,
    type: "sidebarCommand",
  });
}

function closeAppModalFromTitlebarNavigation(area: string): void {
  /*
   * CDXC:SettingsDismissal 2026-06-15-14:07:
   * Titlebar mode switches and titlebar action runners should dismiss the
   * workspace-scoped Settings child window before they change workarea state or
   * run commands. Send the normal app-modal close message through the native
   * bridge so Settings, if open, closes without adding titlebar-specific state.
   */
  window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({ area, type: "close" });
}

function appendTitlebarActionCrashDebugLog(
  diagnosticLogging: DiagnosticLoggingSettings,
  event: string,
  details?: unknown,
): void {
  /**
   * CDXC:TitlebarActions 2026-05-15-17:23:
   * Terminal action button crashes need a breadcrumb from the isolated React
   * titlebar before the native-sidebar command runner receives the click.
   * Persist this trace outside the normal debug-toggle filter so a repro that
   * exits the app still leaves the selected action id and project context.
   *
   * CDXC:GxserverLogs 2026-06-15-20:39:
   * actionCrashTrace is a breadcrumb namespace, not severity. Keep the first
   * titlebar hop only while the native.terminal.focus scenario is enabled so
   * routine action clicks do not persist as normal-mode crash warnings.
   */
  if (!isDiagnosticLoggingScenarioEnabled(diagnosticLogging, "native.terminal.focus")) {
    return;
  }
  postNative({
    details: details === undefined ? undefined : JSON.stringify(details),
    event,
    type: "appendTerminalFocusDebugLog",
  });
}

function appendTitlebarModeSwitchDebugLog(
  diagnosticLogging: DiagnosticLoggingSettings,
  event: string,
  details: Record<string, unknown> = {},
): void {
  /**
   * CDXC:ModeSwitcherDiagnostics 2026-06-15-00:21:
   * Agents, Source, Browser, Kanban, and Manage titlebar clicks need the same first-hop
   * timing breadcrumbs. Send only enum-like mode state, booleans, safe ids,
   * and monotonic timestamps while the native.mode.switcher scenario is enabled; never
   * include project names, paths, URLs, titles, commands, or user text.
   *
   * CDXC:DiagnosticsSettings 2026-06-27-22:07:
   * First-hop titlebar mode-switch breadcrumbs must follow the same exact
   * scenario allowlist as the native writer so Debugging Mode can show debug UI
   * without enabling routine persistent logs.
   */
  if (!isDiagnosticLoggingScenarioEnabled(diagnosticLogging, "native.mode.switcher")) {
    return;
  }
  postNative({
    details: JSON.stringify({
      ...details,
      performanceNowMs: performance.now(),
      source: "titlebar",
      wallTimeMs: Date.now(),
    }),
    event,
    type: "appendModeSwitcherDebugLog",
  });
}

function appendTitlebarChromeResponsivenessDebugLog(
  diagnosticLogging: DiagnosticLoggingSettings,
  event: string,
  details: Record<string, unknown> = {},
): void {
  /*
   * CDXC:ChromeResponsivenessDiagnostics 2026-06-30-23:52:
   * Heavy lag and blank chrome repros need first-hop titlebar timing from the
   * isolated React titlebar. Gate routine breadcrumbs behind the targeted
   * native.chrome.responsiveness scenario and send only counts, timings,
   * booleans, and enum-like phases to the native sanitized writer.
   */
  if (!isDiagnosticLoggingScenarioEnabled(diagnosticLogging, "native.chrome.responsiveness")) {
    return;
  }
  postNative({
    details: JSON.stringify({
      ...details,
      performanceNowMs: Math.round(performance.now()),
      source: "titlebar",
      wallTimeMs: Date.now(),
    }),
    event,
    type: "appendNativeChromeResponsivenessDebugLog",
  });
}

function titlebarModeSwitchLogDetails(input: {
  optimisticMode: TitlebarMode | undefined;
  projectState: TitlebarProjectState;
  targetMode: TitlebarMode;
}): Record<string, unknown> {
  return {
    activeMode: input.projectState.activeMode,
    editorIsOpen: input.projectState.editorIsOpen,
    editorIsSleeping: input.projectState.editorIsSleeping,
    editorStatus: input.projectState.editorStatus,
    hasOptimisticMode: input.optimisticMode !== undefined,
    optimisticMode: input.optimisticMode ?? "none",
    projectId: input.projectState.projectId ?? "none",
    projectIsQuick: input.projectState.projectIsQuick,
    targetMode: input.targetMode,
  };
}

function runNativeProcess(
  executable: string,
  args: string[],
  options: { cwd?: string; env?: Record<string, string>; timeoutMs?: number } = {},
): Promise<NativeProcessResult> {
  const requestId = `titlebar-process-${Date.now().toString(36)}-${Math.random()
    .toString(36)
    .slice(2)}`;
  postNative({
    args,
    cwd: options.cwd,
    env: options.env,
    executable,
    requestId,
    type: "runProcess",
  });
  return new Promise((resolve, reject) => {
    const timeout = window.setTimeout(() => {
      pendingProcessResults.delete(requestId);
      reject(new Error(`${executable} ${args.join(" ")} timed out`));
    }, options.timeoutMs ?? 30_000);
    pendingProcessResults.set(requestId, { reject, resolve, timeout });
  });
}

function runNativeKeepAwakeLidSleepPrevention(
  enabled: boolean,
  options: { installIfNeeded?: boolean; timeoutMs?: number } = {},
): Promise<NativeProcessResult> {
  const requestId = `titlebar-lid-sleep-${Date.now().toString(36)}-${Math.random()
    .toString(36)
    .slice(2)}`;
  postNative({
    enabled,
    installIfNeeded: options.installIfNeeded,
    requestId,
    type: "setKeepAwakeLidSleepPrevention",
  });
  return new Promise((resolve, reject) => {
    const timeout = window.setTimeout(() => {
      pendingProcessResults.delete(requestId);
      reject(new Error(`setKeepAwakeLidSleepPrevention ${enabled} timed out`));
    }, options.timeoutMs ?? KEEP_AWAKE_ADMIN_PROCESS_TIMEOUT_MS);
    pendingProcessResults.set(requestId, { reject, resolve, timeout });
  });
}

function syncKeepAwakeRuntimeToMainTitlebar(syncState: KeepAwakeRuntimeSyncState): void {
  /*
   * CDXC:TitlebarKeepAwake 2026-06-23-19:36:
   * Keep Awake menu actions run inside a native child WKWebView. Relay the committed runtime state back to the main titlebar explicitly so the titlebar icon changes immediately instead of depending on cross-webview localStorage events.
   */
  postNative({
    runtime: syncState.runtime,
    suppressAutoStart: syncState.suppressAutoStart,
    type: "syncTitlebarKeepAwakeRuntime",
  });
}

function parseResourceProcessTable(stdout: string): ResourceProcess[] {
  return stdout
    .split("\n")
    .map((line) => {
      const match = /^\s*(\d+)\s+(\d+)\s+([0-9.]+)\s+(\d+)\s+(.+?)\s*$/.exec(line);
      if (!match) {
        return undefined;
      }
      const pid = Number(match[1]);
      const ppid = Number(match[2]);
      const cpu = Number(match[3]);
      const rssKb = Number(match[4]);
      if (!Number.isFinite(pid) || !Number.isFinite(ppid) || !Number.isFinite(cpu) || !Number.isFinite(rssKb)) {
        return undefined;
      }
      return {
        command: match[5] ?? "",
        cpu,
        pid,
        ppid,
        rssMb: rssKb / 1024,
      };
    })
    .filter((process): process is ResourceProcess => process !== undefined);
}

function parseResourceListeningServerTable(stdout: string): ResourceListeningServer[] {
  const servers: ResourceListeningServer[] = [];
  let currentPid: number | undefined;
  let currentCommandName = "";

  for (const rawLine of stdout.split("\n")) {
    const line = rawLine.trim();
    if (!line) {
      continue;
    }
    const field = line[0];
    const value = line.slice(1);
    if (field === "p") {
      const pid = Number(value);
      currentPid = Number.isFinite(pid) && pid > 0 ? pid : undefined;
      currentCommandName = "";
      continue;
    }
    if (field === "c") {
      currentCommandName = value.trim();
      continue;
    }
    if (field !== "n" || currentPid === undefined) {
      continue;
    }
    const endpoint = parseResourceListeningEndpoint(value);
    if (!endpoint) {
      continue;
    }
    servers.push({
      commandName: currentCommandName || "server",
      host: endpoint.host,
      pid: currentPid,
      port: endpoint.port,
      url: endpoint.url,
    });
  }

  return uniqueResourceListeningServers(servers);
}

function parseResourceListeningEndpoint(endpoint: string): { host: string; port: number; url: string } | undefined {
  const trimmed = endpoint.trim();
  if (!trimmed) {
    return undefined;
  }

  let rawHost = "";
  let rawPort = "";
  if (trimmed.startsWith("[")) {
    const hostEnd = trimmed.indexOf("]:");
    if (hostEnd < 0) {
      return undefined;
    }
    rawHost = trimmed.slice(1, hostEnd);
    rawPort = trimmed.slice(hostEnd + 2);
  } else {
    const separatorIndex = trimmed.lastIndexOf(":");
    if (separatorIndex < 0) {
      return undefined;
    }
    rawHost = trimmed.slice(0, separatorIndex);
    rawPort = trimmed.slice(separatorIndex + 1);
  }

  const port = Number(rawPort);
  if (!Number.isInteger(port) || port < 1 || port > 65_535) {
    return undefined;
  }
  const host = resourceServerDisplayHost(rawHost);
  const formattedHost = host.includes(":") ? `[${host}]` : host;
  return {
    host,
    port,
    url: `http://${formattedHost}:${port}`,
  };
}

function parseResourceListeningServerCwdTable(stdout: string): Map<number, string> {
  const cwdByPid = new Map<number, string>();
  let currentPid: number | undefined;

  for (const rawLine of stdout.split("\n")) {
    const line = rawLine.trim();
    if (!line) {
      continue;
    }
    const field = line[0];
    const value = line.slice(1);
    if (field === "p") {
      const pid = Number(value);
      currentPid = Number.isFinite(pid) && pid > 0 ? pid : undefined;
      continue;
    }
    if (field === "n" && currentPid !== undefined && value.trim()) {
      cwdByPid.set(currentPid, value.trim());
    }
  }

  return cwdByPid;
}

function uniqueResourceListeningServers(servers: ResourceListeningServer[]): ResourceListeningServer[] {
  const seen = new Set<string>();
  return servers.filter((server) => {
    const key = `${server.pid}:${server.port}:${server.host}`;
    if (seen.has(key)) {
      return false;
    }
    seen.add(key);
    return true;
  });
}

function resourceServerDisplayHost(host: string): string {
  const normalized = host.trim().replace(/^\[|\]$/gu, "");
  return !normalized ||
    normalized === "*" ||
    normalized === "0.0.0.0" ||
    normalized === "::" ||
    normalized === "::1" ||
    normalized === "127.0.0.1"
    ? "localhost"
    : normalized;
}

async function readResourceProcesses(): Promise<ResourceProcess[]> {
  const result = await runNativeProcess("/bin/ps", [
    "-axo",
    "pid=,ppid=,pcpu=,rss=,command=",
  ]);
  return result.exitCode === 0 ? parseResourceProcessTable(result.stdout) : [];
}

async function readResourceListeningServers(): Promise<ResourceListeningServer[]> {
  /*
   * CDXC:TitlebarResources 2026-06-22-00:30:
   * Resources needs a top Dev Servers section sourced from real TCP listeners,
   * not terminal text heuristics. Read lsof's structured fields while the panel
   * is open, then use cwd only for internal ownership matching without rendering
   * or logging user paths.
   */
  try {
    const listenerResult = await runNativeProcess(
      "/usr/sbin/lsof",
      ["-nP", "-iTCP", "-sTCP:LISTEN", "-F", "pcn"],
      { timeoutMs: 10_000 },
    );
    if (listenerResult.exitCode !== 0) {
      return [];
    }

    const servers = parseResourceListeningServerTable(listenerResult.stdout);
    const pids = Array.from(new Set(servers.map((server) => server.pid)));
    if (pids.length === 0) {
      return servers;
    }

    const cwdResult = await runNativeProcess(
      "/usr/sbin/lsof",
      ["-nP", "-a", "-d", "cwd", "-F", "pn", "-p", pids.join(",")],
      { timeoutMs: 10_000 },
    );
    if (cwdResult.exitCode !== 0) {
      return servers;
    }

    const cwdByPid = parseResourceListeningServerCwdTable(cwdResult.stdout);
    return servers.map((server) => {
      const cwd = cwdByPid.get(server.pid);
      return cwd ? { ...server, cwd } : server;
    });
  } catch {
    return [];
  }
}

/**
 * CDXC:TitlebarResources 2026-05-23-10:46:
 * Resource-manager Quit is a process-manager action, so it must terminate the
 * exact processes shown in the dropdown while the sidebar separately preserves
 * terminal cards as sleeping sessions. Recheck the command before SIGKILL so a
 * delayed hard kill cannot target an unrelated process that reused the PID.
 *
 * CDXC:TitlebarResources 2026-06-22-00:30:
 * Dev Servers Stop should behave like a terminal Ctrl-C against the listener
 * process tree before escalating. Use SIGINT for server bundles and keep SIGTERM
 * for the existing close/sleep resource cleanup paths.
 */
async function terminateResourceProcesses(
  processes: ResourceProcess[],
  options: { gracefulSignal?: "INT" | "TERM" } = {},
): Promise<void> {
  const targets = new Map(
    processes
      .filter((process) => Number.isFinite(process.pid) && process.pid > 1)
      .map((process) => [process.pid, process.command]),
  );
  if (targets.size === 0) {
    return;
  }

  const gracefulSignal = options.gracefulSignal ?? "TERM";
  await runNativeProcess("/bin/kill", [`-${gracefulSignal}`, ...Array.from(targets.keys()).map(String)]);
  window.setTimeout(() => {
    void (async () => {
      const liveProcesses = await readResourceProcesses();
      const liveTargetPids = liveProcesses
        .filter((process) => targets.get(process.pid) === process.command)
        .map((process) => process.pid);
      if (liveTargetPids.length > 0) {
        await runNativeProcess("/bin/kill", ["-KILL", ...liveTargetPids.map(String)]);
      }
    })().catch((error) => {
      console.warn("Failed to finish terminating Ghostex resources", error);
    });
  }, 1_500);
}

function createResourceGroupViews(
  browserTabs: TitlebarBrowserTabResource[],
  resourceGroups: TitlebarResourceGroup[],
  processes: ResourceProcess[],
  servers: ResourceListeningServer[],
  codeEditorProjectIds: string[],
): {
  browserBundles: ResourceProcessBundle[];
  codeIdeBundles: ResourceProcessBundle[];
  groupViews: ResourceGroupView[];
  orphanBundles: ResourceProcessBundle[];
} {
  const claimedPids = new Set<number>();
  const childrenByParent = createProcessChildrenMap(processes);
  const groupedBrowserTabIds = new Set<string>();
  const groupViews = resourceGroups.map((group) => {
    const groupBrowserTabs = browserTabs
      .filter((tab) => isBrowserTabInResourceGroup(tab, group))
      .map((tab) => ({
        ...tab,
        projectId: tab.projectId ?? resourceGroupProjectIdForBrowserTab(tab, group),
      }));
    groupBrowserTabs.forEach((tab) => groupedBrowserTabIds.add(tab.id));
    const bundles = group.sessions
      .map((session) => createSessionResourceBundle(session, processes, childrenByParent, claimedPids))
      .filter((bundle): bundle is ResourceProcessBundle => bundle !== undefined);
    const browserBundles = createBrowserBundles(groupBrowserTabs, processes, claimedPids, {
      includeRuntimeBundles: false,
    });
    return {
      bundles: [...bundles, ...browserBundles],
      group,
    };
  });
  const codeIdeBundles = createCodeIdeResourceBundles(
    servers,
    processes,
    childrenByParent,
    claimedPids,
    codeEditorProjectIds,
  );
  claimAppRuntimeProcesses(processes, childrenByParent, claimedPids);
  const browserBundles = createBrowserBundles(
    browserTabs.filter((tab) => !groupedBrowserTabIds.has(tab.id)),
    processes,
    claimedPids,
  );
  const orphanBundles = createOrphanBundles(processes, childrenByParent, claimedPids);
  return { browserBundles, codeIdeBundles, groupViews, orphanBundles };
}

function createGhostexResourceProcessTotals(processes: ResourceProcess[]): ResourceProcessTotals {
  /*
   * CDXC:TitlebarResources 2026-06-30-23:17:
   * The Resources header RAM/CPU total must match external monitors that group Ghostex with every owned child process, not only the rows that are visible and safe to Sleep/Close.
   * Compute the app-wide total from the raw `ps` snapshot while leaving row bundles scoped to actionable user resources.
   *
   * CDXC:TitlebarResources 2026-06-30-23:29:
   * Use Ghostex-owned executable roots plus their descendants for the header total.
   * This matches app monitors that aggregate child processes and avoids rescanning every long command line with broad ownership regexes while the Resources child window refreshes.
   *
   * CDXC:TitlebarResources 2026-06-30-23:43:
   * gxserver, zmx, and bundled helper processes can daemonize under launchd while still belonging to Ghostex's app footprint. Seed totals from any executable inside the Ghostex app bundle, then traverse descendants so orphaned helper roots and their agent children remain counted without treating arbitrary command text as ownership evidence.
   */
  const childrenByParent = createProcessChildrenMap(processes);
  const appRootProcesses = processes.filter(isGhostexAppBundleProcess);
  const ownedProcesses = collectProcessTree(appRootProcesses, childrenByParent);
  return {
    cpu: sumProcessCpu(ownedProcesses),
    memoryMb: sumProcessMemory(ownedProcesses),
    processCount: ownedProcesses.length,
  };
}

function isGhostexAppBundleProcess(process: ResourceProcess): boolean {
  const executablePath = process.command.split(/\s+/, 1)[0] ?? "";
  return /\/Ghostex(?:-dev)?\.app\/Contents\//i.test(executablePath);
}

function createResourceServerBundles(
  servers: ResourceListeningServer[],
  resourceViews: ReturnType<typeof createResourceGroupViews>,
  processes: ResourceProcess[],
  portless: SidebarPortlessState | undefined,
): ResourceProcessBundle[] {
  /*
   * CDXC:TitlebarResources 2026-06-22-00:30:
   * The Dev Servers section belongs above project sessions but must still be
   * owned by a visible terminal resource. Attribute listeners by process-tree
   * membership first, then by listener cwd inside the project path when a
   * provider-backed session is visible without a sampled zmx root.
   *
   * CDXC:PortlessResources 2026-06-23-15:18:
   * Resources may show Portless domains only on Ghostex-owned live server rows.
   * Join routePreviews to the existing listener-backed server bundles by
   * project id, session id, and port so assigned domains never become extra
   * rows and Stop continues to target only the live server process tree.
   */
  const portlessPreviewsByOwnerAndPort = createPortlessRoutePreviewMap(portless);
  const processByPid = new Map(processes.map((process) => [process.pid, process]));
  const childrenByParent = createProcessChildrenMap(processes);
  const terminalOwners = resourceViews.groupViews.flatMap((view) =>
    view.bundles
      .filter((bundle) => bundle.type === "session" && bundle.session?.sessionKind === "terminal")
      .map((bundle) => ({ bundle, view })),
  );
  const ownerByPid = new Map<number, { bundle: ResourceProcessBundle; view: ResourceGroupView }>();
  for (const owner of terminalOwners) {
    owner.bundle.pids.forEach((pid) => ownerByPid.set(pid, owner));
  }

  return servers
    .map((server): ResourceProcessBundle | undefined => {
      const owner =
        ownerByPid.get(server.pid) ?? findResourceServerCwdOwner(server, terminalOwners);
      if (!owner) {
        return undefined;
      }

      const process = processByPid.get(server.pid);
      const tree = process ? collectProcessTree([process], childrenByParent) : [];
      const pids = tree.length > 0 ? tree.map((treeProcess) => treeProcess.pid) : [server.pid];
      const portlessPreview = owner.bundle.session
        ? portlessPreviewsByOwnerAndPort.get(
            createPortlessRoutePreviewKeyForSession(owner.bundle.session, server.port),
          )
        : undefined;
      return {
        childProcesses: process ? tree.filter((treeProcess) => treeProcess.pid !== process.pid) : [],
        cpu: sumProcessCpu(tree),
        key: `server:${server.pid}:${server.port}`,
        label: resourceServerLabel(server),
        memoryMb: sumProcessMemory(tree),
        pids,
        ...(portlessPreview ? { portless: portlessPreview } : {}),
        process,
        server,
        session: owner.bundle.session,
        type: "server",
      };
    })
    .filter((bundle): bundle is ResourceProcessBundle => bundle !== undefined)
    .sort((left, right) => {
      const leftPort = left.server?.port ?? 0;
      const rightPort = right.server?.port ?? 0;
      return leftPort === rightPort ? left.label.localeCompare(right.label) : leftPort - rightPort;
    });
}

function findResourceServerCwdOwner(
  server: ResourceListeningServer,
  terminalOwners: { bundle: ResourceProcessBundle; view: ResourceGroupView }[],
): { bundle: ResourceProcessBundle; view: ResourceGroupView } | undefined {
  /*
   * CDXC:TitlebarResources 2026-07-26:
   * Project paths nest, so the first group whose path contains the listener cwd
   * is not necessarily its owner: a home-directory or monorepo-root project
   * would otherwise claim every dev server started inside a project below it.
   * Attribute the listener to the deepest matching project path instead.
   */
  if (!server.cwd) {
    return undefined;
  }
  let owner: { bundle: ResourceProcessBundle; view: ResourceGroupView } | undefined;
  let ownerPathLength = -1;
  for (const candidate of terminalOwners) {
    const projectPath = normalizeResourceOwnershipPath(candidate.view.group.projectPath);
    if (
      !projectPath ||
      projectPath.length <= ownerPathLength ||
      !isResourcePathInsideOrEqualTo(server.cwd, projectPath)
    ) {
      continue;
    }
    owner = candidate;
    ownerPathLength = projectPath.length;
  }
  return owner;
}

function createPortlessRoutePreviewMap(
  portless: SidebarPortlessState | undefined,
): Map<string, ResourcePortlessServerPresentation> {
  const previewsByOwnerAndPort = new Map<string, ResourcePortlessServerPresentation>();
  const routePreviews = portless?.presentation?.routePreviews ?? [];
  if (!portless || routePreviews.length === 0 || portless.presentation?.routePreviewStatus !== "current") {
    return previewsByOwnerAndPort;
  }
  for (const preview of routePreviews) {
    const key = createPortlessRoutePreviewKey(preview.projectId, preview.sessionId, preview.port);
    if (previewsByOwnerAndPort.has(key)) {
      continue;
    }
    previewsByOwnerAndPort.set(key, {
      hostname: preview.hostname,
      isSetupActive: isPortlessResourceSetupActive(portless),
      protocol: preview.protocol,
      setupAction: getTitlebarPortlessResourcesSetupAction(portless),
      setupActionLabel: getTitlebarPortlessResourcesSetupActionLabel(portless),
      setupStatusLabel: getTitlebarPortlessResourcesSetupStatusLabel(portless),
    });
  }
  return previewsByOwnerAndPort;
}

function createPortlessRoutePreviewKey(projectId: string, sessionId: string, port: number): string {
  return `${projectId}:${sessionId}:${port}`;
}

function createPortlessRoutePreviewKeyForSession(
  session: Pick<TitlebarResourceSession, "projectId" | "sessionId">,
  port: number,
): string {
  const combinedSession = parseCombinedProjectSessionId(session.sessionId);
  return createPortlessRoutePreviewKey(
    session.projectId ?? combinedSession?.projectId ?? "",
    combinedSession?.sessionId ?? session.sessionId,
    port,
  );
}

function isPortlessResourceSetupActive(portless: SidebarPortlessState): boolean {
  const health = portless.health;
  return (
    health.enabled === true &&
    health.setupOwnership === "ghostex" &&
    health.setupStatus === "active"
  );
}

function getTitlebarPortlessResourcesSetupAction(
  portless: SidebarPortlessState,
): NativePortlessAdminInstallAction | undefined {
  const actions: readonly NativePortlessAdminInstallAction[] = ["install", "reconfigure", "retry"];
  return actions.find((action) => portless.nativeAdmin.actions[action]?.available === true);
}

function getTitlebarPortlessResourcesSetupActionLabel(portless: SidebarPortlessState): string {
  const action = getTitlebarPortlessResourcesSetupAction(portless);
  if (action === "retry") {
    return "Retry";
  }
  if (action === "install" || action === "reconfigure") {
    return "Set up";
  }
  return "Status";
}

function getTitlebarPortlessResourcesSetupStatusLabel(portless: SidebarPortlessState): string {
  const health = portless.health;
  if (!health.enabled || health.setupStatus === "disabled") {
    return "Portless disabled";
  }
  if (health.setupStatus === "failed") {
    return "Portless setup failed";
  }
  if (health.setupOwnership === "standalone") {
    return "Portless needs reconfigure";
  }
  if (health.setupStatus === "needed" || health.setupOwnership === "missing") {
    return "Portless setup needed";
  }
  return "Portless status";
}

function resourceServerLabel(server: Pick<ResourceListeningServer, "host" | "port">): string {
  return `${server.host}:${server.port}`;
}

function isResourcePathInsideOrEqualTo(childPath: string, parentPath: string): boolean {
  const child = normalizeResourceOwnershipPath(childPath);
  const parent = normalizeResourceOwnershipPath(parentPath);
  if (!child || !parent) {
    return false;
  }
  return child === parent || child.startsWith(`${parent}/`);
}

function normalizeResourceOwnershipPath(path: string): string {
  return path.trim().replace(/\/+$/gu, "");
}

const EMPTY_RESOURCE_GROUP_VIEWS: ReturnType<typeof createResourceGroupViews> = {
  browserBundles: [],
  codeIdeBundles: [],
  groupViews: [],
  orphanBundles: [],
};

const EMPTY_RESOURCE_PROCESS_TOTALS: ResourceProcessTotals = {
  cpu: 0,
  memoryMb: 0,
  processCount: 0,
};

type ResourceItemCollapseTarget = {
  collapsedWhenKeyPresent: boolean;
  key: string;
};

function createResourceItemCollapseTarget(bundle: ResourceProcessBundle): ResourceItemCollapseTarget | undefined {
  if (bundle.childProcesses.length === 0) {
    return undefined;
  }
  const collapsedByDefault = bundle.type === "session" || bundle.type === "browser" || bundle.type === "server";
  return {
    collapsedWhenKeyPresent: !collapsedByDefault,
    key: collapsedByDefault ? `expanded:${bundle.key}` : bundle.key,
  };
}

function createResourceItemCollapseTargets(bundles: ResourceProcessBundle[]): ResourceItemCollapseTarget[] {
  return bundles
    .map((bundle) => createResourceItemCollapseTarget(bundle))
    .filter((target): target is ResourceItemCollapseTarget => target !== undefined);
}

function isResourceItemCollapsed(target: ResourceItemCollapseTarget, collapsedKeys: Set<string>): boolean {
  return target.collapsedWhenKeyPresent
    ? collapsedKeys.has(target.key)
    : !collapsedKeys.has(target.key);
}

function createResourceViewItemCollapseTargets(
  resourceViews: ReturnType<typeof createResourceGroupViews>,
  serverBundles: ResourceProcessBundle[] = [],
): ResourceItemCollapseTarget[] {
  /*
   * CDXC:TitlebarResources 2026-06-11-18:30:
   * Resource project/group sections no longer expose their own collapse controls
   * because per-section headers create a cramped, ambiguous Resources state.
   *
   * CDXC:TitlebarResources 2026-06-12-23:33:
   * The header expand/collapse control beside Sleep Inactive bulk-toggles
   * individual expandable resource rows inside Projects, Browser Tabs, and
   * Orphaned / Detached. It must never collapse those top-level sections.
   *
   * CDXC:TitlebarResources 2026-06-13-02:02:
   * Opening Resources should begin with every expandable row collapsed for that
   * modal instance, not just the user's first-ever Resources visit. Return
   * targets with their default-state polarity so open seeding and button clicks
   * can share the same state transition.
   */
  return createResourceItemCollapseTargets([
    ...serverBundles,
    ...resourceViews.groupViews
      .filter((view) => view.bundles.length > 0)
      .flatMap((view) => view.bundles),
    ...resourceViews.codeIdeBundles,
    ...resourceViews.browserBundles,
    ...resourceViews.orphanBundles,
  ]);
}

function applyResourceItemCollapsedState(
  current: Set<string>,
  targets: readonly ResourceItemCollapseTarget[],
  collapsed: boolean,
): Set<string> {
  const next = new Set(current);
  let changed = false;
  for (const target of targets) {
    const shouldHaveKey = collapsed === target.collapsedWhenKeyPresent;
    if (shouldHaveKey && !next.has(target.key)) {
      next.add(target.key);
      changed = true;
    } else if (!shouldHaveKey && next.delete(target.key)) {
      changed = true;
    }
  }
  return changed ? next : current;
}

function isBrowserTabInResourceGroup(
  tab: TitlebarBrowserTabResource,
  group: TitlebarResourceGroup,
): boolean {
  const tabSessionId = browserTabSessionId(tab);
  if (tabSessionId && group.sessions.some((session) => session.sessionId === tabSessionId)) {
    return true;
  }
  const projectId = browserTabProjectId(tab);
  return Boolean(projectId && group.projectId && projectId === group.projectId);
}

function resourceGroupProjectIdForBrowserTab(
  tab: TitlebarBrowserTabResource,
  group: TitlebarResourceGroup,
): string | undefined {
  const tabSessionId = browserTabSessionId(tab);
  return group.projectId ?? group.sessions.find((session) => session.sessionId === tabSessionId)?.projectId;
}

function createProcessChildrenMap(processes: ResourceProcess[]): Map<number, ResourceProcess[]> {
  const childrenByParent = new Map<number, ResourceProcess[]>();
  for (const process of processes) {
    const children = childrenByParent.get(process.ppid) ?? [];
    children.push(process);
    childrenByParent.set(process.ppid, children);
  }
  return childrenByParent;
}

function collectProcessTree(
  seedProcesses: ResourceProcess[],
  childrenByParent: Map<number, ResourceProcess[]>,
): ResourceProcess[] {
  const collected = new Map<number, ResourceProcess>();
  const queue = [...seedProcesses];
  while (queue.length > 0) {
    const process = queue.shift()!;
    if (collected.has(process.pid)) {
      continue;
    }
    collected.set(process.pid, process);
    queue.push(...(childrenByParent.get(process.pid) ?? []));
  }
  return Array.from(collected.values());
}

function createSessionResourceBundle(
  session: TitlebarResourceSession,
  processes: ResourceProcess[],
  childrenByParent: Map<number, ResourceProcess[]>,
  claimedPids: Set<number>,
): ResourceProcessBundle | undefined {
  const matchTokens = [
    session.sessionPersistenceName,
    session.sessionId,
    session.terminalTitle,
  ]
    .map((token) => token?.trim())
    .filter((token): token is string => Boolean(token && token.length >= 4));
  const seedProcesses = processes.filter((process) =>
    matchTokens.some((token) => process.command.includes(token)),
  );
  if (
    seedProcesses.length === 0 &&
    session.sessionKind !== "browser" &&
    !hasRunningZmxProviderForTitlebarResourceSession(session)
  ) {
    return undefined;
  }
  const tree = collectProcessTree(seedProcesses, childrenByParent);
  tree.forEach((process) => claimedPids.add(process.pid));
  return {
    childProcesses: tree.filter((process) => !seedProcesses.some((seed) => seed.pid === process.pid)),
    cpu: sumProcessCpu(tree),
    key: `session:${session.projectId ?? "active"}:${session.sessionId}`,
    label: session.title,
    memoryMb: sumProcessMemory(tree),
    pids: tree.map((process) => process.pid),
    process: seedProcesses[0],
    session,
    type: "session",
  };
}

function hasRunningZmxProviderForTitlebarResourceSession(
  session: Pick<
    TitlebarResourceSession,
    "providerSessionState" | "sessionKind" | "sessionPersistenceName" | "sessionPersistenceProvider"
  >,
): boolean {
  /*
   * CDXC:TitlebarResources 2026-06-19-19:21:
   * Resources must list every zmx-backed terminal whose provider is running,
   * even when the macOS pane is not loaded and the sampled process command
   * does not expose the zmx session name. The sidebar labels that state as
   * "Active, not loaded"; Resources should show the same live session row and
   * attach CPU/RAM only when a sampled process tree can be matched.
   */
  return (
    session.sessionKind === "terminal" &&
    session.sessionPersistenceProvider === "zmx" &&
    Boolean(session.sessionPersistenceName?.trim()) &&
    session.providerSessionState === "exists"
  );
}

function createCodeIdeResourceBundles(
  servers: ResourceListeningServer[],
  processes: ResourceProcess[],
  childrenByParent: Map<number, ResourceProcess[]>,
  claimedPids: Set<number>,
  codeEditorProjectIds: string[],
): ResourceProcessBundle[] {
  /*
   * CDXC:TitlebarResources 2026-06-22-13:50:
   * Embedded Code is one shared code-server runtime, not a project child process.
   * Identify it from Ghostex's fixed localhost editor listener and render it in a Code IDE section so a root "/" project cannot claim it through path substring matching.
   *
   * CDXC:SourceRuntimeOwnership 2026-06-28-04:05:
   * Resources must use the native Source runtime port from bootstrap so legacy
   * dev bundles and GPUI-specific ports are recognized without reverting to the
   * old global 3775 assumption.
   */
  const runtimePort = codeServerResourcePort();
  const server = servers.find(
    (candidate) =>
      candidate.port === runtimePort && candidate.host === "localhost",
  );
  if (!server) {
    return [];
  }
  const processByPid = new Map(processes.map((process) => [process.pid, process]));
  const seedProcess = processByPid.get(server.pid);
  const tree = seedProcess
    ? collectProcessTree([seedProcess], childrenByParent).filter((process) => !claimedPids.has(process.pid))
    : [];
  tree.forEach((process) => claimedPids.add(process.pid));
  claimedPids.add(server.pid);
  const pids = tree.length > 0 ? tree.map((process) => process.pid) : [server.pid];
  return [
    {
      childProcesses: seedProcess
        ? tree.filter((process) => process.pid !== seedProcess.pid)
        : [],
      cpu: sumProcessCpu(tree),
      key: "code:ide",
      label: "Code",
      memoryMb: sumProcessMemory(tree),
      pids,
      process: seedProcess,
      projectEditorIds: Array.from(new Set(codeEditorProjectIds)),
      type: "code",
    },
  ];
}

function claimAppRuntimeProcesses(
  processes: ResourceProcess[],
  childrenByParent: Map<number, ResourceProcess[]>,
  claimedPids: Set<number>,
): void {
  const appProcesses = processes.filter(
    (process) =>
      !claimedPids.has(process.pid) &&
      /ghostexHost|Ghostex\.app|ghostex/i.test(process.command),
  );
  const appPids = new Set(appProcesses.map((process) => process.pid));
  /**
   * CDXC:TitlebarResources 2026-05-16-19:53:
   * Ghostex-owned app processes need to be claimed as one process tree, not as
   * individual helper matches, so they never leak into detached resource rows.
   *
   * CDXC:TitlebarResources 2026-05-25-16:53:
   * The Resources dropdown should hide Ghostex's own app-runtime rows. Keep
   * matching these processes only to reserve their PIDs before browser and
   * orphan resource sections are built.
   *
   * CDXC:TitlebarResources 2026-05-29-12:02:
   * Ghostex-launched zmx/tmux/zellij and agent roots are user work resources,
   * not app runtime. Do not reserve those roots here; leave them for session or
   * orphan resource tree walking so child processes such as node, npm, Codex,
   * and DevTools helpers stay counted under the Ghostex-owned session root.
   */
  appProcesses
    .filter((process) => !appPids.has(process.ppid) && !isAgentRuntimeProcess(process))
    .slice(0, 3)
    .forEach((process) => {
      const tree = collectProcessTree([process], childrenByParent).filter(
        (treeProcess) =>
          !claimedPids.has(treeProcess.pid) &&
          !isGhostexBrowserProcess(treeProcess) &&
          (treeProcess.pid === process.pid || !isAgentRuntimeProcess(treeProcess)),
      );
      tree.forEach((treeProcess) => claimedPids.add(treeProcess.pid));
    });
}

function createBrowserBundles(
  browserTabs: TitlebarBrowserTabResource[],
  processes: ResourceProcess[],
  claimedPids: Set<number>,
  options: { includeRuntimeBundles?: boolean } = {},
): ResourceProcessBundle[] {
  /**
   * CDXC:TitlebarResources 2026-05-17-03:09:
   * Browser tab resources must only count Ghostex-owned embedded browser helper
   * processes. System-wide Chromium/Electron helpers from Chrome, VS Code,
   * Codex, Discord, or other apps can share the same `--type=renderer`
   * arguments, so ownership must be proven before a process is allowed into the
   * Browser Tabs section.
   */
  const browserProcesses = processes.filter(
    (process) => !claimedPids.has(process.pid) && isGhostexBrowserProcess(process),
  );
  const bundles: ResourceProcessBundle[] = [];
  for (const tab of browserTabs) {
    const tabProcesses = browserProcesses.filter(
      (process) => browserProcessClientId(process) === String(tab.browserId),
    );
    if (tabProcesses.length === 0) {
      continue;
    }
    tabProcesses.forEach((process) => claimedPids.add(process.pid));
    bundles.push({
      browserTab: tab,
      childProcesses: tabProcesses,
      cpu: sumProcessCpu(tabProcesses),
      key: `browser:${tab.id}`,
      label: tab.title,
      memoryMb: sumProcessMemory(tabProcesses),
      pids: tabProcesses.map((process) => process.pid),
      process: tabProcesses[0],
      type: "browser",
    });
  }
  if (options.includeRuntimeBundles === false) {
    return bundles.slice(0, 16);
  }
  const remainingProcesses = browserProcesses.filter((process) => !claimedPids.has(process.pid));
  const unmatchedRendererProcesses = remainingProcesses.filter((process) => browserProcessClientId(process));
  if (unmatchedRendererProcesses.length > 0) {
    unmatchedRendererProcesses.forEach((process) => claimedPids.add(process.pid));
    bundles.push({
      childProcesses: unmatchedRendererProcesses.slice(0, 12),
      cpu: sumProcessCpu(unmatchedRendererProcesses),
      key: "browser:unmatched-renderers",
      label: "Unmatched browser renderers",
      memoryMb: sumProcessMemory(unmatchedRendererProcesses),
      pids: unmatchedRendererProcesses.map((process) => process.pid),
      process: unmatchedRendererProcesses[0],
      type: "browser",
    });
  }
  const runtimeProcesses = remainingProcesses.filter((process) => !claimedPids.has(process.pid));
  if (runtimeProcesses.length > 0) {
    runtimeProcesses.forEach((process) => claimedPids.add(process.pid));
    bundles.push({
      childProcesses: runtimeProcesses.slice(0, 12),
      cpu: sumProcessCpu(runtimeProcesses),
      key: "browser:runtime",
      label: "Browser runtime",
      memoryMb: sumProcessMemory(runtimeProcesses),
      pids: runtimeProcesses.map((process) => process.pid),
      process: runtimeProcesses[0],
      type: "browser",
    });
  }
  return bundles.slice(0, 16);
}

function createOrphanBundles(
  processes: ResourceProcess[],
  childrenByParent: Map<number, ResourceProcess[]>,
  claimedPids: Set<number>,
): ResourceProcessBundle[] {
  const ownedSeedProcesses = processes.filter(
    (process) =>
      !claimedPids.has(process.pid) &&
      isGhostexOwnedResourceProcess(process) &&
      isAgentRuntimeProcess(process),
  );
  const ownedSeedPids = new Set(ownedSeedProcesses.map((process) => process.pid));
  return ownedSeedProcesses
    .filter((process) => !ownedSeedPids.has(process.ppid))
    .slice(0, 16)
    .map((process) => {
      const tree = collectProcessTree([process], childrenByParent).filter(
        (treeProcess) => !claimedPids.has(treeProcess.pid),
      );
      tree.forEach((treeProcess) => claimedPids.add(treeProcess.pid));
      return {
        childProcesses: tree.filter((treeProcess) => treeProcess.pid !== process.pid),
        cpu: sumProcessCpu(tree),
        key: `orphan:${process.pid}`,
        label: getProcessDisplayName(process),
        memoryMb: sumProcessMemory(tree),
        pids: tree.map((treeProcess) => treeProcess.pid),
        process,
        type: "orphan" as const,
      };
    });
}

function isGhostexOwnedResourceProcess(process: ResourceProcess): boolean {
  const command = process.command;
  /**
   * CDXC:TitlebarResources 2026-05-28-21:04:
   * Orphaned / Detached resources are still part of the app's CPU/RAM total, so
   * command-name matches are not enough. Only include ungrouped agent-looking
   * root processes when their command proves Ghostex ownership, then walk only
   * their descendants. External Codex, DevTools, Chrome extension, and
   * computer-use helpers from other terminals must stay out of the Resources
   * dropdown and app resource calculation.
   */
  return (
    /\/(?:Applications\/)?Ghostex(?:-dev)?\.app\b/i.test(command) ||
    /\bghostexHost\b/i.test(command) ||
    /\/\.ghostex(?:-dev)?\//i.test(command) ||
    /\bGHOSTEX_[A-Z0-9_]+=/.test(command) ||
    /\/Resources\/Web\/bin\/zmx\b/.test(command)
  );
}

function isGhostexBrowserProcess(process: ResourceProcess): boolean {
  const command = process.command;
  const isBrowserHelper = /Chromium Embedded Framework|--type=(renderer|gpu-process|utility)\b/.test(command);
  if (!isBrowserHelper) {
    return false;
  }
  return (
    /\/Contents\/Frameworks\/[^/\s]*ghostex[^/\s]* Helper/i.test(command) ||
    /--main-bundle-path=\S*\/ghostex(?:-dev)?\.app\b/i.test(command) ||
    /--user-data-dir=\S*\/\.ghostex\/cef\b/.test(command)
  );
}

function isAgentRuntimeProcess(process: ResourceProcess): boolean {
  return /\b(zmx|codex|code-server|computer-use|chrome-devtools-mcp|devtools)\b/i.test(process.command);
}

function browserProcessClientId(process: ResourceProcess): string | undefined {
  return /--(?:renderer-)?client-id=(\d+)/.exec(process.command)?.[1];
}

function browserTabSessionId(tab: TitlebarBrowserTabResource): string | undefined {
  if (tab.sessionId?.trim()) {
    return tab.sessionId.trim();
  }
  const match = /^browser:(?<sessionId>.+)$/u.exec(tab.id);
  return match?.groups?.sessionId;
}

function browserTabProjectId(tab: TitlebarBrowserTabResource): string | undefined {
  if (tab.projectId?.trim()) {
    return tab.projectId.trim();
  }
  const match = /^project-editor:(?<projectId>.+):[^:]+$/u.exec(tab.id);
  if (!match?.groups?.projectId) {
    return undefined;
  }
  try {
    return decodeURIComponent(match.groups.projectId);
  } catch {
    return undefined;
  }
}

function getBrowserProcessDisplayName(process: ResourceProcess): string {
  const clientId = browserProcessClientId(process);
  if (clientId) {
    return `Browser renderer client ${clientId}`;
  }
  if (process.command.includes("--type=gpu-process")) {
    return "Browser GPU";
  }
  if (process.command.includes("--type=utility")) {
    return getBrowserUtilityProcessDisplayName(process);
  }
  return "Browser renderer";
}

function getBrowserUtilityProcessDisplayName(process: ResourceProcess): string {
  const subtype = /--utility-sub-type=([^\s]+)/.exec(process.command)?.[1];
  if (subtype?.includes("NetworkService")) {
    return "Browser network service";
  }
  if (subtype?.includes("StorageService")) {
    return "Browser storage service";
  }
  if (subtype?.includes("AudioService")) {
    return "Browser audio service";
  }
  if (subtype?.includes("VideoCaptureService")) {
    return "Browser video capture service";
  }
  return "Browser utility";
}

function getProcessDisplayName(process: ResourceProcess): string {
  const command = process.command.split(/\s+/)[0] ?? "Process";
  return command.split("/").pop() || command;
}

function sumProcessCpu(processes: ResourceProcess[]): number {
  return processes.reduce((sum, process) => sum + process.cpu, 0);
}

function sumProcessMemory(processes: ResourceProcess[]): number {
  return processes.reduce((sum, process) => sum + process.rssMb, 0);
}

function sumBundleCpu(bundles: ResourceProcessBundle[]): number {
  return bundles.reduce((sum, bundle) => sum + bundle.cpu, 0);
}

function sumBundleMemory(bundles: ResourceProcessBundle[]): number {
  return bundles.reduce((sum, bundle) => sum + bundle.memoryMb, 0);
}

function createInactiveTerminalSleepSessionIds(resourceGroups: TitlebarResourceGroup[]): string[] {
  /**
   * CDXC:TitlebarResources 2026-05-16-19:53:
   * The dropdown sleep shortcut is intentionally conservative: only awake,
   * idle agent terminal sessions older than seven minutes are eligible. Working
   * and attention sessions must stay awake because those states indicate active
   * output or a user-visible response waiting for review.
   *
   * CDXC:TitlebarResources 2026-05-26-17:16:
   * Sleep Inactive should sleep every awake idle terminal represented in the
   * Resources dropdown, not only old agent-detected rows. Keep working,
   * attention, and already sleeping sessions awake, but do not require agent
   * metadata or a seven-minute age gate.
   *
   * CDXC:TitlebarResources 2026-06-06-06:09:
   * Delayed Send means a terminal has a staged Enter that must fire while the
   * pane is awake. Exclude delayed-send sessions from the Resources sleep count
   * and payload so macOS and Electron do not hide pending sends behind sleep.
   */
  return resourceGroups.flatMap((group) =>
    group.sessions
      .filter((session) => {
        return !(
          session.sessionKind !== "terminal" ||
          session.isSleeping === true ||
          session.activity === "working" ||
          session.activity === "attention" ||
          hasTitlebarResourceDelayedSend(session)
        );
      })
      .map(titlebarResourceSidebarSessionId),
  );
}

function hasTitlebarResourceDelayedSend(
  session: Pick<
    TitlebarResourceSession,
    "delayedSendDeadlineAt" | "delayedSendRemainingLabel" | "delayedSendRemainingMs"
  >,
): boolean {
  return Boolean(
    session.delayedSendRemainingLabel ||
      session.delayedSendDeadlineAt ||
      typeof session.delayedSendRemainingMs === "number",
  );
}

function titlebarResourceSidebarSessionId(
  session: Pick<TitlebarResourceSession, "projectId" | "sessionId">,
): string {
  /*
   * CDXC:TitlebarResources 2026-06-15-15:27:
   * gxserver presentation-backed Resources rows already arrive with combined
   * project/session ids. Focus, Sleep, and Close must forward that id unchanged
   * instead of wrapping it again, or the sidebar resolves a synthetic session
   * id and the visible row action does nothing.
   */
  if (parseCombinedProjectSessionId(session.sessionId)) {
    return session.sessionId;
  }
  return session.projectId
    ? createCombinedProjectSessionId(session.projectId, session.sessionId)
    : session.sessionId;
}

function uniqueResourceBundles(bundles: ResourceProcessBundle[]): ResourceProcessBundle[] {
  const seen = new Set<string>();
  return bundles.filter((bundle) => {
    if (seen.has(bundle.key)) {
      return false;
    }
    seen.add(bundle.key);
    return true;
  });
}

function isResourceBundleActionable(bundle: ResourceProcessBundle): boolean {
  /**
   * CDXC:TitlebarResources 2026-06-15-13:45:
   * Resources must not expose Close for shared Chromium runtime bundles because killing GPU, network, storage, or unmatched renderer helpers can leave the app's embedded browser surfaces broken. Only user-owned browser tabs get resource Close controls; diagnostic browser helper rows stay visible for CPU/RAM accounting.
   */
  return !(bundle.type === "browser" && !bundle.browserTab);
}

function resourceBundleSidebarSessionIds(bundle: ResourceProcessBundle): string[] {
  if (bundle.type === "server") {
    return [];
  }
  const session = bundle.session;
  if (session) {
    return [titlebarResourceSidebarSessionId(session)];
  }
  const browserSessionId = bundle.browserTab ? browserTabSessionId(bundle.browserTab) : undefined;
  if (!browserSessionId) {
    return [];
  }
  return [
    bundle.browserTab?.projectId
      ? createCombinedProjectSessionId(bundle.browserTab.projectId, browserSessionId)
      : browserSessionId,
  ];
}

function resourceBundleFocusSessionId(bundle: ResourceProcessBundle): string | undefined {
  const session = bundle.session;
  if (session) {
    return titlebarResourceSidebarSessionId(session);
  }
  return resourceBundleSidebarSessionIds(bundle)[0];
}

function resourceBundleProjectEditorIds(bundle: ResourceProcessBundle): string[] {
  if (bundle.projectEditorIds) {
    return bundle.projectEditorIds;
  }
  if (bundle.type === "code") {
    const match = /^code:(?<groupId>.+)$/u.exec(bundle.key);
    const projectId = match?.groups?.groupId ? parseCombinedProjectGroupId(match.groups.groupId) : undefined;
    return projectId ? [projectId] : [];
  }
  const projectId = bundle.browserTab ? browserTabProjectId(bundle.browserTab) : undefined;
  return projectId ? [projectId] : [];
}

function sortResourceBundlesForDisplay(
  bundles: ResourceProcessBundle[],
  quittingKeys: Set<string>,
): ResourceProcessBundle[] {
  return [...bundles].sort((left, right) => {
    const leftQuitting = quittingKeys.has(left.key);
    const rightQuitting = quittingKeys.has(right.key);
    return leftQuitting === rightQuitting ? 0 : leftQuitting ? 1 : -1;
  });
}

function formatWholePercent(value: number): string {
  return `${Math.trunc(Math.max(0, value))}%`;
}

function formatResourceMemory(value: number): string {
  /*
   * CDXC:TitlebarResources 2026-06-30-23:17:
   * Resource memory labels must not floor GB values because that made near-2 GB totals render as 1 GB and hid real app pressure.
   * Round GB values to one decimal while keeping whole-MB labels for smaller processes.
   */
  const safeValue = Math.max(0, value);
  if (safeValue >= 1024) {
    const roundedGb = Math.round((safeValue / 1024) * 10) / 10;
    return `${Number.isInteger(roundedGb) ? roundedGb.toFixed(0) : roundedGb.toFixed(1)} GB`;
  }
  return `${Math.round(safeValue)} MB`;
}

export function GhostexTitlebarHost() {
  return <App />;
}

function App() {
  const bootstrap = window.__ghostex_NATIVE_HOST__ ?? {};
  const titlebarPanelKind = useMemo(() => initialTitlebarDropdownPanelKind, []);
  const isDropdownPanel = titlebarPanelKind !== undefined;
  const [projectState, setProjectState] = useState<TitlebarProjectState>(() =>
    createInitialProjectState(bootstrap),
  );
  const [selectedTargetId, setSelectedTargetId] = useState(() => readLastOpenTargetId());
  const [selectedActionCommandId, setSelectedActionCommandId] = useState(() =>
    readLastActionCommandId(createInitialProjectState(bootstrap)),
  );
  const [nativeDropdownOpen, setNativeDropdownOpen] = useState<TitlebarDropdownPanelKind | undefined>();
  const dropdownPanelSizeResolverRef = useRef<(kind: TitlebarDropdownPanelKind) => TitlebarDropdownPanelSize>(
    (kind) =>
      createTitlebarDropdownPanelPreferredSize(kind, {
        actionCount: 0,
        gitItemCount: 1,
        keepAwakeIsRunning: false,
        modeOptionCount: 4,
        targetCount: 0,
      }),
  );
  const [readTipIds, setReadTipIds] = useState<Set<string>>(() => readStoredTitlebarTipIds());
  /*
   * CDXC:ReactTitlebar 2026-06-11-13:22:
   * Dropdown content now lives in native child windows, so the main titlebar
   * WKWebView must never publish a below-titlebar overlay-open state or trigger
   * the workspace interaction shield.
   */
  const titlebarOverlayOpen = false;
  const [keepAwakeRuntime, setKeepAwakeRuntime] = useState<KeepAwakeRuntimeState | undefined>(
    () => readStoredKeepAwakeRuntime(),
  );
  const [keepAwakeAutoStartSuppressed, setKeepAwakeAutoStartSuppressed] = useState(false);
  const [keepAwakeWorkingSessionGraceUntilMs, setKeepAwakeWorkingSessionGraceUntilMs] =
    useState<number | undefined>();
  const previousKeepAwakeWorkingSessionCountRef = useRef(projectState.keepAwake.workingSessionCount);
  const [resourceProcesses, setResourceProcesses] = useState<ResourceProcess[]>([]);
  /*
   * CDXC:SidebarCollapse 2026-06-20-17:10:
   * The macOS titlebar Toggle Sidebar control should be a plain Tabler sidebar
   * glyph instead of the former blue traffic-light dot. Mirror the configured
   * sidebar placement so left sidebars use IconLayoutSidebar and right sidebars
   * use IconLayoutSidebarRight.
   */
  const SidebarCollapseIcon =
    projectState.sidebarSide === "right" ? IconLayoutSidebarRight : IconLayoutSidebar;
  const keepAwakeFeatureEnabled = projectState.keepAwake.featureEnabled === true;
  const [resourceServers, setResourceServers] = useState<ResourceListeningServer[]>([]);
  /*
   * CDXC:TitlebarResources 2026-06-11-18:13:
   * The native Resources child panel should not render zero-memory or missing-session rows while the first `ps` snapshot is still loading.
   * Track first-sample readiness separately from the process array so an intentionally empty process sample can render, while AppKit keeps the child window hidden until the first real sample is committed.
   */
  const [ resourceProcessSnapshotReady, setResourceProcessSnapshotReady ] = useState(false);
  const [collapsedResourceKeys, setCollapsedResourceKeys] = useState<Set<string>>(() => {
    /**
     * CDXC:TitlebarResources 2026-06-12-23:33:
     * Resource section containers stay visible; only individual row disclosures
     * collapse. Session and browser rows encode their default collapsed state by
     * omitting their expanded keys, so the explicit override set starts empty.
     */
    return new Set();
  });
  const [quittingResourceKeys, setQuittingResourceKeys] = useState<Set<string>>(() => new Set());
  const [optimisticMode, setOptimisticMode] = useState<TitlebarMode>();
  const rootRef = useRef<HTMLDivElement | null>(null);
  const resourceRefreshGenerationRef = useRef(0);
  const resourceRefreshInFlightRef = useRef(false);
  const resourcesOpenCollapseSeededRef = useRef(false);
  const titlebarEventLoopLastLogAtRef = useRef(0);
  /*
   * CDXC:TitlebarResources 2026-07-02-05:36:
   * refreshResources must keep a stable identity across project-state pushes.
   * When it depended on projectState.diagnosticLogging, the state hydrate that
   * arrives right after the Resources child panel loads re-ran the poll effect,
   * bumped the refresh generation, and discarded the first process snapshot —
   * so titlebarDropdownPanelReady was never posted and AppKit kept the panel
   * hidden. Diagnostics read the latest settings through this ref instead.
   */
  const diagnosticLoggingRef = useRef(projectState.diagnosticLogging);
  diagnosticLoggingRef.current = projectState.diagnosticLogging;
  const activeMode = optimisticMode ?? projectState.activeMode;
  const resourcesPanelActive = titlebarPanelKind === "resources";
  const resourceViews = useMemo(
    () =>
      resourcesPanelActive
        ? createResourceGroupViews(
            projectState.browserTabs,
            projectState.resourceGroups,
            resourceProcesses,
            resourceServers,
            projectState.codeEditorProjectIds,
          )
        : EMPTY_RESOURCE_GROUP_VIEWS,
    [
      projectState.browserTabs,
      projectState.codeEditorProjectIds,
      projectState.resourceGroups,
      resourceProcesses,
      resourceServers,
      resourcesPanelActive,
    ],
  );
  const resourceProcessTotals = useMemo(
    () =>
      resourcesPanelActive
        ? createGhostexResourceProcessTotals(resourceProcesses)
        : EMPTY_RESOURCE_PROCESS_TOTALS,
    [resourceProcesses, resourcesPanelActive],
  );
  const resourceServerBundles = useMemo(
    () =>
      resourcesPanelActive
        ? createResourceServerBundles(resourceServers, resourceViews, resourceProcesses, projectState.portless)
        : [],
    [projectState.portless, resourceProcesses, resourceServers, resourceViews, resourcesPanelActive],
  );
  const inactiveTerminalSleepSessionIds = useMemo(
    () => createInactiveTerminalSleepSessionIds(projectState.resourceGroups),
    [projectState.resourceGroups],
  );
  const unreadTips = useMemo(
    () => TITLEBAR_TIPS.filter((tip) => !readTipIds.has(tip.id)),
    [readTipIds],
  );
  const readTips = useMemo(
    () => TITLEBAR_TIPS.filter((tip) => readTipIds.has(tip.id)),
    [readTipIds],
  );
  const missingAgentHooksNotice = useMemo(
    () => createTitlebarMissingAgentHooksNotice(projectState.resourceGroups, projectState.agentHookStatus),
    [projectState.agentHookStatus, projectState.resourceGroups],
  );
  const ghostexCliNotice = useMemo(
    () => createTitlebarGhostexCliNotice(projectState.ghostexCliStatus),
    [projectState.ghostexCliStatus],
  );
  const notices = useMemo(
    () => [
      ...(ghostexCliNotice ? [ghostexCliNotice] : []),
      ...(projectState.sessionPersistenceProvider === "off"
        ? [TITLEBAR_PERSISTENCE_OFF_NOTICE]
        : []),
      ...(projectState.debuggingMode ? [TITLEBAR_DEBUGGING_MODE_NOTICE] : []),
      ...(missingAgentHooksNotice ? [missingAgentHooksNotice] : []),
    ],
    [
      ghostexCliNotice,
      missingAgentHooksNotice,
      projectState.debuggingMode,
      projectState.sessionPersistenceProvider,
    ],
  );
  const markTipRead = useCallback((tipId: string) => {
    setReadTipIds((current) => {
      if (current.has(tipId)) {
        return current;
      }
      const next = new Set(current);
      next.add(tipId);
      writeStoredTitlebarTipIds(next);
      return next;
    });
  }, []);
  const requestRuntimeStatusForTips = useCallback(() => {
    postTitlebarSidebarCommand({ type: "requestAgentHookStatus" });
    postTitlebarSidebarCommand({ type: "requestGhostexCliStatus" });
  }, []);
  const openHighlightedFeaturesFromTips = useCallback(() => {
    /*
     * CDXC:TipsAndTricks 2026-06-16-08:17:
     * The Tips & Tricks header should send users to the replayable highlighted
     * features modal instead of exposing a bulk "Read all" action.
     *
     * CDXC:GhostexTutorialVideo 2026-06-18-05:31:
     * The Tips modal Video button should open the tutorial video modal. Leave the
     * old Highlighted Features modal unused instead of deleting its implementation.
     */
    postTitlebarSidebarCommand({ type: "openGhostexTutorialVideo" });
  }, []);
  const viewGhostexGuideFromTips = useCallback(() => {
    /*
     * CDXC:TipsAndTricks 2026-06-16-10:04:
     * The Tips & Tricks header should send users to Video with a filled star
     * action and to the setup guide through a Setup action. Keep the
     * sidebar-owned workspace welcome bridge as the guide entry point because
     * that surface owns setup and onboarding repair.
     *
     * CDXC:TipsAndTricks 2026-06-18-04:53:
     * The setup action label should be the shorter "Setup" copy so the header
     * can also fit Docs, Video, and Updates without truncating action text.
     */
    postTitlebarSidebarCommand({ type: "openWorkspaceWelcome" });
  }, []);
  const openDocsFromTips = useCallback(() => {
    postTitlebarSidebarCommand({ type: "openBrowserPane", url: GHOSTEX_DOCS_URL });
  }, []);
  const openTipAction = useCallback((tip: TitlebarTip) => {
    const action = tip.action;
    if (!action) {
      return;
    }
    if (action.type === "openSettings") {
      /*
       * CDXC:TipsAndTricks 2026-06-28-08:00:
       * Clickable Ghostex skill tips should open Settings > Integrations with
       * the skill name searched so users land on the install/configure detail
       * instead of a generic setup page.
       */
      window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
        initialSearchQuery: action.settingsSearchQuery,
        initialTab: "integrations",
        modal: "settings",
        type: "open",
      });
      return;
    }
    postTitlebarSidebarCommand({ type: "openBrowserPane", url: action.url });
  }, []);
  const openChangelogFromTips = useCallback(() => {
    /*
     * CDXC:TipsAndTricks 2026-06-16-19:42:
     * The Tips & Tricks header should expose the release changelog on the far
     * right and open it as a normal current-project browser session.
     */
    postTitlebarSidebarCommand({ type: "openBrowserPane", url: GHOSTEX_CHANGELOG_URL });
  }, []);
  const syncKeepAwakeRuntimeState = useCallback(
    (syncState: KeepAwakeRuntimeSyncState | undefined) => {
      if (syncState && Object.prototype.hasOwnProperty.call(syncState, "runtime")) {
        /*
         * CDXC:TitlebarKeepAwake 2026-06-23-19:36:
         * Native child dropdowns send the committed Keep Awake runtime directly into the main titlebar bridge. Treat an explicit null runtime as a committed stop so stale localStorage in another WKWebView cannot keep the titlebar icon active.
         */
        setKeepAwakeRuntime(syncState.runtime ?? undefined);
        setKeepAwakeAutoStartSuppressed(syncState.suppressAutoStart === true);
        return;
      }
      if (syncState?.suppressAutoStart === true) {
        setKeepAwakeRuntime(undefined);
        setKeepAwakeAutoStartSuppressed(true);
        return;
      }
      const storedRuntime = readStoredKeepAwakeRuntime();
      setKeepAwakeRuntime(storedRuntime);
      if (syncState?.suppressAutoStart === false || storedRuntime) {
        setKeepAwakeAutoStartSuppressed(false);
      }
    },
    [],
  );
  const closeTitlebarDropdownPanel = useCallback(() => {
    postNative({ type: "closeTitlebarDropdownPanel" });
    setNativeDropdownOpen(undefined);
  }, []);
  useEffect(() => {
    if (!isDropdownPanel) {
      return;
    }

    const closePanelWhenNativeFocusLeaves = () => {
      /*
       * GPUI titlebar panels are native CEF siblings of the app's normal
       * workspace surfaces. A click in the sidebar, browser, terminal, or
       * GPUI shell blurs this browsing context. Close from that exact surface
       * lifecycle instead of installing a broad native mouse monitor.
       */
      closeTitlebarDropdownPanel();
    };

    window.addEventListener("blur", closePanelWhenNativeFocusLeaves);
    return () => {
      window.removeEventListener("blur", closePanelWhenNativeFocusLeaves);
    };
  }, [closeTitlebarDropdownPanel, isDropdownPanel]);
  const showTitlebarDropdownPanel = useCallback(
    (
      kind: TitlebarDropdownPanelKind,
      anchor: HTMLElement,
      options: { closeWhenAlreadyOpen?: boolean } = {},
    ) => {
      /*
       * CDXC:ReactTitlebar 2026-06-11-23:20:
       * Native child-window dropdown triggers should behave like normal menu
       * buttons: requesting the already-open panel closes it instead of
       * reopening or repositioning the same child window.
       *
       * CDXC:TitlebarKeepAwake 2026-06-15-23:25:
       * Keep Awake is a dropdown launcher, not a direct start/stop toggle.
       *
       * CDXC:TitlebarKeepAwake 2026-06-15-23:25:
       * Clicking Keep Awake again while its dropdown is open should close the
       * menu like the other titlebar dropdown triggers.
       */
      if (nativeDropdownOpen === kind && options.closeWhenAlreadyOpen !== false) {
        closeTitlebarDropdownPanel();
        return false;
      }
      const anchorElement =
        anchor.closest<HTMLElement>("[data-titlebar-dropdown-anchor]") ?? anchor;
      const rect = anchorElement.getBoundingClientRect();
      /*
       * CDXC:ReactTitlebar 2026-06-11-13:22:
       * Dropdown content must open as a native child window, not as Radix content
       * portaled below the titlebar WKWebView. Send only the titlebar-strip anchor
       * rectangle so Swift owns screen placement while React keeps rendering the
       * existing menu surface inside the child window.
       */
      setNativeDropdownOpen(kind);
      postNative({
        anchorRect: {
          height: rect.height,
          width: rect.width,
          x: rect.x,
          y: rect.y,
        },
        kind,
        preferredSize: dropdownPanelSizeResolverRef.current(kind),
        type: "showTitlebarDropdownPanel",
      });
      return true;
    },
    [closeTitlebarDropdownPanel, nativeDropdownOpen],
  );
  const openTipsMenuFromTitlebar = useCallback((event: { currentTarget: HTMLElement }) => {
    const didOpen = showTitlebarDropdownPanel("tips", event.currentTarget);
    if (didOpen) {
      requestRuntimeStatusForTips();
    }
  }, [requestRuntimeStatusForTips, showTitlebarDropdownPanel]);

  const requestTitlebarBlankMouseDown = useCallback(
    (event: ReactMouseEvent<HTMLDivElement>) => {
      if (isDropdownPanel || event.button !== 0 || event.defaultPrevented) {
        return;
      }
      const target = event.target;
      if (!(target instanceof Element)) {
        return;
      }
      /*
       * CDXC:ReactTitlebar 2026-06-13-14:08:
       * Blank titlebar drag should use normal DOM event ownership instead of
       * native coordinate hit regions. Interactive controls stop here by their
       * element semantics; passive titlebar text and empty background ask the
       * native WKWebView to drag the current mouseDown event.
       */
      if (
        target.closest(
          'button,a,input,textarea,select,[role="button"],[contenteditable="true"],[data-titlebar-dropdown-anchor]',
        )
      ) {
        return;
      }
      if (nativeDropdownOpen) {
        /*
         * CDXC:ReactTitlebar 2026-06-23-19:36:
         * Clicking blank titlebar chrome while a native child dropdown is open should dismiss that dropdown instead of starting a window drag. Keep this in the titlebar DOM mouse handler so AppKit does not need broad click rerouting or overlapping hit-test regions.
         */
        event.preventDefault();
        closeTitlebarDropdownPanel();
        return;
      }
      event.preventDefault();
      postNative({ type: "titlebarBlankMouseDown" });
    },
    [closeTitlebarDropdownPanel, isDropdownPanel, nativeDropdownOpen],
  );

  useEffect(() => {
    const suppressTitlebarWebviewContextMenu = (event: MouseEvent) => {
      /**
       * CDXC:TitlebarContextMenu 2026-05-15-18:21:
       * Right-clicking titlebar buttons, menus, labels, or project text must
       * not expose WKWebView's native Reload menu. The titlebar has no editable
       * text fields, so suppress the webview default for the whole isolated
       * titlebar document while leaving React click/keyboard behavior intact.
       */
      event.preventDefault();
    };

    document.addEventListener("contextmenu", suppressTitlebarWebviewContextMenu, true);
    return () => {
      document.removeEventListener("contextmenu", suppressTitlebarWebviewContextMenu, true);
    };
  }, []);

  useEffect(() => {
    if (isDropdownPanel) {
      return;
    }
    const compactModeMedia = window.matchMedia("(max-width: 1049px)");
    const closeModeMenuOutsideCompactWidth = () => {
      /**
       * CDXC:ModeSwitcher 2026-05-28-10:38:
       * The compact mode picker exists only below 1050px.
       *
       * CDXC:ReactTitlebar 2026-06-11-13:22:
       * Its dropdown now lives in a native child window; close that panel when
       * the trigger leaves the titlebar layout so no detached panel remains.
       */
      if (!compactModeMedia.matches && nativeDropdownOpen === "mode") {
        closeTitlebarDropdownPanel();
      }
    };
    closeModeMenuOutsideCompactWidth();
    compactModeMedia.addEventListener("change", closeModeMenuOutsideCompactWidth);
    return () => {
      compactModeMedia.removeEventListener("change", closeModeMenuOutsideCompactWidth);
    };
  }, [closeTitlebarDropdownPanel, isDropdownPanel, nativeDropdownOpen]);

  useEffect(() => {
    if (isDropdownPanel) {
      return;
    }
    const narrowTitlebarMedia = window.matchMedia("(max-width: 619.98px)");
    const closeMenusHiddenAtNarrowWidth = () => {
      /**
       * CDXC:ReactTitlebar 2026-05-29-16:05:
       * App widths below 620px hide the top-right Tips, Resources, and Keep
       * Awake controls.
       *
       * CDXC:ReactTitlebar 2026-06-11-13:22:
       * Those dropdowns are native child panels now, so close the panel when its
       * trigger leaves the visible titlebar instead of keeping an orphan window.
       */
      if (
        narrowTitlebarMedia.matches &&
        (nativeDropdownOpen === "keepAwake" ||
          nativeDropdownOpen === "resources" ||
          nativeDropdownOpen === "tips")
      ) {
        closeTitlebarDropdownPanel();
      }
    };
    closeMenusHiddenAtNarrowWidth();
    narrowTitlebarMedia.addEventListener("change", closeMenusHiddenAtNarrowWidth);
    return () => {
      narrowTitlebarMedia.removeEventListener("change", closeMenusHiddenAtNarrowWidth);
    };
  }, [closeTitlebarDropdownPanel, isDropdownPanel, nativeDropdownOpen]);

  const allTargets = useMemo(
    () => createConfiguredOpenTargets(projectState.workspaceOpenTargets),
    [projectState.workspaceOpenTargets],
  );
  const visibleTargets = useMemo(
    () => resolveVisibleOpenTargets(allTargets, projectState.workspaceOpenTargets.availability),
    [allTargets, projectState.workspaceOpenTargets.availability],
  );
  const activeTarget = visibleTargets.find((target) => target.id === selectedTargetId) ?? visibleTargets[0];
  const visibleActions = useMemo(
    () => projectState.sidebarActions.commands,
    [projectState.sidebarActions.commands],
  );
  const activeAction =
    visibleActions.find((command) => command.commandId === selectedActionCommandId) ??
    visibleActions[0];
  const gitPrimaryAction = useMemo(
    () => resolveSidebarGitPrimaryActionState(projectState.git),
    [projectState.git],
  );
  const gitMenuItems = useMemo(
    () => buildSidebarGitMenuItems(projectState.git),
    [projectState.git],
  );
  const publishTitlebarStripState = useCallback(() => {
    if (isDropdownPanel) {
      return;
    }
    /**
     * CDXC:ReactTitlebar 2026-06-13-13:33:
     * Native owns the titlebar as an exact WKWebView strip, while React owns
     * controls through normal DOM layout. Do not measure DOM hit rectangles for
     * AppKit; only publish strip-level overlay lifecycle state.
     */
    postNative({
      overlayOpen: titlebarOverlayOpen,
      type: "setReactTitlebarStripState",
    });
  }, [titlebarOverlayOpen, isDropdownPanel]);

  const publishSettledTitlebarStripState = useCallback(() => {
    publishTitlebarStripState();
    let secondFrame = 0;
    const firstFrame = window.requestAnimationFrame(() => {
      publishTitlebarStripState();
      secondFrame = window.requestAnimationFrame(publishTitlebarStripState);
    });
    const settledTimeout = window.setTimeout(publishTitlebarStripState, 120);
    return () => {
      window.cancelAnimationFrame(firstFrame);
      if (secondFrame !== 0) {
        window.cancelAnimationFrame(secondFrame);
      }
      window.clearTimeout(settledTimeout);
    };
  }, [publishTitlebarStripState]);

  useLayoutEffect(() => {
    if (isDropdownPanel) {
      return;
    }
    /**
     * CDXC:SessionFocusMode 2026-05-26-22:47:
     * The Exit focus button is conditional titlebar chrome. Publish the strip
     * lifecycle state after titlebar layout settles so native receives a fresh
     * titlebar document signal without DOM-region measuring.
     *
     * CDXC:AutoUpdate 2026-06-08-18:21:
     * The update button appears after native Sparkle appcast probes, so
     * updateAvailable must also republish the strip lifecycle state.
     */
    return publishSettledTitlebarStripState();
  }, [
    activeTarget?.id,
    activeAction?.commandId,
    keepAwakeRuntime?.pid,
    resourceProcesses.length,
    resourceServers.length,
    projectState.projectEditorCompanionPaneHidden,
    projectState.gxserverDaemon.state,
    projectState.projectIconDataUrl,
    projectState.isFocusModeActive,
    projectState.projectName,
    projectState.sidebarCollapsed,
    projectState.updateAvailable,
    projectState.updateDownloading,
    publishSettledTitlebarStripState,
    isDropdownPanel,
  ]);

  useEffect(() => {
    if (isDropdownPanel) {
      return;
    }
    window.addEventListener("resize", publishTitlebarStripState);
    return () => window.removeEventListener("resize", publishTitlebarStripState);
  }, [publishTitlebarStripState, isDropdownPanel]);

  useEffect(() => {
    if (isDropdownPanel) {
      return;
    }
    /*
     * CDXC:TooltipLifecycle 2026-06-13-02:30:
     * Native titlebar pointer-leave may hide a currently visible tooltip, but
     * DOM pointer movement inside the titlebar must immediately restore hover
     * eligibility. This keeps native tracking as cleanup, not a persistent gate
     * that waits for a titlebar ownership update.
     */
    const suppressTitlebarTooltips = () => {
      suppressTitlebarTooltipsFromDom();
    };
    const enableTitlebarTooltips = () => {
      enableTitlebarTooltipsFromDom();
    };
    const suppressWhenHidden = () => {
      if (document.visibilityState !== "visible") {
        suppressTitlebarTooltips();
      }
    };
    const suppressWhenPointerLeavesDocument = (event: MouseEvent | PointerEvent) => {
      const relatedTarget = event.relatedTarget;
      if (!(relatedTarget instanceof Node) || !document.documentElement.contains(relatedTarget)) {
        suppressTitlebarTooltips();
      }
    };

    window.addEventListener("blur", suppressTitlebarTooltips);
    window.addEventListener("pagehide", suppressTitlebarTooltips);
    document.addEventListener("visibilitychange", suppressWhenHidden);
    document.addEventListener("mouseout", suppressWhenPointerLeavesDocument, true);
    document.addEventListener("pointerout", suppressWhenPointerLeavesDocument, true);
    document.addEventListener("pointercancel", suppressTitlebarTooltips, true);
    document.addEventListener("mouseenter", enableTitlebarTooltips, true);
    document.addEventListener("pointerenter", enableTitlebarTooltips, true);
    document.addEventListener("pointermove", enableTitlebarTooltips, true);

    return () => {
      window.removeEventListener("blur", suppressTitlebarTooltips);
      window.removeEventListener("pagehide", suppressTitlebarTooltips);
      document.removeEventListener("visibilitychange", suppressWhenHidden);
      document.removeEventListener("mouseout", suppressWhenPointerLeavesDocument, true);
      document.removeEventListener("pointerout", suppressWhenPointerLeavesDocument, true);
      document.removeEventListener("pointercancel", suppressTitlebarTooltips, true);
      document.removeEventListener("mouseenter", enableTitlebarTooltips, true);
      document.removeEventListener("pointerenter", enableTitlebarTooltips, true);
      document.removeEventListener("pointermove", enableTitlebarTooltips, true);
      delete document.body.dataset.nativePointerInside;
    };
  }, [isDropdownPanel]);

  useEffect(() => {
    if (isDropdownPanel) {
      return;
    }
    return () => {
      /**
       * CDXC:ReactTitlebar 2026-05-25-10:09:
       * Native workspace shielding must clear when the titlebar host unmounts
       * or reloads. Publish an explicit closed overlay state instead of making
       * Swift infer it from stale DOM geometry.
       */
      postNative({
        overlayOpen: false,
        type: "setReactTitlebarStripState",
      });
    };
  }, [isDropdownPanel]);

  useEffect(() => {
    window.__ghostex_TITLEBAR__ = {
      closeOpenDropdowns: () => {
        /**
         * CDXC:ReactTitlebar 2026-05-16-20:01:
         * Native app content lives outside this titlebar WKWebView, so Radix
         * cannot observe normal outside clicks in the workspace/sidebar. Expose
         * one explicit close hook that AppKit can call before routing the click
         * to the real app surface behind an open dropdown.
         *
         * CDXC:ReactTitlebar 2026-06-11-13:22:
         * Titlebar dropdowns are now native child windows, so this bridge closes
         * the panel window instead of toggling in-document Radix menu state.
         */
        closeTitlebarDropdownPanel();
      },
      setNativePointerInside: setTitlebarNativePointerInside,
      setWindowFocused: setTitlebarWindowFocused,
      setNativeDropdownOpen,
      syncKeepAwakeRuntime: syncKeepAwakeRuntimeState,
      setLastActionCommandId: (commandId) => {
        /*
         * CDXC:TitlebarActions 2026-06-16-18:31:
         * Quick Actions run from the native dropdown panel must immediately
         * become the main titlebar button action. The dropdown is a separate
         * WKWebView, so native relays the chosen command id back into the main
         * titlebar bridge instead of waiting for a reload to reread localStorage.
         */
        setSelectedActionCommandId(commandId);
      },
      setActiveProjectState: (state) => {
        setProjectState((current) => {
          const next = mergeTitlebarProjectState(current, state);
          cacheTitlebarGitState(next);
          return next;
        });
      },
    };
    if (isRecord(window.__ghostex_PENDING_TITLEBAR_PROJECT_STATE__)) {
      window.__ghostex_TITLEBAR__.setActiveProjectState(window.__ghostex_PENDING_TITLEBAR_PROJECT_STATE__);
    }
    if (typeof window.__ghostex_PENDING_TITLEBAR_UPDATE_AVAILABLE__ === "boolean") {
      /**
       * CDXC:AutoUpdate 2026-06-08-18:21:
       * Native may detect an app update before this React bridge exists. Apply
       * the latest pending native boolean immediately after bridge installation
       * so the titlebar download button appears during startup instead of only
       * after a later 15-minute probe.
       */
      window.__ghostex_TITLEBAR__.setActiveProjectState({
        updateAvailable: window.__ghostex_PENDING_TITLEBAR_UPDATE_AVAILABLE__,
      });
    }
    if (typeof window.__ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOADING__ === "boolean") {
      /**
       * CDXC:AutoUpdate 2026-06-13-17:52:
       * Native may start the Sparkle download before this React bridge exists.
       * Apply the pending boolean immediately so the titlebar button begins
       * showing download state as soon as the document can render the current
       * updater state.
       *
       * CDXC:AutoUpdate 2026-06-30-22:18:
       * Apply the pending nullable progress ratio with the downloading boolean
       * so titlebar reloads preserve the circular fill and hover percent.
       */
      const pendingDownloadState: Partial<TitlebarProjectState> = {
        updateDownloading: window.__ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOADING__,
      };
      if (
        Object.prototype.hasOwnProperty.call(
          window,
          "__ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOAD_PROGRESS__",
        )
      ) {
        pendingDownloadState.updateDownloadProgress =
          window.__ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOAD_PROGRESS__ ?? null;
      }
      window.__ghostex_TITLEBAR__.setActiveProjectState(pendingDownloadState);
    }
    if (typeof window.__ghostex_PENDING_TITLEBAR_WINDOW_FOCUSED__ === "boolean") {
      setTitlebarWindowFocused(window.__ghostex_PENDING_TITLEBAR_WINDOW_FOCUSED__);
    }
    return () => {
      delete window.__ghostex_TITLEBAR__;
      delete document.body.dataset.windowFocused;
    };
  }, [closeTitlebarDropdownPanel, syncKeepAwakeRuntimeState]);

  useEffect(() => {
    setSelectedActionCommandId(readLastActionCommandId(projectState));
  }, [projectState.projectId, projectState.projectPath]);

  useEffect(() => {
    setOptimisticMode(undefined);
  }, [projectState.activeMode, projectState.projectId, projectState.projectPath]);

  useEffect(() => {
    const handleHostEvent = (event: Event) => {
      const hostEvent = (event as CustomEvent<NativeHostEvent>).detail;
      if (hostEvent?.type !== "processResult") {
        return;
      }
      const pending = pendingProcessResults.get(hostEvent.requestId);
      if (!pending) {
        return;
      }
      window.clearTimeout(pending.timeout);
      pendingProcessResults.delete(hostEvent.requestId);
      pending.resolve(hostEvent);
    };
    window.addEventListener("ghostex-native-host-event", handleHostEvent);
    return () => window.removeEventListener("ghostex-native-host-event", handleHostEvent);
  }, []);

  useEffect(() => {
    if (!isDiagnosticLoggingScenarioEnabled(projectState.diagnosticLogging, "native.chrome.responsiveness")) {
      return;
    }
    /*
     * CDXC:ChromeResponsivenessDiagnostics 2026-06-30-23:52:
     * When the titlebar buttons stop responding, the isolated titlebar React
     * event loop may have stalled before WebKit terminates. Sample coarse timer
     * drift only while the targeted diagnostic scenario is enabled, and throttle
     * writes so the watchdog cannot become another source of lag.
     */
    let expectedAtMs = performance.now() + TITLEBAR_EVENT_LOOP_WATCHDOG_INTERVAL_MS;
    const interval = window.setInterval(() => {
      const nowMs = performance.now();
      const driftMs = nowMs - expectedAtMs;
      expectedAtMs = nowMs + TITLEBAR_EVENT_LOOP_WATCHDOG_INTERVAL_MS;
      if (driftMs < TITLEBAR_EVENT_LOOP_STALL_THRESHOLD_MS) {
        return;
      }
      if (nowMs - titlebarEventLoopLastLogAtRef.current < TITLEBAR_EVENT_LOOP_STALL_LOG_THROTTLE_MS) {
        return;
      }
      titlebarEventLoopLastLogAtRef.current = nowMs;
      appendTitlebarChromeResponsivenessDebugLog(
        projectState.diagnosticLogging,
        "nativeChrome.titlebar.eventLoopStall",
        {
          driftMs: Math.round(driftMs),
          intervalMs: TITLEBAR_EVENT_LOOP_WATCHDOG_INTERVAL_MS,
          resourceProcessCount: resourceProcesses.length,
          resourceRefreshInFlight: resourceRefreshInFlightRef.current,
          resourceServerCount: resourceServers.length,
          resourcesPanelActive,
          snapshotReady: resourceProcessSnapshotReady,
          titlebarPanelKind: titlebarPanelKind ?? "main",
        },
      );
    }, TITLEBAR_EVENT_LOOP_WATCHDOG_INTERVAL_MS);
    return () => window.clearInterval(interval);
  }, [
    projectState.diagnosticLogging,
    resourceProcesses.length,
    resourceProcessSnapshotReady,
    resourceServers.length,
    resourcesPanelActive,
    titlebarPanelKind,
  ]);

  const refreshResources = useCallback(async (generation: number) => {
    if (resourceRefreshInFlightRef.current) {
      appendTitlebarChromeResponsivenessDebugLog(
        diagnosticLoggingRef.current,
        "nativeChrome.titlebar.resourcesRefresh.skippedInFlight",
        {
          generationCurrent: generation === resourceRefreshGenerationRef.current,
          resourcesPanelActive,
        },
      );
      return;
    }
    resourceRefreshInFlightRef.current = true;
    const startedAtMs = performance.now();
    try {
      const [processes, servers] = await Promise.all([
        readResourceProcesses(),
        readResourceListeningServers(),
      ]);
      const elapsedMs = Math.round(performance.now() - startedAtMs);
      appendTitlebarChromeResponsivenessDebugLog(
        diagnosticLoggingRef.current,
        "nativeChrome.titlebar.resourcesRefresh.finished",
        {
          elapsedMs,
          generationCurrent: generation === resourceRefreshGenerationRef.current,
          processCount: processes.length,
          resourcesPanelActive,
          serverCount: servers.length,
        },
      );
      if (generation === resourceRefreshGenerationRef.current) {
        setResourceProcesses(processes);
        setResourceServers(servers);
        setResourceProcessSnapshotReady(true);
      }
    } catch (error) {
      appendTitlebarChromeResponsivenessDebugLog(
        diagnosticLoggingRef.current,
        "nativeChrome.titlebar.resourcesRefresh.failed",
        {
          elapsedMs: Math.round(performance.now() - startedAtMs),
          errorName: error instanceof Error ? error.name : typeof error,
          generationCurrent: generation === resourceRefreshGenerationRef.current,
          resourcesPanelActive,
        },
      );
      console.warn("Failed to refresh Ghostex resources", error);
      if (generation === resourceRefreshGenerationRef.current) {
        setResourceProcessSnapshotReady(true);
      }
    } finally {
      resourceRefreshInFlightRef.current = false;
    }
  }, [resourcesPanelActive]);

  useEffect(() => {
    if (!resourcesPanelActive) {
      return;
    }
    /**
     * CDXC:TitlebarResources 2026-05-16-16:08:
     * The Resources dropdown should show live process CPU and memory without a
     * native push channel. Poll `ps` only while the wide dropdown is open so
     * the compact titlebar does not spend idle work on hidden diagnostics.
     *
     * CDXC:TitlebarResources 2026-06-07-16:20:
     * Hidden Resources UI should hold no sampled process table and should never
     * stack overlapping `ps` runs. Treat each open as a generation so slow native
     * process replies cannot repopulate closed-menu state.
     *
     * CDXC:TitlebarResources 2026-06-11-18:13:
     * Each native dropdown open clears readiness so AppKit waits for the current
     * first process sample before revealing the Resources child window.
     */
    const generation = resourceRefreshGenerationRef.current + 1;
    resourceRefreshGenerationRef.current = generation;
    setResourceProcessSnapshotReady(false);
    void refreshResources(generation);
    const interval = window.setInterval(() => {
      void refreshResources(generation);
    }, RESOURCE_POLL_INTERVAL_MS);
    return () => {
      window.clearInterval(interval);
      resourceRefreshGenerationRef.current += 1;
      setResourceProcessSnapshotReady(false);
      setResourceProcesses((current) => current.length === 0 ? current : []);
      setResourceServers((current) => current.length === 0 ? current : []);
    };
  }, [refreshResources, resourcesPanelActive]);

  useEffect(() => {
    if (titlebarPanelKind !== "resources" || !resourceProcessSnapshotReady) {
      return;
    }
    /*
     * CDXC:TitlebarResources 2026-06-11-18:13:
     * The native Resources panel is loaded offscreen until React has committed
     * the first real process snapshot. Report readiness from an effect so AppKit
     * orders the child window onscreen after the non-loading content is painted.
     */
    postNative({ kind: "resources", type: "titlebarDropdownPanelReady" });
  }, [resourceProcessSnapshotReady, titlebarPanelKind]);

  useLayoutEffect(() => {
    if (!resourcesPanelActive) {
      resourcesOpenCollapseSeededRef.current = false;
      return;
    }
    if (resourcesOpenCollapseSeededRef.current) {
      return;
    }
    const resourceItemCollapseTargets = createResourceViewItemCollapseTargets(resourceViews, resourceServerBundles);
    if (resourceItemCollapseTargets.length === 0) {
      return;
    }
    /**
     * CDXC:TitlebarResources 2026-06-13-02:02:
     * Each Resources modal open should begin with every expandable item row
     * collapsed, then show the expand action because all rows are collapsed.
     * Do this once per open in a layout effect after the dynamic process
     * snapshot has row targets, before the Resources child window receives its
     * ready signal. Keep Projects, Browser Tabs, and Orphaned / Detached visible
     * as top-level sections.
     */
    resourcesOpenCollapseSeededRef.current = true;
    setCollapsedResourceKeys((current) =>
      applyResourceItemCollapsedState(current, resourceItemCollapseTargets, true),
    );
  }, [ resourceServerBundles, resourceViews, resourcesPanelActive ]);

  const openTarget = (target: ResolvedOpenTarget | undefined) => {
    if (!target || !projectState.projectPath) {
      return;
    }
    setSelectedTargetId(target.id);
    localStorage.setItem(LAST_OPEN_TARGET_STORAGE_KEY, target.id);
    if (target.id === "finder") {
      postNative({ type: "openWorkspaceInFinder", workspacePath: projectState.projectPath });
      return;
    }
    if (target.kind === "built-in") {
      const targetApp = target.definition.targetApp;
      if (targetApp && target.resolvedCommand) {
        postNative({
          targetApp,
          type: "openWorkspaceInIde",
          workspacePath: projectState.projectPath,
        });
        return;
      }
      const command = target.resolvedCommand ?? target.definition.commands?.[0];
      if (target.resolvedCommand) {
        void runNativeProcess("/usr/bin/env", [
          target.resolvedCommand,
          ...(target.definition.baseArgs ?? []),
          projectState.projectPath,
        ]);
      } else if (target.resolvedAppName) {
        void runNativeProcess("/usr/bin/open", ["-a", target.resolvedAppName, projectState.projectPath]);
      } else if (command) {
        void runNativeProcess("/usr/bin/env", [
          command,
          ...(target.definition.baseArgs ?? []),
          projectState.projectPath,
        ]);
      }
      return;
    }
    void runNativeProcess("/usr/bin/env", [
      target.command,
      ...target.custom.args,
      projectState.projectPath,
    ]);
  };

  const openSidebarActionsSettings = () => {
    /*
    CDXC:ProjectActions 2026-06-15-15:29:
    Empty or unconfigured titlebar Actions clicks should open Settings on the Actions page instead of showing the removed standalone Configure Action modal.
    */
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarActionsSettings");
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      initialTab: "actions",
      modal: "settings",
      type: "open",
    });
  };

  const runSidebarAction = (command: SidebarCommandButton | undefined) => {
    if (!command) {
      openSidebarActionsSettings();
      return;
    }
    if (!isSidebarCommandConfigured(command)) {
      openSidebarActionsSettings();
      return;
    }
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarAction");
    appendTitlebarActionCrashDebugLog(
      projectState.diagnosticLogging,
      "nativeSidebar.actionCrashTrace.titlebarClick",
      {
        actionType: command.actionType,
        closeTerminalOnExit: command.closeTerminalOnExit,
        commandId: command.commandId,
        hasCommand: Boolean(command.command?.trim()),
        hasUrl: Boolean(command.url?.trim()),
        projectId: projectState.projectId,
        projectPath: projectState.projectPath,
      },
    );
    setSelectedActionCommandId(command.commandId);
    persistLastActionCommandId(projectState, command.commandId);
    postNative({ commandId: command.commandId, type: "runSidebarCommandFromTitlebar" });
  };

  const runGitAction = (action: SidebarGitAction) => {
    /*
     * CDXC:TitlebarGit 2026-06-16-18:41:
     * If the Commits row shows no remote delta, a stale titlebar child-window
     * click should be inert instead of starting an unnecessary pull/push flow.
     */
    if (action === "syncRemote" && !hasSidebarGitRemoteCommitDelta(projectState.git)) {
      return;
    }
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarGitAction");
    postNative({ action, type: "runSidebarGitActionFromTitlebar" });
  };
  const openTitlebarSettingsMenuSettings = () => {
    /*
     * CDXC:SidebarTopChrome 2026-06-29-01:43:
     * The visible Settings menu moved from the titlebar into the sidebar shortcut row. Keep this titlebar-panel compatibility route on the same app-modal host so any existing native child-window path still opens Settings as the native modal surface.
     */
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarSettingsMenu");
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      modal: "settings",
      type: "open",
    });
  };

  const openTitlebarSettingsMenuCommands = () => {
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarCommandsMenu");
    openQuickAccess("commands");
  };

  const openTitlebarSettingsMenuHotkeys = () => {
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarHotkeysMenu");
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      modal: "hotkeys",
      type: "open",
    });
  };

  const wakePetFromTitlebarSettingsMenu = () => {
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarWakePetMenu");
    postNative({ type: "togglePetOverlayFromTitlebar" });
  };

  const openTitlebarSettingsMenuDiscord = () => {
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarDiscordMenu");
    postNative({ type: "openExternalUrl", url: GHOSTEX_DISCORD_URL });
  };
  const openGitMenuFromTitlebar = useCallback(
    (event: { currentTarget: HTMLElement }) => {
      postTitlebarSidebarCommand({ type: "refreshGitState" });
      showTitlebarDropdownPanel("git", event.currentTarget);
    },
    [showTitlebarDropdownPanel],
  );

  const toggleResourceCollapse = (key: string) => {
    setCollapsedResourceKeys((current) => {
      const next = new Set(current);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  const setResourceItemsCollapsed = (
    targets: readonly ResourceItemCollapseTarget[],
    collapsed: boolean,
  ) => {
    setCollapsedResourceKeys((current) => applyResourceItemCollapsedState(current, targets, collapsed));
  };

  const focusResourceSession = (sessionId: string) => {
    /**
     * CDXC:TitlebarResources 2026-05-28-10:39:
     * Resources rows need a direct Focus action so users can jump from process
     * diagnostics to the owning session without using the sidebar. Close the
     * dropdown after forwarding the durable combined session id to the sidebar
     * owner, which already handles cross-project and sleeping-session focus.
     *
     * CDXC:TitlebarResources 2026-06-13-02:13:
     * Focus must visibly leave Resources after dispatching the sidebar focus
     * command. The native child window otherwise stays open over the newly
     * focused workspace, making a successful focus request look inert.
     */
    postNative({ sessionId, type: "focusResourceSessionFromTitlebar" });
    closeTitlebarDropdownPanel();
  };

  const quitResourceBundles = (bundles: ResourceProcessBundle[]) => {
    const uniqueBundles = uniqueResourceBundles(bundles).filter(isResourceBundleActionable);
    if (uniqueBundles.length === 0) {
      return;
    }
    /**
     * CDXC:TitlebarResources 2026-05-21-16:38:
     * Any Quit action in the resource manager should immediately mark the row
     * as closing and move it below active resources. Sidebar-owned terminal
     * sessions sleep through sidebar state so their cards remain resumable;
     * non-terminal panes and detached process bundles still use their resource
     * cleanup paths.
     *
     * CDXC:TitlebarResources 2026-05-23-10:46:
     * The resource manager must not rely on sidebar sleep as the only kill
     * mechanism. It also terminates the PIDs currently shown in the dropdown so
     * row Quit, group Quit, and Sleep All actually release RAM while the
     * sidebar keeps durable terminal sessions.
     *
     * CDXC:TitlebarResources 2026-06-22-00:30:
     * Server Stop rows should interrupt only listener-backed server process trees.
     * They intentionally skip sidebar session/project close commands so the
     * terminal that launched the server remains available after the port stops.
     */
    setQuittingResourceKeys((current) => {
      const next = new Set(current);
      uniqueBundles.forEach((bundle) => next.add(bundle.key));
      return next;
    });
    const sessionIds = uniqueBundles.flatMap(resourceBundleSidebarSessionIds);
    const projectIds = uniqueBundles.flatMap(resourceBundleProjectEditorIds);
    if (sessionIds.length > 0 || projectIds.length > 0) {
      postNative({
        projectIds: Array.from(new Set(projectIds)),
        sessionIds: Array.from(new Set(sessionIds)),
        type: "quitResourcesFromTitlebar",
      });
    }
    const processByPid = new Map(resourceProcesses.map((process) => [process.pid, process]));
    const processes = Array.from(
      new Map(
        uniqueBundles
          .flatMap((bundle) => bundle.pids)
          .map((pid) => processByPid.get(pid))
          .filter((process): process is ResourceProcess => process !== undefined)
          .map((process) => [process.pid, process]),
      ).values(),
    );
    const resourceRefreshGeneration = resourceRefreshGenerationRef.current;
    if (processes.length > 0) {
      const gracefulSignal = uniqueBundles.every((bundle) => bundle.type === "server") ? "INT" : "TERM";
      void terminateResourceProcesses(processes, { gracefulSignal }).finally(() => {
        window.setTimeout(() => {
          void refreshResources(resourceRefreshGeneration);
        }, 1_800);
      });
      return;
    }
    window.setTimeout(() => {
      void refreshResources(resourceRefreshGeneration);
    }, 250);
  };

  const sleepInactiveTerminalSessions = () => {
    if (inactiveTerminalSleepSessionIds.length === 0) {
      return;
    }
    postNative({
      sessionIds: inactiveTerminalSleepSessionIds,
      type: "sleepInactiveSessionsFromTitlebar",
    });
  };

  const startGxserverDaemon = () => {
    postNative({ type: "startGxserverFromTitlebar" });
  };

  const stopGxserverDaemon = () => {
    postNative({ type: "stopGxserverFromTitlebar" });
  };

  const restartGxserverDaemon = () => {
    postNative({ type: "restartGxserverFromTitlebar" });
  };

  const setGxserverAlwaysStart = (enabled: boolean) => {
    postNative({ enabled, type: "setGxserverAlwaysStartFromTitlebar" });
  };

  const stopKeepAwake = useCallback(async (options: { suppressAutoStart?: boolean } = {}) => {
    const runtime = keepAwakeRuntime;
    setKeepAwakeRuntime(undefined);
    localStorage.removeItem(KEEP_AWAKE_RUNTIME_STORAGE_KEY);
    if (options.suppressAutoStart !== false) {
      setKeepAwakeAutoStartSuppressed(true);
    }
    const syncState = {
      runtime: null,
      suppressAutoStart: options.suppressAutoStart !== false,
    };
    publishKeepAwakeRuntimeSync(syncState);
    syncKeepAwakeRuntimeToMainTitlebar(syncState);
    if (!runtime) {
      return;
    }
    try {
      await runNativeProcess("/bin/kill", [String(runtime.pid)]);
    } catch (error) {
      console.warn("Failed to stop keep-awake process", error);
    }
  }, [keepAwakeRuntime]);

  const startKeepAwake = useCallback(
    async (
      durationMinutes: KeepAwakeDurationMinutes = projectState.keepAwake.defaultDurationMinutes,
      options: { source?: KeepAwakeRuntimeState["source"] } = {},
    ) => {
      if (!keepAwakeFeatureEnabled) {
        setKeepAwakeAutoStartSuppressed(true);
        return;
      }
      if (keepAwakeRuntime) {
        await stopKeepAwake({ suppressAutoStart: false });
      }
      /**
       * CDXC:TitlebarKeepAwake 2026-05-28-19:28:
       * The normal keep-awake button should prevent idle sleep and AC system sleep.
       * Lid-close sleep is controlled by the separate Settings toggle because macOS does not treat it as a regular caffeinate idle-sleep assertion.
       */
      setKeepAwakeAutoStartSuppressed(false);
      const flags = projectState.keepAwake.allowDisplaySleep ? "-is" : "-dis";
      const timeout = durationMinutes > 0 ? ` -t ${durationMinutes * 60}` : "";
      const result = await runNativeProcess("/bin/sh", [
        "-lc",
        `(/usr/bin/nohup /usr/bin/caffeinate ${flags}${timeout} >/dev/null 2>&1 & echo $!)`,
      ]);
      const pid = Number(result.stdout.trim().split(/\s+/u)[0]);
      if (result.exitCode !== 0 || !Number.isFinite(pid) || pid <= 0) {
        console.warn("Failed to start keep-awake process", result.stderr || result.stdout);
        return;
      }
      const nextRuntime: KeepAwakeRuntimeState = {
        durationMinutes,
        fireAtMs: durationMinutes > 0 ? Date.now() + durationMinutes * 60_000 : undefined,
        pid,
        source: options.source ?? "manual",
        startedAtMs: Date.now(),
      };
      setKeepAwakeRuntime(nextRuntime);
      localStorage.setItem(KEEP_AWAKE_RUNTIME_STORAGE_KEY, JSON.stringify(nextRuntime));
      const syncState = { runtime: nextRuntime, suppressAutoStart: false };
      publishKeepAwakeRuntimeSync(syncState);
      syncKeepAwakeRuntimeToMainTitlebar(syncState);
    },
    [
      keepAwakeFeatureEnabled,
      keepAwakeRuntime,
      projectState.keepAwake.allowDisplaySleep,
      projectState.keepAwake.defaultDurationMinutes,
      stopKeepAwake,
    ],
  );

  useEffect(() => {
    if (isDropdownPanel || !window.__ghostex_TITLEBAR__) {
      return undefined;
    }
    const runKeepAwakeCommand = (command: TitlebarKeepAwakeCommand) => {
      /*
       * CDXC:SidebarTopChrome 2026-06-29-01:43:
       * Keep Awake moved from the titlebar trigger strip into the sidebar shortcut row. Keep this bridge as the only sidebar entry point so the titlebar host remains the single owner of caffeinate start/stop and runtime sync.
       */
      if (command.action === "stop") {
        void stopKeepAwake();
        return;
      }
      void startKeepAwake(command.durationMinutes);
    };
    window.__ghostex_TITLEBAR__.runKeepAwakeCommand = runKeepAwakeCommand;
    return () => {
      if (window.__ghostex_TITLEBAR__?.runKeepAwakeCommand === runKeepAwakeCommand) {
        delete window.__ghostex_TITLEBAR__.runKeepAwakeCommand;
      }
    };
  }, [isDropdownPanel, startKeepAwake, stopKeepAwake]);

  const openPowerSettings = () => {
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      initialSection: "power",
      modal: "settings",
      type: "open",
    });
  };

  const openSessionPersistenceSettings = () => {
    /**
     * CDXC:SessionPersistence 2026-06-04-02:52:
     * The persistence-off Tips notice is an actionable warning. Clicking it
     * should open the Ghostty/Terminal settings tab and pre-fill search with
     * the exact setting label so users land on Session Persistence immediately.
     */
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      initialSearchQuery: "Session Persistence",
      initialTab: "ghostty",
      modal: "settings",
      type: "open",
    });
  };

  const openAgentHooksSettings = () => {
    /**
     * CDXC:AgentHooks 2026-06-23-05:09:
     * The missing-hook Tips warning should deep-link to Settings > Integrations
     * and search for Agent Hooks instead of installing directly from titlebar
     * chrome, so users land on the provider-specific status and install control.
     */
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      initialSearchQuery: "Agent Hooks",
      initialTab: "integrations",
      modal: "settings",
      type: "open",
    });
  };

  const openDebuggingModeSettings = () => {
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      initialSearchQuery: "Debug logging and UI",
      initialTab: "settings",
      modal: "settings",
      type: "open",
    });
  };

  const openGhostexCliSettings = () => {
    /**
     * CDXC:CliInstall 2026-06-07-15:26:
     * The CLI-not-accessible Tips notice should deep-link to Settings where
     * Repair CLI lives, so the notice is actionable without adding titlebar
     * install controls.
     */
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      initialSearchQuery: "Ghostex CLI",
      initialTab: "integrations",
      modal: "settings",
      type: "open",
    });
  };

  const handleNoticeAction = (notice: TitlebarNotice) => {
    const target = notice.settingsTarget;
    if (target === "agentHooks") {
      openAgentHooksSettings();
      return;
    }
    if (target === "debuggingMode") {
      openDebuggingModeSettings();
      return;
    }
    if (target === "ghostexCli") {
      openGhostexCliSettings();
      return;
    }
    openSessionPersistenceSettings();
  };

  useEffect(() => {
    const handleStorage = (event: StorageEvent) => {
      if (
        event.key !== KEEP_AWAKE_RUNTIME_STORAGE_KEY &&
        event.key !== KEEP_AWAKE_RUNTIME_SYNC_STORAGE_KEY
      ) {
        return;
      }
      if (event.key === KEEP_AWAKE_RUNTIME_STORAGE_KEY && event.newValue === null) {
        return;
      }
      syncKeepAwakeRuntimeState(
        event.key === KEEP_AWAKE_RUNTIME_SYNC_STORAGE_KEY
          ? readKeepAwakeRuntimeSyncState(event.newValue)
          : undefined,
      );
    };
    const handleLocalSync = (event: Event) => {
      syncKeepAwakeRuntimeState(
        event instanceof CustomEvent ? event.detail as KeepAwakeRuntimeSyncState : undefined,
      );
    };
    /*
     * CDXC:TitlebarKeepAwake 2026-06-15-10:12:
     * The keep-awake dropdown renders in a native child titlebar window. Runtime changes from that child must update the main titlebar immediately and explicit Don't keep awake must suppress launch/display auto-start for this app run until the user starts keep-awake again.
     */
    window.addEventListener("storage", handleStorage);
    window.addEventListener(KEEP_AWAKE_RUNTIME_CHANGED_EVENT, handleLocalSync);
    return () => {
      window.removeEventListener("storage", handleStorage);
      window.removeEventListener(KEEP_AWAKE_RUNTIME_CHANGED_EVENT, handleLocalSync);
    };
  }, [syncKeepAwakeRuntimeState]);

  useEffect(() => {
    /*
     * CDXC:ExperimentalFeatures 2026-06-28-07:41:
     * Keep Awake is gated by Enable Experimental Features. If the user turns it
     * off while caffeinate is running, stop the hidden runtime instead of
     * leaving a titlebar-invisible power assertion active.
     */
    if (keepAwakeFeatureEnabled || !keepAwakeRuntime) {
      return;
    }
    void stopKeepAwake({ suppressAutoStart: true });
  }, [keepAwakeFeatureEnabled, keepAwakeRuntime, stopKeepAwake]);

  useEffect(() => {
    if (
      !keepAwakeFeatureEnabled ||
      !projectState.keepAwake.activateOnLaunch ||
      keepAwakeRuntime ||
      keepAwakeAutoStartSuppressed
    ) {
      return;
    }
    void startKeepAwake();
  }, [
    keepAwakeFeatureEnabled,
    keepAwakeAutoStartSuppressed,
    keepAwakeRuntime,
    projectState.keepAwake.activateOnLaunch,
    startKeepAwake,
  ]);

  useEffect(() => {
    /*
     * CDXC:TitlebarKeepAwake 2026-06-23-08:20:
     * Working-session keep-awake is optional, but once enabled it should cover the active Working period plus 20 minutes afterward so users have time to reply before the Mac can sleep.
     */
    const previousWorkingSessionCount = previousKeepAwakeWorkingSessionCountRef.current;
    previousKeepAwakeWorkingSessionCountRef.current = projectState.keepAwake.workingSessionCount;
    if (!projectState.keepAwake.whileWorkingSessions) {
      setKeepAwakeWorkingSessionGraceUntilMs(undefined);
      return;
    }
    if (
      projectState.keepAwake.workingSessionCount === 0 &&
      previousWorkingSessionCount > 0
    ) {
      setKeepAwakeWorkingSessionGraceUntilMs(Date.now() + KEEP_AWAKE_WORKING_SESSION_GRACE_MS);
    }
  }, [
    projectState.keepAwake.whileWorkingSessions,
    projectState.keepAwake.workingSessionCount,
  ]);

  useEffect(() => {
    if (
      !projectState.keepAwake.whileWorkingSessions ||
      projectState.keepAwake.workingSessionCount > 0 ||
      keepAwakeWorkingSessionGraceUntilMs === undefined
    ) {
      return;
    }
    const remainingMs = keepAwakeWorkingSessionGraceUntilMs - Date.now();
    if (remainingMs <= 0) {
      setKeepAwakeWorkingSessionGraceUntilMs(undefined);
      return;
    }
    const timeout = window.setTimeout(() => {
      setKeepAwakeWorkingSessionGraceUntilMs(undefined);
    }, remainingMs);
    return () => window.clearTimeout(timeout);
  }, [
    keepAwakeWorkingSessionGraceUntilMs,
    projectState.keepAwake.whileWorkingSessions,
    projectState.keepAwake.workingSessionCount,
  ]);

  useEffect(() => {
    /*
     * CDXC:TitlebarKeepAwake 2026-06-23-08:20:
     * If no manual keep-awake period is running, active Delayed Send timers should still prevent laptop sleep so the scheduled Enter can fire. Manual Keep Awake, especially Until turned off, takes precedence because automatic holds only start when no runtime exists and only stop runtimes they started.
     */
    if (!keepAwakeFeatureEnabled) {
      return;
    }
    const delayedSendHoldActive = projectState.keepAwake.delayedSendSessionCount > 0;
    const workingSessionHoldActive =
      projectState.keepAwake.whileWorkingSessions &&
      (projectState.keepAwake.workingSessionCount > 0 ||
        (keepAwakeWorkingSessionGraceUntilMs !== undefined &&
          keepAwakeWorkingSessionGraceUntilMs > Date.now()));
    const shouldRunAutomaticKeepAwake =
      !keepAwakeAutoStartSuppressed && (delayedSendHoldActive || workingSessionHoldActive);
    if (!shouldRunAutomaticKeepAwake) {
      if (keepAwakeRuntime?.source === "automatic") {
        void stopKeepAwake({ suppressAutoStart: false });
      }
      return;
    }
    if (!keepAwakeRuntime) {
      void startKeepAwake(0, { source: "automatic" });
    }
  }, [
    keepAwakeAutoStartSuppressed,
    keepAwakeFeatureEnabled,
    keepAwakeRuntime,
    keepAwakeWorkingSessionGraceUntilMs,
    projectState.keepAwake.delayedSendSessionCount,
    projectState.keepAwake.whileWorkingSessions,
    projectState.keepAwake.workingSessionCount,
    startKeepAwake,
    stopKeepAwake,
  ]);

  useEffect(() => {
    const desired = Boolean(
      keepAwakeFeatureEnabled && keepAwakeRuntime && projectState.keepAwake.preventLidSleep,
    );
    const ghostexEnabledLidSleepPrevention =
      localStorage.getItem(KEEP_AWAKE_LID_SLEEP_STORAGE_KEY) === "enabled";
    if (!desired && !ghostexEnabledLidSleepPrevention) {
      return;
    }
    let cancelled = false;
    const needsPolicyChange = desired !== ghostexEnabledLidSleepPrevention;
    const applyPolicy = async () => {
      const applied = await applyKeepAwakeLidSleepPrevention(desired, {
        installIfNeeded: desired && needsPolicyChange,
      });
      if (!applied || cancelled) {
        return;
      }
      localStorage.setItem(KEEP_AWAKE_LID_SLEEP_STORAGE_KEY, desired ? "enabled" : "disabled");
    };
    if (needsPolicyChange) {
      void applyPolicy();
    }
    let interval: number | undefined;
    if (desired) {
      interval = window.setInterval(() => {
        void applyKeepAwakeLidSleepPrevention(true, { installIfNeeded: false }).then((applied) => {
          if (applied && !cancelled) {
            localStorage.setItem(KEEP_AWAKE_LID_SLEEP_STORAGE_KEY, "enabled");
          }
        });
      }, 10_000);
    }
    return () => {
      cancelled = true;
      if (interval !== undefined) {
        window.clearInterval(interval);
      }
    };
  }, [keepAwakeFeatureEnabled, keepAwakeRuntime, projectState.keepAwake.preventLidSleep]);

  useEffect(() => {
    if (!keepAwakeRuntime) {
      return;
    }
    const checkRuntime = async () => {
      if (keepAwakeRuntime.fireAtMs !== undefined && Date.now() >= keepAwakeRuntime.fireAtMs) {
        await stopKeepAwake();
        return;
      }
      const pidCheck = await runNativeProcess("/bin/kill", ["-0", String(keepAwakeRuntime.pid)]);
      if (pidCheck.exitCode !== 0) {
        setKeepAwakeRuntime(undefined);
        localStorage.removeItem(KEEP_AWAKE_RUNTIME_STORAGE_KEY);
        publishKeepAwakeRuntimeSync({ suppressAutoStart: false });
      }
    };
    void checkRuntime();
    const interval = window.setInterval(() => {
      void checkRuntime();
    }, KEEP_AWAKE_POWER_CHECK_INTERVAL_MS);
    return () => window.clearInterval(interval);
  }, [keepAwakeRuntime, stopKeepAwake]);

  useEffect(() => {
    const shouldCheckExternalDisplay =
      keepAwakeFeatureEnabled &&
      !keepAwakeRuntime &&
      !keepAwakeAutoStartSuppressed &&
      projectState.keepAwake.activateOnExternalDisplay;
    const shouldCheckBattery =
      Boolean(keepAwakeRuntime && projectState.keepAwake.deactivateBelowBatteryThreshold);
    const shouldCheckLowPowerMode =
      Boolean(keepAwakeRuntime && projectState.keepAwake.deactivateOnLowPowerMode);
    if (!shouldCheckExternalDisplay && !shouldCheckBattery && !shouldCheckLowPowerMode) {
      return;
    }
    const checkPowerRules = async () => {
      const snapshot = await readKeepAwakePowerSnapshot({
        includeBattery: shouldCheckBattery,
        includeExternalDisplay: shouldCheckExternalDisplay,
        includeLowPowerMode: shouldCheckLowPowerMode,
      });
      if (!snapshot) {
        return;
      }
      if (
        keepAwakeRuntime &&
        projectState.keepAwake.deactivateBelowBatteryThreshold &&
        snapshot.batteryPercent !== undefined &&
        snapshot.batteryPercent <= projectState.keepAwake.batteryThresholdPercent
      ) {
        await stopKeepAwake();
        return;
      }
      if (
        keepAwakeRuntime &&
        projectState.keepAwake.deactivateOnLowPowerMode &&
        snapshot.lowPowerMode === true
      ) {
        await stopKeepAwake();
        return;
      }
      if (
        !keepAwakeRuntime &&
        !keepAwakeAutoStartSuppressed &&
        projectState.keepAwake.activateOnExternalDisplay &&
        snapshot.externalDisplayConnected
      ) {
        await startKeepAwake();
      }
    };
    void checkPowerRules();
    const interval = window.setInterval(() => {
      void checkPowerRules();
    }, KEEP_AWAKE_POWER_CHECK_INTERVAL_MS);
    return () => window.clearInterval(interval);
  }, [
    keepAwakeAutoStartSuppressed,
    keepAwakeFeatureEnabled,
    keepAwakeRuntime,
    projectState.keepAwake.activateOnExternalDisplay,
    projectState.keepAwake.batteryThresholdPercent,
    projectState.keepAwake.deactivateBelowBatteryThreshold,
    projectState.keepAwake.deactivateOnLowPowerMode,
    startKeepAwake,
    stopKeepAwake,
  ]);

  const openAgentsMode = () => {
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarAgentsMode");
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickStart",
      titlebarModeSwitchLogDetails({ optimisticMode, projectState, targetMode: "agents" }),
    );
    setOptimisticMode("agents");
    postNative({ type: "openAgentsModeFromTitlebar" });
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickPostedNative",
      {
      projectId: projectState.projectId ?? "none",
      targetMode: "agents",
    });
  };

  const openCodeMode = () => {
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarSourceMode");
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickStart",
      titlebarModeSwitchLogDetails({ optimisticMode, projectState, targetMode: "code" }),
    );
    setOptimisticMode("code");
    postNative({ type: "openActiveProjectEditorFromTitlebar" });
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickPostedNative",
      {
      projectId: projectState.projectId ?? "none",
      targetMode: "code",
    });
  };

  /**
   * CDXC:ProjectBrowserTabs 2026-06-13-00:12:
   * The top project browser mode is now user-facing Browser mode. Keep it
   * disabled only for Quick/projectless contexts; real projects without a
   * GitHub remote still open Browser mode with Google as the first tab so the
   * control is always useful without showing an app-created about:blank page.
   *
   * CDXC:ProjectBrowserTabs 2026-06-16-12:02:
   * Browser + tabs follow the same destination rule: project GitHub remote when available, otherwise Google.
   */
  const browserModeDisabledReason = projectState.projectIsQuick
    ? "Switch to a project to access this view"
    : undefined;
  /*
   * CDXC:ModeSwitcher 2026-06-08-18:39:
   * Quick sessions are projectless work areas, so Kanban should be unavailable
   * there for the same active-context reason as Browser mode. Disable the
   * titlebar tab/button before click dispatch instead of opening an empty
   * project-board surface.
   *
   * CDXC:ModeSwitcher 2026-06-16-16:00:
   * Disabled Browser and Kanban mode tabs should explain the project-context
   * requirement directly on hover. Use one shared message for Quick sessions so
   * users know switching to a project unlocks those views.
   */
  const kanbanModeDisabledReason = projectState.projectIsQuick
    ? "Switch to a project to access this view"
    : undefined;
  const manageModeDisabledReason = projectState.projectIsQuick
    ? "Switch to a project to access this view"
    : undefined;

  const openGitMode = () => {
    if (browserModeDisabledReason) {
      return;
    }
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarBrowserMode");
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickStart",
      titlebarModeSwitchLogDetails({ optimisticMode, projectState, targetMode: "git" }),
    );
    setOptimisticMode("git");
    postNative({ type: "openGitHubProjectFromTitlebar" });
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickPostedNative",
      {
      projectId: projectState.projectId ?? "none",
      targetMode: "git",
    });
  };

  const openAutomateMode = () => {
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarAutomateMode");
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickStart",
      titlebarModeSwitchLogDetails({ optimisticMode, projectState, targetMode: "automate" }),
    );
    setOptimisticMode("automate");
    postNative({ type: "openAutomateFromTitlebar" });
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickPostedNative",
      {
      projectId: projectState.projectId ?? "none",
      targetMode: "automate",
    });
  };

  const openTasksMode = () => {
    if (kanbanModeDisabledReason) {
      return;
    }
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarKanbanMode");
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickStart",
      titlebarModeSwitchLogDetails({ optimisticMode, projectState, targetMode: "tasks" }),
    );
    setOptimisticMode("tasks");
    postNative({ type: "openTasksPlaceholderFromTitlebar" });
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickPostedNative",
      {
      projectId: projectState.projectId ?? "none",
      targetMode: "tasks",
    });
  };

  const openManageMode = () => {
    if (manageModeDisabledReason) {
      return;
    }
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarManageMode");
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickStart",
      titlebarModeSwitchLogDetails({ optimisticMode, projectState, targetMode: "manage" }),
    );
    setOptimisticMode("manage");
    postNative({ type: "openManageFromTitlebar" });
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickPostedNative",
      {
      projectId: projectState.projectId ?? "none",
      targetMode: "manage",
    });
  };

  const toggleProjectEditorCompanion = () => {
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.companionToggle.dispatch",
      {
      activeMode,
      editorIsOpen: projectState.editorIsOpen,
      nextProjectEditorCompanionPaneHidden: projectState.projectEditorCompanionPaneHidden !== true,
      projectEditorCompanionPaneHidden: projectState.projectEditorCompanionPaneHidden,
      projectId: projectState.projectId,
      source: "click",
    });
    postNative({ type: "toggleProjectEditorCompanionFromTitlebar" });
  };
  const showUpdateDialog = () => {
    if (projectState.updateDownloading) {
      return;
    }
    postNative({ type: "showUpdateDialogFromTitlebar" });
  };

  const shouldShowCompanionToggleButton =
    activeMode !== "agents" &&
    projectState.editorIsOpen &&
    !projectState.editorIsSleeping;
  /*
   * CDXC:TitlebarModeTabs 2026-05-31-12:00:
   * macOS titlebar mode switcher labels use title case (Agents, Source, Browser, Kanban, Automate, Docs), not all-caps, so the segmented control reads like navigation chrome rather than shouting labels.
   *
   * CDXC:Manage 2026-06-20-04:36:
   * Manage is a project-scoped file browser workarea and should sit beside Kanban in the same titlebar segmented control instead of being hidden under a menu.
   *
   * CDXC:TitlebarManage 2026-06-28-06:16:
   * Manage is no longer beta or debugging-only chrome. Always show it in the
   * titlebar mode list and keep only the project-context disabled reason for
   * Quick sessions.
   *
   * CDXC:TitlebarDocs 2026-06-28-06:24:
   * The user-facing titlebar name for the Manage-backed project document
   * surface is Docs. Keep the stable internal "manage" mode id so persisted
   * pane state and native bridge messages remain compatible.
   *
   * CDXC:Automations 2026-06-30-11:05:
   * Automations are a first-class titlebar workarea named Automate. Opening Automate uses its own project-editor mode so project automations no longer make the titlebar look like it switched to Kanban.
   *
   * CDXC:TitlebarModeTabs 2026-06-30-12:55:
   * Kanban must appear before Automate in the macOS titlebar mode switcher, preserving the project-management flow before scheduled automation while keeping Docs last.
   */
  const titlebarModes = [
    {
      label: "Agents",
      onSelect: openAgentsMode,
      value: "agents" as const,
    },
    {
      label: "Source",
      onSelect: openCodeMode,
      value: "code" as const,
    },
    {
      disabled: browserModeDisabledReason !== undefined,
      disabledReason: browserModeDisabledReason,
      label: "Browser",
      onSelect: openGitMode,
      value: "git" as const,
    },
    {
      disabled: kanbanModeDisabledReason !== undefined,
      disabledReason: kanbanModeDisabledReason,
      label: "Kanban",
      onSelect: openTasksMode,
      value: "tasks" as const,
    },
    {
      label: "Automate",
      onSelect: openAutomateMode,
      value: "automate" as const,
    },
    {
      disabled: manageModeDisabledReason !== undefined,
      disabledReason: manageModeDisabledReason,
      label: "Docs",
      onSelect: openManageMode,
      value: "manage" as const,
    },
  ];
  const resolveTitlebarDropdownPanelSize = useCallback(
    (kind: TitlebarDropdownPanelKind) =>
      createTitlebarDropdownPanelPreferredSize(kind, {
        actionCount: visibleActions.length,
        gitItemCount: gitMenuItems.length,
        keepAwakeIsRunning: Boolean(keepAwakeRuntime),
        modeOptionCount: titlebarModes.length,
        targetCount: visibleTargets.length,
      }),
    [
      gitMenuItems.length,
      keepAwakeRuntime,
      titlebarModes.length,
      visibleActions.length,
      visibleTargets.length,
    ],
  );

  useLayoutEffect(() => {
    dropdownPanelSizeResolverRef.current = resolveTitlebarDropdownPanelSize;
  }, [resolveTitlebarDropdownPanelSize]);

  useEffect(() => {
    /**
     * CDXC:SidebarTheme 2026-06-15-01:43:
     * The titlebar runs in its own WKWebView, so mirror the resolved sidebar
     * theme onto body for shared CSS tokens. This keeps the titlebar strip and
     * native child-window dropdown panels aligned with Dark 1, Dark 2, and
     * Light Settings changes.
     */
    document.body.dataset.sidebarTheme = projectState.sidebarTheme;
    return () => {
      delete document.body.dataset.sidebarTheme;
    };
  }, [projectState.sidebarTheme]);

  useEffect(() => {
    if (initialTitlebarDropdownPanelKind) {
      return;
    }

    if (projectState.customSidebarTitlebarColorsEnabled) {
      const titlebarGradientColors = getSidebarTitlebarGradientColors(
        projectState.customSidebarTitlebarBackgroundColor,
      );
      const titlebarBackground = `linear-gradient(90deg, ${titlebarGradientColors.titlebarLeft} 0%, ${titlebarGradientColors.titlebarLeft} ${TITLEBAR_GRADIENT_BLEND_START_PERCENT}%, ${titlebarGradientColors.titlebarRight} 100%)`;
      /**
       * CDXC:SidebarTitlebarColors 2026-06-15-11:24:
       * The React titlebar is a separate WKWebView from the sidebar. Apply the
       * experimental custom chrome colors only in this titlebar host; dropdown
       * panels reuse this bundle but must continue using normal dropdown/theme
       * tokens instead of the sidebar/titlebar override.
       *
       * CDXC:SidebarTitlebarColors 2026-06-15-13:22:
       * Foreground is derived from the background before it reaches this state;
       * the titlebar host should not expose or preserve a separate foreground
       * choice.
       *
       * CDXC:SidebarTitlebarColors 2026-06-15-15:01:
       * Custom titlebar separators darken as the slider-selected background gets
       * lighter, but only inside the real titlebar host.
       *
       * CDXC:SidebarTitlebarColors 2026-06-19-12:33:
       * The titlebar should start with the sidebar gradient's top color and
       * use a separate surface token for the gradient paint.
       *
       * CDXC:SidebarTitlebarColors 2026-06-19-13:26:
       * Keep the titlebar's left 40% on the sidebar top stop so it blends with
       * the sidebar edge, then fade to the sidebar bottom stop at the right.
       * The titlebar gradient should now darken across the strip rather than
       * brighten at the right edge.
       */
      document.body.dataset.customSidebarTitlebarColors = "true";
      document.body.style.setProperty(
        "--app-titlebar-background",
        titlebarGradientColors.titlebarLeft,
      );
      document.body.style.setProperty(
        "--app-titlebar-surface-background",
        titlebarBackground,
      );
      document.body.style.setProperty(
        "--custom-sidebar-titlebar-background-color",
        titlebarGradientColors.titlebarLeft,
      );
      document.body.style.setProperty(
        "--custom-sidebar-titlebar-foreground-color",
        projectState.customSidebarTitlebarForegroundColor,
      );
      document.body.style.setProperty(
        "--app-foreground",
        projectState.customSidebarTitlebarForegroundColor,
      );
      document.body.style.setProperty(
        "--titlebar-button-border-color",
        getTitlebarButtonSeparatorColorForBackground(titlebarGradientColors.titlebarLeft),
      );
    } else {
      delete document.body.dataset.customSidebarTitlebarColors;
      document.body.style.removeProperty("--app-titlebar-background");
      document.body.style.removeProperty("--app-titlebar-surface-background");
      document.body.style.removeProperty("--app-foreground");
      document.body.style.removeProperty("--custom-sidebar-titlebar-background-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-foreground-color");
      document.body.style.removeProperty("--titlebar-button-border-color");
    }

    return () => {
      delete document.body.dataset.customSidebarTitlebarColors;
      document.body.style.removeProperty("--app-titlebar-background");
      document.body.style.removeProperty("--app-titlebar-surface-background");
      document.body.style.removeProperty("--app-foreground");
      document.body.style.removeProperty("--custom-sidebar-titlebar-background-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-foreground-color");
      document.body.style.removeProperty("--titlebar-button-border-color");
    };
  }, [
    projectState.customSidebarTitlebarBackgroundColor,
    projectState.customSidebarTitlebarColorsEnabled,
    projectState.customSidebarTitlebarForegroundColor,
  ]);

  const isTitlebarDarkTheme = getTitlebarThemeVariant(projectState.sidebarTheme) === "dark";

  if (titlebarPanelKind) {
    return (
      <TooltipProvider delayDuration={300}>
        <TitlebarDropdownPanelSurface
          activeMode={activeMode}
          activeTarget={activeTarget}
          browserBundles={resourceViews.browserBundles}
          codeIdeBundles={resourceViews.codeIdeBundles}
          collapsedResourceKeys={collapsedResourceKeys}
          daemon={projectState.gxserverDaemon}
          git={projectState.git}
          gitItems={gitMenuItems}
          inactiveTerminalSleepSessionCount={inactiveTerminalSleepSessionIds.length}
          kind={titlebarPanelKind}
          modeOptions={titlebarModes}
          notices={notices}
          onClose={closeTitlebarDropdownPanel}
          onFocusResourceSession={focusResourceSession}
          onGxserverAlwaysStartChange={setGxserverAlwaysStart}
          onGxserverRestart={restartGxserverDaemon}
          onGxserverStart={startGxserverDaemon}
          onGxserverStop={stopGxserverDaemon}
          onMarkTipRead={markTipRead}
          onOpenChangelog={openChangelogFromTips}
          onOpenDocs={openDocsFromTips}
          onOpenHighlightedFeatures={openHighlightedFeaturesFromTips}
          onOpenSettingsMenuCommands={openTitlebarSettingsMenuCommands}
          onOpenSettingsMenuDiscord={openTitlebarSettingsMenuDiscord}
          onOpenSettingsMenuHotkeys={openTitlebarSettingsMenuHotkeys}
          onOpenSettingsMenuSettings={openTitlebarSettingsMenuSettings}
          onOpenNoticeSettings={handleNoticeAction}
          onOpenPowerSettings={openPowerSettings}
          onOpenTipAction={openTipAction}
          onOpenTarget={openTarget}
          onQuitResources={quitResourceBundles}
          onRunAction={runSidebarAction}
          onRunGitAction={runGitAction}
          onViewGhostexGuide={viewGhostexGuideFromTips}
          onSetResourceItemsCollapsed={setResourceItemsCollapsed}
          onSleepInactiveSessions={sleepInactiveTerminalSessions}
          onStartKeepAwake={startKeepAwake}
          onStopKeepAwake={stopKeepAwake}
          onWakePetFromSettingsMenu={wakePetFromTitlebarSettingsMenu}
          onToggleResourceCollapse={toggleResourceCollapse}
          orphanBundles={resourceViews.orphanBundles}
          resourceProcessSnapshotReady={resourceProcessSnapshotReady}
          resourceProcessTotals={resourceProcessTotals}
          quittingResourceKeys={quittingResourceKeys}
          readTips={readTips}
          resourceGroupViews={resourceViews.groupViews}
          serverBundles={resourceServerBundles}
          selectedActionCommandId={selectedActionCommandId}
          hotkeys={projectState.hotkeys}
          sidebarTheme={projectState.sidebarTheme}
          serverOpenTarget={projectState.terminalDevServerOpenTarget}
          sessionPersistenceProvider={
            projectState.sessionPersistenceProvider === "off"
              ? undefined
              : projectState.sessionPersistenceProvider
          }
          visibleActions={visibleActions}
          visibleTargets={visibleTargets}
          unreadTips={unreadTips}
          activeKeepAwakeDuration={keepAwakeRuntime?.durationMinutes}
          keepAwakeIsRunning={Boolean(keepAwakeRuntime)}
        />
      </TooltipProvider>
    );
  }

  const titlebarSidebarCollapseButton = (
    <TitlebarAppTooltip
      content={projectState.toggleSidebarHotkeyLabel}
      side="right"
      sideOffset={4}
    >
      <Button
        aria-label={projectState.sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
        className="titlebar-sidebar-collapse-button"
        onClick={() => postNative({ type: "toggleSidebarCollapsed" })}
        type="button"
        variant="ghost"
      >
        {/*
         * CDXC:SidebarCollapse 2026-06-20-17:10:
         * Users asked for the titlebar Toggle Sidebar affordance to use the
         * Tabler sidebar icon itself, without the blue circular button visual.
         * Keep the existing 33px titlebar hit target but render only the
         * side-aware glyph so the control reads as titlebar chrome.
         *
         * CDXC:SidebarCollapse 2026-06-12-11:10:
         * The update affordance belongs to the right of this collapse
         * button, with a 9px gap between the two compact titlebar buttons.
         * Keep the stable frame so the native titlebar layout does not shift.
         *
         * CDXC:SidebarCollapse 2026-06-13-10:53:
         * The hover tooltip for this button should name Toggle Sidebar and show
         * its assigned hotkey so the tiny titlebar affordance remains clear.
         *
         * CDXC:SidebarCollapse 2026-06-13-02:59:
         * Use the same AppTooltip wrapper as sidebar controls for the hotkey
         * label; keep the titlebar-specific wrapper responsible only for right
         * placement beside the titlebar-side button.
        */}
        <SidebarCollapseIcon aria-hidden="true" data-icon="inline-start" stroke={1.9} />
      </Button>
    </TitlebarAppTooltip>
  );

  return (
    <TooltipProvider delayDuration={300}>
      <div
        className={cn(isTitlebarDarkTheme && "dark")}
        data-sidebar-theme={projectState.sidebarTheme}
        ref={rootRef}
        style={styles.shell}
      >
        <div onMouseDown={requestTitlebarBlankMouseDown} style={styles.titlebar}>
          <div style={styles.projectSlot}>
            {titlebarSidebarCollapseButton}
            {projectState.updateAvailable || projectState.updateDownloading ? (
              <TitlebarAppTooltip
                content={
                  projectState.updateDownloading
                    ? formatTitlebarUpdateDownloadingTooltip(projectState.updateDownloadProgress)
                    : TITLEBAR_UPDATE_AVAILABLE_TOOLTIP
                }
                side="right"
              >
                <Button
                  aria-label={
                    projectState.updateDownloading
                      ? formatTitlebarUpdateDownloadingAriaLabel(projectState.updateDownloadProgress)
                      : "Download update"
                  }
                  aria-disabled={projectState.updateDownloading ? true : undefined}
                  className="titlebar-session-button titlebar-update-button"
                  data-disabled={projectState.updateDownloading ? "true" : undefined}
                  data-downloading={projectState.updateDownloading ? "true" : undefined}
                  onClick={showUpdateDialog}
                  type="button"
                  variant="ghost"
                >
                  {/*
                   * CDXC:AutoUpdate 2026-05-28-14:19:
                   * Available app updates should be subtle titlebar chrome,
                   * not a launch-time modal. Keep this button dim beside the
                   * project identity; clicking it is the user's explicit
                   * handoff into Sparkle's standard update dialog.
                   *
                   * CDXC:AutoUpdate 2026-06-13-17:52:
                   * Once Sparkle is actually downloading the accepted update,
                   * keep this same consent affordance visible while download
                   * activity is indicated without showing Sparkle's separate
                   * progress window.
                   *
                   * CDXC:AutoUpdate 2026-06-15-16:39:
                   * After Sparkle starts downloading, the titlebar update
                   * button must stop accepting repeat download clicks and keep
                   * a download-state hover label while the download is active.
                   *
                   * CDXC:AutoUpdate 2026-06-30-22:18:
                   * Replace the active download spinner with a circular fill
                   * driven by Sparkle's real progress ratio, and include the
                   * current percent in hover/accessibility text whenever native
                   * has enough information to compute it.
                   */}
                  {projectState.updateDownloading ? (
                    <TitlebarUpdateProgressRing progress={projectState.updateDownloadProgress} />
                  ) : (
                    <IconDownload
                      aria-hidden="true"
                      className="titlebar-update-download-icon"
                      size={14}
                      stroke={1.8}
                    />
                  )}
                </Button>
              </TitlebarAppTooltip>
            ) : null}
            <div className="titlebar-project-title">
              {/*
               * CDXC:ReactTitlebar 2026-05-17-02:29:
               * The project name is passive titlebar identity text. Do not use
               * it as a copy-path button and do not attach a tooltip; project
               * path actions should live in explicit menus instead of hidden
               * titlebar hover behavior.
               */}
              {projectState.projectIconDataUrl ? (
                <img
                  alt=""
                  aria-hidden="true"
                  className="titlebar-project-icon"
                  draggable={false}
                  src={projectState.projectIconDataUrl}
                />
              ) : null}
              <span className="truncate">{projectState.projectName}</span>
            </div>
            <TitlebarModeDropdown
              activeMode={activeMode}
              modes={titlebarModes}
              nativeDropdownOpen={nativeDropdownOpen}
              onOpenPanel={showTitlebarDropdownPanel}
            />
          </div>
          <div style={styles.centerSlot}>
            <TitlebarModeSwitcher
              activeMode={activeMode}
              companionPaneHidden={projectState.projectEditorCompanionPaneHidden}
              modes={titlebarModes}
              onToggleCompanion={toggleProjectEditorCompanion}
              showCompanionToggle={shouldShowCompanionToggleButton}
            />
          </div>
          <div style={styles.rightSlot}>
            {projectState.promptEditorOpen ? (
              /*
               * Prompt Editor and Exit Focus share this titlebar slot; while
               * the standalone GhostexEditor daemon reports an open editor
               * window only Prompt Editor renders, and clicking it brings the
               * editor windows forward.
               */
              <button
                aria-label="Bring the Prompt Editor forward"
                className="titlebar-mode-tab titlebar-exit-focus-button"
                data-active="true"
                onClick={() => postNative({ type: "bringPromptEditorToFrontFromTitlebar" })}
                style={{ transformStyle: "preserve-3d" }}
                type="button"
              >
                {/*
                 * Prompt Editor mirrors the Exit focus affordance exactly:
                 * active mode-tab DOM, typography, borders, and highlight
                 * pill, so it reads like the Automate/Docs tabs instead of a
                 * dimmed full-height slab.
                 */}
                <span aria-hidden="true" className="titlebar-mode-tab-active" />
                <span className="titlebar-mode-tab-content">
                  <span aria-hidden="true" className="titlebar-prompt-editor-live-dot" />
                  <span className="titlebar-mode-label">Prompt Editor</span>
                </span>
              </button>
            ) : projectState.isFocusModeActive ? (
              <button
                aria-label="Exit focus mode"
                className="titlebar-mode-tab titlebar-exit-focus-button"
                data-active="true"
                onClick={() => postNative({ type: "exitFocusModeFromTitlebar" })}
                style={{ transformStyle: "preserve-3d" }}
                type="button"
              >
                {/*
                 * CDXC:SessionFocusMode 2026-06-13-18:39:
                 * The focus-mode exit affordance should match the active Agents
                 * tab exactly, including the active segment background,
                 * typography, separators, and square titlebar geometry.
                 */}
                <span aria-hidden="true" className="titlebar-mode-tab-active" />
                <span className="titlebar-mode-tab-content">
                  <span className="titlebar-mode-label">Exit focus</span>
                </span>
              </button>
            ) : null}
            {/*
             * CDXC:ReactTitlebar 2026-05-30-03:11:
             * Top-right titlebar menus are right-click affordances. Keep left
             * click on primary icon actions, hide chevrons, and tell users about
             * right-click options through compact hover tooltips.
             *
             * CDXC:ReactTitlebar 2026-05-30-08:39:
             * Tips & Tricks sits in the top-right titlebar controls as the
             * compact info/help affordance near the mode switcher.
             *
             * CDXC:ReactTitlebar 2026-06-18-05:16:
             * User-facing titlebar labels should use the shorter "Tips" copy
             * while the underlying tips menu behavior stays unchanged.
             *
             * CDXC:SidebarTopChrome 2026-06-29-01:43:
             * Settings and Keep Awake are no longer titlebar triggers; they render in the sidebar shortcut row so this titlebar cluster stays focused on project/window actions.
             */}
            <ButtonGroup
              className="titlebar-open-group titlebar-tips-group"
              data-titlebar-dropdown-anchor
            >
              <TitlebarAppTooltip content="Tips">
                <Button
                  aria-label={
                    unreadTips.length + notices.length > 0
                      ? `Tips, ${unreadTips.length + notices.length} unread`
                      : "Tips"
                  }
                  className="titlebar-session-button titlebar-tips-button"
                  data-state={nativeDropdownOpen === "tips" ? "open" : undefined}
                  onClick={openTipsMenuFromTitlebar}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    openTipsMenuFromTitlebar(event);
                  }}
                  type="button"
                  variant="ghost"
                >
                  {/*
                   * CDXC:TipsAndTricks 2026-05-30-08:39:
                   * The titlebar Tips & Tricks affordance is an info circle,
                   * not the earlier square glyph. Unread state is a small
                   * blue dot without a visible number so the icon stays quiet.
                   */}
                  <IconInfoCircle aria-hidden="true" size={16} stroke={1.8} />
                  {unreadTips.length + notices.length > 0 ? (
                    <span aria-hidden="true" className="titlebar-tips-unread-badge" />
                  ) : null}
                </Button>
              </TitlebarAppTooltip>
            </ButtonGroup>
            <ButtonGroup
              className="titlebar-open-group"
              data-titlebar-dropdown-anchor
            >
              <TitlebarAppTooltip content="Resources Monitor">
                <Button
                  aria-label="Ghostex resources"
                  className="titlebar-session-button titlebar-resource-button"
                  data-state={nativeDropdownOpen === "resources" ? "open" : undefined}
                  onClick={(event) => {
                    showTitlebarDropdownPanel("resources", event.currentTarget);
                  }}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    showTitlebarDropdownPanel("resources", event.currentTarget);
                  }}
                  type="button"
                  variant="ghost"
                >
                {/*
                 * CDXC:TitlebarResources 2026-05-17-02:03:
                 * The Resources button is the first right-side titlebar
                 * control after moving the pet wake/sleep toggle out of
                 * Resources.
                 *
                 * CDXC:TitlebarKeepAwake 2026-05-27-07:32:
                 * The keep-awake button now owns coffee/moon state icons, so
                 * Resources uses the old desktop glyph as the stable manager
                 * icon requested for this titlebar control swap.
                 */}
                  <IconDeviceDesktop aria-hidden="true" size={16} />
                </Button>
              </TitlebarAppTooltip>
            </ButtonGroup>
            <ButtonGroup
              className="titlebar-open-group titlebar-git-group"
              data-titlebar-dropdown-anchor
            >
              <TitlebarAppTooltip content="Git actions">
                <Button
                  aria-disabled={gitPrimaryAction.disabled}
                  aria-expanded={nativeDropdownOpen === "git"}
                  aria-haspopup="menu"
                  aria-label="Git actions"
                  className="titlebar-session-button titlebar-open-main-button titlebar-git-main-button"
                  data-disabled={String(gitPrimaryAction.disabled)}
                  data-state={nativeDropdownOpen === "git" ? "open" : undefined}
                  onClick={openGitMenuFromTitlebar}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    openGitMenuFromTitlebar(event);
                  }}
                  onDoubleClick={openGitMenuFromTitlebar}
                  type="button"
                  variant="ghost"
                >
                  {/*
                   * CDXC:TitlebarGit 2026-05-24-17:41:
                   * The titlebar Git split button mirrors t3code's commit/push control and sits immediately after Resources so commit, push, and PR actions are reachable from top chrome without opening the sidebar Git row.
                   *
                   * CDXC:TitlebarTooltips 2026-06-13-02:59:
                   * Use aria-disabled instead of native disabled here so the
                   * shared AppTooltip trigger can still receive hover, matching
                   * the sidebar toolbar's disabled-action pattern.
                   *
                   * CDXC:TitlebarGit 2026-06-15-23:25:
                   * The Git titlebar button is a dropdown launcher, not a direct
                   * commit/push toggle. Click, right-click, and double-click open
                   * or close the Git actions menu so choosing a Git operation is
                   * always explicit inside the dropdown.
                   */
                  projectState.git.isBusy ? (
                    <IconLoader2 aria-hidden="true" className="titlebar-git-spinner" size={14} />
                  ) : (
                    getTitlebarGitActionIcon(gitPrimaryAction.action)
                  )}
                </Button>
              </TitlebarAppTooltip>
            </ButtonGroup>
            <ButtonGroup
              className="titlebar-open-group titlebar-actions-group"
              data-titlebar-dropdown-anchor
            >
              {/*
               * CDXC:TitlebarTooltips 2026-06-16-01:19:
               * Actions and Open In hover labels should state the button
               * function first, then explain the right-click menu. Do not lead
               * with generic click instructions in these titlebar tooltips.
               */}
              <TitlebarAppTooltip content="Quick Actions. Right click for more options">
                <Button
                  aria-label={
                    activeAction
                      ? `Run ${getSidebarActionLabel(activeAction)}`
                      : "Configure actions"
                  }
                  className="titlebar-session-button titlebar-open-main-button"
                  data-state={nativeDropdownOpen === "actions" ? "open" : undefined}
                  onClick={() => runSidebarAction(activeAction)}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    showTitlebarDropdownPanel("actions", event.currentTarget);
                  }}
                  type="button"
                  variant="ghost"
                >
                  {activeAction ? (
                    getSidebarActionIcon(activeAction)
                  ) : (
                    <IconSettings aria-hidden="true" className="quick-action-icon" size={16} stroke={1.8} />
                  )}
                </Button>
              </TitlebarAppTooltip>
            </ButtonGroup>
            <ButtonGroup
              className="titlebar-open-group"
              data-titlebar-dropdown-anchor
            >
              <TitlebarAppTooltip content="Open in an app. Right click for more options">
                <Button
                  aria-label={activeTarget?.label ?? "Open project"}
                  className="titlebar-session-button titlebar-open-main-button"
                  data-state={nativeDropdownOpen === "openIn" ? "open" : undefined}
                  onClick={() => openTarget(activeTarget)}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    showTitlebarDropdownPanel("openIn", event.currentTarget);
                  }}
                  type="button"
                  variant="ghost"
                >
                  {activeTarget ? (
                    getOpenTargetIcon(activeTarget)
                  ) : (
                    <IconFolderOpen aria-hidden="true" className="size-4 text-zinc-400" />
                  )}
                </Button>
              </TitlebarAppTooltip>
            </ButtonGroup>
          </div>
        </div>
      </div>
    </TooltipProvider>
  );
}

function TitlebarDropdownPanelSurface({
  activeKeepAwakeDuration,
  activeMode,
  activeTarget,
  browserBundles,
  codeIdeBundles,
  collapsedResourceKeys,
  daemon,
  git,
  gitItems,
  inactiveTerminalSleepSessionCount,
  keepAwakeIsRunning,
  kind,
  modeOptions,
  notices,
  onClose,
  onFocusResourceSession,
  onGxserverAlwaysStartChange,
  onGxserverRestart,
  onGxserverStart,
  onGxserverStop,
  onMarkTipRead,
  onOpenChangelog,
  onOpenDocs,
  onOpenHighlightedFeatures,
  onOpenSettingsMenuCommands,
  onOpenSettingsMenuDiscord,
  onOpenSettingsMenuHotkeys,
  onOpenSettingsMenuSettings,
  onOpenNoticeSettings,
  onOpenPowerSettings,
  onOpenTipAction,
  onOpenTarget,
  onQuitResources,
  onRunAction,
  onRunGitAction,
  onSetResourceItemsCollapsed,
  onSleepInactiveSessions,
  onStartKeepAwake,
  onStopKeepAwake,
  onWakePetFromSettingsMenu,
  onViewGhostexGuide,
  onToggleResourceCollapse,
  orphanBundles,
  resourceProcessSnapshotReady,
  resourceProcessTotals,
  quittingResourceKeys,
  readTips,
  resourceGroupViews,
  serverBundles,
  selectedActionCommandId,
  hotkeys,
  sidebarTheme,
  serverOpenTarget,
  sessionPersistenceProvider,
  visibleActions,
  visibleTargets,
  unreadTips,
}: {
  activeKeepAwakeDuration: KeepAwakeDurationMinutes | undefined;
  activeMode: TitlebarMode;
  activeTarget: ResolvedOpenTarget | undefined;
  browserBundles: ResourceProcessBundle[];
  codeIdeBundles: ResourceProcessBundle[];
  collapsedResourceKeys: Set<string>;
  daemon: TitlebarGxserverDaemonStatus;
  git: SidebarGitState;
  gitItems: ReturnType<typeof buildSidebarGitMenuItems>;
  inactiveTerminalSleepSessionCount: number;
  keepAwakeIsRunning: boolean;
  kind: TitlebarDropdownPanelKind;
  modeOptions: TitlebarModeOption[];
  notices: TitlebarNotice[];
  onClose: () => void;
  onFocusResourceSession: (sessionId: string) => void;
  onGxserverAlwaysStartChange: (enabled: boolean) => void;
  onGxserverRestart: () => void;
  onGxserverStart: () => void;
  onGxserverStop: () => void;
  onMarkTipRead: (tipId: string) => void;
  onOpenChangelog: () => void;
  onOpenDocs: () => void;
  onOpenHighlightedFeatures: () => void;
  onOpenSettingsMenuCommands: () => void;
  onOpenSettingsMenuDiscord: () => void;
  onOpenSettingsMenuHotkeys: () => void;
  onOpenSettingsMenuSettings: () => void;
  onOpenNoticeSettings: (notice: TitlebarNotice) => void;
  onOpenPowerSettings: () => void;
  onOpenTipAction: (tip: TitlebarTip) => void;
  onOpenTarget: (target: ResolvedOpenTarget | undefined) => void;
  onQuitResources: (bundles: ResourceProcessBundle[]) => void;
  onRunAction: (command: SidebarCommandButton | undefined) => void;
  onRunGitAction: (action: SidebarGitAction) => void;
  onSetResourceItemsCollapsed: (
    targets: readonly ResourceItemCollapseTarget[],
    collapsed: boolean,
  ) => void;
  onSleepInactiveSessions: () => void;
  onStartKeepAwake: (durationMinutes?: KeepAwakeDurationMinutes) => Promise<void>;
  onStopKeepAwake: () => Promise<void>;
  onWakePetFromSettingsMenu: () => void;
  onViewGhostexGuide: () => void;
  onToggleResourceCollapse: (key: string) => void;
  orphanBundles: ResourceProcessBundle[];
  resourceProcessSnapshotReady: boolean;
  resourceProcessTotals: ResourceProcessTotals;
  quittingResourceKeys: Set<string>;
  readTips: TitlebarTip[];
  resourceGroupViews: ResourceGroupView[];
  serverBundles: ResourceProcessBundle[];
  selectedActionCommandId: string | undefined;
  hotkeys: ghostexHotkeySettings;
  sidebarTheme: SidebarTheme;
  serverOpenTarget: TerminalDevServerOpenTarget;
  sessionPersistenceProvider: Exclude<SessionPersistenceProvider, "off"> | undefined;
  visibleActions: SidebarCommandButton[];
  visibleTargets: ResolvedOpenTarget[];
  unreadTips: TitlebarTip[];
}) {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  const closeAfter = (action: () => void | Promise<void>) => {
    void Promise.resolve()
      .then(action)
      .catch((error) => {
        console.warn("Titlebar dropdown action failed", error);
      })
      .finally(onClose);
  };
  const isPanelDarkTheme = getTitlebarThemeVariant(sidebarTheme) === "dark";
  const gitBranchLabel = titlebarGitBranchLabel(git.branch);
  const settingsMenuHotkeys = normalizeghostexHotkeySettings(hotkeys);

  return (
    <div
      className={cn(isPanelDarkTheme && "dark", "titlebar-dropdown-panel-root")}
      data-panel-kind={kind}
      data-sidebar-theme={sidebarTheme}
    >
      {kind === "mode" ? (
        <div className="titlebar-open-menu titlebar-mode-picker-menu min-w-[180px] rounded-none border-border/80 p-1 text-[13px] text-foreground shadow-2xl">
          {modeOptions.map((mode) => (
            <TitlebarPanelMenuItem
              disabled={mode.disabled}
              key={mode.value}
              onClick={() => closeAfter(mode.onSelect)}
            >
              {getTitlebarModeIcon(mode.value)}
              <span className="min-w-0 flex-1 truncate">{mode.label}</span>
              {mode.value === activeMode ? (
                <IconCheck aria-hidden="true" className="ml-2 size-4 opacity-75" />
              ) : null}
            </TitlebarPanelMenuItem>
          ))}
        </div>
      ) : null}
      {kind === "tips" ? (
        <div className="titlebar-open-menu titlebar-tips-menu rounded-none border-border/80 p-0 text-[13px] text-foreground shadow-2xl">
          <TitlebarTipsMenu
            notices={notices}
            onMarkRead={onMarkTipRead}
            onOpenChangelog={() => closeAfter(onOpenChangelog)}
            onOpenDocs={() => closeAfter(onOpenDocs)}
            onOpenHighlightedFeatures={() => closeAfter(onOpenHighlightedFeatures)}
            onOpenNoticeSettings={(notice) => closeAfter(() => onOpenNoticeSettings(notice))}
            onOpenTipAction={(tip) => closeAfter(() => onOpenTipAction(tip))}
            onViewGhostexGuide={() => closeAfter(onViewGhostexGuide)}
            readTips={readTips}
            unreadTips={unreadTips}
          />
        </div>
      ) : null}
      {kind === "keepAwake" ? (
        <div className="titlebar-open-menu min-w-[220px] rounded-none border-border/80 p-1 text-[13px] text-foreground shadow-2xl">
          {/*
            CDXC:TitlebarKeepAwake 2026-06-15-02:34:
            The compact titlebar dropdown should name the duration group as "Keep awake period" and keep each row label short: Until turned off, For 2 hours, For 5 hours, and Don't keep awake.

            CDXC:TitlebarKeepAwake 2026-06-15-02:57:
            The Don't keep awake action should use IconSquareMinus so the row reads as disabling the keep-awake state instead of a moon or sleep-mode action.

            CDXC:TitlebarKeepAwake 2026-06-23-19:36:
            Keep Awake start/stop actions are async because they launch or kill caffeinate through the native process bridge. Keep this child dropdown alive until the action commits its runtime sync so the main titlebar icon updates immediately after a menu click.
          */}
          <div className="titlebar-menu-section-label">Keep awake period</div>
          {KEEP_AWAKE_DURATION_OPTIONS.map((option) => (
            <TitlebarPanelMenuItem
              key={option.value}
              onClick={() => closeAfter(() => onStartKeepAwake(option.value))}
            >
              <IconCoffee aria-hidden="true" size={14} stroke={1.8} />
              <span className="min-w-0 flex-1 truncate">{getTitlebarKeepAwakeMenuLabel(option.label)}</span>
              {activeKeepAwakeDuration === option.value ? (
                <IconCheck aria-hidden="true" className="ml-2 size-4 opacity-75" />
              ) : null}
            </TitlebarPanelMenuItem>
          ))}
          {keepAwakeIsRunning ? (
            <TitlebarPanelMenuItem
              onClick={() => closeAfter(onStopKeepAwake)}
            >
              <IconSquareMinus aria-hidden="true" size={14} stroke={1.8} />
              <span>Don't keep awake</span>
            </TitlebarPanelMenuItem>
          ) : null}
          <TitlebarPanelMenuSeparator />
          <TitlebarPanelMenuItem onClick={() => closeAfter(onOpenPowerSettings)}>
            <IconSettings aria-hidden="true" size={16} />
            <span>Power Settings</span>
          </TitlebarPanelMenuItem>
        </div>
      ) : null}
      {kind === "resources" ? (
        <div className="titlebar-open-menu titlebar-resources-menu rounded-none border-border/80 p-0 text-[13px] text-foreground shadow-2xl">
          <TitlebarResourcesMenu
            browserBundles={browserBundles}
            codeIdeBundles={codeIdeBundles}
            collapsedKeys={collapsedResourceKeys}
            daemon={daemon}
            groupViews={resourceGroupViews}
            inactiveTerminalSleepSessionCount={inactiveTerminalSleepSessionCount}
            onFocusSession={(sessionId) => {
              onFocusResourceSession(sessionId);
              onClose();
            }}
            onGxserverAlwaysStartChange={onGxserverAlwaysStartChange}
            onGxserverRestart={onGxserverRestart}
            onGxserverStart={onGxserverStart}
            onGxserverStop={onGxserverStop}
            onQuit={onQuitResources}
            onSetResourceItemsCollapsed={onSetResourceItemsCollapsed}
            processSnapshotReady={resourceProcessSnapshotReady}
            onSleepInactiveSessions={onSleepInactiveSessions}
            onToggle={onToggleResourceCollapse}
            orphanBundles={orphanBundles}
            processTotals={resourceProcessTotals}
            quittingKeys={quittingResourceKeys}
            serverBundles={serverBundles}
            serverOpenTarget={serverOpenTarget}
            sessionPersistenceProvider={sessionPersistenceProvider}
          />
        </div>
      ) : null}
      {kind === "git" ? (
        <div className="titlebar-open-menu titlebar-git-menu rounded-none border-border/80 p-1 text-[13px] text-foreground shadow-2xl">
          {/*
            CDXC:TitlebarGit 2026-06-15-23:25:
            The titlebar Git dropdown should expose branch context, colored
            working-tree change stats, remote tracking counts, and a sync action
            before Commit. Branch and working-tree stats are compact status
            controls above the existing Git action list.

            CDXC:TitlebarGit 2026-06-15-23:25:
            Titlebar Git rows must call the same titlebar Git action bridge as
            Commit, Push, and PR. The sidebar-owned Git pipeline refreshes
            status before running the operation and republishes the updated
            state to both the dropdown child window and the sidebar project
            chrome.

            CDXC:TitlebarGit 2026-06-16-07:31:
            The always-visible sync row is now remote push/pull sync. Worktree
            Sync with Main remains a separate workflow action in the command list
            so normal branches can sync with origin without entering the
            worktree-only agent flow.

            CDXC:TitlebarGit 2026-06-16-15:15:
            Status and Actions labels should divide read-only branch/diff/remote
            state from runnable Git commands. The Changes row uses a code icon so
            it does not read like the remote-sync compare action below it.

            CDXC:TitlebarGit 2026-06-16-19:03:
            The status block is a label/value table: Branch, Changes, and Commits
            stay left-aligned while their values are right-aligned. Commits
            always shows ↑ahead then ↓behind, including ↑0 ↓0 when no sync is
            needed. The branch value is a tooltip-backed copy target.

            CDXC:TitlebarGit 2026-06-16-19:10:
            The full Branch row is a copy target, not only the visible branch
            value. The Changes/files row opens the Commit review screen like the
            Commit action. Tooltips must explain both click targets.

            CDXC:TitlebarGit 2026-06-16-19:19:
            The changed-files stat label should read Changes instead of Lines.
          */}
          <div className="titlebar-menu-section-label">Status</div>
          <AppTooltip
            {...TITLEBAR_TOOLTIP_ROOT_PROPS}
            content={
              <span className="titlebar-git-branch-tooltip-copy">
                <span>{gitBranchLabel}</span>
                <span>Click to copy branch name</span>
              </span>
            }
            contentClassName="titlebar-git-branch-tooltip whitespace-normal text-left"
            side="left"
            sideOffset={6}
          >
            <button
              aria-label={`Copy branch ${gitBranchLabel}`}
              className="titlebar-open-menu-item titlebar-git-meta-row titlebar-git-copy-branch-row"
              onClick={() => {
                void navigator.clipboard.writeText(gitBranchLabel);
              }}
              type="button"
            >
              <IconGitCommit aria-hidden="true" className="titlebar-git-icon" size={15} stroke={1.8} />
              <span className="titlebar-git-branch-field">
                <span className="titlebar-git-meta-label">Branch</span>
                <span className="titlebar-git-branch-name">
                  {gitBranchLabel}
                </span>
              </span>
            </button>
          </AppTooltip>
          <AppTooltip
            {...TITLEBAR_TOOLTIP_ROOT_PROPS}
            content="Open commit screen"
            contentClassName="titlebar-git-action-tooltip"
            side="left"
            sideOffset={6}
          >
            <TitlebarPanelMenuItem onClick={() => closeAfter(() => onRunGitAction("commit"))}>
              <IconCode aria-hidden="true" className="titlebar-git-icon" size={15} stroke={1.8} />
              <TitlebarGitStatPair firstCount={git.additions} label="Changes" secondCount={git.deletions} />
            </TitlebarPanelMenuItem>
          </AppTooltip>
          <TitlebarPanelMenuItem
            disabled={titlebarGitRemoteSyncDisabledReason(git) !== undefined}
            onClick={() => closeAfter(() => onRunGitAction("syncRemote"))}
          >
            {getTitlebarGitActionIcon("syncRemote")}
            <TitlebarGitStatPair
              firstCount={git.aheadCount}
              firstPrefix="↑"
              label="Commits"
              secondCount={git.behindCount}
              secondPrefix="↓"
              tone="commits"
            />
          </TitlebarPanelMenuItem>
          <TitlebarPanelMenuSeparator />
          <div className="titlebar-menu-section-label">Actions</div>
          {gitItems.map((item) => (
            <TitlebarPanelMenuItem
              disabled={item.disabled}
              key={item.action}
              onClick={() => closeAfter(() => onRunGitAction(item.action))}
            >
              {getTitlebarGitActionIcon(item.action)}
              <span>{item.label}</span>
            </TitlebarPanelMenuItem>
          ))}
        </div>
      ) : null}
      {kind === "actions" ? (
        <div className="titlebar-open-menu min-w-[220px] rounded-none border-border/80 p-1 text-[13px] text-foreground shadow-2xl">
          {visibleActions.length > 0 ? (
            visibleActions.map((command) => {
              const actionCommandPreview = getSidebarCommandPreviewLabel(command);
              return (
                <TitlebarPanelMenuItem
                  className="titlebar-action-menu-item"
                  key={command.commandId}
                  onClick={() => closeAfter(() => onRunAction(command))}
                >
                  <span className="titlebar-action-menu-icon">{getSidebarActionIcon(command)}</span>
                  <span className="titlebar-action-menu-copy">
                    <span className="titlebar-action-menu-title">
                      {getSidebarActionLabel(command)}
                    </span>
                    <AppTooltip
                      {...TITLEBAR_TOOLTIP_ROOT_PROPS}
                      content={actionCommandPreview}
                      contentClassName="titlebar-action-command-tooltip whitespace-normal text-left"
                      side="left"
                      sideOffset={6}
                    >
                      <span
                        className="titlebar-action-command-preview"
                        data-unconfigured={String(!isSidebarCommandConfigured(command))}
                      >
                        {actionCommandPreview}
                      </span>
                    </AppTooltip>
                  </span>
                  {selectedActionCommandId === command.commandId ? (
                    <IconCheck aria-hidden="true" className="ml-2 size-4 shrink-0 opacity-75" />
                  ) : null}
                </TitlebarPanelMenuItem>
              );
            })
          ) : (
            <div className="px-2 py-2 text-muted-foreground">No Actions configured</div>
          )}
          <TitlebarPanelMenuSeparator />
          <TitlebarPanelMenuItem
            onClick={() =>
              closeAfter(() =>
                window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
                  modal: "configureActions",
                  type: "open",
                }),
              )
            }
          >
            <IconSettings aria-hidden="true" size={16} />
            <span>Configure</span>
          </TitlebarPanelMenuItem>
        </div>
      ) : null}
      {kind === "settings" ? (
        <div className="titlebar-open-menu titlebar-settings-menu min-w-[220px] rounded-none border-border/80 p-1 text-[13px] text-foreground shadow-2xl">
          {/*
           * CDXC:SidebarTopChrome 2026-06-29-01:43:
           * The visible Settings trigger moved into the sidebar shortcut row as a normal dropdown. Keep this titlebar panel content aligned for native child-window compatibility paths, but do not render a titlebar Settings trigger.
           *
           * CDXC:TitlebarSettingsMenu 2026-06-19-00:35:
           * Menu rows need right-aligned shortcut labels. Commands must be titled "Commands" with Cmd+Shift+P in the shortcut column, and Pinned Prompts plus Scratch Pad stay hidden from this dropdown while remaining available elsewhere in the app.
           */}
          <TitlebarPanelMenuItem onClick={() => closeAfter(onOpenSettingsMenuSettings)}>
            <TitlebarSettingsMenuItemContent
              icon={<IconSettings aria-hidden="true" size={16} />}
              label="Settings"
              shortcut={formatTitlebarSettingsMenuShortcut(settingsMenuHotkeys.openSettings)}
            />
          </TitlebarPanelMenuItem>
          <TitlebarPanelMenuSeparator />
          <TitlebarPanelMenuItem onClick={() => closeAfter(onOpenSettingsMenuHotkeys)}>
            <TitlebarSettingsMenuItemContent
              icon={<IconKeyboard aria-hidden="true" size={16} />}
              label="Hotkeys"
              shortcut={formatTitlebarSettingsMenuShortcut(settingsMenuHotkeys.openHotkeys)}
            />
          </TitlebarPanelMenuItem>
          <TitlebarPanelMenuItem onClick={() => closeAfter(onOpenSettingsMenuCommands)}>
            <TitlebarSettingsMenuItemContent
              icon={<IconCommand aria-hidden="true" size={16} />}
              label="Commands"
              shortcut={formatTitlebarSettingsMenuShortcut(settingsMenuHotkeys.openCommandPalette)}
            />
          </TitlebarPanelMenuItem>
          <TitlebarPanelMenuSeparator />
          <TitlebarPanelMenuItem onClick={() => closeAfter(onWakePetFromSettingsMenu)}>
            <TitlebarSettingsMenuItemContent
              icon={<IconRobotFace aria-hidden="true" size={16} />}
              label="Wake Pet"
            />
          </TitlebarPanelMenuItem>
          <TitlebarPanelMenuItem onClick={() => closeAfter(onOpenSettingsMenuDiscord)}>
            <TitlebarSettingsMenuItemContent
              icon={<IconUsersGroup aria-hidden="true" size={16} />}
              label="Join Discord"
            />
          </TitlebarPanelMenuItem>
        </div>
      ) : null}
      {kind === "openIn" ? (
        <div className="titlebar-open-menu min-w-[220px] rounded-none border-border/80 p-1 text-[13px] text-foreground shadow-2xl">
          {visibleTargets.map((target) => (
            <TitlebarPanelMenuItem
              key={target.id}
              onClick={() => closeAfter(() => onOpenTarget(target))}
            >
              {getOpenTargetIcon(target)}
              <span className="min-w-0 flex-1 truncate">{target.label}</span>
              {activeTarget?.id === target.id ? (
                <IconCheck aria-hidden="true" className="ml-2 size-4 opacity-75" />
              ) : null}
            </TitlebarPanelMenuItem>
          ))}
          <TitlebarPanelMenuSeparator />
          <TitlebarPanelMenuItem
            onClick={() =>
              closeAfter(() =>
                window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
                  modal: "openTargets",
                  type: "open",
                }),
              )
            }
          >
            <IconSettings aria-hidden="true" size={16} />
            <span>Configure</span>
          </TitlebarPanelMenuItem>
        </div>
      ) : null}
    </div>
  );
}

function TitlebarPanelMenuItem({
  children,
  className,
  disabled,
  onClick,
}: {
  children: ReactNode;
  className?: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      className={["titlebar-open-menu-item", className].filter(Boolean).join(" ")}
      disabled={disabled}
      onClick={() => {
        if (!disabled) {
          onClick();
        }
      }}
      type="button"
    >
      {children}
    </button>
  );
}

function TitlebarSettingsMenuItemContent({
  icon,
  label,
  shortcut,
}: {
  icon: ReactNode;
  label: string;
  shortcut?: string;
}) {
  return (
    <>
      <span className="titlebar-settings-menu-icon">{icon}</span>
      <span className="titlebar-settings-menu-label">{label}</span>
      <span className="titlebar-settings-menu-shortcut">{shortcut ?? ""}</span>
    </>
  );
}

function formatTitlebarSettingsMenuShortcut(hotkey: string | undefined): string {
  return hotkey ? formatSidebarHotkeyLabel(hotkey) : "";
}

function TitlebarPanelMenuSeparator() {
  return <div aria-hidden="true" className="bg-border/70 titlebar-panel-menu-separator" />;
}

function getTitlebarKeepAwakeMenuLabel(label: string): string {
  return label === "Until turned off" ? label : `For ${label.toLowerCase()}`;
}

function getTitlebarThemeVariant(theme: SidebarTheme): "dark" | "light" {
  return theme.startsWith("light-") || theme === "plain-light" ? "light" : "dark";
}

type TitlebarRgbColor = {
  blue: number;
  green: number;
  red: number;
};

function parseTitlebarHexRgbColor(color: string): TitlebarRgbColor | undefined {
  const normalized = color.trim().toLowerCase();
  const match = /^#([0-9a-f]{6})$/u.exec(normalized);
  if (!match) {
    return undefined;
  }

  const hex = match[1];
  return {
    red: Number.parseInt(hex.slice(0, 2), 16),
    green: Number.parseInt(hex.slice(2, 4), 16),
    blue: Number.parseInt(hex.slice(4, 6), 16),
  };
}

function getTitlebarButtonSeparatorColorForBackground(backgroundColor: string): string {
  /**
   * CDXC:SidebarTitlebarColors 2026-06-15-15:01:
   * When the experimental sidebar/titlebar background gets lighter, titlebar
   * button separators should get darker so the chrome reads as deliberate lines
   * instead of faint raised rows.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-16:03:
   * A 90 contrast background made the previous separator curve nearly match
   * the background. Darken separators much faster as the background lightens so
   * the button dividers stay visible throughout the 85-100 slider range.
   *
   * CDXC:SidebarTitlebarColors 2026-06-16-15:52:
   * The 93 contrast + white tint default computes to #141414. The previous
   * curve crossed over there and returned #151515, making 1px titlebar button
   * separators disappear. Keep very dark backgrounds on the subtle lighter
   * separator curve, then switch to the dark divider floor once the background
   * reaches the new default range.
   *
   * CDXC:SidebarTitlebarColors 2026-06-19-12:33:
   * The titlebar now paints a horizontal gradient but separators use the solid
   * left stop. Keep very dark left stops on a lighter divider floor so
   * separators between titlebar items stay visible instead of blending into the
   * chrome.
   */
  const color = parseTitlebarHexRgbColor(backgroundColor);
  if (!color) {
    return "#252525";
  }

  const averageChannel = Math.round((color.red + color.green + color.blue) / 3);
  const separatorChannel =
    averageChannel <= 22
      ? 37
      : averageChannel <= 34
        ? Math.max(18, Math.min(37, Math.round(37 - (averageChannel - 22) * 1.5)))
        : 6;
  const separatorHex = separatorChannel.toString(16).padStart(2, "0");
  return `#${separatorHex}${separatorHex}${separatorHex}`;
}

function mergeTitlebarProjectState(
  current: TitlebarProjectState,
  state: Partial<TitlebarProjectState>,
): TitlebarProjectState {
  const customSidebarTitlebarBackgroundColor =
    state.customSidebarTitlebarBackgroundColor ?? current.customSidebarTitlebarBackgroundColor;
  const projectIdentity = {
    projectId: state.projectId ?? current.projectId,
    projectPath: state.projectPath ?? current.projectPath,
  };
  return {
    ...current,
    ...state,
    activeMode:
      state.activeMode === undefined
        ? current.activeMode
        : normalizeTitlebarMode(state.activeMode),
    agentHookStatus: state.agentHookStatus ?? current.agentHookStatus,
    ghostexCliStatus: state.ghostexCliStatus ?? current.ghostexCliStatus,
    portless: state.portless ?? current.portless,
    debuggingMode: state.debuggingMode ?? current.debuggingMode,
    diagnosticLogging:
      state.diagnosticLogging === undefined
        ? current.diagnosticLogging
        : normalizeghostexSettings({ diagnosticLogging: state.diagnosticLogging }).diagnosticLogging,
    showBetaFeatures: state.showBetaFeatures ?? current.showBetaFeatures,
    diffStats: state.diffStats ?? current.diffStats,
    git: resolveTitlebarGitStateForMerge(current.git, state.git, projectIdentity),
    gxserverDaemon: state.gxserverDaemon ?? current.gxserverDaemon,
    hotkeys: normalizeghostexHotkeySettings(state.hotkeys ?? current.hotkeys),
    keepAwake: state.keepAwake ?? current.keepAwake,
    browserTabs: state.browserTabs ?? current.browserTabs,
    codeEditorProjectIds: state.codeEditorProjectIds ?? current.codeEditorProjectIds,
    projectEditorCompanionPaneHidden:
      state.projectEditorCompanionPaneHidden ?? current.projectEditorCompanionPaneHidden,
    projectIsQuick: state.projectIsQuick ?? current.projectIsQuick,
    petOverlayEnabled: state.petOverlayEnabled ?? current.petOverlayEnabled,
    resourceGroups: state.resourceGroups ?? current.resourceGroups,
    sidebarTheme: state.sidebarTheme ?? current.sidebarTheme,
    customSidebarTitlebarColorsEnabled:
      state.customSidebarTitlebarColorsEnabled ?? current.customSidebarTitlebarColorsEnabled,
    customSidebarTitlebarForegroundColor: getSidebarTitlebarForegroundForBackground(
      customSidebarTitlebarBackgroundColor,
    ),
    customSidebarTitlebarBackgroundColor,
    sidebarActions: state.sidebarActions ?? current.sidebarActions,
    sidebarSide: state.sidebarSide ?? current.sidebarSide,
    sessionPersistenceProvider:
      state.sessionPersistenceProvider ?? current.sessionPersistenceProvider,
    toggleSidebarHotkeyLabel:
      state.toggleSidebarHotkeyLabel ?? current.toggleSidebarHotkeyLabel,
    workspaceOpenTargets: state.workspaceOpenTargets ?? current.workspaceOpenTargets,
    isFocusModeActive: state.isFocusModeActive ?? current.isFocusModeActive,
    promptEditorOpen: state.promptEditorOpen ?? current.promptEditorOpen,
    updateAvailable: state.updateAvailable ?? current.updateAvailable,
    updateDownloadProgress: Object.prototype.hasOwnProperty.call(state, "updateDownloadProgress")
      ? normalizeTitlebarUpdateDownloadProgress(state.updateDownloadProgress)
      : current.updateDownloadProgress,
    updateDownloading: state.updateDownloading ?? current.updateDownloading,
  };
}

function resolveTitlebarGitStateForMerge(
  current: SidebarGitState,
  incoming: SidebarGitState | undefined,
  projectIdentity: Pick<TitlebarProjectState, "projectId" | "projectPath">,
): SidebarGitState {
  const cached = readCachedTitlebarGitState(projectIdentity);
  if (incoming === undefined) {
    return shouldHydrateMissingTitlebarGitStateFromCache(current, cached) ? cached : current;
  }
  if (shouldUseCachedTitlebarGitState(incoming, cached)) {
    /*
     * CDXC:TitlebarGit 2026-06-16-19:19:
     * Git refresh publishes a transient busy/default state before branch and
     * diff probes finish. Keep the last cached project Git snapshot visible
     * during that refresh so titlebar dropdowns do not flash detached/default
     * metadata before the real branch result arrives.
     */
    return {
      ...cached,
      confirmSuggestedCommit: incoming.confirmSuggestedCommit,
      generateCommitBody: incoming.generateCommitBody,
      isBusy: incoming.isBusy,
      primaryAction: incoming.primaryAction,
    };
  }
  return incoming;
}

function shouldHydrateMissingTitlebarGitStateFromCache(
  current: SidebarGitState,
  cached: SidebarGitState | undefined,
): cached is SidebarGitState {
  return cached !== undefined && !isCacheableTitlebarGitState(current);
}

function shouldUseCachedTitlebarGitState(
  incoming: SidebarGitState,
  cached: SidebarGitState | undefined,
): cached is SidebarGitState {
  return (
    cached !== undefined &&
    incoming.isBusy &&
    incoming.branch === null &&
    (cached.branch !== null || cached.isRepo)
  );
}

function cacheTitlebarGitState(state: TitlebarProjectState): void {
  const cacheKey = titlebarGitStateCacheKey(state);
  if (cacheKey === undefined || state.git.isBusy || !isCacheableTitlebarGitState(state.git)) {
    return;
  }
  localStorage.setItem(cacheKey, JSON.stringify(state.git));
}

function isCacheableTitlebarGitState(state: SidebarGitState): boolean {
  return (
    state.isRepo ||
    state.hasCheckedGitHubRemote ||
    state.branch !== null ||
    state.files.length > 0
  );
}

function readCachedTitlebarGitState(
  projectIdentity: Pick<TitlebarProjectState, "projectId" | "projectPath">,
): SidebarGitState | undefined {
  const cacheKey = titlebarGitStateCacheKey(projectIdentity);
  if (cacheKey === undefined) {
    return undefined;
  }
  try {
    const parsed = JSON.parse(localStorage.getItem(cacheKey) || "null");
    return normalizeCachedTitlebarGitState(parsed);
  } catch {
    return undefined;
  }
}

function normalizeCachedTitlebarGitState(value: unknown): SidebarGitState | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const baseState = createDefaultSidebarGitState();
  return {
    ...baseState,
    additions: readCachedTitlebarNumber(value.additions),
    aheadCount: readCachedTitlebarNumber(value.aheadCount),
    behindCount: readCachedTitlebarNumber(value.behindCount),
    branch: typeof value.branch === "string" ? value.branch : null,
    confirmSuggestedCommit: value.confirmSuggestedCommit === true,
    deletions: readCachedTitlebarNumber(value.deletions),
    generateCommitBody: value.generateCommitBody !== false,
    hasCheckedGitHubRemote: value.hasCheckedGitHubRemote === true,
    hasGitHubCli: value.hasGitHubCli === true,
    hasGitHubRemote: value.hasGitHubRemote === true,
    hasOriginRemote: value.hasOriginRemote === true,
    hasUpstream: value.hasUpstream === true,
    hasWorkingTreeChanges: value.hasWorkingTreeChanges === true,
    files: normalizeCachedTitlebarGitFiles(value.files),
    isBusy: value.isBusy === true,
    isRepo: value.isRepo === true,
    isWorktree: value.isWorktree === true,
    pr: normalizeCachedTitlebarGitPullRequest(value.pr),
    primaryAction: normalizeCachedTitlebarGitAction(value.primaryAction, baseState.primaryAction),
    worktreeName: typeof value.worktreeName === "string" ? value.worktreeName : undefined,
  };
}

function normalizeCachedTitlebarGitAction(
  value: unknown,
  fallback: SidebarGitAction,
): SidebarGitAction {
  return value === "commit" ||
    value === "push" ||
    value === "pr" ||
    value === "syncRemote" ||
    value === "syncMain" ||
    value === "multiRelease" ||
    value === "release"
    ? value
    : fallback;
}

function normalizeCachedTitlebarGitPullRequest(value: unknown): SidebarGitState["pr"] {
  if (
    !isRecord(value) ||
    typeof value.title !== "string" ||
    typeof value.url !== "string" ||
    (value.state !== "open" && value.state !== "closed" && value.state !== "merged")
  ) {
    return null;
  }
  return {
    number:
      typeof value.number === "number" && Number.isFinite(value.number)
        ? value.number
        : undefined,
    state: value.state,
    title: value.title,
    url: value.url,
  };
}

function normalizeCachedTitlebarGitFiles(value: unknown): SidebarGitState["files"] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.flatMap((file) => {
    if (!isRecord(file) || typeof file.path !== "string") {
      return [];
    }
    return [{
      additions: readCachedTitlebarNumber(file.additions),
      deletions: readCachedTitlebarNumber(file.deletions),
      path: file.path,
    }];
  });
}

function readCachedTitlebarNumber(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function titlebarGitStateCacheKey(
  projectIdentity: Pick<TitlebarProjectState, "projectId" | "projectPath">,
): string | undefined {
  const projectKey = projectIdentity.projectId || projectIdentity.projectPath;
  return projectKey
    ? `${TITLEBAR_GIT_STATE_CACHE_STORAGE_PREFIX}${encodeURIComponent(projectKey)}`
    : undefined;
}

function formatToggleSidebarTooltipLabel(hotkey: string | undefined): string {
  if (!hotkey) {
    return "";
  }
  /*
   * CDXC:SidebarCollapse 2026-06-15-13:34:
   * The titlebar-side collapse control tooltip should name the command and
   * show the assigned shortcut, matching native hover help language while
   * preserving the empty label when Toggle Sidebar has no hotkey.
   */
  return `Toggle Sidebar (${formatSidebarHotkeyLabel(hotkey)})`;
}

function createInitialProjectState(bootstrap: Record<string, unknown>): TitlebarProjectState {
  const projectPath = typeof bootstrap.cwd === "string" ? bootstrap.cwd : "";
  const pathParts = projectPath.split("/").filter(Boolean);
  const sharedSettingsJson = isRecord(bootstrap.sharedSidebarStorage)
    ? bootstrap.sharedSidebarStorage.settings
    : undefined;
  const settings = normalizeghostexSettings(parseSharedSettings(sharedSettingsJson));
  const initialState: TitlebarProjectState = {
    activeMode: resolveInitialTitlebarMode(bootstrap),
    agentHookStatus: undefined,
    ghostexCliStatus: undefined,
    portless: undefined,
    browserTabs: [],
    codeEditorProjectIds: [],
    debuggingMode: settings.debuggingMode,
    diagnosticLogging: settings.diagnosticLogging,
    showBetaFeatures: settings.showBetaFeatures,
    diffStats: createDefaultSidebarProjectDiffStats(false),
    editorIsOpen: false,
    editorIsSleeping: false,
    editorStatus: "idle",
    git: createDefaultSidebarGitState(),
    gxserverDaemon: {
      alwaysStart: true,
      state: "unknown",
    },
    hotkeys: settings.hotkeys,
    keepAwake: createTitlebarKeepAwakeSettings(settings),
    projectEditorCompanionPaneHidden: false,
    projectIsQuick: false,
    projectName:
      (typeof bootstrap.workspaceName === "string" && bootstrap.workspaceName) ||
      pathParts[pathParts.length - 1] ||
      "Ghostex",
    projectPath,
    petOverlayEnabled: settings.petOverlayEnabled,
    resourceGroups: [],
    sidebarTheme: resolveSidebarTheme(settings.sidebarTheme, "dark"),
    customSidebarTitlebarColorsEnabled: settings.customSidebarTitlebarColorsEnabled,
    customSidebarTitlebarForegroundColor: getSidebarTitlebarForegroundForBackground(
      settings.customSidebarTitlebarBackgroundColor,
    ),
    customSidebarTitlebarBackgroundColor: settings.customSidebarTitlebarBackgroundColor,
    sidebarCollapsed: bootstrap.sidebarCollapsed === true,
    sidebarSide: bootstrap.sidebarSide === "right" ? "right" : settings.sidebarSide,
    sidebarActions: {
      commands: [],
    },
    showProjectEditorDiffFileCount: settings.showProjectEditorDiffFileCount,
    sessionPersistenceProvider: settings.sessionPersistenceProvider,
    terminalDevServerOpenTarget: settings.terminalDevServerOpenTarget,
    toggleSidebarHotkeyLabel: formatToggleSidebarTooltipLabel(
      settings.hotkeys.toggleSidebarCollapsed,
    ),
    workspaceOpenTargets: {
      availability: settings.workspaceOpenTargetAvailability,
      customTargets: settings.customWorkspaceOpenTargets,
      hiddenTargetIds: settings.workspaceOpenTargetHiddenIds,
    },
    updateAvailable: readInitialTitlebarUpdateAvailable(bootstrap),
    updateDownloadProgress: readInitialTitlebarUpdateDownloadProgress(bootstrap),
    updateDownloading: readInitialTitlebarUpdateDownloading(bootstrap),
  };
  /*
   * CDXC:ReactTitlebar 2026-06-11-18:06:
   * Native dropdown child windows need the latest titlebar project/resource
   * payload before first render. Swift injects that payload into the bootstrap
   * object at document start; merge it here so Resources does not briefly or
   * permanently render default state when the post-load bridge push races React.
   */
  const mergedState = mergeTitlebarProjectState(initialState, bootstrap as Partial<TitlebarProjectState>);
  cacheTitlebarGitState(mergedState);
  return mergedState;
}

function readInitialTitlebarUpdateAvailable(bootstrap: Record<string, unknown>): boolean {
  /**
   * CDXC:AutoUpdate 2026-06-08-18:21:
   * The native launch probe can finish before or during titlebar startup.
   * Accept both the injected bootstrap boolean and the pending native bridge
   * boolean so detected updates show the titlebar button on first render.
   */
  return bootstrap.updateAvailable === true || window.__ghostex_PENDING_TITLEBAR_UPDATE_AVAILABLE__ === true;
}

function readInitialTitlebarUpdateDownloading(bootstrap: Record<string, unknown>): boolean {
  /**
   * CDXC:AutoUpdate 2026-06-13-17:52:
   * Download animation is native-owned Sparkle state. Accept both the injected
   * bootstrap boolean and the pending bridge boolean so titlebar reloads do not
   * lose the active download indicator while an update is already downloading.
   */
  return bootstrap.updateDownloading === true || window.__ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOADING__ === true;
}

function readInitialTitlebarUpdateDownloadProgress(bootstrap: Record<string, unknown>): number | null {
  /**
   * CDXC:AutoUpdate 2026-06-30-22:18:
   * Download progress is a nullable native-owned ratio. Prefer the pending
   * bridge value over bootstrap because `null` is an intentional clear when
   * Sparkle leaves the download phase.
   */
  if (
    Object.prototype.hasOwnProperty.call(
      window,
      "__ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOAD_PROGRESS__",
    )
  ) {
    return normalizeTitlebarUpdateDownloadProgress(
      window.__ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOAD_PROGRESS__,
    );
  }
  return normalizeTitlebarUpdateDownloadProgress(bootstrap.updateDownloadProgress);
}

function createTitlebarKeepAwakeSettings(
  settings: ReturnType<typeof normalizeghostexSettings>,
): TitlebarKeepAwakeSettings {
  /*
   * CDXC:ExperimentalFeatures 2026-06-28-07:41:
   * The macOS Keep Awake feature is experimental-only. Build the titlebar-facing
   * state with one effective visibility flag so startup, Settings sync, and
   * native child dropdown windows all hide the button when Enable Experimental
   * Features is off.
   */
  const featureEnabled = settings.showBetaFeatures;
  return {
    activateOnExternalDisplay: settings.keepAwakeActivateOnExternalDisplay,
    activateOnLaunch: settings.keepAwakeActivateOnLaunch,
    allowDisplaySleep: settings.keepAwakeAllowDisplaySleep,
    batteryThresholdPercent: settings.keepAwakeBatteryThresholdPercent,
    deactivateBelowBatteryThreshold: settings.keepAwakeDeactivateBelowBatteryThreshold,
    deactivateOnLowPowerMode: settings.keepAwakeDeactivateOnLowPowerMode,
    deactivateOnUserSwitch: settings.keepAwakeDeactivateOnUserSwitch,
    defaultDurationMinutes: settings.keepAwakeDefaultDurationMinutes,
    delayedSendSessionCount: 0,
    featureEnabled,
    hideTitlebarControl: !featureEnabled || settings.hideKeepAwakeTitlebarControl,
    preventLidSleep: settings.keepAwakePreventLidSleep,
    whileWorkingSessions: settings.keepAwakeWhileWorkingSessions,
    workingSessionCount: 0,
  };
}

function readStoredKeepAwakeRuntime(): KeepAwakeRuntimeState | undefined {
  try {
    const parsed = JSON.parse(localStorage.getItem(KEEP_AWAKE_RUNTIME_STORAGE_KEY) || "null");
    return parseKeepAwakeRuntimeState(parsed);
  } catch {
    return undefined;
  }
}

function parseKeepAwakeRuntimeState(value: unknown): KeepAwakeRuntimeState | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const pid = typeof value.pid === "number" ? value.pid : Number.NaN;
  const durationMinutes = typeof value.durationMinutes === "number"
    ? value.durationMinutes
    : Number.NaN;
  if (
    !Number.isFinite(pid) ||
    pid <= 0 ||
    !KEEP_AWAKE_DURATION_OPTIONS.some((option) => option.value === durationMinutes)
  ) {
    return undefined;
  }
  return {
    durationMinutes: durationMinutes as KeepAwakeDurationMinutes,
    fireAtMs: typeof value.fireAtMs === "number" ? value.fireAtMs : undefined,
    pid,
    source: value.source === "automatic" ? "automatic" : "manual",
    startedAtMs: typeof value.startedAtMs === "number" ? value.startedAtMs : Date.now(),
  };
}

function readKeepAwakeRuntimeSyncState(raw: string | null): KeepAwakeRuntimeSyncState | undefined {
  try {
    const parsed = JSON.parse(raw || "null");
    if (!isRecord(parsed)) {
      return undefined;
    }
    const hasRuntime = Object.prototype.hasOwnProperty.call(parsed, "runtime");
    const runtime = parseKeepAwakeRuntimeState(parsed.runtime);
    return {
      ...(hasRuntime ? { runtime: runtime ?? null } : {}),
      suppressAutoStart: parsed.suppressAutoStart === true,
    };
  } catch {
    return undefined;
  }
}

function publishKeepAwakeRuntimeSync(state: KeepAwakeRuntimeSyncState): void {
  const payload = {
    runtime: state.runtime,
    suppressAutoStart: state.suppressAutoStart,
    updatedAtMs: Date.now(),
  };
  localStorage.setItem(KEEP_AWAKE_RUNTIME_SYNC_STORAGE_KEY, JSON.stringify(payload));
  window.dispatchEvent(new CustomEvent<KeepAwakeRuntimeSyncState>(KEEP_AWAKE_RUNTIME_CHANGED_EVENT, {
    detail: {
      runtime: state.runtime,
      suppressAutoStart: state.suppressAutoStart,
    },
  }));
}

function readStoredTitlebarTipIds(): Set<string> {
  try {
    const parsed = JSON.parse(localStorage.getItem(TITLEBAR_TIPS_READ_STORAGE_KEY) || "[]");
    if (!Array.isArray(parsed)) {
      return new Set();
    }
    return new Set(parsed.filter((id): id is string => typeof id === "string" && id.length > 0));
  } catch {
    return new Set();
  }
}

function writeStoredTitlebarTipIds(ids: Set<string>) {
  localStorage.setItem(TITLEBAR_TIPS_READ_STORAGE_KEY, JSON.stringify([...ids]));
}

async function applyKeepAwakeLidSleepPrevention(
  enabled: boolean,
  options: { installIfNeeded?: boolean } = {},
): Promise<boolean> {
  /**
   * CDXC:TitlebarKeepAwake 2026-05-28-19:28:
   * User-requested closed-lid wakefulness requires a privileged helper because
   * `caffeinate` cannot cover MacBook lid-close sleep. The helper is installed
   * only when this setting and Keep Awake are both active. Lease refreshes never
   * request installation, so cancelling the administrator prompt does not create
   * repeated password prompts; the user can retry by starting Keep Awake again.
   */
  try {
    const result = await runNativeKeepAwakeLidSleepPrevention(enabled, {
      installIfNeeded: options.installIfNeeded,
    });
    if (result.exitCode !== 0) {
      console.warn("Failed to update lid-close sleep prevention", result.stderr || result.stdout);
      return false;
    }
  } catch (error) {
    console.warn("Failed to update lid-close sleep prevention", error);
    return false;
  }
  return true;
}

async function readKeepAwakePowerSnapshot(options: {
  includeBattery: boolean;
  includeExternalDisplay: boolean;
  includeLowPowerMode: boolean;
}): Promise<
  | {
      batteryPercent?: number;
      externalDisplayConnected: boolean;
      lowPowerMode?: boolean;
    }
  | undefined
> {
  try {
    /*
    CDXC:TitlebarKeepAwake 2026-06-07-16:20:
    Keep Awake automation should not run heavyweight power probes just because
    Keep Awake is active. Build the shell command from the enabled rules so
    hidden checks skip system_profiler, pmset battery, or low-power reads when no
    rule can act on that value.
    */
    const result = await runNativeProcess("/bin/sh", [
      "-lc",
      [
        options.includeBattery
          ? "battery=$(/usr/bin/pmset -g batt 2>/dev/null | /usr/bin/awk -F';' '/InternalBattery/ {gsub(/[^0-9]/, \"\", $1); print $1; exit}')"
          : "battery=",
        options.includeLowPowerMode
          ? "low=$(/usr/bin/pmset -g 2>/dev/null | /usr/bin/awk '/lowpowermode/ {print $2; exit}')"
          : "low=",
        options.includeExternalDisplay
          ? "displays=$(/usr/sbin/system_profiler SPDisplaysDataType 2>/dev/null | /usr/bin/awk '/Resolution:/ {count++} END {print count+0}')"
          : "displays=0",
        "/bin/echo \"battery=${battery:-};low=${low:-};displays=${displays:-0}\"",
      ].join("; "),
    ]);
    if (result.exitCode !== 0) {
      return undefined;
    }
    const fields = new Map(
      result.stdout
        .trim()
        .split(";")
        .map((field) => {
          const [key, value = ""] = field.split("=");
          return [key, value] as const;
        }),
    );
    const batteryPercent = Number(fields.get("battery"));
    const displays = Number(fields.get("displays"));
    return {
      batteryPercent: Number.isFinite(batteryPercent) ? batteryPercent : undefined,
      externalDisplayConnected: Number.isFinite(displays) && displays > 1,
      lowPowerMode: fields.get("low") === "1",
    };
  } catch (error) {
    console.warn("Failed to read keep-awake power state", error);
    return undefined;
  }
}

function TitlebarTipsMenu({
  notices,
  onMarkRead,
  onOpenChangelog,
  onOpenDocs,
  onOpenHighlightedFeatures,
  onOpenNoticeSettings,
  onOpenTipAction,
  onViewGhostexGuide,
  readTips,
  unreadTips,
}: {
  notices: TitlebarNotice[];
  onMarkRead: (tipId: string) => void;
  onOpenChangelog: () => void;
  onOpenDocs: () => void;
  onOpenHighlightedFeatures: () => void;
  onOpenNoticeSettings: (notice: TitlebarNotice) => void;
  onOpenTipAction: (tip: TitlebarTip) => void;
  onViewGhostexGuide: () => void;
  readTips: TitlebarTip[];
  unreadTips: TitlebarTip[];
}) {
  return (
    <div className="titlebar-tips-panel" onClick={(event) => event.stopPropagation()}>
      <div className="titlebar-tips-header">
        <div className="titlebar-tips-title">
          <IconInfoCircle aria-hidden="true" size={18} stroke={1.8} />
          <span>Tips</span>
        </div>
        <div className="titlebar-tips-actions">
          <button
            aria-label="Open Docs"
            className="titlebar-tips-action-button"
            onClick={onOpenDocs}
            type="button"
          >
            <IconBook2 aria-hidden="true" size={14} stroke={1.9} />
            <span>Docs</span>
          </button>
          <button
            aria-label="Open Video"
            className="titlebar-tips-action-button"
            onClick={onOpenHighlightedFeatures}
            type="button"
          >
            <IconStarFilled aria-hidden="true" size={14} />
            <span>Video</span>
          </button>
          <button
            aria-label="Setup"
            className="titlebar-tips-action-button"
            onClick={onViewGhostexGuide}
            type="button"
          >
            <IconTool aria-hidden="true" size={14} stroke={1.9} />
            <span>Setup</span>
          </button>
          <button
            aria-label="Open Updates"
            className="titlebar-tips-action-button"
            onClick={onOpenChangelog}
            type="button"
          >
            <IconHistory aria-hidden="true" size={14} stroke={1.9} />
            <span>Updates</span>
          </button>
        </div>
      </div>
      <div className="titlebar-tips-scroll">
        {notices.length > 0 ? (
          <TitlebarTipsSection
            count={notices.length}
            emptyText=""
            title="Notices"
          >
            {notices.map((notice) => (
              <TitlebarNoticeRow
                key={notice.id}
                notice={notice}
                onOpenSettings={() => onOpenNoticeSettings(notice)}
              />
            ))}
          </TitlebarTipsSection>
        ) : null}
        {/*
         CDXC:TipsAndTricks 2026-06-12-10:56:
         Hide the Unread section when every tip is read so the panel does not show an empty "All caught up." block.
        */}
        {unreadTips.length > 0 ? (
          <TitlebarTipsSection
            count={unreadTips.length}
            emptyText=""
            title="Unread"
          >
            {unreadTips.map((tip) => (
              <TitlebarTipRow
                key={tip.id}
                onMarkRead={onMarkRead}
                onOpenTipAction={onOpenTipAction}
                read={false}
                tip={tip}
              />
            ))}
          </TitlebarTipsSection>
        ) : null}
        <TitlebarTipsSection
          count={readTips.length}
          emptyText="No read tips yet."
          title="Read"
        >
          {readTips.map((tip) => (
            <TitlebarTipRow
              key={tip.id}
              onMarkRead={onMarkRead}
              onOpenTipAction={onOpenTipAction}
              read
              tip={tip}
            />
          ))}
        </TitlebarTipsSection>
      </div>
    </div>
  );
}

/**
 * CDXC:TipsAndTricks 2026-06-12-08:20:
 * Tips & Tricks section headers must stay expanded. Collapsible Notices, Unread,
 * and Read groups hid content behind extra clicks without improving scanability.
 *
 * CDXC:TipsAndTricks 2026-06-12-23:28:
 * The macOS Tips & Tricks panel should not show right-aligned section counts.
 * Keep the item count internal for empty-state rendering, but make section
 * headers read as labels only.
 */
function TitlebarTipsSection({
  children,
  count,
  emptyText,
  title,
}: {
  children: ReactNode;
  count: number;
  emptyText: string;
  title: string;
}) {
  return (
    <section className="titlebar-tips-section">
      <div className="titlebar-tips-section-heading">
        <span>{title}</span>
      </div>
      <div className="titlebar-tips-list">
        {count > 0 ? children : <div className="titlebar-tips-empty">{emptyText}</div>}
      </div>
    </section>
  );
}

function TitlebarNoticeRow({
  notice,
  onOpenSettings,
}: {
  notice: TitlebarNotice;
  onOpenSettings: () => void;
}) {
  return (
    <button
      aria-label={`${notice.title}. Open related settings.`}
      className="titlebar-tip-row titlebar-tip-row-notice"
      data-read="false"
      onClick={onOpenSettings}
      type="button"
    >
      <div className="titlebar-tip-icon">{getTitlebarTipIcon(notice.icon)}</div>
      <div className="titlebar-tip-copy">
        <div className="titlebar-tip-title">{notice.title}</div>
        <div className="titlebar-tip-body">{notice.body}</div>
      </div>
    </button>
  );
}

function TitlebarTipRow({
  onMarkRead,
  onOpenTipAction,
  read,
  tip,
}: {
  onMarkRead: (tipId: string) => void;
  onOpenTipAction: (tip: TitlebarTip) => void;
  read: boolean;
  tip: TitlebarTip;
}) {
  const detailContent = (
    <>
      <span className="titlebar-tip-icon">{getTitlebarTipIcon(tip.icon)}</span>
      <span className="titlebar-tip-copy">
        <span className="titlebar-tip-title">{tip.title}</span>
        <span className="titlebar-tip-body">{tip.body}</span>
      </span>
    </>
  );
  return (
    <article
      className="titlebar-tip-row"
      data-actionable={String(Boolean(tip.action))}
      data-read={String(read)}
    >
      {tip.action ? (
        <button
          aria-label={`${tip.title}. Open related details.`}
          className="titlebar-tip-detail titlebar-tip-detail-button"
          onClick={() => onOpenTipAction(tip)}
          type="button"
        >
          {detailContent}
        </button>
      ) : (
        <span className="titlebar-tip-detail">{detailContent}</span>
      )}
      {read ? (
        <span className="titlebar-tip-read-state" aria-label="Read">
          <IconCheck aria-hidden="true" size={15} stroke={1.9} />
        </span>
      ) : (
        <button
          aria-label={`Mark ${tip.title} as read`}
          className="titlebar-tip-read-button"
          onClick={() => onMarkRead(tip.id)}
          type="button"
        >
          <IconCheck aria-hidden="true" size={15} stroke={1.9} />
        </button>
      )}
    </article>
  );
}

function getTitlebarTipIcon(icon: TitlebarTipIcon): ReactNode {
  switch (icon) {
    case "browser":
      return <IconWorld aria-hidden="true" size={16} stroke={1.8} />;
    case "command":
      return <IconCommand aria-hidden="true" size={16} stroke={1.8} />;
    case "moon":
      return <IconMoon aria-hidden="true" size={16} stroke={1.8} />;
    case "resources":
      return <IconDeviceDesktop aria-hidden="true" size={16} stroke={1.8} />;
    case "search":
      return <IconSearch aria-hidden="true" size={16} stroke={1.8} />;
    case "sidebar":
      return <IconLayoutSidebarLeftExpand aria-hidden="true" size={16} stroke={1.8} />;
    case "warning":
      return <IconAlertTriangle aria-hidden="true" size={16} stroke={1.8} />;
  }
}

function TitlebarResourcesMenu({
  browserBundles,
  codeIdeBundles,
  collapsedKeys,
  daemon,
  groupViews,
  inactiveTerminalSleepSessionCount,
  onFocusSession,
  onGxserverAlwaysStartChange,
  onGxserverRestart,
  onGxserverStart,
  onGxserverStop,
  onQuit,
  onSetResourceItemsCollapsed,
  processSnapshotReady,
  processTotals,
  onSleepInactiveSessions,
  onToggle,
  orphanBundles,
  quittingKeys,
  serverBundles,
  serverOpenTarget,
  sessionPersistenceProvider,
}: {
  browserBundles: ResourceProcessBundle[];
  codeIdeBundles: ResourceProcessBundle[];
  collapsedKeys: Set<string>;
  daemon: TitlebarGxserverDaemonStatus;
  groupViews: ResourceGroupView[];
  inactiveTerminalSleepSessionCount: number;
  onFocusSession: (sessionId: string) => void;
  onGxserverAlwaysStartChange: (enabled: boolean) => void;
  onGxserverRestart: () => void;
  onGxserverStart: () => void;
  onGxserverStop: () => void;
  onQuit: (bundles: ResourceProcessBundle[]) => void;
  onSetResourceItemsCollapsed: (
    targets: readonly ResourceItemCollapseTarget[],
    collapsed: boolean,
  ) => void;
  processSnapshotReady: boolean;
  processTotals: ResourceProcessTotals;
  onSleepInactiveSessions: () => void;
  onToggle: (key: string) => void;
  orphanBundles: ResourceProcessBundle[];
  quittingKeys: Set<string>;
  serverBundles: ResourceProcessBundle[];
  serverOpenTarget: TerminalDevServerOpenTarget;
  sessionPersistenceProvider?: Exclude<SessionPersistenceProvider, "off">;
}) {
  const visibleGroupViews = processSnapshotReady
    ? groupViews.filter((view) => view.bundles.length > 0)
    : [];
  const metricBundles = processSnapshotReady
    ? [
        ...visibleGroupViews.flatMap((view) => view.bundles),
        ...codeIdeBundles,
        ...browserBundles,
        ...orphanBundles,
      ]
    : [];
  /*
   * CDXC:TitlebarResources 2026-06-22-00:30:
   * Dev-server rows intentionally duplicate process ownership for discovery,
   * so bundle lists used for row controls avoid folding those duplicates into
   * Sleep/Close targets while row and section metrics still show each listener's
   * current process usage.
   */
  const allBundles = processSnapshotReady ? [...serverBundles, ...metricBundles] : [];
  /**
   * CDXC:TitlebarResources 2026-05-23-10:52:
   * Header actions should be two matching resource controls: one for sleeping
   * only inactive terminal sessions, and one for sleeping all terminal session
   * resources without targeting the app runtime.
   *
   * CDXC:TitlebarResources 2026-06-12-23:37:
   * Header Sleep actions should rely on visible labels and normal button hover
   * instead of tooltip wrappers. Sleep releases live CPU/RAM while preserving
   * the sidebar card, but clickability is more important than hover copy here.
   *
   * CDXC:TitlebarResources 2026-05-25-16:53:
   * The Resources dropdown should manage user-owned work resources, not expose
   * Ghostex's own app-runtime process rows. Keep app process matching available
   * for internal PID ownership, but exclude App Runtime bundles from visible
   * sections and bulk resource actions.
   *
   * CDXC:TitlebarResources 2026-06-30-23:17:
   * The header total is different from row actions: it reports Ghostex's full
   * owned process footprint so it matches external app monitors, while Sleep
   * and Close stay scoped to visible user-resource bundles.
   *
   * CDXC:TitlebarResources 2026-05-25-16:59:
   * The old yellow zmx warning duplicated the action wording and made the menu
   * noisier than the controls themselves. Remove that note and expose the bulk
   * terminal action as Sleep All only when session persistence is active through
   * tmux, zmx, or zellij.
   */
  const persistentSessionMode =
    sessionPersistenceProvider === "tmux" ||
    sessionPersistenceProvider === "zmx" ||
    sessionPersistenceProvider === "zellij";
  const sleepAllSessionBundles = visibleGroupViews
    .flatMap((view) => view.bundles)
    .filter((bundle) => bundle.type === "session" && bundle.session?.sessionKind === "terminal");
  /**
   * CDXC:TitlebarResources 2026-05-24-20:58:
   * Resource summary and row-action tooltips must stay compact enough for the titlebar area.
   * Keep explanatory copy short and apply the width cap inline because the
   * shared TooltipContent sets its viewport cap with inline styles.
   *
   * CDXC:TitlebarResources 2026-05-25-09:37:
   * Resource summary tooltips need the same compact width cap as action
   * tooltips so Live CPU and Live memory do not stretch across the toolbar.
   *
   * CDXC:TitlebarResources 2026-06-11-18:13:
   * Keep the fixed-size native Resources dropdown stable while the first process table loads.
   * The native child window stays hidden until this view commits with real snapshot data; the loading copy is only an internal fallback.
   *
   * CDXC:TitlebarResources 2026-06-12-23:33:
   * The header bulk control targets individual expandable resource rows, not
   * the top-level Projects, Browser Tabs, or Orphaned / Detached sections.
   * Keep section containers expanded while the button toggles the same row
   * disclosure state as the per-item chevrons.
   *
   * CDXC:TitlebarResources 2026-06-12-23:37:
   * Header Sleep actions should behave like normal buttons: always visible,
   * always hit-testable, and styled by ordinary CSS :hover/:disabled states.
   * Avoid React hover gates and native-pointer body flags because they made the
   * child-panel buttons appear visible while still rejecting clicks.
  */
  const resourceTooltipStyle = { maxWidth: 220 };
  const liveCpuLabel = processSnapshotReady ? formatWholePercent(processTotals.cpu) : "--";
  const liveMemoryLabel = processSnapshotReady ? formatResourceMemory(processTotals.memoryMb) : "--";
  const resourceItemCollapseTargets = createResourceItemCollapseTargets(allBundles);
  const allResourceItemsCollapsed =
    resourceItemCollapseTargets.length > 0 &&
    resourceItemCollapseTargets.every((target) => isResourceItemCollapsed(target, collapsedKeys));
  const resourceItemToggleLabel = allResourceItemsCollapsed
    ? "Expand all resource items"
    : "Collapse all resource items";
  const [resourcesInfoOpen, setResourcesInfoOpen] = useState(false);
  return (
    <div className="titlebar-resources-panel">
      <div className="titlebar-resources-header">
        <div className="titlebar-resources-title">
          <IconDeviceDesktop aria-hidden="true" size={18} />
          <span>Resources</span>
        </div>
        <div className="titlebar-resources-actions">
          <div className="titlebar-resources-info-control">
            <button
              aria-expanded={resourcesInfoOpen}
              aria-label="Resources information"
              className="titlebar-resources-info-button"
              onClick={() => setResourcesInfoOpen((open) => !open)}
              type="button"
            >
              {/*
               * CDXC:TitlebarResources 2026-06-16-01:08:
               * Resources explanatory copy belongs behind a click-only info
               * affordance beside the bulk expand/collapse control. Keep the
               * dropdown 400px wide and separate each note line with whitespace
               * so the header stays compact while the copy remains available.
               *
               * CDXC:TitlebarResources 2026-06-16-01:54:
               * The info dropdown must fit inside the Resources panel and draw
               * only one background/border surface. Position it from the full
               * header instead of the small icon wrapper so the 400px text area
               * is not clipped.
               *
               * CDXC:TitlebarResources 2026-06-16-02:02:
               * Make the info dropdown wider and lighter than the Resources
               * panel so the explanatory sentences can fit without looking
               * like the same dark layer as the modal behind it.
               */}
              <IconInfoCircle aria-hidden="true" size={14} stroke={1.9} />
            </button>
            {resourcesInfoOpen ? (
              <div className="titlebar-resources-info-popover" role="dialog">
                <div className="titlebar-resources-info-note">
                  <p>This app uses native Ghostty terminals as they're lighter on CPU & RAM than electron/web terminals.</p>
                  <p>The RAM use you see here is the lowest possible for the Agent CLI that you're using.</p>
                  <p>Keep in mind that each CLI uses more/less RAM based on a lot of factors.</p>
                  <p>You can easily sleep all inactive terminals here (Auto-sleep can be configured in settings).</p>
                </div>
              </div>
            ) : null}
          </div>
          <button
            aria-label={resourceItemToggleLabel}
            className="titlebar-resources-collapse-all-button"
            disabled={resourceItemCollapseTargets.length === 0}
            onClick={() =>
              onSetResourceItemsCollapsed(resourceItemCollapseTargets, !allResourceItemsCollapsed)
            }
            type="button"
          >
            {/*
             * CDXC:TitlebarResources 2026-06-12-23:33:
             * The header expand/collapse control belongs to Resources itself:
             * it sits immediately before Sleep Inactive and toggles individual
             * expandable resource items inside each group. It must not collapse
             * Projects, Browser Tabs, or Orphaned / Detached as sections.
             *
             * CDXC:TitlebarResources 2026-06-13-01:54:
             * Match the sidebar Projects bulk-control icon language: the
             * collapse action uses IconArrowsDiagonalMinimize, while the expand
             * action uses IconArrowsDiagonal2.
             */}
            {allResourceItemsCollapsed ? (
              <IconArrowsDiagonal2 aria-hidden="true" size={14} stroke={1.9} />
            ) : (
              <IconArrowsDiagonalMinimize aria-hidden="true" size={14} stroke={1.9} />
            )}
          </button>
          <button
            className="titlebar-resources-action-button"
            data-enabled={String(inactiveTerminalSleepSessionCount > 0)}
            data-variant="sleep"
            disabled={inactiveTerminalSleepSessionCount === 0}
            onClick={onSleepInactiveSessions}
            type="button"
          >
            <IconMoon aria-hidden="true" size={14} stroke={1.8} />
            <span>Sleep Inactive</span>
          </button>
          {persistentSessionMode ? (
            <>
              <button
                className="titlebar-resources-action-button"
                data-variant="sleep"
                disabled={sleepAllSessionBundles.length === 0}
                onClick={() => onQuit(sleepAllSessionBundles)}
                type="button"
              >
                <IconMoon aria-hidden="true" size={14} stroke={1.9} />
                <span>Sleep All</span>
              </button>
            </>
          ) : null}
          <div className="titlebar-resources-summary">
            <AppTooltip
              {...TITLEBAR_TOOLTIP_ROOT_PROPS}
              content={
                <>
                  <span className="titlebar-resource-tooltip-title">Live CPU</span>
                  <span>CPU used by Ghostex and owned child processes.</span>
                </>
              }
              contentClassName="titlebar-resource-tooltip"
              contentStyle={resourceTooltipStyle}
            >
              <span>
                <IconCpu aria-hidden="true" size={13} stroke={1.8} />
                {liveCpuLabel}
              </span>
            </AppTooltip>
            <AppTooltip
              {...TITLEBAR_TOOLTIP_ROOT_PROPS}
              content={
                <>
                  <span className="titlebar-resource-tooltip-title">Live memory</span>
                  <span>RAM used by Ghostex and owned child processes, including app runtime and helper processes.</span>
                </>
              }
              contentClassName="titlebar-resource-tooltip"
              contentStyle={resourceTooltipStyle}
            >
              <span>
                <IconDeviceDesktop aria-hidden="true" size={13} stroke={1.8} />
                {liveMemoryLabel}
              </span>
            </AppTooltip>
          </div>
        </div>
      </div>
      <div className="titlebar-resources-scroll" data-loading={String(!processSnapshotReady)}>
        <TitlebarGxserverDaemonSection
          daemon={daemon}
          onAlwaysStartChange={onGxserverAlwaysStartChange}
          onRestart={onGxserverRestart}
          onStart={onGxserverStart}
          onStop={onGxserverStop}
        />
        {processSnapshotReady ? (
          <>
            {/*
             * CDXC:TitlebarResources 2026-06-22-00:30:
             * Running dev servers should be the first Resources body section,
             * above project session resource groups, so localhost ports are
             * discoverable before users scan terminal/session rows.
             */}
            <TitlebarResourceSection
              collapsedKeys={collapsedKeys}
              onQuit={onQuit}
              onFocusSession={onFocusSession}
              onToggle={onToggle}
              quittingKeys={quittingKeys}
              serverOpenTarget={serverOpenTarget}
              title="Dev Servers"
              bundles={serverBundles}
            />
            {visibleGroupViews.length > 0 ? (
              visibleGroupViews.map((view) => (
                <TitlebarResourceSection
                  collapsedKeys={collapsedKeys}
                  key={view.group.groupId}
                  onQuit={onQuit}
                  onFocusSession={onFocusSession}
                  onToggle={onToggle}
                  quittingKeys={quittingKeys}
                  title={view.group.title}
                  bundles={view.bundles}
                />
              ))
            ) : (
              <div className="titlebar-resources-empty">No grouped sessions matched running processes.</div>
            )}
            {/*
             * CDXC:TitlebarResources 2026-06-22-13:50:
             * The shared embedded Code runtime belongs after project-owned session groups and before Browser Tabs, where users expect app-wide IDE infrastructure rather than a specific project process.
             */}
            <TitlebarResourceSection
              collapsedKeys={collapsedKeys}
              onQuit={onQuit}
              onFocusSession={onFocusSession}
              onToggle={onToggle}
              quittingKeys={quittingKeys}
              title="Code IDE"
              bundles={codeIdeBundles}
            />
            <TitlebarResourceSection
              collapsedKeys={collapsedKeys}
              onQuit={onQuit}
              onFocusSession={onFocusSession}
              onToggle={onToggle}
              quittingKeys={quittingKeys}
              title="Browser Tabs"
              bundles={browserBundles}
            />
            <TitlebarResourceSection
              collapsedKeys={collapsedKeys}
              onQuit={onQuit}
              onFocusSession={onFocusSession}
              onToggle={onToggle}
              quittingKeys={quittingKeys}
              title="Orphaned / Detached"
              bundles={orphanBundles}
            />
          </>
        ) : (
          <div className="titlebar-resources-loading" role="status" aria-live="polite">
            <IconLoader2 aria-hidden="true" className="titlebar-resources-loading-icon" size={16} stroke={1.9} />
            <span>Loading resources...</span>
          </div>
        )}
      </div>
    </div>
  );
}

function TitlebarGxserverDaemonSection({
  daemon,
  onAlwaysStartChange,
  onRestart,
  onStart,
  onStop,
}: {
  daemon: TitlebarGxserverDaemonStatus;
  onAlwaysStartChange: (enabled: boolean) => void;
  onRestart: () => void;
  onStart: () => void;
  onStop: () => void;
}) {
  const isRunning = daemon.state === "running";
  const isStarting = daemon.state === "starting";
  const shouldShowReloadApp = !isRunning;
  const statusLabel = daemon.version
    ? `${daemon.state} - v${daemon.version}`
    : daemon.state;
  return (
    <section className="titlebar-gxserver-daemon">
      <div className="titlebar-gxserver-daemon-main">
        <span className="titlebar-gxserver-daemon-dot" data-state={daemon.ok === false ? "error" : daemon.state} />
        <div className="titlebar-gxserver-daemon-copy">
          {daemon.message ? <span>{daemon.message}</span> : null}
          <span>{statusLabel}</span>
        </div>
      </div>
      <div className="titlebar-gxserver-daemon-controls">
        {/*
         * CDXC:TitlebarDaemonControls 2026-06-12-11:51:
         * The Resources dropdown should expose Restart as the primary daemon action. Hide manual Start/Stop controls so users do not manage daemon lifecycle from this compact status row.
         */}
        <AppTooltip
          {...TITLEBAR_TOOLTIP_ROOT_PROPS}
          content="Restart daemon"
          contentClassName="titlebar-resource-tooltip"
        >
          <button
            aria-label="Restart gxserver"
            className="titlebar-gxserver-daemon-icon-button"
            disabled={isStarting}
            onClick={onRestart}
            type="button"
          >
            <IconRefresh aria-hidden="true" size={14} stroke={1.9} />
          </button>
        </AppTooltip>
        {shouldShowReloadApp ? (
          <AppTooltip
            {...TITLEBAR_TOOLTIP_ROOT_PROPS}
            content="Reload app"
            contentClassName="titlebar-resource-tooltip"
          >
            <button
              aria-label="Reload Ghostex"
              className="titlebar-gxserver-daemon-icon-button"
              onClick={() => {
                window.location.reload();
              }}
              type="button"
            >
              <IconRefresh aria-hidden="true" size={14} stroke={1.9} />
            </button>
          </AppTooltip>
        ) : null}
        {/*
         * CDXC:TitlebarDaemonControls 2026-06-12-11:51:
         * If gxserver is off unexpectedly, show Reload App as the recovery action so the webview can rehydrate and reconnect instead of asking users to manually start the daemon here.
         */}
        {/*
         * CDXC:TitlebarDaemonControls 2026-06-12-11:56:
         * Hide the Always start checkbox from the compact Resources daemon row; this status surface should only offer Restart, plus Reload App when gxserver is off.
         */}
        {/* <label className="titlebar-gxserver-daemon-checkbox">
          <input
            checked={daemon.alwaysStart}
            onChange={(event) => onAlwaysStartChange(event.currentTarget.checked)}
            type="checkbox"
          />
          <span>Always start</span>
        </label> */}
      </div>
    </section>
  );
}

function TitlebarResourceSection({
  bundles,
  collapsedKeys,
  onQuit,
  onFocusSession,
  onToggle,
  quittingKeys,
  serverOpenTarget,
  title,
}: {
  bundles: ResourceProcessBundle[];
  collapsedKeys: Set<string>;
  onQuit: (bundles: ResourceProcessBundle[]) => void;
  onFocusSession: (sessionId: string) => void;
  onToggle: (key: string) => void;
  quittingKeys: Set<string>;
  serverOpenTarget?: TerminalDevServerOpenTarget;
  title: string;
}) {
  if (bundles.length === 0) {
    return null;
  }
  const sectionCpu = sumBundleCpu(bundles);
  const sectionMemory = sumBundleMemory(bundles);
  const sortedBundles = sortResourceBundlesForDisplay(bundles, quittingKeys);
  const actionableBundles = bundles.filter(isResourceBundleActionable);
  const hasTerminalSession = actionableBundles.some(
    (bundle) => bundle.type === "session" && bundle.session?.sessionKind === "terminal",
  );
  const hasServer = actionableBundles.some((bundle) => bundle.type === "server");
  const sectionActionBundles = hasTerminalSession
    ? actionableBundles.filter((bundle) => bundle.type === "session" && bundle.session?.sessionKind === "terminal")
    : actionableBundles;
  const sectionActionLabel = hasTerminalSession ? "Sleep Project" : hasServer ? "Stop Servers" : "Quit";
  const sectionActionTooltipTitle = hasTerminalSession
    ? "Sleep project"
    : hasServer
      ? "Stop servers"
      : "Quit this group";
  const sectionActionTooltipBody = hasTerminalSession
    ? "Sleeps this project's terminal sessions and keeps them restorable in the sidebar."
    : hasServer
      ? "Stops the listener-backed server processes without sleeping the owning terminal sessions."
      : "Stops user-owned live processes and closes related surfaces.";
  /**
   * CDXC:TitlebarResources 2026-05-25-14:21:
   * Resource action tooltips share the compact width cap used by header and
   * summary tooltips, including Quit group, so long process-management copy
   * wraps near the hovered control instead of spanning the window.
   *
   * CDXC:TitlebarResources 2026-05-26-13:11:
   * Project resource groups that include terminal sessions should expose the
   * group action as Sleep Project, not Quit. Limit that action to terminal
   * session bundles so browser/code resources are not closed by a sleep-labeled
   * control.
   *
   * CDXC:TitlebarResources 2026-06-11-18:30:
   * Resource section headers are static labels now: no per-section chevron and
   * no click target, so the fixed native dropdown avoids visually noisy
   * competing collapse controls.
   *
   * CDXC:TitlebarResources 2026-06-12-23:33:
   * The single header button controls the individual resource rows in bulk,
   * not this section container. Always render section bodies so Projects,
   * Browser Tabs, and Orphaned / Detached remain visible grouping labels.
   *
   * CDXC:TitlebarResources 2026-06-15-13:45:
   * Section-level Quit must target the same action-eligible resources as row
   * Close. Keep shared browser helper bundles visible for diagnostics, but do
   * not let a bulk action close infrastructure that embedded browser panes need
   * to keep working.
   *
   * CDXC:TitlebarResources 2026-06-22-00:30:
   * Dev Servers rows use Stop language because the action targets only the
   * listener process tree. Do not route those rows through session sleep or
   * project close semantics.
   */
  const resourceTooltipStyle = { maxWidth: 220 };
  return (
    <section className="titlebar-resource-section">
      <div className="titlebar-resource-section-heading">
        <div className="titlebar-resource-section-label">
          <span>{title}</span>
          <span className="titlebar-resource-section-summary">
            <span>
              <IconCpu aria-hidden="true" size={12} stroke={1.8} />
              {formatWholePercent(sectionCpu)}
            </span>
            <span>
              <IconDeviceDesktop aria-hidden="true" size={12} stroke={1.8} />
              {formatResourceMemory(sectionMemory)}
            </span>
            <span className="titlebar-resource-section-count">{bundles.length}</span>
          </span>
        </div>
        {sectionActionBundles.length > 0 ? (
          <AppTooltip
            {...TITLEBAR_TOOLTIP_ROOT_PROPS}
            content={
              <>
                <span className="titlebar-resource-tooltip-title">{sectionActionTooltipTitle}</span>
                <span>{sectionActionTooltipBody}</span>
              </>
            }
            contentClassName="titlebar-resource-tooltip"
            contentStyle={resourceTooltipStyle}
          >
            <button
              className="titlebar-resource-section-quit-button"
              data-action={hasTerminalSession ? "sleep" : hasServer ? "stop" : "quit"}
              onClick={() => onQuit(sectionActionBundles)}
              type="button"
            >
              {sectionActionLabel}
            </button>
          </AppTooltip>
        ) : null}
      </div>
      <div className="titlebar-resource-section-body">
        {sortedBundles.map((bundle) => (
          <TitlebarResourceBundle
            bundle={bundle}
            collapsedKeys={collapsedKeys}
            isQuitting={quittingKeys.has(bundle.key)}
            key={bundle.key}
            onFocusSession={onFocusSession}
            onQuit={onQuit}
            onToggle={onToggle}
            serverOpenTarget={serverOpenTarget}
          />
        ))}
      </div>
    </section>
  );
}

function TitlebarResourceBundle({
  bundle,
  collapsedKeys,
  isQuitting,
  onQuit,
  onFocusSession,
  onToggle,
  serverOpenTarget,
}: {
  bundle: ResourceProcessBundle;
  collapsedKeys: Set<string>;
  isQuitting: boolean;
  onQuit: (bundles: ResourceProcessBundle[]) => void;
  onFocusSession: (sessionId: string) => void;
  onToggle: (key: string) => void;
  serverOpenTarget?: TerminalDevServerOpenTarget;
}) {
  const hasChildren = bundle.childProcesses.length > 0;
  /**
   * CDXC:TitlebarResources 2026-05-16-18:28:
   * Sessions often own several agent/runtime child processes, so their rows
   * should start collapsed to keep the Resources menu scannable. Store only
   * explicit user expansions for session bundles while section rows and other
   * bundle types keep the existing collapsed-key behavior.
   *
   * CDXC:TitlebarResources 2026-06-12-23:33:
   * The Resources header bulk toggle uses the same target helper as row
   * chevrons so it collapses individual items inside groups, not the group
   * sections themselves.
   */
  const bundleCollapseTarget = createResourceItemCollapseTarget(bundle);
  const bundleToggleKey = bundleCollapseTarget?.key ?? bundle.key;
  const isCollapsed = bundleCollapseTarget
    ? isResourceItemCollapsed(bundleCollapseTarget, collapsedKeys)
    : false;
  /**
   * CDXC:TitlebarResources 2026-05-23-10:52:
   * Terminal-session Quit from Resources terminates the live process tree but
   * intentionally keeps the session card in the sidebar as sleeping. Use the
   * sleep affordance for those rows; keep the quit affordance for browser,
   * code, and detached process rows that are actually removed or closed.
   */
  const preservesSidebarSession =
    bundle.type === "session" && bundle.session?.sessionKind === "terminal";
  const isServer = bundle.type === "server";
  const serverPortless = bundle.portless;
  const mainLabel = getResourceBundleMainLabel(bundle);
  const mainUrl = getResourceBundleMainUrl(bundle);
  const showPortlessSetupAction =
    isServer && serverPortless !== undefined && !serverPortless.isSetupActive;
  const focusSessionId = resourceBundleFocusSessionId(bundle);
  const isActionable = isResourceBundleActionable(bundle);
  const actionLabel = preservesSidebarSession
    ? `Sleep ${bundle.label}`
    : isServer
      ? `Stop server ${bundle.label}`
      : `Close ${bundle.label}`;
  /**
   * CDXC:TitlebarResources 2026-05-28-10:39:
   * Session resource rows expose Focus beside Sleep/Close. Focus uses the same
   * sidebar session id as Sleep so cross-project Resources rows activate the
   * exact owning session.
   *
   * CDXC:TitlebarResources 2026-06-13-00:56:
   * Per-item resource action buttons should behave like normal visible controls,
   * not hover-revealed overlays. Keep metrics visible, keep actions in stable
   * grid columns, and avoid tooltip trigger wrappers or native-pointer hover
   * gates that can make visible row buttons reject clicks.
   *
   * CDXC:TitlebarResources 2026-06-15-13:45:
   * Row-level Close should disappear for app-critical shared browser helper
   * bundles instead of disabling the button or letting the click reach process
   * termination. Users should only be able to close resource rows that map to a
   * restorable terminal session or an owned browser/code/orphan surface.
   *
   * CDXC:TitlebarResources 2026-06-22-00:30:
   * Dev-server Focus may jump to the owning terminal, but Stop must only signal
   * the listener process tree and must not sleep the terminal session.
   */
  return (
    <div className="titlebar-resource-bundle" data-quitting={String(isQuitting)}>
      <div
        className="titlebar-resource-row"
        data-expandable={String(hasChildren)}
        onClick={() => {
          if (hasChildren) {
            onToggle(bundleToggleKey);
          }
        }}
      >
        <div className="titlebar-resource-main">
          {hasChildren ? (
            <button
              className="titlebar-resource-collapse-button"
              onClick={(event) => {
                event.stopPropagation();
                onToggle(bundleToggleKey);
              }}
              type="button"
            >
              <IconChevronDown aria-hidden="true" data-collapsed={String(isCollapsed)} size={14} stroke={1.8} />
            </button>
          ) : (
            <span className="titlebar-resource-collapse-spacer" />
          )}
          <span className="titlebar-resource-avatar">{getResourceBundleAvatar(bundle)}</span>
          <span className="titlebar-resource-text">
            {mainUrl ? (
              <a
                className="titlebar-resource-name titlebar-resource-main-link"
                href={mainUrl}
                onClick={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  openResourceBundleMainUrl(bundle, mainUrl, serverOpenTarget);
                }}
              >
                {mainLabel}
              </a>
            ) : (
              <span className="titlebar-resource-name">{mainLabel}</span>
            )}
            <span className="titlebar-resource-meta">
              {isQuitting ? (
                preservesSidebarSession ? (
                  "Sleeping..."
                ) : isServer ? (
                  "Stopping..."
                ) : (
                  "Quitting..."
                )
              ) : (
                <>
                  <span className="titlebar-resource-meta-text">{getResourceBundleMeta(bundle)}</span>
                  {showPortlessSetupAction && serverPortless ? (
                    <button
                      className="titlebar-resource-portless-action"
                      onClick={(event) => {
                        event.preventDefault();
                        event.stopPropagation();
                        runPortlessResourcesSetupAction(serverPortless);
                      }}
                      type="button"
                    >
                      {serverPortless.setupActionLabel}
                    </button>
                  ) : null}
                </>
              )}
            </span>
          </span>
        </div>
        <div className="titlebar-resource-metrics" aria-label="Resource usage">
          <span className="titlebar-resource-metric">
            <IconCpu aria-hidden="true" size={13} stroke={1.8} />
            {formatWholePercent(bundle.cpu)}
          </span>
          <span className="titlebar-resource-metric">
            <IconDeviceDesktop aria-hidden="true" size={13} stroke={1.8} />
            {formatResourceMemory(bundle.memoryMb)}
          </span>
        </div>
        {focusSessionId ? (
          <button
            aria-label={`Focus ${bundle.label}`}
            className="titlebar-resource-focus-button"
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onFocusSession(focusSessionId);
            }}
            type="button"
          >
            <IconFocus2 aria-hidden="true" size={13} stroke={1.9} />
          </button>
        ) : null}
        {isActionable ? (
          <button
            aria-label={actionLabel}
            className="titlebar-resource-kill-button"
            data-action={preservesSidebarSession ? "sleep" : isServer ? "stop" : "quit"}
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onQuit([ bundle ]);
            }}
            type="button"
          >
            {preservesSidebarSession ? (
              <IconMoon aria-hidden="true" size={13} stroke={1.9} />
            ) : isServer ? (
              <IconSquareMinus aria-hidden="true" size={13} stroke={1.9} />
            ) : (
              <IconX aria-hidden="true" size={13} stroke={2} />
            )}
          </button>
        ) : null}
      </div>
      {hasChildren && !isCollapsed ? (
        <div className="titlebar-resource-children">
          {bundle.childProcesses.slice(0, 8).map((process) => (
            <div className="titlebar-resource-child-row" key={process.pid}>
              <span className="titlebar-resource-child-name">
                {getResourceChildProcessName(bundle, process)} pid {process.pid}
              </span>
              <div className="titlebar-resource-child-metrics" aria-label="Child process resource usage">
                <span className="titlebar-resource-metric">
                  <IconCpu aria-hidden="true" size={12} stroke={1.8} />
                  {formatWholePercent(process.cpu)}
                </span>
                <span className="titlebar-resource-metric">
                  <IconDeviceDesktop aria-hidden="true" size={12} stroke={1.8} />
                  {formatResourceMemory(process.rssMb)}
                </span>
              </div>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function getResourceChildProcessName(
  bundle: ResourceProcessBundle,
  process: ResourceProcess,
): string {
  return bundle.type === "browser" ? getBrowserProcessDisplayName(process) : getProcessDisplayName(process);
}

function getResourceBundleAvatar(bundle: ResourceProcessBundle): ReactNode {
  const agentIcon = bundle.session?.agentIcon;
  if (isSidebarAgentIcon(agentIcon)) {
    /**
     * CDXC:TitlebarResources 2026-05-26-13:24:
     * Resource rows should use the same shared agent-logo mask assets as Agents
     * Hub profile chips instead of two-letter text abbreviations. This keeps
     * Codex, Claude, T3, browser, and other agent identities visually aligned
     * across the sidebar and resource manager.
     */
    return (
      <span
        aria-hidden="true"
        className="titlebar-resource-avatar-logo"
        data-agent-icon={agentIcon}
        style={{
          backgroundColor: AGENT_LOGO_COLORS[agentIcon],
          maskImage: `url("${AGENT_LOGOS[agentIcon]}")`,
          WebkitMaskImage: `url("${AGENT_LOGOS[agentIcon]}")`,
        }}
      />
    );
  }
  if (bundle.type === "code") {
    return <IconCode aria-hidden="true" size={15} stroke={1.9} />;
  }
  if (bundle.type === "browser") {
    return <IconWorld aria-hidden="true" size={15} stroke={1.9} />;
  }
  if (bundle.type === "server") {
    return <IconWorld aria-hidden="true" size={15} stroke={1.9} />;
  }
  if (bundle.session?.sessionKind === "terminal") {
    return <IconTerminal2 aria-hidden="true" size={15} stroke={1.9} />;
  }
  return <IconBox aria-hidden="true" size={15} stroke={1.9} />;
}

function isSidebarAgentIcon(candidate: unknown): candidate is SidebarAgentIcon {
  return typeof candidate === "string" && Object.prototype.hasOwnProperty.call(AGENT_LOGOS, candidate);
}

function getResourceBundleMainLabel(bundle: ResourceProcessBundle): string {
  if (bundle.server && bundle.portless?.isSetupActive) {
    return bundle.portless.hostname;
  }
  if (bundle.server && bundle.portless) {
    return resourceServerLocalhostLabel(bundle.server);
  }
  return bundle.label;
}

function getResourceBundleMainUrl(bundle: ResourceProcessBundle): string | undefined {
  if (!bundle.server) {
    return undefined;
  }
  if (bundle.portless?.isSetupActive) {
    return resourcePortlessUrl(bundle.portless);
  }
  return resourceServerLocalhostUrl(bundle.server);
}

function openResourceBundleMainUrl(
  bundle: ResourceProcessBundle,
  url: string,
  serverOpenTarget: TerminalDevServerOpenTarget | undefined,
): void {
  /*
   * CDXC:TerminalDevServers 2026-06-23-19:22:
   * Resources dev-server links should open either in the user's system default browser or the internal browser. Do not expose a per-browser target list here; only server bundles should read this setting so future resource links keep their existing route.
   */
  if (bundle.type === "server" && serverOpenTarget === "system-default-browser") {
    postNative({ type: "openExternalUrl", url });
    return;
  }
  postTitlebarSidebarCommand({ type: "openBrowserPane", url });
}

function getResourceBundleMeta(bundle: ResourceProcessBundle): string {
  if (bundle.server) {
    const pid = bundle.process?.pid ?? bundle.server.pid;
    if (bundle.portless) {
      const processMeta = `${bundle.server.commandName} pid ${pid}`;
      return bundle.portless.isSetupActive
        ? `${resourceServerLocalhostLabel(bundle.server)} - ${processMeta}`
        : `${bundle.portless.setupStatusLabel} - ${processMeta}`;
    }
    return `${bundle.server.commandName} pid ${pid}`;
  }
  if (bundle.session) {
    const provider = bundle.session.sessionPersistenceProvider
      ? `${bundle.session.sessionPersistenceProvider} terminal`
      : bundle.session.sessionKind ?? "session";
    const pid = bundle.process?.pid ? ` pid ${bundle.process.pid}` : "";
    return `${provider}${pid}`;
  }
  if (bundle.browserTab) {
    return bundle.browserTab.url?.trim() || "Browser tab";
  }
  if (bundle.type === "browser") {
    if (bundle.key === "browser:runtime") {
      return "Shared GPU, network, and storage helpers";
    }
    if (bundle.key === "browser:unmatched-renderers") {
      return "No visible Browser tab matched these helpers";
    }
    return "Browser helper processes";
  }
  if (bundle.process?.pid) {
    return `pid ${bundle.process.pid}`;
  }
  return bundle.type;
}

function resourceServerLocalhostLabel(server: Pick<ResourceListeningServer, "port">): string {
  return `localhost:${server.port}`;
}

function resourceServerLocalhostUrl(server: Pick<ResourceListeningServer, "port">): string {
  return `http://${resourceServerLocalhostLabel(server)}`;
}

function resourcePortlessUrl(portless: Pick<ResourcePortlessServerPresentation, "hostname" | "protocol">): string {
  return `${portless.protocol}://${portless.hostname}`;
}

function runPortlessResourcesSetupAction(portless: ResourcePortlessServerPresentation): void {
  if (!portless.setupAction) {
    openPortlessResourcesSettings();
    return;
  }
  postTitlebarSidebarCommand({
    action: portless.setupAction,
    protocol: portless.protocol,
    requestId: createPortlessResourcesAdminRequestId(portless.setupAction),
    type: "runPortlessSettingsAdminAction",
  });
}

function openPortlessResourcesSettings(): void {
  window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
    initialSearchQuery: "Portless",
    initialTab: "projects",
    modal: "settings",
    type: "open",
  });
}

function createPortlessResourcesAdminRequestId(action: NativePortlessAdminInstallAction): string {
  return `portless-resources-${action}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

function normalizeTitlebarMode(candidate: unknown): TitlebarMode {
  /**
   * CDXC:ModeSwitcher 2026-05-15-18:20:
   * The top titlebar mode must mirror the workarea mode restored by the sidebar
   * at launch and after each mode transition. Treat the sidebar/native payload
   * as authoritative so a restored Source, Browser, Kanban, Automate, or Docs
   * pane cannot leave the segmented control highlighted on Agents.
   *
   * CDXC:ModeSwitcher 2026-05-15-18:30:
   * User clicks still need optimistic local mode selection so the shared-layout
   * pill animates immediately while slow Source/Browser/Kanban/Automate/Docs surfaces load. Clear
   * that optimistic value when sidebar state arrives so startup restore and
   * failed transitions remain synchronized with the real visible workarea.
   */
  return candidate === "code" ||
    candidate === "git" ||
    candidate === "automate" ||
    candidate === "tasks" ||
    candidate === "manage"
    ? candidate
    : "agents";
}

function resolveInitialTitlebarMode(bootstrap: Record<string, unknown>): TitlebarMode {
  const explicitMode = normalizeTitlebarMode(bootstrap.activeMode);
  if (explicitMode !== "agents") {
    return explicitMode;
  }
  /*
  CDXC:ProjectSidebarOwnership 2026-06-02-12:29:
  The titlebar must not infer startup mode from the old native-sidebar-projects.json payload. gxserver owns shared project/session inventory now, while the macOS window owns the explicit active mode passed in bootstrap state.
  */
  return "agents";
}

function getTitlebarModeIcon(mode: TitlebarMode): ReactNode {
  switch (mode) {
    case "code":
      return <IconCode aria-hidden="true" size={14} stroke={1.8} />;
    case "git":
      return <IconWorld aria-hidden="true" size={14} stroke={1.8} />;
    case "automate":
      return <IconCalendarTime aria-hidden="true" size={14} stroke={1.8} />;
    case "tasks":
      return <IconChecklist aria-hidden="true" size={14} stroke={1.8} />;
    case "manage":
      return <IconFolderOpen aria-hidden="true" size={14} stroke={1.8} />;
    case "agents":
    default:
      /**
       * CDXC:ModeSwitcher 2026-05-28-12:15:
       * The Agents page should use a single-person glyph in both the full
       * titlebar switcher and compact picker, not the group icon previously
       * used for multi-agent page identity.
       */
      return <IconUser aria-hidden="true" size={14} stroke={1.8} />;
  }
}

type TitlebarModeOption = {
  disabled?: boolean;
  disabledReason?: string;
  label: string;
  meta?: ReactNode;
  onSelect: () => void;
  value: TitlebarMode;
};

/*
CDXC:ModeSwitcher 2026-06-15-20:07:
Titlebar mode tabs should show the active segment immediately on click instead of animating the shared-layout pill between Agents, Source, Browser, Kanban, and Manage. Keep the previous Motion transition commented here so the animated behavior can be restored if the requirement changes.

Previous Motion wiring:
* import { motion } from "motion/react";
* const TITLEBAR_MODE_PILL_TRANSITION = {
*   type: "spring",
*   bounce: 0,
*   duration: 0.39,
* } as const;
*/

function TitlebarModeDropdown({
  activeMode,
  modes,
  nativeDropdownOpen,
  onOpenPanel,
}: {
  activeMode: TitlebarMode;
  modes: TitlebarModeOption[];
  nativeDropdownOpen: TitlebarDropdownPanelKind | undefined;
  onOpenPanel: (kind: TitlebarDropdownPanelKind, anchor: HTMLElement) => void;
}) {
  const activeModeOption = modes.find((mode) => mode.value === activeMode) ?? modes[0];
  if (!activeModeOption) {
    return null;
  }
  return (
    <Button
      aria-label="Mode menu"
      className="titlebar-session-button titlebar-mode-picker-trigger"
      data-state={nativeDropdownOpen === "mode" ? "open" : undefined}
      data-titlebar-dropdown-anchor
      onClick={(event) => onOpenPanel("mode", event.currentTarget)}
      onContextMenu={(event) => {
        event.preventDefault();
        onOpenPanel("mode", event.currentTarget);
      }}
      type="button"
      variant="ghost"
    >
      {/*
       * CDXC:ModeSwitcher 2026-05-28-10:38:
       * When app width is below 1050px, Agents/Source/Browser/Kanban/Automate/Docs moves from
       * the centered segmented control into a keep-awake-style mode picker
       * beside the project title. Keep the current mode icon visible on the
       * main segment so narrow titlebar chrome still exposes the active action.
       *
       * CDXC:ModeSwitcher 2026-05-28-11:52:
       * The compact mode picker should be one button, not a split button:
       * clicking either the current-mode icon or the chevron opens the same
       * dropdown so there is no separate immediate mode action in tight chrome.
       *
       * CDXC:ReactTitlebar 2026-06-11-13:22:
       * The compact mode picker opens a native child-window dropdown so the
       * titlebar WKWebView remains clipped to the fixed titlebar strip.
       */}
      <span>{activeModeOption.label}</span>
      <IconChevronDown aria-hidden="true" size={14} />
    </Button>
  );
}
function TitlebarModeSwitcher({
  activeMode,
  companionPaneHidden,
  modes,
  onToggleCompanion,
  showCompanionToggle,
}: {
  activeMode: TitlebarMode;
  companionPaneHidden: boolean;
  modes: TitlebarModeOption[];
  onToggleCompanion: () => void;
  showCompanionToggle: boolean;
}) {
  const companionToggleLabel = companionPaneHidden
    ? "Expand Companion Sidepane"
    : "Hide Companion Sidepane";
  return (
    <div
      aria-label="Mode switcher"
      className="titlebar-mode-switcher"
      role="tablist"
    >
      {/*
        CDXC:ModeSwitcher 2026-05-15-12:54:
        The app titlebar mode switcher must sit in the center as one four-part
        animated segmented control with visible icon+text labels. Use the
        shadcn-space Tabs-01 motion layout highlight pattern, but keep content
        switching owned by the native sidebar bridge instead of rendering tab
        panels inside the titlebar.

        CDXC:ModeSwitcher 2026-05-15-14:47:
        The animation must closely match shadcn-space Tabs-01: each tab is a
        single button with the active segment rendered as the selected button's
        shared-layout motion background. Avoid a clipped segmented track
        because it changes the motion shape and makes the spring look unlike
        the referenced component.

        CDXC:ModeSwitcher 2026-05-15-14:54:
        The active pill must visibly travel from the previously active mode to
        the newly selected mode. Keep tab overflow visible so Framer Motion's
        shared-layout element is not clipped to the destination button, which
        would make Agents-to-Tasks look like a direct jump.

        CDXC:ModeSwitcher 2026-05-26-13:52:
        Titlebar mode tabs should match the sidebar session button roundness
        instead of using fully rounded pills, so the top navigation and session
        controls share one chrome language.

        CDXC:ProjectEditorCompanion 2026-06-12-03:18:
        The companion sidepane toggle must sit exactly to the left of Agents and
        share the same segmented border language. Keep it inside the switcher
        row instead of a floating restore slot so expanding and collapsing use
        one stable titlebar affordance.

        CDXC:ProjectEditorCompanion 2026-06-12-04:02:
        The toggle is anchor-positioned off the switcher's left edge so the
        Agents/Source/Browser/Kanban/Automate/Docs group keeps its original centered titlebar
        geometry while staying normal DOM inside the titlebar WKWebView.
      */}
      {showCompanionToggle ? (
        <TitlebarAppTooltip content={companionToggleLabel}>
          <button
            aria-label={companionToggleLabel}
            className="titlebar-mode-tab titlebar-companion-toggle-button"
            onClick={onToggleCompanion}
            type="button"
          >
            {/*
             * CDXC:TitlebarTooltips 2026-06-13-02:59:
             * Companion sidepane titlebar hover text should use AppTooltip like
             * sidebar buttons; keep it left-positioned through the titlebar
             * wrapper so it stays out of the workspace/editor area.
             */}
            <span className="titlebar-mode-tab-content">
              {companionPaneHidden ? (
                <IconLayoutSidebarLeftExpand
                  aria-hidden="true"
                  size={COMPANION_SIDEPANE_ICON_SIZE}
                  stroke={1.8}
                />
              ) : (
                <IconLayoutSidebarLeftCollapse
                  aria-hidden="true"
                  size={COMPANION_SIDEPANE_ICON_SIZE}
                  stroke={1.8}
                />
              )}
            </span>
          </button>
        </TitlebarAppTooltip>
      ) : null}
      {modes.map((mode) => {
        const isActive = mode.value === activeMode;
        const modeButton = (
          <button
            aria-disabled={mode.disabled === true ? true : undefined}
            aria-label={mode.disabledReason ?? mode.label}
            aria-selected={isActive}
            className="titlebar-mode-tab"
            data-active={String(isActive)}
            data-disabled={String(mode.disabled === true)}
            onClick={() => {
              if (mode.disabled) {
                return;
              }
              mode.onSelect();
            }}
            role="tab"
            style={{ transformStyle: "preserve-3d" }}
            type="button"
          >
            {isActive ? (
              <>
                {/*
                 * CDXC:ModeSwitcher 2026-06-15-20:07:
                 * Clicking a titlebar tab should instantly paint the active
                 * state on that tab. Previous animated implementation, kept
                 * for a possible restore:
                 *   <motion.div
                 *     className="titlebar-mode-tab-active"
                 *     layoutId="clickedbutton"
                 *     transition={TITLEBAR_MODE_PILL_TRANSITION}
                 *   />
                 */}
                <span aria-hidden="true" className="titlebar-mode-tab-active" />
              </>
            ) : null}
            <span className="titlebar-mode-tab-content">
              {getTitlebarModeIcon(mode.value)}
              <span className="titlebar-mode-label">{mode.label}</span>
              {mode.meta ? <span className="titlebar-mode-meta">{mode.meta}</span> : null}
            </span>
          </button>
        );
        /*
         * CDXC:ModeSwitcher 2026-06-16-16:00:
         * Disabled titlebar mode tabs still need hover and focus events so the
         * same AppTooltip used by Keep Awake and Resources can show the reason.
         * Keep native disabled off this button path and guard selection in the
         * click handler instead; place disabled explanations on the right side.
         */
        return (
          <TitlebarAppTooltip
            content={mode.disabled ? mode.disabledReason : undefined}
            key={mode.value}
            side="right"
          >
            {modeButton}
          </TitlebarAppTooltip>
        );
      })}
    </div>
  );
}

function parseSharedSettings(candidate: unknown): unknown {
  if (typeof candidate !== "string") {
    return undefined;
  }
  try {
    return JSON.parse(candidate || "null");
  } catch {
    return undefined;
  }
}

function createConfiguredOpenTargets(settings: TitlebarOpenTargetsSettings): ResolvedOpenTarget[] {
  const hiddenTargetIds = new Set(settings.hiddenTargetIds);
  return [
    ...BUILT_IN_WORKSPACE_OPEN_TARGETS.filter((target) => !hiddenTargetIds.has(target.id)).map(
      (definition): ResolvedOpenTarget => ({
        definition,
        id: definition.id,
        kind: "built-in",
        label: definition.label,
      }),
    ),
    ...settings.customTargets.map(
      (custom): ResolvedOpenTarget => ({
        command: custom.command,
        custom,
        id: custom.id,
        kind: "custom",
        label: custom.label,
      }),
    ),
  ];
}

function resolveVisibleOpenTargets(
  targets: ResolvedOpenTarget[],
  availability: WorkspaceOpenTargetAvailability,
): ResolvedOpenTarget[] {
  const availableTargetIds = new Set(availability.availableTargetIds);
  return targets
    .map((target) => {
      if (target.id === "finder") {
        return target;
      }
      if (target.kind === "custom") {
        return target;
      }
      if (!availableTargetIds.has(target.id as WorkspaceOpenTargetDefinition["id"])) {
        return undefined;
      }
      /**
       * CDXC:ReactTitlebar 2026-05-11-02:03
       * The titlebar menu shows only persisted installed built-ins plus custom
       * targets. Hidden ids are applied before this step, so startup detection
       * cannot re-add an editor the user turned off in Settings.
       */
      return {
        ...target,
        resolvedAppName: availability.resolvedAppNames[target.id],
        resolvedCommand: availability.resolvedCommands[target.id],
      };
    })
    .filter((target): target is ResolvedOpenTarget => target !== undefined);
}

function getOpenTargetIcon(target: ResolvedOpenTarget): ReactNode {
  if (target.id === "finder") {
    return <IconFolderOpen aria-hidden="true" className="size-4 text-zinc-400" />;
  }
  const editorIcon = getEditorBrandIconId(target.id);
  if (editorIcon) {
    return <EditorBrandIcon className="size-4" icon={editorIcon} />;
  }
  return <IconBox aria-hidden="true" className="size-4 text-zinc-400" />;
}

function getTitlebarGitActionIcon(action: SidebarGitAction): ReactNode {
  if (action === "syncMain" || action === "syncRemote") {
    return (
      <IconGitCompare aria-hidden="true" className="titlebar-git-icon" size={15} stroke={1.8} />
    );
  }
  if (action === "push") {
    return <IconUpload aria-hidden="true" className="titlebar-git-icon" size={15} stroke={1.8} />;
  }
  if (action === "multiRelease") {
    return (
      <IconStackPush aria-hidden="true" className="titlebar-git-icon" size={15} stroke={1.8} />
    );
  }
  if (action === "release") {
    return <IconRocket aria-hidden="true" className="titlebar-git-icon" size={15} stroke={1.8} />;
  }
  if (action === "pr") {
    return (
      <IconGitPullRequest
        aria-hidden="true"
        className="titlebar-git-icon"
        size={15}
        stroke={1.8}
      />
    );
  }
  return <IconGitCommit aria-hidden="true" className="titlebar-git-icon" size={15} stroke={1.8} />;
}

function formatTitlebarGitStatCount(value: number): string {
  const normalized = Math.max(0, Math.trunc(value));
  return String(Math.min(normalized, 9999));
}

function titlebarGitBranchLabel(branch: string | null): string {
  return branch?.trim() || "(detached HEAD)";
}

function TitlebarGitStatPair({
  firstCount,
  firstPrefix = "+",
  label,
  secondCount,
  secondPrefix = "-",
  tone = "changes",
}: {
  firstCount: number;
  firstPrefix?: string;
  label: string;
  secondCount: number;
  secondPrefix?: string;
  tone?: "changes" | "commits";
}) {
  const firstStatClassName =
    tone === "changes"
      ? "titlebar-git-stat titlebar-git-stat-additions"
      : "titlebar-git-stat";
  const secondStatClassName =
    tone === "changes"
      ? "titlebar-git-stat titlebar-git-stat-deletions"
      : "titlebar-git-stat";

  return (
    <span className="titlebar-git-stat-pair" data-tone={tone}>
      <span className="titlebar-git-meta-label">{label}</span>
      <span className="titlebar-git-stat-values">
        <span className={firstStatClassName}>
          {firstPrefix}
          {formatTitlebarGitStatCount(firstCount)}
        </span>
        <span className={secondStatClassName}>
          {secondPrefix}
          {formatTitlebarGitStatCount(secondCount)}
        </span>
      </span>
    </span>
  );
}

function titlebarGitRemoteSyncDisabledReason(state: SidebarGitState): string | undefined {
  const disabledReason = getSidebarGitDisabledReason(state, "syncRemote");
  if (disabledReason !== undefined) {
    return disabledReason;
  }
  if (!hasSidebarGitRemoteCommitDelta(state)) {
    return "No remote commits to sync.";
  }
  return undefined;
}

function readLastOpenTargetId(): string {
  return localStorage.getItem(LAST_OPEN_TARGET_STORAGE_KEY) || "finder";
}

function readLastActionCommandId(state: Pick<TitlebarProjectState, "projectId" | "projectPath">): string | undefined {
  const storageKey = getLastActionCommandStorageKey(state);
  return storageKey ? localStorage.getItem(storageKey)?.trim() || undefined : undefined;
}

function persistLastActionCommandId(
  state: Pick<TitlebarProjectState, "projectId" | "projectPath">,
  commandId: string,
): void {
  const storageKey = getLastActionCommandStorageKey(state);
  if (!storageKey) {
    return;
  }
  localStorage.setItem(storageKey, commandId);
}

function getLastActionCommandStorageKey(
  state: Pick<TitlebarProjectState, "projectId" | "projectPath">,
): string | undefined {
  const projectKey = state.projectId?.trim() || state.projectPath.trim();
  if (!projectKey) {
    return undefined;
  }
  /**
   * CDXC:TitlebarActions 2026-05-11-02:46
   * Moving Actions from the sidebar header to the titlebar keeps the same
   * project-scoped primary-action behavior: the split button's left side runs
   * the last chosen action for the active project, not a global last action.
   */
  return `${LAST_ACTION_COMMAND_STORAGE_PREFIX}${projectKey}`;
}

function getSidebarActionLabel(command: SidebarCommandButton): string {
  return command.name.trim() || command.commandId;
}

function getSidebarActionIcon(command: SidebarCommandButton | undefined): ReactNode {
  if (command?.icon) {
    /*
     * CDXC:TitlebarActions 2026-06-16-07:48:
     * User-configured action icons must inherit titlebar chrome color instead
     * of using per-action colors. The titlebar action menu should read as one
     * native control group with glyphs matching the adjacent menu icons.
     */
    return (
      <SidebarCommandIconGlyph
        className="quick-action-icon"
        icon={command.icon}
        size={16}
        stroke={1.8}
      />
    );
  }
  if (command?.actionType === "browser") {
    return <IconWorld aria-hidden="true" className="quick-action-icon" size={16} stroke={1.8} />;
  }
  if (command?.actionType === "terminal") {
    return <IconTerminal2 aria-hidden="true" className="quick-action-icon" size={16} stroke={1.8} />;
  }
  return <IconPlayerPlay aria-hidden="true" className="quick-action-icon" size={16} stroke={1.8} />;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

const styles = {
  centerSlot: {
    /*
     * CDXC:TitlebarModeTabs 2026-06-30-12:55:
     * The six desktop mode tabs need more center-titlebar weight so Agents, Source, Browser, Kanban, Automate, and Docs render as full labels instead of truncating to short ellipses.
     *
     * CDXC:TitlebarModeTabs 2026-06-30-17:04:
     * The expanded six-tab switcher should stay compact after visual review. Cap the centered group at six equal 84px tabs and rely on reduced tab padding, not oversized button width, to keep full labels readable.
     */
    alignItems: "center",
    display: "flex",
    left: "50%",
    minWidth: 0,
    position: "absolute",
    top: TITLEBAR_CENTER_CONTROLS_TOP,
    transform: "translateX(-50%)",
    width: "clamp(0px, calc(100vw - 420px), 504px)",
  },
  projectSlot: {
    alignItems: "center",
    display: "flex",
    gap: 0,
    left: 81,
    maxWidth: "min(620px, calc(100vw - 350px))",
    minWidth: 0,
    position: "absolute",
    top: TITLEBAR_PROJECT_TOP,
  },
  rightSlot: {
    alignItems: "center",
    display: "flex",
    gap: 0,
    position: "absolute",
    /*
     * CDXC:ReactTitlebar 2026-05-30-12:00:
     * Right-side titlebar controls should sit flush with the window edge.
     *
     * CDXC:SidebarTopChrome 2026-06-29-01:43:
     * Settings and Keep Awake moved to the sidebar shortcut row. Keep the titlebar right slot flush so the remaining project/window controls still align with the window edge.
     */
    right: 0,
    top: TITLEBAR_RIGHT_CONTROLS_TOP,
  },
  shell: {
    background: "transparent",
    inset: 0,
    overflow: "visible",
    position: "fixed",
  },
  titlebar: {
    alignItems: "center",
    background: "var(--app-titlebar-surface-background, var(--app-titlebar-background))",
    display: "flex",
    height: TITLEBAR_HEIGHT,
    justifyContent: "center",
    position: "relative",
    width: "100vw",
  },
} satisfies Record<string, CSSProperties>;

document.body.style.margin = "0";
document.documentElement.style.margin = "0";
document.documentElement.style.padding = "0";
document.body.style.background = "transparent";
document.body.style.overflow = "hidden";
document.body.style.padding = "0";
if (initialTitlebarDropdownPanelKind) {
  document.documentElement.dataset.titlebarDropdownPanel = "true";
  document.documentElement.style.height = "100%";
  document.documentElement.style.overflow = "hidden";
  document.documentElement.style.width = "100%";
  document.body.dataset.titlebarDropdownPanel = "true";
  document.body.style.display = "block";
  document.body.style.height = "100%";
  document.body.style.width = "100%";
}
const styleElement = document.createElement("style");
styleElement.textContent = `
  :root {
    /**
     * CDXC:ReactTitlebar 2026-06-04-18:37:
     * Titlebar text should use the same font family as the macOS sidebar. Bind
     * the titlebar font token to the imported sidebar shadcn sans token instead
     * of the older bespoke monospace stack while leaving titlebar sizing and
     * weight rules unchanged.
    */
    --titlebar-font-family: var(--font-sans, "Inter Variable", sans-serif);
    /*
     * CDXC:SidebarTitlebarColors 2026-06-15-15:01:
     * Experimental custom titlebar colors override this token from body so
     * button separators can darken as the custom background gets lighter.
     */
    --titlebar-button-border-color: #252525;
  }
  /*
   * CDXC:SidebarTitlebarColors 2026-06-15-11:24:
   * Foreground color needs to beat older hardcoded white titlebar text/icon
   * rules, but only in the real titlebar host. The dropdown-panel host never
   * gets this body data attribute, so menu surfaces keep their theme colors.
   */
  body[data-custom-sidebar-titlebar-colors="true"] :is(
    .titlebar-sidebar-collapse-button,
    .titlebar-session-button,
    .titlebar-project-title,
    .titlebar-project-name,
    .titlebar-mode-tab,
    .titlebar-mode-label,
    .titlebar-git-label,
    .titlebar-open-group,
    .titlebar-open-main-button,
    svg
  ) {
    color: var(--custom-sidebar-titlebar-foreground-color) !important;
  }
  /*
   * CDXC:ReactTitlebar 2026-06-10-23:44:
   * Native AppKit reports whether the pointer is inside the titlebar strip.
   * Keep the DOM interactive and only neutralize stale hover styling below;
   * AppKit already owns the real titlebar boundary.
   */
  /**
   * CDXC:ReactTitlebar 2026-05-11-09:00
   * The right titlebar controls should read as flat chrome text/icons rather
   * than framed buttons. Remove the manual installed-target refresh button and
   * preserve the 20px centered control height so the 35px titlebar
   * keeps top/bottom breathing room.
   *
   * CDXC:ReactTitlebar 2026-05-17-00:57:
   * The right titlebar controls should use spacing instead of separator rules.
   * Keep a consistent 9px gap between control groups and show a subtle hover
   * background on each button so pointer focus is visible without making the
   * chrome look heavy.
   *
   * CDXC:ReactTitlebar 2026-05-15-19:41
   * The top-right titlebar should not duplicate the Commands pane entry point.
   * Remove the corner terminal icon and its left separator so Commands access
   * lives in the sidebar footer instead of competing with project actions.
   *
   * CDXC:ReactTitlebar 2026-05-30-07:37:
   * Titlebar button left/right separators should use #252525 so they match the
   * native workarea and commands-pane separator lines.
   */
  .titlebar-session-button {
    /*
     * CDXC:ReactTitlebar 2026-06-12-02:50:
     * All clickable macOS titlebar controls must share the same 34px control
     * height so icon-only menus, compact mode, and text actions align inside
     * the 35px native titlebar reservation.
     */
    box-sizing: border-box;
    height: ${TITLEBAR_CONTROL_HEIGHT}px;
    max-height: ${TITLEBAR_CONTROL_HEIGHT}px;
    min-height: ${TITLEBAR_CONTROL_HEIGHT}px;
    min-width: 0;
    border: 0;
    border-left: 1px solid var(--titlebar-button-border-color);
    border-radius: 0;
    background: transparent;
    color: rgba(255,255,255,0.84);
    font: 650 12.5px/${TITLEBAR_CONTROL_HEIGHT}px var(--titlebar-font-family);
    letter-spacing: 0;
    box-shadow: none;
  }
  .titlebar-session-button:hover,
  .titlebar-session-button:focus-visible,
  .titlebar-session-button[data-state="open"] {
    background: rgba(255,255,255,0.08);
    color: rgba(255,255,255,0.96);
    outline: none;
  }
  .titlebar-session-button[data-disabled="true"] {
    /*
     * CDXC:TitlebarTooltips 2026-06-13-02:59:
     * Titlebar icon buttons that still need hover tooltips use aria-disabled
     * instead of native disabled, matching sidebar toolbar controls. Preserve a
     * muted disabled look while keeping the AppTooltip trigger hoverable.
     */
    color: rgba(255,255,255,0.34);
    cursor: default;
  }
  .titlebar-session-button[data-disabled="true"]:hover:not([data-state="open"]) {
    background: transparent;
    color: rgba(255,255,255,0.34);
  }
  .titlebar-session-button svg,
  .titlebar-session-button .quick-action-icon {
    height: 16px;
    width: 16px;
  }
  .titlebar-open-chevron-button svg {
    height: 14px;
    width: 14px;
  }
  .titlebar-project-button {
    max-width: 210px;
    padding: 0 10px;
  }
  .titlebar-sidebar-collapse-button {
    /*
     * CDXC:SidebarCollapse 2026-06-20-17:10:
     * The titlebar Toggle Sidebar button now paints only a side-aware Tabler
     * layout-sidebar glyph, not the previous blue circular dot. Preserve the
     * existing 33x33px titlebar hit target and compact offset so AppKit layout
     * and click ownership stay unchanged while the visible chrome is simplified.
     *
     * CDXC:SidebarCollapse 2026-06-20-17:58:
     * Visual review moved only the visible sidebar glyph 2px lower. Keep the
     * titlebar button frame unchanged so the native hit target and neighboring
     * controls do not shift.
     *
     * CDXC:SidebarCollapse 2026-06-20-18:28:
     * Visual review moved only the visible sidebar glyph 1px right while
     * preserving the titlebar button placement and native hit target.
     *
     * CDXC:SidebarCollapse 2026-06-22-22:46:
     * Visual review shifted the Toggle Sidebar glyph up by 1px from the prior
     * offset while keeping the button frame and hit target unchanged.
     *
     * CDXC:SidebarCollapse 2026-06-22-23:26:
     * Follow-up visual tuning moved the Toggle Sidebar glyph 0.5px back down,
     * so the final icon translation is 1px right and 1.5px down.
     *
     * CDXC:SidebarCollapse 2026-06-23-03:35:
     * The Toggle Sidebar and available-update download glyphs should both render
     * as 14x14 icons while keeping the current visual translation.
     *
     * CDXC:SidebarCollapse 2026-06-23-04:35:
     * The Tabler Toggle Sidebar glyph has internal viewBox padding, so a 14px
     * SVG box measured as an 11px visible mark. Use an 18px SVG box for this
     * glyph so the visible icon reads as the requested 14x14 size.
     *
     * CDXC:SidebarCollapse 2026-06-13-02:59:
     * The assigned hotkey renders through AppTooltip, matching sidebar controls
     * instead of a titlebar-only data-tooltip pseudo-element.
     */
    align-items: center;
    background: transparent !important;
    border: 0 !important;
    border-radius: 0;
    box-shadow: none;
    color: #ffffff !important;
    display: inline-flex;
    flex: 0 0 33px;
    height: 33px !important;
    justify-content: center;
    margin: 0 0 0 -9px;
    min-height: 33px !important;
    min-width: 33px !important;
    padding: 0 !important;
    position: relative;
    width: 33px !important;
  }
  .titlebar-sidebar-collapse-button:hover,
  .titlebar-sidebar-collapse-button:focus-visible {
    background: transparent !important;
    color: #ffffff !important;
    outline: none;
  }
  .titlebar-sidebar-collapse-button svg {
    height: 18px !important;
    transform: translate(1px, 1.5px);
    width: 18px !important;
  }
  .titlebar-update-button {
    /**
     * CDXC:AutoUpdate 2026-05-28-14:19:
     * The update affordance sits immediately to the left of the project
     * identity with a fixed 7px gap, so available updates read as subtle
     * chrome and never shift center or right-side titlebar controls.
     *
     * CDXC:AutoUpdate 2026-06-13-02:59:
     * Its hover label uses AppTooltip like the sidebar. Keep the left titlebar
     * affordance spacing in CSS while tooltip rendering stays shared.
     *
     * CDXC:SidebarCollapse 2026-06-12-11:10:
     * When both compact titlebar affordances are visible, the update button
     * sits to the right of the sidebar collapse button. Let the collapse button
     * own the 9px inter-button gap instead of adding a second left margin here.
     */
    color: rgba(255,255,255,0.46);
    margin-left: 0;
    border-left: 0 !important;
    margin-right: 7px;
    padding: 0;
    position: relative;
    width: 20px;
  }
  .titlebar-update-button:hover,
  .titlebar-update-button:focus-visible {
    color: rgba(255,255,255,0.84);
  }
  .titlebar-update-button[data-downloading="true"] {
    /*
     * CDXC:AutoUpdate 2026-06-13-17:52:
     * Downloading updates should keep the existing titlebar update button
     * visible so the compact titlebar layout and hit target stay stable while
     * Sparkle performs the actual download.
     *
     * CDXC:AutoUpdate 2026-06-15-16:39:
     * The active download state uses a disabled, hoverable titlebar button. Do
     * not fade the whole button; the inline progress indicator should
     * communicate activity while the tooltip describes the download state.
     *
     * CDXC:AutoUpdate 2026-06-30-22:18:
     * The active download state renders a circular fill and hover percent from
     * native Sparkle progress instead of a spinner.
     */
    color: rgba(255,255,255,0.92);
  }
  .titlebar-update-button[data-downloading="true"]:hover,
  .titlebar-update-button[data-downloading="true"]:focus-visible {
    background: transparent;
    color: rgba(255,255,255,0.92);
  }
  .titlebar-update-download-icon {
    /*
     * CDXC:AutoUpdate 2026-06-19-20:09:
     * Visual review moved only the available-update download glyph 2px lower.
     * Keep the titlebar button full-height and move the SVG visually so the
     * native titlebar hit target and neighboring layout stay unchanged.
     *
     * CDXC:AutoUpdate 2026-06-22-23:26:
     * The available-update download glyph should match the Toggle Sidebar
     * titlebar glyph's exact size and 1px/1.5px visual placement.
     *
     * CDXC:AutoUpdate 2026-06-23-03:35:
     * The matching titlebar glyph size is now 14x14 for both available-update
     * download and Toggle Sidebar icons.
     */
    height: 14px !important;
    transform: translate(1px, 1.5px);
    width: 14px !important;
  }
  .titlebar-update-progress-ring {
    /*
     * CDXC:AutoUpdate 2026-06-30-22:18:
     * The active update affordance should be a circular fill, not a spinner,
     * while preserving the same compact titlebar button frame and visual offset
     * as the old active download indicator.
     */
    display: inline-flex;
    height: 16px;
    transform: translate(1px, 1.5px);
    width: 16px;
  }
  .titlebar-update-progress-ring svg {
    display: block;
    height: 16px;
    overflow: visible;
    width: 16px;
  }
  .titlebar-update-progress-track,
  .titlebar-update-progress-fill {
    fill: none;
    stroke-width: 2;
  }
  .titlebar-update-progress-track {
    stroke: rgba(255,255,255,0.24);
  }
  .titlebar-update-progress-fill {
    stroke: currentColor;
    stroke-dasharray: 1;
    stroke-dashoffset: var(--titlebar-update-progress-offset, 1);
    stroke-linecap: round;
    transform: rotate(-90deg);
    transform-origin: 50% 50%;
    transition: stroke-dashoffset 140ms ease-out;
  }
  .titlebar-update-progress-ring[data-progress-known="false"] .titlebar-update-progress-fill {
    animation: titlebar-update-progress-pending-fill 1.25s ease-in-out infinite;
    opacity: 0.76;
  }
  @keyframes titlebar-update-progress-pending-fill {
    0% {
      stroke-dashoffset: 0.96;
    }
    55% {
      stroke-dashoffset: 0.28;
    }
    100% {
      stroke-dashoffset: 0.96;
    }
  }
  .titlebar-project-title {
    /**
     * CDXC:ReactTitlebar 2026-06-04-18:55:
     * The React titlebar project title in the macOS app should sit 2px lower
     * without changing the shared titlebar height or moving neighboring
     * controls. Use a visual transform so layout stays anchored to the existing
     * titlebar row.
     */
    align-items: center;
    color: rgba(255,255,255,0.9);
    cursor: default;
    display: inline-flex;
    flex: 1 1 auto;
    font: 650 13.5px/${TITLEBAR_CONTROL_HEIGHT}px var(--titlebar-font-family);
    height: ${TITLEBAR_CONTROL_HEIGHT}px;
    letter-spacing: 0;
    max-width: 210px;
    min-width: 0;
    overflow: hidden;
    padding: 0 3px;
    transform: translateY(2px);
  }
  .titlebar-project-title > .truncate {
    display: block;
    min-width: 0;
  }
  .titlebar-mode-picker-trigger {
    align-items: center;
    display: none !important;
    gap: 1px;
    flex: 0 0 auto;
    padding: 0 8px;
    width: max-content;
  }
  .titlebar-mode-picker-menu {
    max-width: 220px;
    /*
     * CDXC:ModeSwitcher 2026-05-28-11:52:
     * The compact picker opens over the native left sidebar edge. Keep its
     * portaled Radix content above sidebar chrome instead of letting the menu
     * appear behind the project list.
     */
    z-index: 2200 !important;
  }
  .titlebar-project-icon {
    /**
     * CDXC:ProjectIcons 2026-05-11-01:50
     * React titlebar project identity should use the same shared project image
     * as macOS notifications, positioned before the project title without
     * changing titlebar height or competing with the right-side controls.
     */
    border-radius: 0;
    flex: 0 0 auto;
    height: 14px;
    margin-right: 5px;
    object-fit: contain;
    width: 14px;
  }
  .titlebar-open-main-button {
    padding: 0 12px;
    width: 42px;
  }
  .titlebar-git-main-button {
    gap: 0;
    padding: 0 12px;
    width: 42px;
  }
  .titlebar-git-label {
    max-width: 110px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-git-label-compact {
    display: none;
  }
  .titlebar-git-icon {
    flex: 0 0 auto;
  }
  .titlebar-git-spinner {
    animation: titlebar-git-spin 1s linear infinite;
    flex: 0 0 auto;
  }
  @keyframes titlebar-git-spin {
    to {
      transform: rotate(360deg);
    }
  }
  .titlebar-command-panel-button {
    padding: 0 12px;
    width: 42px;
  }
  .titlebar-mode-switcher {
    /**
     * CDXC:ModeSwitcher 2026-05-26-13:52:
     * Match the top mode-tab radius to sidebar session buttons. The session
     * card uses calc(10px * var(--sidebar-density-scale)); keep the titlebar
     * tab highlight on the same radius so it is less pill-shaped.
     *
     * CDXC:TitlebarModeTabs 2026-06-30-12:55:
     * The desktop switcher should consume the wider center slot and divide it across all six tabs so the titlebar shows full view names rather than clipped labels.
     *
     * CDXC:TitlebarModeTabs 2026-06-30-17:04:
     * Keep all mode tabs equal width while reducing horizontal padding so the button group feels lighter without returning to clipped text.
     */
    --titlebar-mode-tab-radius: 0;
    align-items: center;
    display: flex;
    flex: 0 1 auto;
    height: ${TITLEBAR_CONTROL_HEIGHT}px;
    max-height: ${TITLEBAR_CONTROL_HEIGHT}px;
    min-height: ${TITLEBAR_CONTROL_HEIGHT}px;
    max-width: 100%;
    overflow: visible;
    padding: 0;
    perspective: 1000px;
    position: relative;
    width: 100%;
  }
  @media (max-width: 1049px) {
    .titlebar-mode-switcher {
      /*
       * CDXC:ModeSwitcher 2026-05-28-10:38:
       * App widths below 1050px do not have enough horizontal room for the
       * centered Agents/Source/Browser/Kanban/Automate/Docs switcher plus right-side titlebar
       * actions. Replace it with the split picker beside the project name.
       */
      display: none;
    }
    .titlebar-mode-picker-trigger {
      display: inline-flex !important;
    }
  }
  .titlebar-mode-tab {
    /**
     * CDXC:ReactTitlebar 2026-06-04-20:08:
     * The macOS titlebar mode tabs should be 2px smaller and 100 weight units
     * heavier than the primary sidebar navigation buttons after visual review.
     * Use 13.55px / 400 typography while preserving the titlebar-owned line
     * height for vertical containment.
     */
    appearance: none;
    -webkit-appearance: none;
    align-items: center;
    background: transparent;
    /*
     * CDXC:ModeSwitcher 2026-06-23-04:45:
     * Each mode tab carries a full 4-direction #252525 outline (the shared
     * --titlebar-button-border-color token) so the titlebar, Settings, and
     * Agents Hub tab bars all read as boxed tabs. box-sizing: border-box keeps
     * the bordered tab inside the 34px control height so the new top/bottom
     * lines never overflow the 35px titlebar reservation. Top/bottom/left live
     * on every tab and the last child adds the right edge, keeping internal
     * separators a single 1px line instead of doubling between neighbors.
     */
    border: 0;
    border-top: 1px solid var(--titlebar-button-border-color);
    border-bottom: 1px solid var(--titlebar-button-border-color);
    border-left: 1px solid var(--titlebar-button-border-color);
    border-radius: var(--titlebar-mode-tab-radius);
    box-sizing: border-box;
    /*
     * CDXC:ModeSwitcher 2026-06-23-04:45:
     * Inactive mode tabs read as slightly dimmed white; hover and active both
     * go full white. Hover only brightens the label (no background fill) so the
     * highlight box stays exclusive to the active tab.
     */
    color: rgba(255,255,255,0.75);
    cursor: default;
    display: inline-flex;
    font: 400 13.55px/${TITLEBAR_CONTROL_HEIGHT}px var(--titlebar-font-family);
    height: ${TITLEBAR_CONTROL_HEIGHT}px;
    max-height: ${TITLEBAR_CONTROL_HEIGHT}px;
    min-height: ${TITLEBAR_CONTROL_HEIGHT}px;
    flex: 1 1 0;
    justify-content: center;
    letter-spacing: 0;
    min-width: 84px;
    overflow: visible;
    padding: 0 8px;
    position: relative;
    white-space: nowrap;
  }
  .titlebar-mode-tab:last-child {
    border-right: 1px solid var(--titlebar-button-border-color);
  }
  .titlebar-companion-toggle-button {
    /**
     * CDXC:ProjectEditorCompanion 2026-06-12-03:18:
     * The companion toggle is an icon-only mode-switcher segment. Use the same
     * left-border separator model as Agents/Source/Browser/Kanban/Automate/Docs, with Agents'
     * own left border providing the boundary to its right.
     *
     * CDXC:ProjectEditorCompanion 2026-06-12-04:02:
     * Anchor this control to the left edge of the centered mode switcher without
     * participating in flex layout, so the Agents/Source/Browser/Kanban/Automate/Docs button
     * group remains centered in the titlebar.
     *
     * CDXC:ProjectEditorCompanion 2026-06-12-04:23:
     * Hover should use #282828 as a clear but restrained affordance for the
     * icon-only sidepane toggle.
     */
    min-width: 42px;
    padding: 0;
    position: absolute;
    right: 100%;
    top: 0;
    transition: background-color 120ms ease, color 120ms ease;
    width: 42px;
  }
  .titlebar-companion-toggle-button:hover,
  .titlebar-companion-toggle-button:focus-visible {
    background: #282828;
  }
  .titlebar-companion-toggle-button .titlebar-mode-tab-content {
    justify-content: center;
    width: 100%;
  }
  .titlebar-companion-toggle-button .titlebar-mode-tab-content svg {
    display: block;
    flex-shrink: 0;
    height: ${COMPANION_SIDEPANE_ICON_SIZE}px;
    width: ${COMPANION_SIDEPANE_ICON_SIZE}px;
  }
  .titlebar-mode-tab:hover,
  .titlebar-mode-tab:focus-visible {
    color: #ffffff;
    outline: none;
  }
  .titlebar-mode-tab:disabled,
  .titlebar-mode-tab[data-disabled="true"] {
    color: rgba(255,255,255,0.26);
  }
  .titlebar-mode-tab:disabled .titlebar-mode-tab-content,
  .titlebar-mode-tab[data-disabled="true"] .titlebar-mode-tab-content {
    opacity: 0.72;
  }
  .titlebar-mode-tab[data-active="true"] {
    color: #ffffff;
  }
  .titlebar-mode-tab:disabled[data-active="true"],
  .titlebar-mode-tab[data-disabled="true"][data-active="true"] {
    color: rgba(255,255,255,0.42);
  }
  .titlebar-mode-tab-active {
    background: rgba(255,255,255,0.11);
    border-radius: var(--titlebar-mode-tab-radius);
    inset: 0;
    position: absolute;
  }
  .titlebar-mode-tab-content {
    align-items: center;
    display: inline-flex;
    gap: 0;
    min-width: 0;
    position: relative;
    z-index: 1;
  }
  .titlebar-mode-tab-content svg {
    display: none;
  }
  .titlebar-prompt-editor-live-dot {
    background: #95d7f6;
    border-radius: 999px;
    flex: 0 0 auto;
    height: 6px;
    margin-right: 7px;
    width: 6px;
  }
  .titlebar-mode-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .titlebar-mode-meta {
    align-items: center;
    display: inline-flex;
    gap: 4px;
    margin-left: 1px;
  }
  .titlebar-exit-focus-button {
    /*
     * CDXC:SessionFocusMode 2026-06-13-18:39:
     * Exit focus reuses the active mode-tab DOM and styling so it matches the
     * Agents button from the titlebar screenshot instead of carrying a separate
     * outlined button skin.
     */
    --titlebar-mode-tab-radius: 0;
  }
  .titlebar-resource-button {
    padding: 0 12px;
    width: 42px;
  }
  .titlebar-tips-button {
    padding: 0 12px;
    position: relative;
    width: 42px;
  }
  .titlebar-tips-unread-badge {
    /*
     * CDXC:TipsAndTricks 2026-05-30-08:39:
     * The unread indicator is intentionally a quiet half-size dot instead of a
     * numbered badge: use #95d7f6 and a circular shape at the top-right of the
     * Tips & Tricks icon.
     *
     * CDXC:SidebarTheme 2026-06-15-01:43:
     * The badge border follows the titlebar background so Dark 1, Dark 2, and
     * Light keep the dot clean against the selected titlebar surface.
     */
    align-items: center;
    background: #95d7f6;
    border: 1px solid var(--app-titlebar-background);
    display: inline-flex;
    height: 7.5px;
    justify-content: center;
    min-width: 0;
    padding: 0;
    position: absolute;
    right: 8px;
    top: 5px;
    width: 7.5px;
    border-radius: 999px;
  }
  @media (max-width: 619.98px) {
    /**
     * CDXC:ReactTitlebar 2026-05-29-16:05:
     * App widths below 620px need the top-right titlebar chrome to prioritize
     * the primary Git action. Hide Exit Focus, Tips, and Resources,
     * and remove visible Commit wording from the Git primary label while
     * keeping non-commit destination text such as push or PR when there is room.
     */
    .titlebar-exit-focus-button,
    .titlebar-tips-group,
    .titlebar-resource-button {
      display: none !important;
    }
    .titlebar-git-label-full[data-compact-below-620="true"] {
      display: none;
    }
    .titlebar-git-label-compact {
      display: inline;
    }
  }
  .titlebar-open-chevron-button {
    padding: 0;
    width: 24px;
  }
  .titlebar-open-chevron-button-hidden {
    border-left: 0;
    opacity: 0;
    overflow: hidden;
    pointer-events: none;
    width: 0;
  }
  .titlebar-open-group {
    /*
     * CDXC:TitlebarTooltips 2026-06-13-02:59:
     * Right-side titlebar icon buttons use the shared AppTooltip wrapper from
     * the sidebar. Keep only group geometry here; tooltip rendering must not
     * drift back to local data-tooltip pseudo-elements.
     */
    gap: 0 !important;
    height: ${TITLEBAR_CONTROL_HEIGHT}px;
    max-height: ${TITLEBAR_CONTROL_HEIGHT}px;
    min-height: ${TITLEBAR_CONTROL_HEIGHT}px;
    overflow: visible;
  }
  .titlebar-open-group > .titlebar-session-button {
    border-radius: 0;
  }
  .titlebar-open-group > .titlebar-open-chevron-button {
    border-left: 0;
  }
  .titlebar-open-group > .titlebar-session-button:first-child {
    border-bottom-left-radius: 0;
    border-top-left-radius: 0;
  }
  .titlebar-open-group > .titlebar-session-button:last-child {
    border-bottom-right-radius: 0;
    border-top-right-radius: 0;
  }
  .titlebar-app-tooltip {
    /*
     * CDXC:TitlebarTooltips 2026-06-15-23:25:
     * Titlebar hover labels should be 6px shorter than the shared app tooltip
     * chrome. Reduce vertical padding by 3px per side only for titlebar-owned
     * AppTooltip content so sidebar and resource-detail tooltips keep their size.
     */
    padding-bottom: 3px !important;
    padding-top: 3px !important;
  }
  .titlebar-open-menu {
    /**
     * CDXC:TitlebarMenus 2026-05-28-13:52:
     * Titlebar dropdown surfaces should match the unified app overlay
     * background instead of using the older #181818 menu shell.
     *
     * CDXC:SidebarTheme 2026-06-15-01:43:
     * Titlebar dropdowns follow --app-dropdown-background so Dark 1 uses
     * #191919, Dark 2 preserves #0e0e0e, and Light uses a light overlay.
     */
    background: var(--app-dropdown-background) !important;
    background-color: var(--app-dropdown-background) !important;
    border: 1px solid rgba(255,255,255,0.14);
    box-shadow: 0 18px 42px rgba(0,0,0,0.44);
  }
  /*
   * CDXC:ReactTitlebar 2026-06-11-13:22:
   * Native child-window dropdowns reuse the existing web menu components, but
   * their document is the panel itself rather than Radix portal content inside
   * the titlebar WKWebView. Remove portal-era viewport offsets so the Swift
   * child window owns placement.
   *
   * CDXC:ReactTitlebar 2026-06-12-02:50:
   * Native panels are still sized before they open, but compact dropdown height
   * now comes from the rendered option count while Tips/Resources keep their
   * larger reading surfaces. The React panel fills the child WebView exactly
   * without ResizeObserver-driven native resize messages after open.
   */
  .titlebar-dropdown-panel-root {
    background: var(--app-dropdown-background);
    color: var(--foreground);
    display: block;
    height: 100vh;
    min-height: 1px;
    overflow: hidden;
    width: 100vw;
  }
  .titlebar-dropdown-panel-root .titlebar-open-menu {
    box-sizing: border-box;
    box-shadow: none;
    height: 100%;
    max-height: none;
    max-width: none;
    min-height: 0;
    min-width: 0 !important;
    overflow: auto;
    position: static;
    width: 100% !important;
  }
  .titlebar-dropdown-panel-root .titlebar-open-menu-item {
    align-items: center;
    appearance: none;
    background: transparent;
    border: 0;
    color: inherit;
    display: flex;
    padding: 6px 8px;
    text-align: left;
    width: 100%;
  }
  .titlebar-dropdown-panel-root .titlebar-open-menu-item:not(:disabled):hover {
    background: rgba(255,255,255,0.08);
  }
  .titlebar-dropdown-panel-root .titlebar-open-menu-item:disabled {
    color: rgba(255,255,255,0.34);
  }
  .titlebar-dropdown-panel-root .titlebar-tips-menu,
  .titlebar-dropdown-panel-root .titlebar-resources-menu {
    width: 100% !important;
  }
  .titlebar-dropdown-panel-root .titlebar-tips-panel,
  .titlebar-dropdown-panel-root .titlebar-resources-panel {
    height: 100%;
    max-height: none;
    min-height: 0;
  }
  .titlebar-dropdown-panel-root .titlebar-tips-scroll,
  .titlebar-dropdown-panel-root .titlebar-resources-scroll {
    max-height: none;
    min-height: 0;
    overflow: auto;
  }
  .titlebar-dropdown-panel-root .titlebar-tips-scroll::-webkit-scrollbar,
  .titlebar-dropdown-panel-root .titlebar-resources-scroll::-webkit-scrollbar {
    width: 2px;
  }
  .titlebar-panel-menu-separator {
    height: 1px;
    margin: 4px 0;
  }
  .titlebar-menu-section-label {
    align-items: center;
    color: var(--muted-foreground);
    display: flex;
    font: 600 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    height: ${TITLEBAR_DROPDOWN_MENU_LABEL_HEIGHT}px;
    letter-spacing: 0;
    padding: 4px 8px 2px;
  }
  /**
   * CDXC:TitlebarGit 2026-05-24-20:40:
   * The Git split menu opens from the chevron segment, but the menu must be wide enough to show Commit, Push, and Create PR labels. Pin the menu width instead of letting Radix size it from the narrow chevron trigger.
   *
   * CDXC:TitlebarGit 2026-05-25-10:16:
   * Release-oriented Git actions add longer dropdown labels such as Multicommit & Release, so the pinned menu width must fit them without clipping.
   */
  .titlebar-git-menu {
    max-width: 320px;
    min-width: 300px !important;
    overflow-x: visible;
    width: 300px !important;
  }
  /*
   * CDXC:TitlebarGit 2026-06-16-00:00:
   * Git dropdown rows need one visual rhythm: fixed icon column, shared menu
   * font, and left-aligned content. Keep branch, changed-file stats, sync, and
   * action rows on the same grid so the menu does not look like separate
   * components stacked together.
   *
   * CDXC:TitlebarGit 2026-06-16-00:05:
   * Do not vary font weight between branch, changes, sync, and action text.
   * Disabled sync rows should not switch to a separate gray family; keep the
   * same text color system and use icon/row opacity only for availability.
   *
   * CDXC:TitlebarGit 2026-06-16-00:18:
   * Keep Git dropdown row text light. The compact metadata and action rows are
   * dense enough that medium-weight text reads too heavy in the dark menu.
   *
   * CDXC:TitlebarGit 2026-06-16-07:31:
   * Long changed-file counts such as +9999 and -9999 should share the same
   * right-aligned value edge as Branch and Commits in the macOS titlebar Git
   * menu.
   *
   * CDXC:TitlebarGit 2026-06-16-07:31:
   * Working-tree stats and the remote-sync row should use the same compact
   * two-cell layout without a slash divider. Keep the cells wide enough for the
   * capped four-digit counts while reducing the visual gap between short values.
   *
   * CDXC:TitlebarGit 2026-06-16-09:49:
   * The changed-file stat row is labeled Changes, and the remote-sync row uses
   * neutral down/up commit arrows labeled Commits. Keep the two number cells and
   * label in one gap system so the labels do not change number spacing.
   *
   * CDXC:TitlebarGit 2026-06-16-13:31:
   * Git metadata rows in the titlebar dropdown should read label-first:
   * Branch, Changes, and Commits. Labels use inherited row typography so
   * branch, stat, and action rows match the rest of the menu.
   *
   * CDXC:TitlebarGit 2026-06-16-15:11:
   * Branch, Changes, and Commits must occupy the same label-column width so
   * the branch value, line counts, and commit counts start on one vertical
   * alignment line inside the titlebar Git dropdown.
   *
   * CDXC:TitlebarGit 2026-06-16-18:41:
   * Git metadata labels should not include trailing punctuation; the shared
   * label column provides alignment without needing Branch, Changes, or Commits
   * to end with a colon.
   *
   * CDXC:TitlebarGit 2026-06-16-19:03:
   * Status rows are a label/value table: labels stay left-aligned while branch,
   * line counts, and commit counts share a right-aligned value edge. The
   * commits value always uses ↑ahead ↓behind, including the zero state.
   */
  .titlebar-git-menu .titlebar-open-menu-item {
    --titlebar-git-value-color: rgba(255,255,255,0.86);
    color: var(--titlebar-git-value-color);
    display: grid !important;
    font-weight: 400;
    grid-template-columns: 18px minmax(0, 1fr);
    padding-inline: 8px;
  }
  .titlebar-dropdown-panel-root .titlebar-git-menu .titlebar-open-menu-item:disabled {
    color: var(--titlebar-git-value-color);
  }
  .titlebar-dropdown-panel-root .titlebar-git-menu .titlebar-open-menu-item:disabled > svg,
  .titlebar-dropdown-panel-root .titlebar-git-menu .titlebar-open-menu-item:disabled .titlebar-git-icon {
    opacity: 0.42;
  }
  .titlebar-dropdown-panel-root .titlebar-git-menu .titlebar-open-menu-item:not(:disabled):hover,
  .titlebar-dropdown-panel-root .titlebar-git-menu .titlebar-open-menu-item:not(:disabled):focus-visible {
    /*
     * CDXC:TitlebarGit 2026-06-16-00:10:
     * Clickable Git dropdown rows need a visible hover affordance inside the
     * dark native child menu. Use a Git-scoped state color instead of relying on
     * the subtler generic titlebar menu hover.
     */
    background: rgba(255,255,255,0.14);
    color: rgba(255,255,255,0.94);
    outline: none;
  }
  .titlebar-git-menu .titlebar-open-menu-item > svg,
  .titlebar-git-menu .titlebar-git-icon {
    grid-column: 1;
    justify-self: center;
  }
  .titlebar-git-menu .titlebar-open-menu-item > span {
    min-width: 0;
  }
  .titlebar-git-menu .titlebar-open-menu-item > svg + span,
  .titlebar-git-menu .titlebar-git-icon + span {
    grid-column: 2;
  }
  .titlebar-git-branch-field {
    align-items: center;
    display: grid;
    gap: 12px;
    grid-column: 2;
    grid-template-columns: 62px minmax(0, 1fr);
    min-width: 0;
    width: 100%;
  }
  .titlebar-git-meta-label {
    color: var(--titlebar-git-value-color);
    font: inherit;
    min-width: 62px;
    text-align: left;
    white-space: nowrap;
    width: 62px;
  }
  .titlebar-git-branch-name {
    color: var(--titlebar-git-value-color);
    font: inherit;
    justify-self: end;
    max-width: 100%;
    min-width: 0;
    overflow: hidden;
    text-align: right;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-git-copy-branch-row {
    cursor: copy !important;
  }
  .titlebar-git-branch-tooltip {
    /*
     * CDXC:TitlebarGit 2026-06-16-19:03:
     * Long branch names should be inspectable before copying, but the tooltip
     * must stay compact inside the titlebar dropdown child window.
     */
    max-width: 250px;
  }
  .titlebar-git-branch-tooltip-copy {
    /*
     * CDXC:TitlebarGit 2026-06-16-19:10:
     * The branch tooltip must explain the full-row copy action while still
     * exposing the full branch name in a compact 250px popup.
     */
    display: grid;
    gap: 3px;
  }
  .titlebar-git-stat-pair {
    align-items: center;
    display: grid;
    gap: 12px;
    grid-column: 2;
    grid-template-columns: 62px minmax(0, 1fr);
    width: 100%;
  }
  .titlebar-git-stat-values {
    display: inline-flex;
    /*
     * CDXC:TitlebarGit 2026-06-16-19:10:
     * Files and Commits rows should keep their paired numbers visually close
     * while the whole value group remains right-aligned.
     */
    gap: 4px;
    justify-self: end;
    min-width: 0;
  }
  .titlebar-git-stat {
    font: inherit;
    min-width: 48px;
    text-align: right;
  }
  .titlebar-git-stat-pair[data-tone="commits"] .titlebar-git-stat {
    color: var(--titlebar-git-value-color);
  }
  .titlebar-git-stat-additions {
    color: rgb(74, 222, 128);
  }
  .titlebar-git-stat-deletions {
    color: rgb(248, 113, 113);
  }
  .titlebar-open-menu-item {
    /*
     * CDXC:TitlebarMenus 2026-06-16-00:18:
     * Titlebar dropdown item labels should use normal-weight text. Menu rows are
     * compact and icon-led, so medium weight makes Actions/Open/Git lists feel
     * visually loud compared with the rest of the titlebar chrome.
     */
    cursor: default !important;
    min-height: 30px;
    gap: 10px;
    border-radius: 0;
    font: 400 13px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
  }
  .titlebar-settings-menu .titlebar-open-menu-item {
    /*
     * CDXC:TitlebarSettingsMenu 2026-06-19-00:35:
     * Settings menu shortcuts must appear in a right-aligned column instead of being embedded in labels. Keep a stable icon/label/shortcut grid so rows with no assigned shortcut, such as Wake Pet and Join Discord, still align with shortcut-backed rows.
     */
    display: grid !important;
    grid-template-columns: 18px minmax(0, 1fr) auto;
  }
  .titlebar-settings-menu-icon {
    align-items: center;
    display: inline-flex;
    grid-column: 1;
    justify-content: center;
  }
  .titlebar-settings-menu-label {
    grid-column: 2;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-settings-menu-shortcut {
    color: rgba(255, 255, 255, 0.5);
    grid-column: 3;
    justify-self: end;
    min-width: 1ch;
    white-space: nowrap;
  }
  /**
   * CDXC:TitlebarActions 2026-05-19-16:05:
   * Action rows stack the configured title above a single-line dimmed command
   * preview. Hovering the preview opens a wrapped tooltip capped at 190px wide.
   */
  .titlebar-action-menu-item {
    align-items: flex-start !important;
    min-height: 44px;
    padding-block: 7px;
  }
  .titlebar-action-menu-icon {
    display: inline-flex;
    flex-shrink: 0;
    margin-top: 1px;
  }
  .titlebar-action-menu-copy {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .titlebar-action-menu-title {
    display: block;
    font-weight: inherit;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-action-command-preview {
    color: rgba(255, 255, 255, 0.48);
    display: block;
    font-size: 11px;
    font-weight: 400;
    line-height: 1.2;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-action-command-preview[data-unconfigured="true"] {
    font-style: italic;
  }
  .titlebar-action-command-tooltip {
    max-width: 190px !important;
    overflow-wrap: anywhere;
  }
  .titlebar-tips-menu {
    /**
     * CDXC:TipsAndTricks 2026-05-30-08:31:
     * Tips should use the same maximum dropdown height as Resources and keep
     * the authored array order on screen. The menu is a reading surface, not an
     * editor, so it stays dense and square like the Resources manager.
     *
     * CDXC:TipsAndTricks 2026-06-12-08:56:
     * The macOS Tips & Tricks child panel is 100px narrower than the Resources
     * reading panel so the guide occupies less horizontal space.
     */
    background: var(--app-dropdown-background) !important;
    background-color: var(--app-dropdown-background) !important;
    width: min(556px, calc(100vw - 24px));
    max-height: min(760px, calc(100vh - 46px));
    overflow: hidden;
  }
  .titlebar-tips-panel {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    max-height: min(760px, calc(100vh - 46px));
    overflow: hidden;
  }
  .titlebar-tips-header {
    align-items: stretch;
    border-bottom: 1px solid rgba(255,255,255,0.12);
    display: flex;
    gap: 12px;
    justify-content: space-between;
    min-height: 47px;
    padding: 0 0 0 12px;
  }
  .titlebar-tips-title,
  .titlebar-tips-actions,
  .titlebar-tips-section-heading,
  .titlebar-tip-read-button,
  .titlebar-tip-read-state {
    align-items: center;
    display: inline-flex;
  }
  .titlebar-tips-title {
    color: rgba(255,255,255,0.96);
    font: 750 14px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    gap: 8px;
    min-width: 0;
  }
  .titlebar-tips-actions {
    /*
     * CDXC:TipsAndTricks 2026-06-16-10:04:
     * Tips & Tricks header actions should use matching button widths, point to
     * Features and Setup, and omit the previous unread
     * text summary from the top-right action row.
     *
     * CDXC:TipsAndTricks 2026-06-16-19:42:
     * Add the release-updates action as the rightmost equal-width header action
     * so release notes are available without changing the existing titlebar Tips
     * layout model.
     *
     * CDXC:TipsAndTricks 2026-06-18-04:53:
     * Add Docs as a fourth equal-width action and keep the labels short enough
     * that all actions fit in the native titlebar dropdown.
     *
     * CDXC:TipsAndTricks 2026-06-30-01:38:
     * The Tips header action buttons should fill the header height and touch side by side. Use left/right borders as the only separators so the row reads as connected titlebar chrome instead of separate inset buttons.
     *
     * CDXC:TipsAndTricks 2026-06-30-03:22:
     * The rightmost Tips header action should sit flush with the panel edge, the idle buttons should have no fill, and every action should share the widest button's width with only 15px of side padding.
     *
     * CDXC:TipsAndTricks 2026-06-30-04:28:
     * The visible Tips action labels should stay compact: Video opens the tutorial video, and Updates opens the releases changelog. Short labels keep the equal-width action columns from widening the dropdown header.
     */
    align-self: stretch;
    align-items: stretch;
    display: grid;
    gap: 0;
    grid-template-columns: repeat(4, minmax(max-content, 1fr));
    margin-left: auto;
    width: max-content;
  }
  .titlebar-tips-action-button {
    align-items: center;
    background: transparent;
    border: 0;
    border-left: 1px solid rgba(255,255,255,0.12);
    border-radius: 0;
    box-sizing: border-box;
    color: rgba(255,255,255,0.78);
    display: inline-flex;
    gap: 6px;
    font: 750 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    height: 100%;
    justify-content: center;
    padding: 0 15px;
    white-space: nowrap;
    width: 100%;
  }
  .titlebar-tips-action-button:last-child {
    border-right: 1px solid rgba(255,255,255,0.12);
  }
  .titlebar-tips-panel button:not(:disabled),
  .titlebar-tips-panel [role="button"]:not([aria-disabled="true"]) {
    /*
     * CDXC:TipsAndTricks 2026-06-16-10:04:
     * Every actionable control inside the Tips & Tricks panel should expose the
     * pointer cursor so clickable rows and buttons advertise their interaction.
     */
    cursor: pointer;
  }
  .titlebar-tips-action-button:not(:disabled):hover {
    background: rgba(255,255,255,0.14);
    color: rgba(255,255,255,0.94);
  }
  .titlebar-tips-action-button:disabled {
    color: rgba(255,255,255,0.3);
    cursor: default;
  }
  .titlebar-tips-scroll {
    display: grid;
    gap: 0;
    max-height: min(700px, calc(100vh - 104px));
    overflow: auto;
    padding: 8px 10px 10px;
  }
  .titlebar-tips-section + .titlebar-tips-section {
    margin-top: 10px;
  }
  .titlebar-tips-section-heading {
    align-items: center;
    color: rgba(255,255,255,0.62);
    display: flex;
    font: 750 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    gap: 6px;
    justify-content: space-between;
    letter-spacing: 0.08em;
    padding: 4px 2px 7px;
    text-transform: uppercase;
    width: 100%;
  }
  .titlebar-tips-list {
    display: grid;
    gap: 7px;
  }
  .titlebar-tip-row {
    align-items: start;
    background: rgba(255,255,255,0.025);
    border: 1px solid rgba(255,255,255,0.1);
    display: grid;
    gap: 10px;
    grid-template-columns: minmax(0, 1fr) 28px;
    min-height: 72px;
    overflow: hidden;
    padding: 9px 8px;
    transition: background 120ms ease, border-color 120ms ease;
  }
  .titlebar-tip-row[data-read="true"] {
    opacity: 0.72;
  }
  .titlebar-tip-row[data-actionable="true"]:hover {
    /*
     * CDXC:TipsAndTricks 2026-06-28-08:00:
     * Action-backed tips should read like clickable detail rows without making
     * the per-row read check part of the navigation target.
     */
    background: rgba(255,255,255,0.05);
    border-color: rgba(255,255,255,0.18);
  }
  .titlebar-tip-row-notice {
    cursor: pointer;
    grid-template-columns: 28px minmax(0, 1fr);
    text-align: left;
    transition: background 120ms ease, border-color 120ms ease;
    width: 100%;
  }
  .titlebar-tip-row-notice:hover {
    background: rgba(245,158,11,0.06);
    border-color: rgba(245,158,11,0.34);
  }
  .titlebar-tip-row-notice .titlebar-tip-icon {
    background: rgba(245,158,11,0.14);
    color: rgba(251,191,36,0.95);
  }
  .titlebar-tip-row-notice .titlebar-tip-body {
    /**
     * CDXC:CliInstall 2026-06-07-15:26:
     * Runtime notices can describe an action plus a short benefit list, but
     * Tips & Tricks should remain dense. Clamp notice descriptions to three
     * lines so the CLI accessibility warning cannot dominate the dropdown.
     */
    -webkit-line-clamp: 3;
  }
  .titlebar-tip-icon {
    align-items: center;
    align-self: start;
    background: rgba(255,255,255,0.1);
    color: rgba(255,255,255,0.84);
    display: inline-flex;
    height: 28px;
    justify-content: center;
    width: 28px;
  }
  .titlebar-tip-detail {
    align-items: start;
    display: grid;
    gap: 10px;
    grid-template-columns: 28px minmax(0, 1fr);
    min-width: 0;
    text-align: left;
  }
  .titlebar-tip-detail-button {
    appearance: none;
    background: transparent;
    border: 0;
    color: inherit;
    font: inherit;
    padding: 0;
    width: 100%;
  }
  .titlebar-tip-copy {
    display: grid;
    gap: 7px;
    min-width: 0;
  }
  .titlebar-tip-title {
    color: rgba(255,255,255,0.94);
    display: block;
    font: 700 13px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-tip-body {
    color: rgba(255,255,255,0.58);
    display: -webkit-box;
    font: 500 12px/1.35 -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    overflow: hidden;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
  }
  .titlebar-tip-read-button,
  .titlebar-tip-read-state {
    align-self: end;
    justify-self: end;
    justify-content: center;
  }
  .titlebar-tip-read-button {
    background: rgba(255,255,255,0.14);
    border: 1px solid rgba(255,255,255,0.16);
    border-radius: 0;
    color: rgba(255,255,255,0.9);
    height: 24px;
    padding: 0;
    transition: background 120ms ease, color 120ms ease;
    width: 24px;
  }
  .titlebar-tip-read-button:hover {
    background: rgba(255,255,255,0.2);
    color: rgba(255,255,255,0.96);
  }
  .titlebar-tip-read-state {
    color: rgba(255,255,255,0.46);
    height: 24px;
    width: 24px;
  }
  .titlebar-tips-empty {
    color: rgba(255,255,255,0.54);
    font: 500 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    padding: 10px 4px;
  }
  .titlebar-resources-menu {
    /**
     * CDXC:TitlebarResources 2026-05-28-13:22:
     * The Resources manager background must match the titlebar dropdown family
     * while adjacent titlebar dropdowns keep the existing titlebar menu color.
     *
     * CDXC:SidebarTheme 2026-06-15-01:43:
     * Resources uses the dropdown token so the large child panel switches with
     * Dark 1, Dark 2, and Light.
     */
    background: var(--app-dropdown-background) !important;
    background-color: var(--app-dropdown-background) !important;
    width: min(656px, calc(100vw - 24px));
    max-height: min(760px, calc(100vh - 46px));
    overflow: hidden;
  }
  .titlebar-resources-panel {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    max-height: min(760px, calc(100vh - 46px));
    overflow: hidden;
  }
  .titlebar-resources-panel button:not(:disabled) {
    /*
     * CDXC:TitlebarResources 2026-06-16-10:36:
     * Resources should show the pointer cursor only over real button controls.
     * CPU/RAM metric chips are read-only status, so they override expandable row
     * pointer inheritance back to the default cursor below.
     *
     * CDXC:TitlebarResources 2026-06-16-12:34:
     * The Resources modal should not show a hand cursor over expandable row
     * chrome in the macOS titlebar. Keep expansion clickable through the row
     * handler, but reserve pointer cursor feedback for explicit buttons only.
     */
    cursor: pointer;
  }
  .titlebar-resources-panel button:disabled {
    cursor: default;
  }
  .titlebar-resources-header {
    align-items: center;
    border-bottom: 1px solid rgba(255,255,255,0.12);
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 11px 12px;
    position: relative;
    z-index: 2;
  }
  .titlebar-resources-title,
  .titlebar-resources-actions,
  .titlebar-resources-summary,
  .titlebar-resource-section-summary,
  .titlebar-resource-section-summary span,
  .titlebar-resources-summary span {
    align-items: center;
    display: inline-flex;
  }
  .titlebar-resources-title {
    gap: 8px;
    /*
     * CDXC:TitlebarResources 2026-06-16-00:19:
     * The Resources dropdown should use the same lighter text treatment as the
     * titlebar action menus. Keep labels, metrics, daemon status, and controls
     * visually consistent instead of mixing heavy font weights across the panel.
     */
    font: 400 14px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    min-width: 0;
  }
  .titlebar-resource-tooltip {
    background: var(--ghostex-tooltip-background, rgba(24,24,24,0.98));
    border: 1px solid var(--ghostex-tooltip-border, rgba(255,255,255,0.12));
    box-shadow: var(--ghostex-tooltip-shadow, 0 12px 30px rgba(0,0,0,0.35));
    color: var(--ghostex-tooltip-foreground, rgba(255,255,255,0.78));
    display: grid;
    font: var(--ghostex-tooltip-font, 500 12px/1.35 -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif);
    gap: 3px;
    max-width: 292px;
    padding: 8px 9px;
  }
  .titlebar-resource-tooltip-title {
    color: var(--ghostex-tooltip-strong-foreground, rgba(255,255,255,0.94));
    font-weight: 760;
  }
  .titlebar-resources-actions {
    gap: 10px;
    margin-left: auto;
  }
  .titlebar-resources-info-control {
    display: inline-flex;
  }
  .titlebar-resources-info-button {
    align-items: center;
    appearance: none;
    background: rgba(255,255,255,0.08);
    border: 1px solid rgba(255,255,255,0.12);
    border-radius: 0;
    color: rgba(255,255,255,0.78);
    display: inline-flex;
    height: 24px;
    justify-content: center;
    padding: 0;
    transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
    width: 24px;
  }
  .titlebar-resources-info-button:hover,
  .titlebar-resources-info-button:focus-visible,
  .titlebar-resources-info-button[aria-expanded="true"] {
    background: rgba(255,255,255,0.14);
    border-color: rgba(255,255,255,0.18);
    color: rgba(255,255,255,0.94);
    outline: none;
  }
  .titlebar-resources-info-popover {
    background: color-mix(in srgb, var(--app-dropdown-background) 82%, #ffffff 18%) !important;
    border: 1px solid rgba(255,255,255,0.14);
    box-shadow: 0 14px 36px rgba(0,0,0,0.36);
    box-sizing: border-box;
    color: rgba(255,255,255,0.72);
    padding: 10px;
    position: absolute;
    right: 12px;
    top: calc(100% + 8px);
    width: min(620px, calc(100% - 24px));
    z-index: 5;
  }
  .titlebar-resources-collapse-all-button {
    /*
     * CDXC:TitlebarResources 2026-06-12-20:20:
     * Keep the Resources bulk section toggle visible at rest. Sleep actions
     * intentionally fade in only after header interaction, but this Resources
     * affordance is the user's fixed control immediately to their left.
     */
    align-items: center;
    appearance: none;
    background: rgba(255,255,255,0.12);
    border: 1px solid rgba(255,255,255,0.18);
    border-radius: 0;
    color: rgba(255,255,255,0.82);
    display: inline-flex;
    flex: 0 0 24px;
    height: 24px;
    justify-content: center;
    padding: 0;
    width: 24px;
  }
  .titlebar-resources-collapse-all-button:hover,
  .titlebar-resources-collapse-all-button:focus-visible {
    background: rgba(255,255,255,0.2);
    color: rgba(255,255,255,0.96);
    outline: none;
  }
  .titlebar-resources-collapse-all-button:disabled {
    cursor: default;
    opacity: 0.45;
  }
  .titlebar-resources-action-button {
    /*
     * CDXC:TitlebarResources 2026-06-12-23:37:
     * Header Sleep buttons are ordinary controls. Keep them visible and
     * hit-testable at rest; use only standard hover/disabled selectors for
     * interaction feedback.
     */
    align-items: center;
    appearance: none;
    background: rgba(255,255,255,0.08);
    border: 1px solid rgba(255,255,255,0.12);
    border-radius: 0;
    color: rgba(255,255,255,0.78);
    display: inline-flex;
    gap: 6px;
    font: 400 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    height: 24px;
    justify-content: center;
    padding: 0 8px;
    transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
    white-space: nowrap;
  }
  .titlebar-resources-action-button[data-variant="quit"] {
    background: rgba(220,38,38,0.18);
    border-color: rgba(248,113,113,0.28);
    color: rgba(255,255,255,0.86);
  }
  .titlebar-resources-action-button:disabled {
    color: rgba(255,255,255,0.3);
    cursor: default;
    opacity: 0.55;
  }
  .titlebar-resources-action-button[data-variant="sleep"]:not(:disabled):hover {
    background: rgba(255,255,255,0.14);
    color: rgba(255,255,255,0.92);
  }
  .titlebar-resources-action-button[data-variant="quit"]:not(:disabled):hover {
    background: rgba(220,38,38,0.28);
    color: rgba(255,255,255,0.96);
  }
  .titlebar-resource-section-quit-button {
    align-items: center;
    appearance: none;
    background: rgba(220,38,38,0.18);
    border: 1px solid rgba(248,113,113,0.28);
    border-radius: 0;
    color: rgba(255,255,255,0.86);
    display: inline-flex;
    font: 400 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    height: 24px;
    justify-content: center;
    opacity: 0;
    padding: 0 8px;
    pointer-events: none;
    transition: opacity 120ms ease, background 120ms ease, color 120ms ease;
    white-space: nowrap;
  }
  .titlebar-resource-section-quit-button[data-action="sleep"],
  .titlebar-resource-section-quit-button[data-action="stop"] {
    background: rgba(255,255,255,0.08);
    border-color: rgba(255,255,255,0.13);
  }
  .titlebar-resource-section-heading:hover .titlebar-resource-section-quit-button,
  .titlebar-resource-section-heading:focus-within .titlebar-resource-section-quit-button {
    /*
     * CDXC:TitlebarResources 2026-05-21-16:58:
     * Resource-manager Quit controls should stay available without crowding the
     * header or section chrome. Reveal destructive buttons only while the row is
     * hovered or keyboard-focused.
     *
     * CDXC:TitlebarResources 2026-05-26-13:11:
     * Sleep Project is a non-destructive project-group action, but it should
     * use the same hover reveal slot as section Quit so resource metrics remain
     * stable until the user targets the group action area.
     */
    opacity: 1;
    pointer-events: auto;
  }
  .titlebar-resource-section-quit-button[data-action="sleep"]:hover,
  .titlebar-resource-section-quit-button[data-action="stop"]:hover {
    background: rgba(255,255,255,0.14);
    color: rgba(255,255,255,0.92);
  }
  .titlebar-resource-section-quit-button[data-action="quit"]:hover {
    background: rgba(220,38,38,0.28);
    color: rgba(255,255,255,0.96);
  }
  .titlebar-resources-summary {
    color: rgba(255,255,255,0.72);
    gap: 12px;
    font: 400 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
  }
  .titlebar-resources-summary span {
    gap: 5px;
  }
  .titlebar-resources-scroll {
    /*
     * CDXC:TitlebarResources 2026-06-16-09:49:
     * Resources sections must stay stacked at the top of the fixed-height child
     * panel when few rows are visible. Keep implicit grid rows content-sized
     * and align the grid content to the start so spare height remains after the
     * final section instead of expanding gaps between sections.
     */
    align-content: start;
    display: grid;
    gap: 0;
    grid-auto-rows: max-content;
    max-height: min(700px, calc(100vh - 104px));
    overflow: auto;
    padding: 8px 10px 10px;
  }
  .titlebar-resources-scroll[data-loading="true"] {
    grid-template-rows: auto minmax(260px, 1fr);
  }
  .titlebar-resources-loading {
    align-items: center;
    color: rgba(255,255,255,0.58);
    display: flex;
    font: 400 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    gap: 8px;
    justify-content: center;
    min-height: 260px;
  }
  .titlebar-resources-loading-icon {
    animation: titlebar-git-spin 1s linear infinite;
    flex: 0 0 auto;
  }
  .titlebar-resources-info-note {
    /*
     * CDXC:TitlebarResources 2026-05-21-16:58:
     * Keep explanatory copy out of the crowded titlebar. Put the general
     * resource-usage note in the scroll body above the resource sections.
     *
     * CDXC:TitlebarResources 2026-06-16-01:08:
     * The note now appears only inside the click-triggered info dropdown next
     * to the bulk expand/collapse control, with paragraph spacing instead of
     * inline line breaks.
     *
     * CDXC:TitlebarResources 2026-06-16-01:54:
     * The popover shell owns the only card background and border. Keep this
     * inner text wrapper visually transparent so the note is not a card inside
     * another boxed surface.
     */
    color: rgba(255,255,255,0.62);
    font: 400 12px/1.35 -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
  }
  .titlebar-resources-info-note p {
    margin: 0;
  }
  .titlebar-resources-info-note p + p {
    margin-top: 10px;
  }
  .titlebar-gxserver-daemon {
    /*
     * CDXC:TitlebarResources 2026-05-31-03:56:
     * The Resources dropdown must expose gxserver daemon status, version, stop/restart controls, and a small Always start checkbox without changing the sidebar session restore list.
     *
     * CDXC:TitlebarResources 2026-06-12-11:30:
     * The gxserver status headline should show the live status message (for example "gxserver is running and uses the expected protocol.") beside the state dot instead of a generic "Daemon" label, with the state/version line directly underneath.
     *
     * CDXC:TitlebarResources 2026-06-16-00:56:
     * Hide the gxserver daemon status strip in the Resources dropdown with CSS
     * only. Keep the component mounted so the surrounding daemon controls and
     * status plumbing do not need a separate conditional path.
     */
    align-items: center;
    background: rgba(255,255,255,0.045);
    border: 1px solid rgba(255,255,255,0.1);
    color: rgba(255,255,255,0.72);
    display: none;
    gap: 6px 10px;
    grid-template-columns: minmax(0, 1fr) auto;
    margin-bottom: 8px;
    min-width: 0;
    padding: 8px 10px;
  }
  .titlebar-gxserver-daemon-main,
  .titlebar-gxserver-daemon-controls,
  .titlebar-gxserver-daemon-checkbox {
    align-items: center;
    display: inline-flex;
    min-width: 0;
  }
  .titlebar-gxserver-daemon-main {
    gap: 8px;
  }
  .titlebar-gxserver-daemon-dot {
    background: rgba(255,255,255,0.35);
    border-radius: 999px;
    box-shadow: 0 0 0 3px rgba(255,255,255,0.05);
    flex: 0 0 auto;
    height: 7px;
    width: 7px;
  }
  .titlebar-gxserver-daemon-dot[data-state="running"] {
    background: #4ade80;
    box-shadow: 0 0 0 3px rgba(74,222,128,0.14);
  }
  .titlebar-gxserver-daemon-dot[data-state="starting"] {
    background: #facc15;
    box-shadow: 0 0 0 3px rgba(250,204,21,0.16);
  }
  .titlebar-gxserver-daemon-dot[data-state="error"],
  .titlebar-gxserver-daemon-dot[data-state="nodeUnavailable"],
  .titlebar-gxserver-daemon-dot[data-state="runtimeUnavailable"],
  .titlebar-gxserver-daemon-dot[data-state="startFailed"] {
    background: #fb7185;
    box-shadow: 0 0 0 3px rgba(251,113,133,0.16);
  }
  .titlebar-gxserver-daemon-copy {
    display: grid;
    font: 400 11px/1.25 -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    gap: 1px;
    min-width: 0;
  }
  .titlebar-gxserver-daemon-copy span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-gxserver-daemon-copy span:first-child {
    color: rgba(255,255,255,0.92);
    font-weight: 400;
  }
  .titlebar-gxserver-daemon-controls {
    gap: 6px;
  }
  .titlebar-gxserver-daemon-icon-button {
    align-items: center;
    background: rgba(255,255,255,0.08);
    border: 1px solid rgba(255,255,255,0.12);
    color: rgba(255,255,255,0.78);
    display: inline-flex;
    height: 24px;
    justify-content: center;
    width: 24px;
  }
  .titlebar-gxserver-daemon-icon-button:disabled {
    color: rgba(255,255,255,0.28);
  }
  .titlebar-gxserver-daemon-icon-button:not(:disabled):hover {
    background: rgba(255,255,255,0.14);
    color: rgba(255,255,255,0.94);
  }
  .titlebar-gxserver-daemon-checkbox {
    color: rgba(255,255,255,0.58);
    gap: 4px;
    font: 400 10px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    white-space: nowrap;
  }
  .titlebar-gxserver-daemon-checkbox input {
    height: 12px;
    margin: 0;
    width: 12px;
  }
  .titlebar-resource-section + .titlebar-resource-section {
    margin-top: 8px;
    padding-top: 0;
  }
  .titlebar-resource-section-heading {
    align-items: center;
    color: rgba(255,255,255,0.62);
    display: flex;
    font: 400 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    gap: 6px;
    letter-spacing: 0.08em;
    padding: 4px 2px 7px;
    position: relative;
    text-transform: uppercase;
    width: 100%;
  }
  .titlebar-resource-section-label {
    align-items: center;
    color: inherit;
    display: inline-flex;
    flex: 1;
    font: inherit;
    gap: 6px;
    letter-spacing: inherit;
    min-width: 0;
    padding: 0;
    text-transform: inherit;
  }
  .titlebar-resource-section-quit-button {
    height: 22px;
    position: absolute;
    right: 2px;
    top: 2px;
  }
  .titlebar-resource-section-heading:hover .titlebar-resource-section-summary,
  .titlebar-resource-section-heading:focus-within .titlebar-resource-section-summary {
    /*
     * CDXC:TitlebarResources 2026-05-22-23:21:
     * Section-level Quit actions should replace the CPU/RAM/count metrics on
     * hover, matching resource session rows where destructive controls occupy
     * the metrics area instead of adding another right-edge control.
     */
    opacity: 0;
  }
  .titlebar-resource-collapse-button svg[data-collapsed="true"] {
    transform: rotate(-90deg);
  }
  .titlebar-resource-section-count {
    color: rgba(255,255,255,0.38);
  }
  .titlebar-resource-section-summary {
    color: rgba(255,255,255,0.52);
    gap: 10px;
    margin-left: auto;
    text-transform: none;
    transition: opacity 120ms ease;
  }
  .titlebar-resource-section-summary span {
    gap: 4px;
    letter-spacing: 0;
  }
  .titlebar-resource-section-body {
    /*
     * CDXC:TitlebarResources 2026-05-28-10:17:
     * Expanded project sections need a small gutter below the project header so
     * the hover-revealed Sleep Project button does not visually touch the first
     * resource row.
     */
    display: grid;
    gap: 7px;
    margin-top: 5px;
  }
  .titlebar-resource-bundle {
    border: 1px solid rgba(255,255,255,0.1);
    border-radius: 0;
    overflow: hidden;
    background: rgba(255,255,255,0.025);
  }
  .titlebar-resource-bundle[data-quitting="true"] {
    opacity: 0.3;
  }
  .titlebar-resource-row {
    /*
     * CDXC:TitlebarResources 2026-05-16-20:07:
     * Long session titles must not shift row controls. Keep identity controls in
     * fixed grid tracks and let only the text track shrink.
     *
     * CDXC:TitlebarResources 2026-06-13-00:56:
     * Per-item Focus and Sleep/Close buttons are fixed visible columns. Do not
     * overlay them on hover or hide metrics to reveal
     * actions; normal hover on the buttons is the only interaction treatment.
     *
     * CDXC:TitlebarResources 2026-06-13-02:07:
     * CPU and RAM should read as one usage cluster. Keep the text and action
     * tracks stable so values do not drift into the action area.
     *
     * CDXC:TitlebarResources 2026-06-16-01:10:
     * CPU and RAM must always occupy the far-right row area. Focus and
     * Sleep/Close sit immediately to the left of the metrics so usage values
     * stay aligned at the panel edge across all resource rows.
     *
     * CDXC:TitlebarResources 2026-06-16-07:37:
     * Resource row action buttons must stay on the same line as the CPU/RAM
     * cards. Explicitly pin every row item to grid row 1 so reordered or
     * conditionally missing controls cannot create a second implicit row.
     *
     * CDXC:TitlebarResources 2026-06-16-07:37:
     * CPU and RAM cards should keep the smaller collapsed-row dimensions at
     * every hierarchy level. Use one fixed metrics cluster for parent rows and
     * expanded child-process rows instead of allowing parent rows to stretch.
     */
    align-items: center;
    display: grid;
    gap: 8px;
    grid-template-columns: minmax(0, 1fr) 24px 24px 200px;
    min-height: 44px;
    overflow: hidden;
    padding: 7px 8px;
    position: relative;
  }
  .titlebar-resource-main {
    align-items: center;
    display: grid;
    gap: 8px;
    grid-column: 1;
    grid-row: 1;
    grid-template-columns: 20px 28px minmax(0, 1fr);
    min-width: 0;
  }
  .titlebar-resource-collapse-button {
    align-items: center;
    background: transparent;
    border: 0;
    color: rgba(255,255,255,0.55);
    display: inline-flex;
    height: 20px;
    justify-content: center;
    padding: 0;
    width: 20px;
  }
  .titlebar-resource-collapse-spacer {
    display: block;
    width: 20px;
  }
  .titlebar-resource-avatar {
    align-items: center;
    background: rgba(255,255,255,0.1);
    border-radius: 0;
    color: rgba(255,255,255,0.84);
    display: inline-flex;
    flex: 0 0 auto;
    font: 400 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    height: 28px;
    justify-content: center;
    width: 28px;
  }
  .titlebar-resource-avatar svg {
    color: rgba(255,255,255,0.82);
  }
  .titlebar-resource-avatar-logo {
    /*
     * CDXC:TitlebarResources 2026-05-26-13:24:
     * Resource avatars use the Agents Hub mask-logo rendering path, so rows get
     * recognizable agent icons without changing the fixed avatar column size.
     */
    display: block;
    height: 15px;
    mask-position: center;
    mask-repeat: no-repeat;
    mask-size: contain;
    width: 15px;
    -webkit-mask-position: center;
    -webkit-mask-repeat: no-repeat;
    -webkit-mask-size: contain;
  }
  .titlebar-resource-text {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .titlebar-resource-name {
    color: rgba(255,255,255,0.94);
    font: 400 13px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-resource-main-link {
    text-decoration: none;
  }
  .titlebar-resource-main-link:hover {
    color: rgba(157, 215, 246, 0.98);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .titlebar-resource-meta {
    align-items: center;
    color: rgba(255,255,255,0.58);
    display: flex;
    font: 400 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
  }
  .titlebar-resource-meta-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .titlebar-resource-child-name {
    color: rgba(255,255,255,0.58);
    font: 400 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-resource-portless-action {
    background: rgba(157, 215, 246, 0.12);
    border: 1px solid rgba(157, 215, 246, 0.26);
    border-radius: 4px;
    color: rgba(201, 232, 248, 0.94);
    flex: 0 0 auto;
    font: 500 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    height: 19px;
    padding: 0 6px;
  }
  .titlebar-resource-portless-action:hover {
    background: rgba(157, 215, 246, 0.18);
  }
  .titlebar-resource-metrics,
  .titlebar-resource-child-metrics {
    align-items: center;
    cursor: default;
    display: grid;
    gap: 8px;
    grid-template-columns: 86px 106px;
    justify-self: end;
    max-width: 200px;
    min-width: 200px;
    width: 200px;
  }
  .titlebar-resource-metrics {
    grid-column: 4;
    grid-row: 1;
  }
  .titlebar-resource-child-metrics {
    grid-column: 2;
  }
  .titlebar-resource-metric {
    align-items: center;
    background: rgba(255,255,255,0.055);
    border: 1px solid rgba(255,255,255,0.105);
    box-sizing: border-box;
    color: rgba(255,255,255,0.88);
    cursor: default;
    display: inline-flex;
    font: 400 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    font-variant-numeric: tabular-nums;
    gap: 6px;
    height: 24px;
    justify-content: center;
    min-width: 0;
    padding: 0 8px;
    white-space: nowrap;
    width: 100%;
  }
  .titlebar-resource-metric svg {
    color: rgba(255,255,255,0.62);
  }
  .titlebar-resource-focus-button,
  .titlebar-resource-kill-button {
    align-items: center;
    appearance: none;
    background: rgba(255,255,255,0.14);
    border: 1px solid transparent;
    border-radius: 0;
    color: rgba(255,255,255,0.9);
    display: inline-flex;
    height: 22px;
    justify-content: center;
    padding: 0;
    transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
    width: 22px;
  }
  .titlebar-resource-focus-button {
    /*
     * CDXC:TitlebarResources 2026-05-28-10:39:
     * Keep row Focus directly left of Sleep/Close in a stable action column so
     * the session label and process totals never shift.
     */
    border-color: rgba(255,255,255,0.16);
    grid-column: 2;
    grid-row: 1;
    justify-self: center;
  }
  .titlebar-resource-focus-button:hover,
  .titlebar-resource-focus-button:focus-visible {
    background: rgba(255,255,255,0.2);
    color: rgba(255,255,255,0.96);
    outline: none;
  }
  .titlebar-resource-kill-button {
    /*
     * CDXC:TitlebarResources 2026-06-14-16:50:
     * Row-level Close should carry the same neutral background, border, and
     * icon color as Sleep. The Resources modal still distinguishes the action
     * by the X icon and aria label without using a destructive red palette.
     */
    background: rgba(255,255,255,0.14);
    border-color: rgba(255,255,255,0.16);
    color: rgba(255,255,255,0.9);
    grid-column: 3;
    grid-row: 1;
    justify-self: center;
  }
  .titlebar-resource-kill-button[data-action="sleep"],
  .titlebar-resource-kill-button[data-action="stop"] {
    background: rgba(255,255,255,0.14);
    border-color: rgba(255,255,255,0.16);
    color: rgba(255,255,255,0.9);
  }
  .titlebar-resource-kill-button[data-action="sleep"]:hover,
  .titlebar-resource-kill-button[data-action="sleep"]:focus-visible,
  .titlebar-resource-kill-button[data-action="stop"]:hover,
  .titlebar-resource-kill-button[data-action="stop"]:focus-visible,
  .titlebar-resource-kill-button[data-action="quit"]:hover,
  .titlebar-resource-kill-button[data-action="quit"]:focus-visible {
    background: rgba(255,255,255,0.2);
    color: rgba(255,255,255,0.96);
    outline: none;
  }
  .titlebar-resource-children {
    display: grid;
    padding: 0 8px 8px 64px;
  }
  .titlebar-resource-child-row {
    align-items: center;
    display: grid;
    gap: 8px;
    grid-template-columns: minmax(0, 1fr) 200px;
    min-height: 24px;
  }
  .titlebar-resources-empty {
    color: rgba(255,255,255,0.54);
    font: 400 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    padding: 10px 4px;
  }
  /*
   * CDXC:TooltipLifecycle 2026-06-13-02:30:
   * Titlebar native pointer-out may hide currently visible tooltip surfaces,
   * but it must not reset all hover styling or stay false until a click. The
   * main titlebar document restores this flag on DOM pointer movement so hover
   * tooltips can appear again immediately.
   */
  body[data-native-pointer-inside="false"] [data-slot="tooltip-content"],
  body[data-native-pointer-inside="false"] .titlebar-action-command-tooltip,
  body[data-native-pointer-inside="false"] .titlebar-resource-tooltip {
    opacity: 0 !important;
    pointer-events: none !important;
    visibility: hidden !important;
  }
`;
document.head.append(styleElement);

const titlebarRootElement = document.getElementById("root");
if (titlebarRootElement && initialTitlebarDropdownPanelKind) {
  titlebarRootElement.dataset.titlebarDropdownPanel = "true";
  titlebarRootElement.style.display = "block";
  titlebarRootElement.style.height = "100%";
  titlebarRootElement.style.margin = "0";
  titlebarRootElement.style.overflow = "hidden";
  titlebarRootElement.style.padding = "0";
  titlebarRootElement.style.width = "100%";
}
if (titlebarRootElement && titlebarRootElement.dataset.ghostexTitlebar !== "false") {
  createRoot(titlebarRootElement).render(<App />);
}
