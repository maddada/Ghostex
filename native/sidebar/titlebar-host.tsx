import {
  IconAlertTriangle,
  IconArrowsDiagonal2,
  IconArrowsDiagonalMinimize,
  IconBox,
  IconCheck,
  IconChevronDown,
  IconChevronLeft,
  IconChevronRight,
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
  IconInfoCircle,
  IconLayoutSidebarLeftCollapse,
  IconLayoutSidebarLeftExpand,
  IconLoader2,
  IconMoon,
  IconPlayerPlay,
  IconRefresh,
  IconRocket,
  IconSearch,
  IconSettings,
  IconStackPush,
  IconTerminal2,
  IconUpload,
  IconUser,
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
  type ReactElement,
  type ReactNode,
} from "react";
import { createRoot } from "react-dom/client";
import { motion } from "motion/react";
import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import { AppTooltip, TooltipProvider } from "../../sidebar/app-tooltip";
import type { SidebarProjectDiffStats } from "../../shared/project-diff-stats";
import { createDefaultSidebarProjectDiffStats } from "../../shared/project-diff-stats";
import {
  DEFAULT_BROWSER_ACTION_URL,
  getSidebarCommandPreviewLabel,
  isSidebarCommandConfigured,
  type SidebarCommandButton,
} from "../../shared/sidebar-commands";
import {
  DEFAULT_SIDEBAR_COMMAND_ICON,
  DEFAULT_SIDEBAR_COMMAND_ICON_COLOR,
} from "../../shared/sidebar-command-icons";
import type { CommandConfigDraft } from "../../sidebar/command-config-modal";
import { AGENT_LOGO_COLORS, AGENT_LOGOS } from "../../sidebar/agent-logos";
import {
  getDefaultSidebarAgentByIcon,
  type SidebarAgentIcon,
} from "../../shared/sidebar-agents";
import type {
  SidebarAgentHookStatusMessage,
  SidebarGhostexCliStatusMessage,
} from "../../shared/session-grid-contract-sidebar";
import {
  KEEP_AWAKE_DURATION_OPTIONS,
  normalizeghostexSettings,
  type KeepAwakeDurationMinutes,
  type SessionPersistenceProvider,
} from "../../shared/ghostex-settings";
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
import { createCombinedProjectSessionId, parseCombinedProjectGroupId } from "./combined-sidebar-mode";
import "../../sidebar/styles.css";
import {
  buildSidebarGitMenuItems,
  createDefaultSidebarGitState,
  resolveSidebarGitPrimaryActionState,
  type SidebarGitAction,
  type SidebarGitState,
} from "../../shared/sidebar-git";

type ProjectEditorLoadStatus = "idle" | "opening" | "running" | "error";
type TitlebarMode = "agents" | "code" | "git" | "tasks";
type TitlebarDropdownPanelKind =
  | "actions"
  | "git"
  | "keepAwake"
  | "mode"
  | "openIn"
  | "resources"
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
  hideTitlebarControl: boolean;
  preventLidSleep: boolean;
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
  projectId?: string;
  sessionId: string;
  sessionKind?: "browser" | "terminal" | "t3";
  sessionPersistenceName?: string;
  sessionPersistenceProvider?: string;
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
  body: string;
  icon: TitlebarTipIcon;
  id: string;
  title: string;
};

type TitlebarNotice = {
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
  kind: "browser" | "code" | "git" | "tasks" | string;
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
  agentHookStatus?: SidebarAgentHookStatusMessage;
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  debuggingMode: boolean;
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
  sidebarCollapsed: boolean;
  sidebarActions: TitlebarSidebarActionsSettings;
  showProjectEditorDiffFileCount: boolean;
  sessionPersistenceProvider: SessionPersistenceProvider;
  toggleSidebarHotkeyLabel: string;
  workspaceOpenTargets: TitlebarOpenTargetsSettings;
  isFocusModeActive?: boolean;
  updateAvailable: boolean;
};

type ResourceProcess = {
  command: string;
  cpu: number;
  pid: number;
  ppid: number;
  rssMb: number;
};

type ResourceProcessBundle = {
  childProcesses: ResourceProcess[];
  cpu: number;
  key: string;
  label: string;
  memoryMb: number;
  pids: number[];
  process?: ResourceProcess;
  browserTab?: TitlebarBrowserTabResource;
  session?: TitlebarResourceSession;
  type: "browser" | "code" | "orphan" | "session";
};

type ResourceGroupView = {
  bundles: ResourceProcessBundle[];
  group: TitlebarResourceGroup;
};

type NativeTitlebarCommand =
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
  | { type: "openActiveProjectEditorFromTitlebar" }
  | { type: "toggleProjectEditorCompanionFromTitlebar" }
  | { type: "exitFocusModeFromTitlebar" }
  | { type: "openAgentsModeFromTitlebar" }
  | { type: "openGitHubProjectFromTitlebar" }
  | { type: "openTasksPlaceholderFromTitlebar" }
  | { type: "refreshWorkspaceOpenTargetAvailabilityFromTitlebar" }
  | { type: "toggleCommandsPanelFromTitlebar" }
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
  | {
      anchorRect: { height: number; width: number; x: number; y: number };
      kind: TitlebarDropdownPanelKind;
      preferredSize: TitlebarDropdownPanelSize;
      type: "showTitlebarDropdownPanel";
    }
  | { type: "closeTitlebarDropdownPanel" }
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
      regions: Array<{ height: number; width: number; x: number; y: number }>;
      type: "setReactTitlebarHitRegions";
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
    __ghostex_TITLEBAR_PANEL_KIND__?: string;
    __ghostex_PENDING_TITLEBAR_PROJECT_STATE__?: Partial<TitlebarProjectState>;
    __ghostex_TITLEBAR__?: {
      closeOpenDropdowns: () => void;
      setActiveProjectState: (state: Partial<TitlebarProjectState>) => void;
      setNativeDropdownOpen: (kind: TitlebarDropdownPanelKind | undefined) => void;
      setNativePointerInside: (isInside: boolean) => void;
    };
  }
}

const LAST_OPEN_TARGET_STORAGE_KEY = "ghostex.titlebar.lastOpenTargetId";
const LAST_ACTION_COMMAND_STORAGE_PREFIX = "ghostex.titlebar.lastActionCommandByProject:";
const KEEP_AWAKE_RUNTIME_STORAGE_KEY = "ghostex.titlebar.keepAwakeRuntime";
const KEEP_AWAKE_LID_SLEEP_STORAGE_KEY = "ghostex.titlebar.lidSleepPrevention";
const TITLEBAR_TIPS_READ_STORAGE_KEY = "ghostex.titlebar.tips.readIds";
const KEEP_AWAKE_POWER_CHECK_INTERVAL_MS = 30_000;
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
const TITLEBAR_CONTROL_TOP = 1;
const TITLEBAR_PROJECT_TOP = TITLEBAR_CONTROL_TOP;
const TITLEBAR_CENTER_CONTROLS_TOP = TITLEBAR_CONTROL_TOP;
const TITLEBAR_RIGHT_CONTROLS_TOP = TITLEBAR_CONTROL_TOP;
const RESOURCE_POLL_INTERVAL_MS = 5_000;
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
      return compactTitlebarDropdownPanelSize(
        titlebarMenuHeight(Math.max(1, counts.gitItemCount)),
      );
    case "keepAwake":
      return compactTitlebarDropdownPanelSize(
        titlebarMenuHeight(
          KEEP_AWAKE_DURATION_OPTIONS.length + (counts.keepAwakeIsRunning ? 1 : 0) + 1,
          { separatorCount: 1 },
        ),
      );
    case "mode":
      return compactTitlebarDropdownPanelSize(
        titlebarMenuHeight(Math.max(1, counts.modeOptionCount)),
      );
    case "openIn":
      return compactTitlebarDropdownPanelSize(
        titlebarMenuHeight(Math.max(0, counts.targetCount) + 1, { separatorCount: 1 }),
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
 * CDXC:TipsAndTricks 2026-06-10-22:15:
 * The first tip should introduce Cmd K as the universal entry point for app actions, not only pane moves.
 */
const TITLEBAR_TIPS: TitlebarTip[] = [
  {
    body: "Search for project actions, pane splits and moves, session controls, settings shortcuts, and other Ghostex actions.",
    icon: "command",
    id: "command-palette-all-actions",
    title: "Press Cmd K anywhere to open the Command Palette",
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
 * Debugging Mode intentionally writes detailed diagnostics to disk and can
 * affect app performance. Surface a non-dismissable Tips & Tricks notice while
 * it is enabled so users turn it off after reproducing an issue.
 */
const TITLEBAR_DEBUGGING_MODE_NOTICE: TitlebarNotice = {
  body: "Ghostex is writing detailed diagnostics to disk. Turn Debug logging and UI off when you are not actively debugging to reduce CPU and disk use.",
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
  const missingLiveAgents = new Map<string, string>();
  const outdatedLiveAgents = new Map<string, string>();
  for (const group of resourceGroups) {
    for (const session of group.sessions) {
      if (!isTitlebarLiveTerminalAgentSession(session)) {
        continue;
      }
      const agent = getDefaultSidebarAgentByIcon(session.agentIcon as SidebarAgentIcon | undefined);
      if (!agent || agent.agentId === "t3") {
        continue;
      }
      const status = hookStatusByAgentId.get(agent.agentId);
      if (!status || status.status === "installed" || status.status === "notRequired") {
        continue;
      }
      if (status.status === "updateRequired") {
        outdatedLiveAgents.set(agent.agentId, agent.name);
      } else {
        missingLiveAgents.set(agent.agentId, agent.name);
      }
    }
  }
  const agentNames = [...outdatedLiveAgents.values(), ...missingLiveAgents.values()];
  if (agentNames.length === 0) {
    return undefined;
  }

  /**
   * CDXC:AgentHookSettings 2026-06-07-08:51:
   * Live supported agents without installed Ghostex hooks should surface in
   * Tips & Tricks as non-dismissable runtime notices. Hooks power gxserver's
   * working/attention status transitions, exact resume metadata, and
   * first-message naming, so read-once tips are the wrong model while affected
   * sessions are still running.
   *
   * CDXC:AgentHooks 2026-06-07-11:05:
   * gxserver now distinguishes old Ghostex hooks from absent hooks. The
   * titlebar notice should ask users to update old hooks instead of saying they
   * are not installed, because the reliable fix is migration to the current
   * gxserver ingest hook rather than accepting stale native-era artifacts.
   */
  const formattedAgents = formatTitlebarNoticeNameList(agentNames);
  const plural = agentNames.length > 1;
  const hasOutdatedHooks = outdatedLiveAgents.size > 0;
  const hasMissingHooks = missingLiveAgents.size > 0;
  const action = hasOutdatedHooks && hasMissingHooks ? "setup" : hasOutdatedHooks ? "update" : "install";
  const actionVerb = action === "setup" ? "set up" : action === "update" ? "updated" : "installed";
  return {
    body: `${formattedAgents} ${plural ? "need" : "needs"} ${plural ? "their" : "its"} Ghostex ${plural ? "hooks" : "hook"} ${actionVerb}. Working/done statuses, attention state, resume metadata, and first-message session naming can be unreliable until hooks are ${action === "setup" ? "installed or updated" : actionVerb}.`,
    icon: "warning",
    id: `agent-hooks-${action}-${[...outdatedLiveAgents.keys(), ...missingLiveAgents.keys()].sort().join("-")}`,
    settingsTarget: "agentHooks",
    title: action === "setup"
      ? "Set up hooks for live agents"
      : action === "update"
        ? plural ? "Update hooks for live agents" : `${formattedAgents} hook needs update`
        : plural ? "Install hooks for live agents" : `${formattedAgents} hook is missing`,
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
  startedAtMs: number;
};

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
   * the flag false until a click enters a measured hit region, so titlebar
   * tooltips must rely on normal CSS hover and local tooltip state instead.
   */
  document.body.dataset.nativePointerInside = isInside ? "true" : "false";
}

function suppressTitlebarTooltipsFromDom(): void {
  setTitlebarNativePointerInside(false);
}

function enableTitlebarTooltipsFromDom(): void {
  setTitlebarNativePointerInside(true);
}

function TitlebarAppTooltip({
  children,
  content,
  side = "left",
  sideOffset = 7,
}: {
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
      content={content}
      contentClassName="titlebar-app-tooltip"
      side={side}
      sideOffset={sideOffset}
    >
      {children}
    </AppTooltip>
  );
}

function postTitlebarSidebarCommand(message: { type: "requestAgentHookStatus" } | { type: "requestGhostexCliStatus" }): void {
  /*
  CDXC:AgentHooks 2026-06-07-11:05:
  Opening Tips & Tricks should refresh gxserver hook status instead of relying
  on the titlebar's cached layout snapshot. Route through the existing
  app-modal sidebarCommand bridge so the native sidebar remains the owner of
  authenticated gxserver requests and hook-status state publication.

  CDXC:CliInstall 2026-06-07-15:26:
  Tips & Tricks CLI notices must use the native sidebar's real PATH inspection
  instead of probing from the isolated titlebar webview.
  */
  window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
    message,
    type: "sidebarCommand",
  });
}

function appendTitlebarActionCrashDebugLog(event: string, details?: unknown): void {
  /**
   * CDXC:TitlebarActions 2026-05-15-17:23:
   * Terminal action button crashes need a breadcrumb from the isolated React
   * titlebar before the native-sidebar command runner receives the click.
   * Persist this trace outside the normal debug-toggle filter so a repro that
   * exits the app still leaves the selected action id and project context.
   */
  postNative({
    details: details === undefined ? undefined : JSON.stringify(details),
    event,
    type: "appendTerminalFocusDebugLog",
  });
}

function appendTitlebarCodeLagDebugLog(
  debuggingMode: boolean,
  event: string,
  details?: unknown,
): void {
  /**
   * CDXC:ModeSwitcher 2026-05-16-07:23:
   * Titlebar Code-click lag breadcrumbs are regular diagnostics. Send them only
   * while Settings Debugging Mode is enabled, matching the app-wide requirement
   * that non-error logging stays silent during normal use.
   */
  if (!debuggingMode) {
    return;
  }
  postNative({
    details: JSON.stringify({
      details,
      performanceNowMs: performance.now(),
      wallTimeMs: Date.now(),
    }),
    event,
    type: "appendSessionTitleDebugLog",
  });
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

async function readResourceProcesses(): Promise<ResourceProcess[]> {
  const result = await runNativeProcess("/bin/ps", [
    "-axo",
    "pid=,ppid=,pcpu=,rss=,command=",
  ]);
  return result.exitCode === 0 ? parseResourceProcessTable(result.stdout) : [];
}

/**
 * CDXC:TitlebarResources 2026-05-23-10:46:
 * Resource-manager Quit is a process-manager action, so it must terminate the
 * exact processes shown in the dropdown while the sidebar separately preserves
 * terminal cards as sleeping sessions. Recheck the command before SIGKILL so a
 * delayed hard kill cannot target an unrelated process that reused the PID.
 */
async function terminateResourceProcesses(processes: ResourceProcess[]): Promise<void> {
  const targets = new Map(
    processes
      .filter((process) => Number.isFinite(process.pid) && process.pid > 1)
      .map((process) => [process.pid, process.command]),
  );
  if (targets.size === 0) {
    return;
  }

  await runNativeProcess("/bin/kill", ["-TERM", ...Array.from(targets.keys()).map(String)]);
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
): { browserBundles: ResourceProcessBundle[]; groupViews: ResourceGroupView[]; orphanBundles: ResourceProcessBundle[] } {
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
    const codeBundle = createProjectCodeServerBundle(group, processes, childrenByParent, claimedPids);
    const browserBundles = createBrowserBundles(groupBrowserTabs, processes, claimedPids, {
      includeRuntimeBundles: false,
    });
    return {
      bundles: [...bundles, ...(codeBundle ? [codeBundle] : []), ...browserBundles],
      group,
    };
  });
  claimAppRuntimeProcesses(processes, childrenByParent, claimedPids);
  const browserBundles = createBrowserBundles(
    browserTabs.filter((tab) => !groupedBrowserTabIds.has(tab.id)),
    processes,
    claimedPids,
  );
  const orphanBundles = createOrphanBundles(processes, childrenByParent, claimedPids);
  return { browserBundles, groupViews, orphanBundles };
}

const EMPTY_RESOURCE_GROUP_VIEWS: ReturnType<typeof createResourceGroupViews> = {
  browserBundles: [],
  groupViews: [],
  orphanBundles: [],
};

type ResourceItemCollapseTarget = {
  collapsedWhenKeyPresent: boolean;
  key: string;
};

function createResourceItemCollapseTarget(bundle: ResourceProcessBundle): ResourceItemCollapseTarget | undefined {
  if (bundle.childProcesses.length === 0) {
    return undefined;
  }
  const collapsedByDefault = bundle.type === "session" || bundle.type === "browser";
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
    ...resourceViews.groupViews
      .filter((view) => view.bundles.length > 0)
      .flatMap((view) => view.bundles),
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
  if (seedProcesses.length === 0 && session.sessionKind !== "browser") {
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

function createProjectCodeServerBundle(
  group: TitlebarResourceGroup,
  processes: ResourceProcess[],
  childrenByParent: Map<number, ResourceProcess[]>,
  claimedPids: Set<number>,
): ResourceProcessBundle | undefined {
  if (!group.projectPath) {
    return undefined;
  }
  const seedProcesses = processes.filter(
    (process) =>
      !claimedPids.has(process.pid) &&
      process.command.includes("code-server") &&
      process.command.includes(group.projectPath),
  );
  if (seedProcesses.length === 0) {
    return undefined;
  }
  const tree = collectProcessTree(seedProcesses, childrenByParent);
  tree.forEach((process) => claimedPids.add(process.pid));
  return {
    childProcesses: tree.filter((process) => !seedProcesses.some((seed) => seed.pid === process.pid)),
    cpu: sumProcessCpu(tree),
    key: `code:${group.groupId}`,
    label: "Code",
    memoryMb: sumProcessMemory(tree),
    pids: tree.map((process) => process.pid),
    process: seedProcesses[0],
    type: "code",
  };
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
      .map((session) =>
        session.projectId
          ? createCombinedProjectSessionId(session.projectId, session.sessionId)
          : session.sessionId,
      ),
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

function resourceBundleSidebarSessionIds(bundle: ResourceProcessBundle): string[] {
  const session = bundle.session;
  if (session) {
    return [
      session.projectId
        ? createCombinedProjectSessionId(session.projectId, session.sessionId)
        : session.sessionId,
    ];
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

function resourceBundleProjectEditorIds(bundle: ResourceProcessBundle): string[] {
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

function formatWholeMemory(value: number): string {
  return value >= 1024
    ? `${Math.trunc(value / 1024)} GB`
    : `${Math.trunc(Math.max(0, value))} MB`;
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
  const [resourceProcesses, setResourceProcesses] = useState<ResourceProcess[]>([]);
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
  const lastCompanionHitRegionSignatureRef = useRef("");
  const resourceRefreshGenerationRef = useRef(0);
  const resourceRefreshInFlightRef = useRef(false);
  const resourcesOpenCollapseSeededRef = useRef(false);
  const activeMode = optimisticMode ?? projectState.activeMode;
  const resourcesPanelActive = titlebarPanelKind === "resources";
  const resourceViews = useMemo(
    () =>
      resourcesPanelActive
        ? createResourceGroupViews(projectState.browserTabs, projectState.resourceGroups, resourceProcesses)
        : EMPTY_RESOURCE_GROUP_VIEWS,
    [projectState.browserTabs, projectState.resourceGroups, resourceProcesses, resourcesPanelActive],
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
  const markAllTipsRead = useCallback(() => {
    setReadTipIds((current) => {
      const next = new Set(current);
      let changed = false;
      for (const tip of TITLEBAR_TIPS) {
        if (!next.has(tip.id)) {
          next.add(tip.id);
          changed = true;
        }
      }
      if (!changed) {
        return current;
      }
      writeStoredTitlebarTipIds(next);
      return next;
    });
  }, []);
  const requestRuntimeStatusForTips = useCallback(() => {
    postTitlebarSidebarCommand({ type: "requestAgentHookStatus" });
    postTitlebarSidebarCommand({ type: "requestGhostexCliStatus" });
  }, []);
  const closeTitlebarDropdownPanel = useCallback(() => {
    postNative({ type: "closeTitlebarDropdownPanel" });
    setNativeDropdownOpen(undefined);
  }, []);
  const showTitlebarDropdownPanel = useCallback(
    (kind: TitlebarDropdownPanelKind, anchor: HTMLElement) => {
      /*
       * CDXC:ReactTitlebar 2026-06-11-23:20:
       * Native child-window dropdown triggers should behave like normal menu
       * buttons: requesting the already-open panel closes it instead of
       * reopening or repositioning the same child window.
       */
      if (nativeDropdownOpen === kind) {
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
  const gitPrimaryLabel = titlebarPrimaryGitActionLabel(gitPrimaryAction.label);
  const gitPrimaryCompactLabel = compactTitlebarPrimaryGitActionLabel(gitPrimaryAction.label);
  const shouldCompactGitPrimaryLabel = gitPrimaryCompactLabel !== gitPrimaryLabel;
  const gitMenuItems = useMemo(
    () => buildSidebarGitMenuItems(projectState.git),
    [projectState.git],
  );
  const publishHitRegions = useCallback(() => {
    if (isDropdownPanel) {
      return;
    }
    /**
     * CDXC:ReactTitlebar 2026-05-11-00:22
     * Measure titlebar hit-region elements in the document so AppKit lets fixed
     * titlebar controls receive pointer events while blank titlebar space remains
     * native draggable chrome.
     *
     * CDXC:ReactTitlebar 2026-05-12-18:58
     * Publish the measured rectangles after layout settles as well as during the
     * commit so conditional titlebar controls cannot briefly be treated as blank
     * AppKit titlebar pixels.
     *
     * CDXC:ReactTitlebar 2026-06-11-13:22:
     * Dropdown panels are separate native child windows, so the titlebar document
     * must publish only strip-contained hit regions and must never publish panel
     * geometry from the child-window document.
     */
    const regions = Array.from(
      document.querySelectorAll<HTMLElement>("[data-titlebar-hit-region]"),
    ).map((element) => {
      const rect = element.getBoundingClientRect();
      return {
        height: rect.height,
        width: rect.width,
        x: rect.x,
        y: rect.y,
      };
    });
    const companionToggleButton = document.querySelector<HTMLElement>(
      ".titlebar-companion-toggle-button",
    );
    if (companionToggleButton) {
      /*
       * CDXC:ProjectEditorCompanion 2026-06-12-03:18:
       * The titlebar toggle now owns both expanding and collapsing the companion
       * pane. Log the measured React hit rect when it changes so missed AppKit
       * clicks can still be compared to the actual DOM geometry without logging
       * every pointer event.
       */
      const rect = companionToggleButton.getBoundingClientRect();
      const signature = [
        activeMode,
        projectState.editorIsOpen ? "open" : "closed",
        projectState.editorIsSleeping ? "sleeping" : "awake",
        projectState.projectEditorCompanionPaneHidden ? "hidden" : "visible",
        projectState.projectId,
        Math.round(rect.x),
        Math.round(rect.y),
        Math.round(rect.width),
        Math.round(rect.height),
        titlebarOverlayOpen ? "overlay" : "plain",
      ].join("|");
      if (signature !== lastCompanionHitRegionSignatureRef.current) {
        lastCompanionHitRegionSignatureRef.current = signature;
        appendTitlebarCodeLagDebugLog(
          projectState.debuggingMode,
          "titlebarCompanionToggle.hitRegionMeasured",
          {
            activeMode,
            editorIsOpen: projectState.editorIsOpen,
            editorIsSleeping: projectState.editorIsSleeping,
            projectEditorCompanionPaneHidden: projectState.projectEditorCompanionPaneHidden,
            projectId: projectState.projectId,
            rect: {
              height: rect.height,
              width: rect.width,
              x: rect.x,
              y: rect.y,
            },
            titlebarOverlayOpen,
          },
        );
      }
    }
    postNative({
      overlayOpen: titlebarOverlayOpen,
      regions,
      type: "setReactTitlebarHitRegions",
    });
  }, [
    activeMode,
    projectState.editorIsOpen,
    projectState.editorIsSleeping,
    projectState.projectEditorCompanionPaneHidden,
    projectState.projectId,
    titlebarOverlayOpen,
    isDropdownPanel,
  ]);

  const publishSettledHitRegions = useCallback(() => {
    publishHitRegions();
    let secondFrame = 0;
    const firstFrame = window.requestAnimationFrame(() => {
      publishHitRegions();
      secondFrame = window.requestAnimationFrame(publishHitRegions);
    });
    const settledTimeout = window.setTimeout(publishHitRegions, 120);
    return () => {
      window.cancelAnimationFrame(firstFrame);
      if (secondFrame !== 0) {
        window.cancelAnimationFrame(secondFrame);
      }
      window.clearTimeout(settledTimeout);
    };
  }, [publishHitRegions]);

  useLayoutEffect(() => {
    if (isDropdownPanel) {
      return;
    }
    /**
     * CDXC:SessionFocusMode 2026-05-26-22:47:
     * The Exit focus button is conditional titlebar chrome. Republish native
     * hit regions whenever focus mode enters or exits so AppKit routes clicks
     * to the new button instead of treating its frame as draggable titlebar.
     *
     * CDXC:AutoUpdate 2026-06-08-18:21:
     * The update button appears after native Sparkle appcast probes, so
     * updateAvailable must also republish hit regions. Otherwise AppKit can
     * keep treating the new button's pixels as draggable titlebar instead of a
     * clickable handoff into Sparkle.
     */
    return publishSettledHitRegions();
  }, [
    activeTarget?.id,
    activeAction?.commandId,
    keepAwakeRuntime?.pid,
    resourceProcesses.length,
    projectState.projectEditorCompanionPaneHidden,
    projectState.gxserverDaemon.state,
    projectState.projectIconDataUrl,
    projectState.isFocusModeActive,
    projectState.projectName,
    projectState.sidebarCollapsed,
    projectState.updateAvailable,
    publishSettledHitRegions,
    isDropdownPanel,
  ]);

  useEffect(() => {
    if (isDropdownPanel) {
      return;
    }
    window.addEventListener("resize", publishHitRegions);
    return () => window.removeEventListener("resize", publishHitRegions);
  }, [publishHitRegions, isDropdownPanel]);

  useEffect(() => {
    if (isDropdownPanel) {
      return;
    }
    /*
     * CDXC:TooltipLifecycle 2026-06-13-02:30:
     * Native titlebar pointer-leave may hide a currently visible tooltip, but
     * DOM pointer movement inside the titlebar must immediately restore hover
     * eligibility. This keeps native tracking as cleanup, not a persistent gate
     * that waits for a click on a measured hit region.
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
       * Swift infer it from stale DOM hit-region geometry.
       */
      postNative({
        overlayOpen: false,
        regions: [],
        type: "setReactTitlebarHitRegions",
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
      setNativeDropdownOpen,
      setActiveProjectState: (state) => {
        setProjectState((current) => mergeTitlebarProjectState(current, state));
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
    return () => {
      delete window.__ghostex_TITLEBAR__;
    };
  }, [closeTitlebarDropdownPanel]);

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

  const refreshResources = useCallback(async (generation: number) => {
    if (resourceRefreshInFlightRef.current) {
      return;
    }
    resourceRefreshInFlightRef.current = true;
    try {
      const processes = await readResourceProcesses();
      if (generation === resourceRefreshGenerationRef.current) {
        setResourceProcesses(processes);
        setResourceProcessSnapshotReady(true);
      }
    } catch (error) {
      console.warn("Failed to refresh Ghostex resources", error);
      if (generation === resourceRefreshGenerationRef.current) {
        setResourceProcessSnapshotReady(true);
      }
    } finally {
      resourceRefreshInFlightRef.current = false;
    }
  }, []);

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
    const resourceItemCollapseTargets = createResourceViewItemCollapseTargets(resourceViews);
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
  }, [ resourceViews, resourcesPanelActive ]);

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

  const openSidebarActionEditor = (command: SidebarCommandButton) => {
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      commandDraft: createTitlebarCommandConfigDraft(command),
      modal: "commandConfig",
      type: "open",
    });
  };

  const runSidebarAction = (command: SidebarCommandButton | undefined) => {
    if (!command) {
      appendTitlebarActionCrashDebugLog("nativeSidebar.actionCrashTrace.titlebarMissingAction", {
        projectId: projectState.projectId,
        projectPath: projectState.projectPath,
      });
      return;
    }
    if (!isSidebarCommandConfigured(command)) {
      openSidebarActionEditor(command);
      return;
    }
    appendTitlebarActionCrashDebugLog("nativeSidebar.actionCrashTrace.titlebarClick", {
      actionType: command.actionType,
      closeTerminalOnExit: command.closeTerminalOnExit,
      commandId: command.commandId,
      hasCommand: Boolean(command.command?.trim()),
      hasUrl: Boolean(command.url?.trim()),
      projectId: projectState.projectId,
      projectPath: projectState.projectPath,
    });
    setSelectedActionCommandId(command.commandId);
    persistLastActionCommandId(projectState, command.commandId);
    postNative({ commandId: command.commandId, type: "runSidebarCommandFromTitlebar" });
  };

  const runGitAction = (action: SidebarGitAction) => {
    postNative({ action, type: "runSidebarGitActionFromTitlebar" });
  };

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
    const uniqueBundles = uniqueResourceBundles(bundles);
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
      void terminateResourceProcesses(processes).finally(() => {
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

  const stopKeepAwake = useCallback(async () => {
    const runtime = keepAwakeRuntime;
    setKeepAwakeRuntime(undefined);
    localStorage.removeItem(KEEP_AWAKE_RUNTIME_STORAGE_KEY);
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
    async (durationMinutes: KeepAwakeDurationMinutes = projectState.keepAwake.defaultDurationMinutes) => {
      if (keepAwakeRuntime) {
        await stopKeepAwake();
      }
      /**
       * CDXC:TitlebarKeepAwake 2026-05-28-19:28:
       * The normal keep-awake button should prevent idle sleep and AC system sleep.
       * Lid-close sleep is controlled by the separate Settings toggle because macOS does not treat it as a regular caffeinate idle-sleep assertion.
       */
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
        startedAtMs: Date.now(),
      };
      setKeepAwakeRuntime(nextRuntime);
      localStorage.setItem(KEEP_AWAKE_RUNTIME_STORAGE_KEY, JSON.stringify(nextRuntime));
    },
    [keepAwakeRuntime, projectState.keepAwake.allowDisplaySleep, projectState.keepAwake.defaultDurationMinutes, stopKeepAwake],
  );

  const toggleKeepAwake = () => {
    if (keepAwakeRuntime) {
      void stopKeepAwake();
      return;
    }
    void startKeepAwake();
  };

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
     * CDXC:AgentHookSettings 2026-06-07-08:51:
     * Missing-hook Tips notices are actionable runtime warnings. Clicking one
     * should open Settings on the Integrations tab because that page exposes
     * the direct Agent Hooks install row without requiring an expanded details
     * panel.
     */
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
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

  const openNoticeSettings = (target: TitlebarNotice["settingsTarget"]) => {
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
    if (!projectState.keepAwake.activateOnLaunch || keepAwakeRuntime) {
      return;
    }
    void startKeepAwake();
  }, [keepAwakeRuntime, projectState.keepAwake.activateOnLaunch, startKeepAwake]);

  useEffect(() => {
    const desired = Boolean(keepAwakeRuntime && projectState.keepAwake.preventLidSleep);
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
  }, [keepAwakeRuntime, projectState.keepAwake.preventLidSleep]);

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
      !keepAwakeRuntime && projectState.keepAwake.activateOnExternalDisplay;
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
    keepAwakeRuntime,
    projectState.keepAwake.activateOnExternalDisplay,
    projectState.keepAwake.batteryThresholdPercent,
    projectState.keepAwake.deactivateBelowBatteryThreshold,
    projectState.keepAwake.deactivateOnLowPowerMode,
    startKeepAwake,
    stopKeepAwake,
  ]);

  const openAgentsMode = () => {
    setOptimisticMode("agents");
    postNative({ type: "openAgentsModeFromTitlebar" });
  };

  const openCodeMode = () => {
    appendTitlebarCodeLagDebugLog(projectState.debuggingMode, "titlebarCodeLag.titlebarClickStart", {
      activeMode: projectState.activeMode,
      editorIsOpen: projectState.editorIsOpen,
      editorIsSleeping: projectState.editorIsSleeping,
      editorStatus: projectState.editorStatus,
      optimisticMode,
      projectId: projectState.projectId,
      projectPath: projectState.projectPath,
    });
    setOptimisticMode("code");
    postNative({ type: "openActiveProjectEditorFromTitlebar" });
    appendTitlebarCodeLagDebugLog(projectState.debuggingMode, "titlebarCodeLag.titlebarClickPostedNative", {
      projectId: projectState.projectId,
      projectPath: projectState.projectPath,
    });
  };

  /**
   * CDXC:ProjectBrowserTabs 2026-06-13-00:12:
   * The top project browser mode is now user-facing Browser mode. Keep it
   * disabled only for Quick/projectless contexts; real projects without a
   * GitHub remote still open Browser mode with the Ghostex repository as the
   * first tab so the control is always useful.
   */
  const browserModeDisabledReason = projectState.projectIsQuick
    ? "Quick sessions do not have a project Browser view."
    : undefined;
  /*
   * CDXC:ModeSwitcher 2026-06-08-18:39:
   * Quick sessions are projectless work areas, so Kanban should be unavailable
   * there for the same active-context reason as Browser mode. Disable the
   * titlebar tab/button before click dispatch instead of opening an empty
   * project-board surface.
   */
  const kanbanModeDisabledReason = projectState.projectIsQuick
    ? "Quick sessions do not have a Kanban project view."
    : undefined;

  const openGitMode = () => {
    if (browserModeDisabledReason) {
      return;
    }
    setOptimisticMode("git");
    postNative({ type: "openGitHubProjectFromTitlebar" });
  };

  const openTasksMode = () => {
    if (kanbanModeDisabledReason) {
      return;
    }
    setOptimisticMode("tasks");
    postNative({ type: "openTasksPlaceholderFromTitlebar" });
  };

  const toggleProjectEditorCompanion = () => {
    appendTitlebarCodeLagDebugLog(projectState.debuggingMode, "titlebarCompanionToggle.dispatch", {
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
    postNative({ type: "showUpdateDialogFromTitlebar" });
  };

  const shouldShowCompanionToggleButton =
    activeMode !== "agents" &&
    projectState.editorIsOpen &&
    !projectState.editorIsSleeping;
  /*
   * CDXC:TitlebarModeTabs 2026-05-31-12:00:
   * macOS titlebar mode switcher labels use title case (Agents, Source, Browser, Kanban), not all-caps, so the segmented control reads like navigation chrome rather than shouting labels.
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

  if (titlebarPanelKind) {
    return (
      <TooltipProvider delayDuration={300}>
        <TitlebarDropdownPanelSurface
          activeMode={activeMode}
          activeTarget={activeTarget}
          browserBundles={resourceViews.browserBundles}
          collapsedResourceKeys={collapsedResourceKeys}
          daemon={projectState.gxserverDaemon}
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
          onMarkAllTipsRead={markAllTipsRead}
          onMarkTipRead={markTipRead}
          onOpenNoticeSettings={openNoticeSettings}
          onOpenPowerSettings={openPowerSettings}
          onOpenTarget={openTarget}
          onQuitResources={quitResourceBundles}
          onRunAction={runSidebarAction}
          onRunGitAction={runGitAction}
          onSetResourceItemsCollapsed={setResourceItemsCollapsed}
          onSleepInactiveSessions={sleepInactiveTerminalSessions}
          onStartKeepAwake={startKeepAwake}
          onStopKeepAwake={stopKeepAwake}
          onToggleResourceCollapse={toggleResourceCollapse}
          orphanBundles={resourceViews.orphanBundles}
          resourceProcessSnapshotReady={resourceProcessSnapshotReady}
          quittingResourceKeys={quittingResourceKeys}
          readTips={readTips}
          resourceGroupViews={resourceViews.groupViews}
          selectedActionCommandId={selectedActionCommandId}
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
        data-titlebar-hit-region
        onClick={() => postNative({ type: "toggleSidebarCollapsed" })}
        type="button"
        variant="ghost"
      >
        {/*
         * CDXC:SidebarCollapse 2026-06-12-10:57:
         * Users need a traffic-light-sized titlebar button immediately
         * before the project identity to collapse or expand the entire
         * native sidebar. The chevron points left while expanded and
         * right while collapsed so it always indicates the next action.
         *
         * CDXC:SidebarCollapse 2026-06-12-11:10:
         * The update affordance belongs to the right of this collapse
         * button, with a 9px gap between the two compact titlebar buttons.
         * Keep the collapse button one pixel lower than its first pass and
         * use a 10px chevron so the glyph reads clearly inside the 14px dot.
         *
         * CDXC:SidebarCollapse 2026-06-12-21:03:
         * The visible collapse affordance is now 15x15px, while the actual
         * titlebar hit target is a 33x33px square that extends 9px around
         * the small dot without painting that larger area.
         *
         * CDXC:SidebarCollapse 2026-06-13-10:53:
         * The hover tooltip for this button must contain only the assigned Toggle
         * Sidebar hotkey so the tiny titlebar affordance stays terse.
         *
         * CDXC:SidebarCollapse 2026-06-13-01:00:
         * Move only the visible 15x15 dot 2px lower. The 33x33 hit target stays
         * fixed so clicking and native hit-region routing remain stable.
         *
         * CDXC:SidebarCollapse 2026-06-13-02:59:
         * Use the same AppTooltip wrapper as sidebar controls for the hotkey
         * label; keep the titlebar-specific wrapper responsible only for right
         * placement beside the traffic-light-side button.
         */}
        <span className="titlebar-sidebar-collapse-button-visual">
          {projectState.sidebarCollapsed ? (
            <IconChevronRight aria-hidden="true" size={10} stroke={2.4} />
          ) : (
            <IconChevronLeft aria-hidden="true" size={10} stroke={2.4} />
          )}
        </span>
      </Button>
    </TitlebarAppTooltip>
  );

  return (
    <TooltipProvider delayDuration={300}>
      <div className="dark" ref={rootRef} style={styles.shell}>
        <div style={styles.titlebar}>
          <div style={styles.projectSlot}>
            {titlebarSidebarCollapseButton}
            {projectState.updateAvailable ? (
              <TitlebarAppTooltip content="Download update" side="right">
                <Button
                  aria-label="Download update"
                  className="titlebar-session-button titlebar-update-button"
                  data-titlebar-hit-region
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
                   */}
                  <IconDownload aria-hidden="true" size={15} stroke={1.8} />
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
            {projectState.isFocusModeActive ? (
              <Button
                aria-label="Exit focus mode"
                className="titlebar-exit-focus-button"
                data-titlebar-hit-region
                onClick={() => postNative({ type: "exitFocusModeFromTitlebar" })}
                size="sm"
                type="button"
                variant="outline"
              >
                Exit focus
              </Button>
            ) : null}
            {/*
             * CDXC:ReactTitlebar 2026-05-30-03:11:
             * Top-right titlebar menus are right-click affordances. Keep left
             * click on primary icon actions, hide chevrons, and tell users about
             * right-click options through compact hover tooltips.
             *
             * CDXC:ReactTitlebar 2026-05-30-08:39:
             * Tips & Tricks should sit before Keep Awake in the top-right
             * titlebar control order, keeping the info/help affordance closer to
             * the mode switcher while power controls remain farther right.
             */}
            <ButtonGroup
              className="titlebar-open-group titlebar-tips-group"
              data-titlebar-dropdown-anchor
              data-titlebar-hit-region
            >
              <TitlebarAppTooltip content="Tips & Tricks">
                <Button
                  aria-label={
                    unreadTips.length + notices.length > 0
                      ? `Tips and tricks, ${unreadTips.length + notices.length} unread`
                      : "Tips and tricks"
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
            {!projectState.keepAwake.hideTitlebarControl ? (
              <ButtonGroup
                className="titlebar-open-group titlebar-keep-awake-group"
                data-titlebar-dropdown-anchor
                data-titlebar-hit-region
              >
                <TitlebarAppTooltip content="Click to toggle. Right-click for options.">
                  <Button
                    aria-label={keepAwakeRuntime ? "Allow Mac sleep" : "Keep Mac awake"}
                    className="titlebar-session-button titlebar-open-main-button"
                    data-active={String(Boolean(keepAwakeRuntime))}
                    data-state={nativeDropdownOpen === "keepAwake" ? "open" : undefined}
                    onClick={toggleKeepAwake}
                    onContextMenu={(event) => {
                      event.preventDefault();
                      showTitlebarDropdownPanel("keepAwake", event.currentTarget);
                    }}
                    type="button"
                    variant={keepAwakeRuntime ? "outline" : "ghost"}
                  >
                    {/*
                     * CDXC:TitlebarKeepAwake 2026-05-27-07:32:
                     * Keep-awake titlebar chrome must be icon-only so it cannot
                     * clip in the narrow right-side slot. Coffee means Ghostex is
                     * keeping the Mac awake; moon means clicking will allow sleep.
                     */}
                    {keepAwakeRuntime ? (
                      <IconCoffee aria-hidden="true" size={14} stroke={1.8} />
                    ) : (
                      <IconMoon aria-hidden="true" size={14} stroke={1.8} />
                    )}
                  </Button>
                </TitlebarAppTooltip>
              </ButtonGroup>
            ) : null}
            <ButtonGroup
              className="titlebar-open-group"
              data-titlebar-dropdown-anchor
              data-titlebar-hit-region
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
                 * control after moving the pet wake/sleep toggle into the
                 * sidebar overflow menu.
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
              data-titlebar-hit-region
            >
              <TitlebarAppTooltip content="Commit. Right-click for more actions">
                <Button
                  aria-disabled={gitPrimaryAction.disabled}
                  aria-label={gitPrimaryAction.disabledReason ?? gitPrimaryLabel}
                  className="titlebar-session-button titlebar-open-main-button titlebar-git-main-button"
                  data-disabled={String(gitPrimaryAction.disabled)}
                  data-state={nativeDropdownOpen === "git" ? "open" : undefined}
                  onClick={() => {
                    if (!gitPrimaryAction.disabled) {
                      runGitAction(gitPrimaryAction.action);
                    }
                  }}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    showTitlebarDropdownPanel("git", event.currentTarget);
                  }}
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
                   */
                  projectState.git.isBusy ? (
                    <IconLoader2 aria-hidden="true" className="titlebar-git-spinner" size={14} />
                  ) : (
                    getTitlebarGitActionIcon(gitPrimaryAction.action)
                  )}
                  {/*
                  <span
                    className="titlebar-git-label titlebar-git-label-full"
                    data-compact-below-620={String(shouldCompactGitPrimaryLabel)}
                  >
                    {gitPrimaryLabel}
                  </span>
                  {gitPrimaryCompactLabel ? (
                    <span aria-hidden="true" className="titlebar-git-label titlebar-git-label-compact">
                      {gitPrimaryCompactLabel}
                    </span>
                  ) : null}
                  */}
                </Button>
              </TitlebarAppTooltip>
            </ButtonGroup>
            <ButtonGroup
              className="titlebar-open-group titlebar-actions-group"
              data-titlebar-dropdown-anchor
              data-titlebar-hit-region
            >
              <TitlebarAppTooltip content="Click to run. Right-click for actions.">
                <Button
                  aria-disabled={!activeAction}
                  aria-label={
                    activeAction
                      ? `Run ${getSidebarActionLabel(activeAction)}`
                      : "No actions configured"
                  }
                  className="titlebar-session-button titlebar-open-main-button"
                  data-disabled={String(!activeAction)}
                  data-state={nativeDropdownOpen === "actions" ? "open" : undefined}
                  onClick={() => {
                    if (activeAction) {
                      runSidebarAction(activeAction);
                    }
                  }}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    showTitlebarDropdownPanel("actions", event.currentTarget);
                  }}
                  type="button"
                  variant="ghost"
                >
                  {getSidebarActionIcon(activeAction)}
                </Button>
              </TitlebarAppTooltip>
            </ButtonGroup>
            <ButtonGroup
              className="titlebar-open-group"
              data-titlebar-dropdown-anchor
              data-titlebar-hit-region
            >
              <TitlebarAppTooltip content="Click to open. Right-click for targets.">
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
  collapsedResourceKeys,
  daemon,
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
  onMarkAllTipsRead,
  onMarkTipRead,
  onOpenNoticeSettings,
  onOpenPowerSettings,
  onOpenTarget,
  onQuitResources,
  onRunAction,
  onRunGitAction,
  onSetResourceItemsCollapsed,
  onSleepInactiveSessions,
  onStartKeepAwake,
  onStopKeepAwake,
  onToggleResourceCollapse,
  orphanBundles,
  resourceProcessSnapshotReady,
  quittingResourceKeys,
  readTips,
  resourceGroupViews,
  selectedActionCommandId,
  sessionPersistenceProvider,
  visibleActions,
  visibleTargets,
  unreadTips,
}: {
  activeKeepAwakeDuration: KeepAwakeDurationMinutes | undefined;
  activeMode: TitlebarMode;
  activeTarget: ResolvedOpenTarget | undefined;
  browserBundles: ResourceProcessBundle[];
  collapsedResourceKeys: Set<string>;
  daemon: TitlebarGxserverDaemonStatus;
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
  onMarkAllTipsRead: () => void;
  onMarkTipRead: (tipId: string) => void;
  onOpenNoticeSettings: (target: TitlebarNotice["settingsTarget"]) => void;
  onOpenPowerSettings: () => void;
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
  onToggleResourceCollapse: (key: string) => void;
  orphanBundles: ResourceProcessBundle[];
  resourceProcessSnapshotReady: boolean;
  quittingResourceKeys: Set<string>;
  readTips: TitlebarTip[];
  resourceGroupViews: ResourceGroupView[];
  selectedActionCommandId: string | undefined;
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

  const closeAfter = (action: () => void) => {
    action();
    onClose();
  };

  return (
    <div
      className="dark titlebar-dropdown-panel-root"
      data-panel-kind={kind}
    >
      {kind === "mode" ? (
        <div className="titlebar-open-menu titlebar-mode-picker-menu min-w-[180px] rounded-none border-border/80 !bg-[#0e0e0e] p-1 text-[13px] text-foreground shadow-2xl">
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
        <div className="titlebar-open-menu titlebar-tips-menu rounded-none border-border/80 !bg-[#0e0e0e] p-0 text-[13px] text-foreground shadow-2xl">
          <TitlebarTipsMenu
            notices={notices}
            onMarkAllRead={onMarkAllTipsRead}
            onMarkRead={onMarkTipRead}
            onOpenNoticeSettings={(target) => closeAfter(() => onOpenNoticeSettings(target))}
            readTips={readTips}
            unreadTips={unreadTips}
          />
        </div>
      ) : null}
      {kind === "keepAwake" ? (
        <div className="titlebar-open-menu min-w-[220px] rounded-none border-border/80 !bg-[#0e0e0e] p-1 text-[13px] text-foreground shadow-2xl">
          {KEEP_AWAKE_DURATION_OPTIONS.map((option) => (
            <TitlebarPanelMenuItem
              key={option.value}
              onClick={() => {
                void onStartKeepAwake(option.value);
                onClose();
              }}
            >
              <IconCoffee aria-hidden="true" size={14} stroke={1.8} />
              <span className="min-w-0 flex-1 truncate">Keep awake {option.label.toLowerCase()}</span>
              {activeKeepAwakeDuration === option.value ? (
                <IconCheck aria-hidden="true" className="ml-2 size-4 opacity-75" />
              ) : null}
            </TitlebarPanelMenuItem>
          ))}
          {keepAwakeIsRunning ? (
            <TitlebarPanelMenuItem
              onClick={() => {
                void onStopKeepAwake();
                onClose();
              }}
            >
              <IconMoon aria-hidden="true" size={14} stroke={1.8} />
              <span>Allow sleep now</span>
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
        <div className="titlebar-open-menu titlebar-resources-menu rounded-none border-border/80 !bg-[#0e0e0e] p-0 text-[13px] text-foreground shadow-2xl">
          <TitlebarResourcesMenu
            browserBundles={browserBundles}
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
            quittingKeys={quittingResourceKeys}
            sessionPersistenceProvider={sessionPersistenceProvider}
          />
        </div>
      ) : null}
      {kind === "git" ? (
        <div className="titlebar-open-menu titlebar-git-menu rounded-none border-border/80 !bg-[#0e0e0e] p-1 text-[13px] text-foreground shadow-2xl">
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
        <div className="titlebar-open-menu min-w-[220px] rounded-none border-border/80 !bg-[#0e0e0e] p-1 text-[13px] text-foreground shadow-2xl">
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
      {kind === "openIn" ? (
        <div className="titlebar-open-menu min-w-[220px] rounded-none border-border/80 !bg-[#0e0e0e] p-1 text-[13px] text-foreground shadow-2xl">
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

function TitlebarPanelMenuSeparator() {
  return <div aria-hidden="true" className="bg-border/70 titlebar-panel-menu-separator" />;
}

function mergeTitlebarProjectState(
  current: TitlebarProjectState,
  state: Partial<TitlebarProjectState>,
): TitlebarProjectState {
  return {
    ...current,
    ...state,
    activeMode:
      state.activeMode === undefined
        ? current.activeMode
        : normalizeTitlebarMode(state.activeMode),
    agentHookStatus: state.agentHookStatus ?? current.agentHookStatus,
    ghostexCliStatus: state.ghostexCliStatus ?? current.ghostexCliStatus,
    debuggingMode: state.debuggingMode ?? current.debuggingMode,
    diffStats: state.diffStats ?? current.diffStats,
    git: state.git ?? current.git,
    gxserverDaemon: state.gxserverDaemon ?? current.gxserverDaemon,
    keepAwake: state.keepAwake ?? current.keepAwake,
    browserTabs: state.browserTabs ?? current.browserTabs,
    projectEditorCompanionPaneHidden:
      state.projectEditorCompanionPaneHidden ?? current.projectEditorCompanionPaneHidden,
    projectIsQuick: state.projectIsQuick ?? current.projectIsQuick,
    petOverlayEnabled: state.petOverlayEnabled ?? current.petOverlayEnabled,
    resourceGroups: state.resourceGroups ?? current.resourceGroups,
    sidebarActions: state.sidebarActions ?? current.sidebarActions,
    sessionPersistenceProvider:
      state.sessionPersistenceProvider ?? current.sessionPersistenceProvider,
    toggleSidebarHotkeyLabel:
      state.toggleSidebarHotkeyLabel ?? current.toggleSidebarHotkeyLabel,
    workspaceOpenTargets: state.workspaceOpenTargets ?? current.workspaceOpenTargets,
    isFocusModeActive: state.isFocusModeActive ?? current.isFocusModeActive,
    updateAvailable: state.updateAvailable ?? current.updateAvailable,
  };
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
    browserTabs: [],
    debuggingMode: settings.debuggingMode,
    diffStats: createDefaultSidebarProjectDiffStats(false),
    editorIsOpen: false,
    editorIsSleeping: false,
    editorStatus: "idle",
    git: createDefaultSidebarGitState(),
    gxserverDaemon: {
      alwaysStart: true,
      state: "unknown",
    },
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
    sidebarCollapsed: bootstrap.sidebarCollapsed === true,
    sidebarActions: {
      commands: [],
    },
    showProjectEditorDiffFileCount: settings.showProjectEditorDiffFileCount,
    sessionPersistenceProvider: settings.sessionPersistenceProvider,
    toggleSidebarHotkeyLabel: settings.hotkeys.toggleSidebarCollapsed
      ? formatSidebarHotkeyLabel(settings.hotkeys.toggleSidebarCollapsed)
      : "",
    workspaceOpenTargets: {
      availability: settings.workspaceOpenTargetAvailability,
      customTargets: settings.customWorkspaceOpenTargets,
      hiddenTargetIds: settings.workspaceOpenTargetHiddenIds,
    },
    updateAvailable: readInitialTitlebarUpdateAvailable(bootstrap),
  };
  /*
   * CDXC:ReactTitlebar 2026-06-11-18:06:
   * Native dropdown child windows need the latest titlebar project/resource
   * payload before first render. Swift injects that payload into the bootstrap
   * object at document start; merge it here so Resources does not briefly or
   * permanently render default state when the post-load bridge push races React.
   */
  return mergeTitlebarProjectState(initialState, bootstrap as Partial<TitlebarProjectState>);
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

function createTitlebarKeepAwakeSettings(
  settings: ReturnType<typeof normalizeghostexSettings>,
): TitlebarKeepAwakeSettings {
  return {
    activateOnExternalDisplay: settings.keepAwakeActivateOnExternalDisplay,
    activateOnLaunch: settings.keepAwakeActivateOnLaunch,
    allowDisplaySleep: settings.keepAwakeAllowDisplaySleep,
    batteryThresholdPercent: settings.keepAwakeBatteryThresholdPercent,
    deactivateBelowBatteryThreshold: settings.keepAwakeDeactivateBelowBatteryThreshold,
    deactivateOnLowPowerMode: settings.keepAwakeDeactivateOnLowPowerMode,
    deactivateOnUserSwitch: settings.keepAwakeDeactivateOnUserSwitch,
    defaultDurationMinutes: settings.keepAwakeDefaultDurationMinutes,
    hideTitlebarControl: settings.hideKeepAwakeTitlebarControl,
    preventLidSleep: settings.keepAwakePreventLidSleep,
  };
}

function readStoredKeepAwakeRuntime(): KeepAwakeRuntimeState | undefined {
  try {
    const parsed = JSON.parse(localStorage.getItem(KEEP_AWAKE_RUNTIME_STORAGE_KEY) || "null");
    if (!isRecord(parsed)) {
      return undefined;
    }
    const pid = typeof parsed.pid === "number" ? parsed.pid : Number.NaN;
    const durationMinutes = typeof parsed.durationMinutes === "number"
      ? parsed.durationMinutes
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
      fireAtMs: typeof parsed.fireAtMs === "number" ? parsed.fireAtMs : undefined,
      pid,
      startedAtMs: typeof parsed.startedAtMs === "number" ? parsed.startedAtMs : Date.now(),
    };
  } catch {
    return undefined;
  }
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
  onMarkAllRead,
  onMarkRead,
  onOpenNoticeSettings,
  readTips,
  unreadTips,
}: {
  notices: TitlebarNotice[];
  onMarkAllRead: () => void;
  onMarkRead: (tipId: string) => void;
  onOpenNoticeSettings: (target: TitlebarNotice["settingsTarget"]) => void;
  readTips: TitlebarTip[];
  unreadTips: TitlebarTip[];
}) {
  const unreadTotal = notices.length + unreadTips.length;
  return (
    <div className="titlebar-tips-panel" onClick={(event) => event.stopPropagation()}>
      <div className="titlebar-tips-header">
        <div className="titlebar-tips-title">
          <IconInfoCircle aria-hidden="true" size={18} stroke={1.8} />
          <span>Tips & Tricks</span>
        </div>
        <div className="titlebar-tips-actions">
          <button
            aria-label="Mark all tips as read"
            className="titlebar-tips-action-button"
            disabled={unreadTips.length === 0}
            onClick={onMarkAllRead}
            type="button"
          >
            <IconCheck aria-hidden="true" size={14} stroke={1.9} />
            <span>Read all</span>
          </button>
          <span className="titlebar-tips-summary">{unreadTotal} unread</span>
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
                onOpenSettings={() => onOpenNoticeSettings(notice.settingsTarget)}
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
  read,
  tip,
}: {
  onMarkRead: (tipId: string) => void;
  read: boolean;
  tip: TitlebarTip;
}) {
  return (
    <article className="titlebar-tip-row" data-read={String(read)}>
      <div className="titlebar-tip-icon">{getTitlebarTipIcon(tip.icon)}</div>
      <div className="titlebar-tip-copy">
        <div className="titlebar-tip-title">{tip.title}</div>
        <div className="titlebar-tip-body">{tip.body}</div>
      </div>
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
  onSleepInactiveSessions,
  onToggle,
  orphanBundles,
  quittingKeys,
  sessionPersistenceProvider,
}: {
  browserBundles: ResourceProcessBundle[];
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
  onSleepInactiveSessions: () => void;
  onToggle: (key: string) => void;
  orphanBundles: ResourceProcessBundle[];
  quittingKeys: Set<string>;
  sessionPersistenceProvider?: Exclude<SessionPersistenceProvider, "off">;
}) {
  const visibleGroupViews = processSnapshotReady
    ? groupViews.filter((view) => view.bundles.length > 0)
    : [];
  const allBundles = processSnapshotReady
    ? [
        ...visibleGroupViews.flatMap((view) => view.bundles),
        ...browserBundles,
        ...orphanBundles,
      ]
    : [];
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
   * sections, visible totals, and bulk resource actions.
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
  const liveCpuLabel = processSnapshotReady ? formatWholePercent(sumBundleCpu(allBundles)) : "--";
  const liveMemoryLabel = processSnapshotReady ? formatWholeMemory(sumBundleMemory(allBundles)) : "--";
  const resourceItemCollapseTargets = createResourceItemCollapseTargets(allBundles);
  const allResourceItemsCollapsed =
    resourceItemCollapseTargets.length > 0 &&
    resourceItemCollapseTargets.every((target) => isResourceItemCollapsed(target, collapsedKeys));
  const resourceItemToggleLabel = allResourceItemsCollapsed
    ? "Expand all resource items"
    : "Collapse all resource items";
  return (
    <div className="titlebar-resources-panel">
      <div className="titlebar-resources-header">
        <div className="titlebar-resources-title">
          <IconDeviceDesktop aria-hidden="true" size={18} />
          <span>Resources</span>
        </div>
        <div className="titlebar-resources-actions">
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
              content={
                <>
                  <span className="titlebar-resource-tooltip-title">Live CPU</span>
                  <span>CPU used by resources in this dropdown.</span>
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
              content={
                <>
                  <span className="titlebar-resource-tooltip-title">Live memory</span>
                  <span>RAM used by resources in this dropdown.</span>
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
            <div className="titlebar-resources-info-note">
              This app uses native Ghostty terminals as they're lighter on CPU & RAM than electron/web terminals.<br />
              The RAM use you see here is the lowest possible for the Agent CLI that you're using<br />
              Keep in mind that each CLI uses more/less RAM based on a lot of factors.<br />
              You can easily sleep all inactive terminals here (Auto-sleep can be configured in settings).
            </div>
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
        <AppTooltip content="Restart daemon" contentClassName="titlebar-resource-tooltip">
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
          <AppTooltip content="Reload app" contentClassName="titlebar-resource-tooltip">
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
  title,
}: {
  bundles: ResourceProcessBundle[];
  collapsedKeys: Set<string>;
  onQuit: (bundles: ResourceProcessBundle[]) => void;
  onFocusSession: (sessionId: string) => void;
  onToggle: (key: string) => void;
  quittingKeys: Set<string>;
  title: string;
}) {
  if (bundles.length === 0) {
    return null;
  }
  const sectionCpu = sumBundleCpu(bundles);
  const sectionMemory = sumBundleMemory(bundles);
  const sortedBundles = sortResourceBundlesForDisplay(bundles, quittingKeys);
  const hasTerminalSession = bundles.some(
    (bundle) => bundle.type === "session" && bundle.session?.sessionKind === "terminal",
  );
  const sectionActionBundles = hasTerminalSession
    ? bundles.filter((bundle) => bundle.type === "session" && bundle.session?.sessionKind === "terminal")
    : bundles;
  const sectionActionLabel = hasTerminalSession ? "Sleep Project" : "Quit";
  const sectionActionTooltipTitle = hasTerminalSession ? "Sleep project" : "Quit this group";
  const sectionActionTooltipBody = hasTerminalSession
    ? "Sleeps this project's terminal sessions and keeps them restorable in the sidebar."
    : "Stops live processes and closes related surfaces.";
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
              {formatWholeMemory(sectionMemory)}
            </span>
            <span className="titlebar-resource-section-count">{bundles.length}</span>
          </span>
        </div>
        <AppTooltip
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
            data-action={hasTerminalSession ? "sleep" : "quit"}
            onClick={() => onQuit(sectionActionBundles)}
            type="button"
          >
            {sectionActionLabel}
          </button>
        </AppTooltip>
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
}: {
  bundle: ResourceProcessBundle;
  collapsedKeys: Set<string>;
  isQuitting: boolean;
  onQuit: (bundles: ResourceProcessBundle[]) => void;
  onFocusSession: (sessionId: string) => void;
  onToggle: (key: string) => void;
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
  const focusSessionId = bundle.type === "session" ? resourceBundleSidebarSessionIds(bundle)[0] : undefined;
  const actionLabel = preservesSidebarSession ? `Sleep ${bundle.label}` : `Close ${bundle.label}`;
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
            <span className="titlebar-resource-name">{bundle.label}</span>
            <span className="titlebar-resource-meta">
              {isQuitting
                ? preservesSidebarSession
                  ? "Sleeping..."
                  : "Quitting..."
                : getResourceBundleMeta(bundle)}
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
            {formatWholeMemory(bundle.memoryMb)}
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
        <button
          aria-label={actionLabel}
          className="titlebar-resource-kill-button"
          data-action={preservesSidebarSession ? "sleep" : "quit"}
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onQuit([ bundle ]);
          }}
          type="button"
        >
          {preservesSidebarSession ? (
            <IconMoon aria-hidden="true" size={13} stroke={1.9} />
          ) : (
            <IconX aria-hidden="true" size={13} stroke={2} />
          )}
        </button>
      </div>
      {hasChildren && !isCollapsed ? (
        <div className="titlebar-resource-children">
          {bundle.childProcesses.slice(0, 8).map((process) => (
            <div className="titlebar-resource-child-row" key={process.pid}>
              <span className="titlebar-resource-child-name">
                {getResourceChildProcessName(bundle, process)} pid {process.pid}
              </span>
              <span className="titlebar-resource-metric">
                <IconCpu aria-hidden="true" size={12} stroke={1.8} />
                {formatWholePercent(process.cpu)}
              </span>
              <span className="titlebar-resource-metric">
                <IconDeviceDesktop aria-hidden="true" size={12} stroke={1.8} />
                {formatWholeMemory(process.rssMb)}
              </span>
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
  if (bundle.session?.sessionKind === "terminal") {
    return <IconTerminal2 aria-hidden="true" size={15} stroke={1.9} />;
  }
  return <IconBox aria-hidden="true" size={15} stroke={1.9} />;
}

function isSidebarAgentIcon(candidate: unknown): candidate is SidebarAgentIcon {
  return typeof candidate === "string" && Object.prototype.hasOwnProperty.call(AGENT_LOGOS, candidate);
}

function getResourceBundleMeta(bundle: ResourceProcessBundle): string {
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

function normalizeTitlebarMode(candidate: unknown): TitlebarMode {
  /**
   * CDXC:ModeSwitcher 2026-05-15-18:20:
   * The top titlebar mode must mirror the workarea mode restored by the sidebar
   * at launch and after each mode transition. Treat the sidebar/native payload
   * as authoritative so a restored Code, Git, or Project pane cannot leave the
   * segmented control highlighted on Agents.
   *
   * CDXC:ModeSwitcher 2026-05-15-18:30:
   * User clicks still need optimistic local mode selection so the shared-layout
   * pill animates immediately while slow Code/Git/Project surfaces load. Clear
   * that optimistic value when sidebar state arrives so startup restore and
   * failed transitions remain synchronized with the real visible workarea.
   */
  return candidate === "code" || candidate === "git" || candidate === "tasks"
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
    case "tasks":
      return <IconChecklist aria-hidden="true" size={14} stroke={1.8} />;
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
      data-titlebar-hit-region
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
       * When app width is below 1050px, Agents/Code/Git/Project moves from
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
      data-titlebar-hit-region
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
        Agents/Source/Browser/Kanban group keeps its original centered titlebar
        geometry. Publish the toggle as its own hit region because it sits
        outside the switcher's flex-flow bounds.
      */}
      {showCompanionToggle ? (
        <TitlebarAppTooltip content={companionToggleLabel}>
          <button
            aria-label={companionToggleLabel}
            className="titlebar-mode-tab titlebar-companion-toggle-button"
            data-titlebar-hit-region
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
        return (
          <button
            aria-disabled={mode.disabled === true}
            aria-selected={isActive}
            aria-label={mode.disabledReason ?? mode.label}
            className="titlebar-mode-tab"
            data-active={String(isActive)}
            data-disabled={String(mode.disabled === true)}
            disabled={mode.disabled}
            key={mode.value}
            onClick={mode.onSelect}
            role="tab"
            style={{ transformStyle: "preserve-3d" }}
            type="button"
          >
            {isActive ? (
              <motion.div
                className="titlebar-mode-tab-active"
                layoutId="clickedbutton"
                transition={{ type: "spring", bounce: 0.3, duration: 0.6 }}
              />
            ) : null}
            <span className="titlebar-mode-tab-content">
              {getTitlebarModeIcon(mode.value)}
              <span className="titlebar-mode-label">{mode.label}</span>
              {mode.meta ? <span className="titlebar-mode-meta">{mode.meta}</span> : null}
            </span>
          </button>
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

function titlebarPrimaryGitActionLabel(label: string): string {
  return label.replace(/\bPush\b/g, "push").replace(/\bPR\b/g, "PR");
}

function compactTitlebarPrimaryGitActionLabel(label: string): string {
  /**
   * CDXC:TitlebarGit 2026-05-29-16:05:
   * Below 620px, the top-right Git primary button needs to remove the visible
   * Commit wording while preserving any following push or PR destination text.
   * Keep the full aria label on the button so the compact visual label does not
   * reduce screen-reader context.
   */
  return titlebarPrimaryGitActionLabel(label)
    .replace(/^Commit(?:\s*&\s*|,\s*)?/i, "")
    .trim();
}

function getTitlebarGitActionIcon(action: SidebarGitAction): ReactNode {
  if (action === "syncMain") {
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

function createTitlebarCommandConfigDraft(command: SidebarCommandButton): CommandConfigDraft {
  return {
    actionType: command.actionType,
    closeTerminalOnExit: command.closeTerminalOnExit,
    command: command.command ?? (command.actionType === "terminal" ? "" : undefined),
    commandId: command.commandId,
    icon: command.icon ?? DEFAULT_SIDEBAR_COMMAND_ICON,
    iconColor: command.iconColor ?? DEFAULT_SIDEBAR_COMMAND_ICON_COLOR,
    name: command.name,
    playCompletionSound: command.playCompletionSound,
    url: command.url ?? (command.actionType === "browser" ? DEFAULT_BROWSER_ACTION_URL : undefined),
  };
}

function getSidebarActionIcon(command: SidebarCommandButton | undefined): ReactNode {
  if (command?.icon) {
    return (
      <SidebarCommandIconGlyph
        className="quick-action-icon"
        color={command.iconColor}
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
    alignItems: "center",
    display: "flex",
    left: "50%",
    maxWidth: "min(440px, calc(100vw - 520px))",
    minWidth: 0,
    position: "absolute",
    top: TITLEBAR_CENTER_CONTROLS_TOP,
    transform: "translateX(-50%)",
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
     * Right-side titlebar controls should sit flush with the window edge. The
     * Open split button is the rightmost control, so do not reserve trailing
     * inset on the slot container.
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
    background: "#0e0e0e",
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
    --titlebar-button-border-color: #252525;
  }
  /*
   * CDXC:ReactTitlebar 2026-06-10-23:44:
   * Native AppKit reports whether the pointer is inside a measured titlebar
   * hit region. Keep the DOM interactive and only neutralize stale hover styling
   * below; AppKit already owns the real titlebar hit boundary.
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
     * CDXC:SidebarCollapse 2026-06-12-10:57:
     * The sidebar collapse affordance belongs beside the macOS traffic lights,
     * not in the sidebar content. Match the requested 14px gray traffic-light
     * footprint and keep a compact gap before the next titlebar affordance.
     *
     * CDXC:SidebarCollapse 2026-06-12-11:10:
     * Visual review moved the button 1px lower, increased the chevron to 10px,
     * and made the next titlebar affordance sit 9px to the right of this button.
     *
     * CDXC:SidebarCollapse 2026-06-12-11:36:
     * The first gray traffic-light styling disappeared on the dark titlebar
     * because the shadcn ghost button reset background/color. Keep the 14px
     * footprint but use light chrome and !important so collapse/expand stays visible.
     *
     * CDXC:SidebarCollapse 2026-06-12-20:09:
     * The traffic-light-side collapse button was required to finish at 14x14px,
     * but the gray fill must not have an outline around it.
     *
     * CDXC:SidebarCollapse 2026-06-12-21:03:
     * The visible dot should be 15x15px, but pointer hit testing should use a
     * 33x33px square that includes 9px of invisible space on every side.
     *
     * CDXC:SidebarCollapse 2026-06-13-10:53:
     * Keep the 33x33px hit target inside the 35px titlebar vertically. Only
     * offset the target left so the 15px dot stays in its traffic-light-side
     * visual slot while clicks still work across the expanded target.
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
    color: rgba(255,255,255,0.86) !important;
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
    color: rgba(255,255,255,0.96) !important;
    outline: none;
  }
  .titlebar-sidebar-collapse-button-visual {
    align-items: center;
    background: rgba(255,255,255,0.16);
    border-radius: 999px;
    display: inline-flex;
    height: 15px;
    justify-content: center;
    transform: translateY(2px);
    width: 15px;
  }
  .titlebar-sidebar-collapse-button:hover .titlebar-sidebar-collapse-button-visual,
  .titlebar-sidebar-collapse-button:focus-visible .titlebar-sidebar-collapse-button-visual {
    background: rgba(255,255,255,0.24);
  }
  .titlebar-sidebar-collapse-button svg {
    height: 10px;
    width: 10px;
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
  .titlebar-project-title {
    /**
     * CDXC:ReactTitlebar 2026-06-04-18:55:
     * The React titlebar project title in the macOS app should sit 2px lower
     * without changing the shared titlebar height or moving neighboring
     * controls. Use a visual transform so layout and hit-region math stay
     * anchored to the existing titlebar row.
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
  }
  @media (max-width: 1049px) {
    .titlebar-mode-switcher {
      /*
       * CDXC:ModeSwitcher 2026-05-28-10:38:
       * App widths below 1050px do not have enough horizontal room for the
       * centered Agents/Code/Git/Project switcher plus right-side titlebar
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
    border: 0;
    border-left: 1px solid var(--titlebar-button-border-color);
    border-radius: var(--titlebar-mode-tab-radius);
    color: rgba(255,255,255,0.68);
    cursor: default;
    display: inline-flex;
    font: 400 13.55px/${TITLEBAR_CONTROL_HEIGHT}px var(--titlebar-font-family);
    height: ${TITLEBAR_CONTROL_HEIGHT}px;
    max-height: ${TITLEBAR_CONTROL_HEIGHT}px;
    min-height: ${TITLEBAR_CONTROL_HEIGHT}px;
    justify-content: center;
    letter-spacing: 0;
    min-width: 70px;
    overflow: visible;
    padding: 0 14px;
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
     * left-border separator model as Agents/Source/Browser/Kanban, with Agents'
     * own left border providing the boundary to its right.
     *
     * CDXC:ProjectEditorCompanion 2026-06-12-04:02:
     * Anchor this control to the left edge of the centered mode switcher without
     * participating in flex layout, so the Agents/Source/Browser/Kanban button
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
    color: rgba(255,255,255,0.92);
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
    color: rgba(255,255,255,0.98);
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
    /**
     * CDXC:SessionFocusMode 2026-05-26-22:22:
     * The titlebar focus exit control should visually belong with Agents/Code/Git/Project.
     * Match the mode-tab height, font size, weight, and radius so focus mode does not introduce a separate button scale in the native titlebar.
     */
    appearance: none;
    -webkit-appearance: none;
    background: rgba(255,255,255,0.2) !important;
    border: 0 !important;
    border-left: 1px solid var(--titlebar-button-border-color) !important;
    border-radius: 0 !important;
    box-shadow: none !important;
    color: rgba(255,255,255,0.98) !important;
    cursor: default;
    display: inline-flex;
    font: 720 12px/${TITLEBAR_CONTROL_HEIGHT}px var(--titlebar-font-family) !important;
    height: ${TITLEBAR_CONTROL_HEIGHT}px !important;
    max-height: ${TITLEBAR_CONTROL_HEIGHT}px;
    min-height: ${TITLEBAR_CONTROL_HEIGHT}px;
    letter-spacing: 0;
    margin-top: 0;
    min-width: 0;
    padding: 0 14px !important;
    white-space: nowrap;
  }
  .titlebar-exit-focus-button:hover,
  .titlebar-exit-focus-button:focus-visible {
    background: rgba(255,255,255,0.24) !important;
    color: rgba(255,255,255,1) !important;
    outline: none;
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
     */
    align-items: center;
    background: #95d7f6;
    border: 1px solid #0e0e0e;
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
     * the primary Git action. Hide Exit Focus, Keep Awake, Tips, and Resources,
     * and remove visible Commit wording from the Git primary label while
     * keeping non-commit destination text such as push or PR when there is room.
     */
    .titlebar-exit-focus-button,
    .titlebar-keep-awake-group,
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
  .titlebar-open-menu {
    /**
     * CDXC:TitlebarMenus 2026-05-28-13:52:
     * Titlebar dropdown surfaces should match the unified #0e0e0e app-modal
     * background instead of using the older #181818 menu shell.
     */
    background: #0e0e0e !important;
    background-color: #0e0e0e !important;
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
    background: #0e0e0e;
    color: rgba(255,255,255,0.92);
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
  .titlebar-panel-menu-separator {
    height: 1px;
    margin: 4px 0;
  }
  /**
   * CDXC:TitlebarGit 2026-05-24-20:40:
   * The Git split menu opens from the chevron segment, but the menu must be wide enough to show Commit, Push, and Create PR labels. Pin the menu width instead of letting Radix size it from the narrow chevron trigger.
   *
   * CDXC:TitlebarGit 2026-05-25-10:16:
   * Release-oriented Git actions add longer dropdown labels such as Multicommit & Release, so the pinned menu width must fit them without clipping.
   */
  .titlebar-git-menu {
    max-width: 260px;
    min-width: 240px !important;
    overflow-x: visible;
    width: 240px !important;
  }
  .titlebar-open-menu-item {
    cursor: default !important;
    min-height: 30px;
    gap: 10px;
    border-radius: 0;
    font: 500 13px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
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
    background: #0e0e0e !important;
    background-color: #0e0e0e !important;
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
    align-items: center;
    border-bottom: 1px solid rgba(255,255,255,0.12);
    display: flex;
    gap: 12px;
    justify-content: space-between;
    padding: 11px 12px;
  }
  .titlebar-tips-title,
  .titlebar-tips-actions,
  .titlebar-tips-summary,
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
    gap: 10px;
    margin-left: auto;
  }
  .titlebar-tips-action-button {
    align-items: center;
    background: rgba(255,255,255,0.08);
    border: 1px solid rgba(255,255,255,0.12);
    border-radius: 0;
    color: rgba(255,255,255,0.78);
    display: inline-flex;
    gap: 6px;
    font: 750 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    height: 24px;
    justify-content: center;
    padding: 0 8px;
    white-space: nowrap;
  }
  .titlebar-tips-action-button:not(:disabled):hover {
    background: rgba(255,255,255,0.14);
    color: rgba(255,255,255,0.94);
  }
  .titlebar-tips-action-button:disabled {
    color: rgba(255,255,255,0.3);
    cursor: default;
  }
  .titlebar-tips-summary {
    color: rgba(255,255,255,0.62);
    font: 650 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    white-space: nowrap;
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
    grid-template-columns: 28px minmax(0, 1fr) 28px;
    min-height: 72px;
    overflow: hidden;
    padding: 9px 8px;
  }
  .titlebar-tip-row[data-read="true"] {
    opacity: 0.72;
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
  .titlebar-tip-copy {
    display: grid;
    gap: 7px;
    min-width: 0;
  }
  .titlebar-tip-title {
    color: rgba(255,255,255,0.94);
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
     * The Resources manager background must match #0e0e0e while adjacent
     * titlebar dropdowns keep the existing titlebar menu color.
     */
    background: #0e0e0e !important;
    background-color: #0e0e0e !important;
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
  .titlebar-resources-header {
    align-items: center;
    border-bottom: 1px solid rgba(255,255,255,0.12);
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 11px 12px;
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
    font: 750 14px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
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
    font: 750 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
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
    font: 750 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    height: 24px;
    justify-content: center;
    opacity: 0;
    padding: 0 8px;
    pointer-events: none;
    transition: opacity 120ms ease, background 120ms ease, color 120ms ease;
    white-space: nowrap;
  }
  .titlebar-resource-section-quit-button[data-action="sleep"] {
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
  .titlebar-resource-section-quit-button[data-action="sleep"]:hover {
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
    font: 650 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
  }
  .titlebar-resources-summary span {
    gap: 5px;
  }
  .titlebar-resources-scroll {
    display: grid;
    gap: 0;
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
    font: 650 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
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
     */
    background: rgba(255,255,255,0.06);
    border: 1px solid rgba(255,255,255,0.1);
    border-radius: 0;
    color: rgba(255,255,255,0.62);
    font: 600 12px/1.35 -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    margin-bottom: 8px;
    padding: 8px 10px;
  }
  .titlebar-gxserver-daemon {
    /*
     * CDXC:TitlebarResources 2026-05-31-03:56:
     * The Resources dropdown must expose gxserver daemon status, version, stop/restart controls, and a small Always start checkbox without changing the sidebar session restore list.
     *
     * CDXC:TitlebarResources 2026-06-12-11:30:
     * The gxserver status headline should show the live status message (for example "gxserver is running and uses the expected protocol.") beside the state dot instead of a generic "Daemon" label, with the state/version line directly underneath.
     */
    align-items: center;
    background: rgba(255,255,255,0.045);
    border: 1px solid rgba(255,255,255,0.1);
    color: rgba(255,255,255,0.72);
    display: grid;
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
    font: 650 11px/1.25 -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
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
    font-weight: 760;
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
    font: 650 10px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
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
    font: 750 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
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
     * Per-item Focus and Sleep/Close buttons are fixed visible columns after
     * CPU/RAM metrics. Do not overlay them on hover or hide metrics to reveal
     * actions; normal hover on the buttons is the only interaction treatment.
     *
     * CDXC:TitlebarResources 2026-06-13-02:07:
     * CPU and RAM should read as one centered usage cluster between the session
     * text and the right-side buttons. Keep the text and action tracks stable,
     * and center a fixed metrics track so the values do not drift into the
     * action area.
     */
    align-items: center;
    display: grid;
    gap: 8px;
    grid-template-columns: minmax(0, 1fr) minmax(184px, 220px) 24px 24px;
    min-height: 44px;
    overflow: hidden;
    padding: 7px 8px;
    position: relative;
  }
  .titlebar-resource-row[data-expandable="true"] {
    cursor: pointer;
  }
  .titlebar-resource-main {
    align-items: center;
    display: grid;
    gap: 8px;
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
    font: 750 11px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
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
    font: 700 13px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-resource-meta,
  .titlebar-resource-child-name {
    color: rgba(255,255,255,0.58);
    font: 500 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-resource-metrics {
    align-items: center;
    display: grid;
    gap: 8px;
    grid-template-columns: minmax(68px, 0.85fr) minmax(100px, 1fr);
    justify-self: center;
    max-width: 220px;
    min-width: 184px;
    width: 100%;
  }
  .titlebar-resource-metric {
    align-items: center;
    background: rgba(255,255,255,0.055);
    border: 1px solid rgba(255,255,255,0.105);
    box-sizing: border-box;
    color: rgba(255,255,255,0.88);
    display: inline-flex;
    font: 680 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
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
    grid-column: 3;
  }
  .titlebar-resource-focus-button:hover,
  .titlebar-resource-focus-button:focus-visible {
    background: rgba(255,255,255,0.2);
    color: rgba(255,255,255,0.96);
    outline: none;
  }
  .titlebar-resource-kill-button {
    background: rgb(220 38 38);
    color: white;
    grid-column: 4;
  }
  .titlebar-resource-kill-button[data-action="sleep"] {
    background: rgba(255,255,255,0.14);
    border-color: rgba(255,255,255,0.16);
    color: rgba(255,255,255,0.9);
  }
  .titlebar-resource-kill-button[data-action="sleep"]:hover,
  .titlebar-resource-kill-button[data-action="sleep"]:focus-visible {
    background: rgba(255,255,255,0.2);
    color: rgba(255,255,255,0.96);
    outline: none;
  }
  .titlebar-resource-kill-button[data-action="quit"]:hover,
  .titlebar-resource-kill-button[data-action="quit"]:focus-visible {
    background: rgb(185 28 28);
    border-color: rgba(248,113,113,0.45);
    outline: none;
  }
  .titlebar-resource-children {
    display: grid;
    padding: 0 8px 8px 64px;
  }
  .titlebar-resource-child-row {
    align-items: center;
    display: grid;
    gap: 10px;
    grid-template-columns: minmax(220px, 1fr) 86px 106px;
    min-height: 24px;
  }
  .titlebar-resources-empty {
    color: rgba(255,255,255,0.54);
    font: 500 12px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
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
if (titlebarRootElement?.dataset.ghostexTitlebar !== "false") {
  createRoot(titlebarRootElement!).render(<App />);
}
