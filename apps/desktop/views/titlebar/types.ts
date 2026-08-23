import type { SidebarProjectDiffStats } from "@/packages/shared/project-diff-stats";
import type { SidebarCommandButton } from "@/packages/shared/sidebar-commands";
import type {
  SidebarAgentHookStatusMessage,
  SidebarGhostexCliStatusMessage,
  SidebarPortlessState,
} from "@/packages/shared/session-grid-contract-sidebar";
import type { NativePortlessAdminInstallAction } from "@/packages/shared/native-ghostty-host-protocol";
import type { SidebarTheme } from "@/packages/shared/session-grid-contract";
import type {
  DiagnosticLoggingSettings,
  KeepAwakeDurationMinutes,
  SidebarSide,
  SessionPersistenceProvider,
  WebLinkOpenTarget,
} from "@/packages/shared/ghostex-settings";
import type { ghostexHotkeySettings } from "@/packages/shared/ghostex-hotkeys";
import type {
  CustomWorkspaceOpenTarget,
  WorkspaceIdeTargetApp,
  WorkspaceOpenTargetAvailability,
  WorkspaceOpenTargetDefinition,
} from "@/packages/shared/workspace-open-targets";
import type { SidebarGitAction, SidebarGitState } from "@/packages/shared/sidebar-git";

export type ProjectEditorLoadStatus = "idle" | "opening" | "running" | "error";
export type TitlebarMode = "agents" | "code" | "git" | "automate" | "tasks" | "manage";
export type TitlebarDropdownPanelKind = "resources" | "tips";
export type TitlebarDropdownPanelSize = {
  height: number;
  width: number;
};

export type NativeProcessResult = {
  exitCode: number;
  requestId: string;
  stderr: string;
  stdout: string;
  type: "processResult";
};

export type NativeHostEvent = NativeProcessResult | { protocolVersion: 1; type: "hostReady" };

export type TitlebarOpenTargetsSettings = {
  availability: WorkspaceOpenTargetAvailability;
  customTargets: CustomWorkspaceOpenTarget[];
  hiddenTargetIds: string[];
};

export type TitlebarSidebarActionsSettings = {
  commands: SidebarCommandButton[];
};

export type TitlebarKeepAwakeSettings = {
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
  sessionKind?: "browser" | "terminal";
  sessionPersistenceName?: string;
  sessionPersistenceProvider?: SessionPersistenceProvider;
  terminalTitle?: string;
  title: string;
};

export type TitlebarTipIcon =
  | "browser"
  | "command"
  | "moon"
  | "resources"
  | "search"
  | "sidebar"
  | "warning";

export type TitlebarTip = {
  action?: TitlebarTipAction;
  body: string;
  icon: TitlebarTipIcon;
  id: string;
  title: string;
};

export type TitlebarTipAction =
  | {
      settingsSearchQuery: string;
      type: "openSettings";
    }
  | {
      type: "openBrowserPane";
      url: string;
    };

export type TitlebarNotice = {
  action?: "openSettings";
  body: string;
  icon: TitlebarTipIcon;
  id: string;
  settingsTarget: "agentHooks" | "debuggingMode" | "ghostexCli" | "sessionPersistence";
  title: string;
};

export type TitlebarBrowserTabResource = {
  browserId: number;
  id: string;
  isActive?: boolean;
  kind: "browser" | "code" | "git" | "tasks" | "manage" | string;
  projectId?: string;
  sessionId?: string;
  title: string;
  url?: string;
};

export type TitlebarGxserverDaemonStatus = {
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

export type TitlebarProjectState = {
  activeMode: TitlebarMode;
  browserTabs: TitlebarBrowserTabResource[];
  codeEditorProjectIds: string[];
  agentHookStatus?: SidebarAgentHookStatusMessage;
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  portless?: SidebarPortlessState;
  debuggingMode: boolean;
  diagnosticLogging: DiagnosticLoggingSettings;
  showBetaFeatures: boolean;
  codeViewTabHidden: boolean;
  browserViewTabHidden: boolean;
  kanbanViewTabHidden: boolean;
  automateViewTabHidden: boolean;
  docsViewTabHidden: boolean;
  tipsAndTricksTitlebarButtonHidden: boolean;
  resourcesTitlebarButtonHidden: boolean;
  gitActionsTitlebarButtonHidden: boolean;
  quickActionsTitlebarButtonHidden: boolean;
  openInTitlebarButtonHidden: boolean;
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
  webLinkOpenTarget: WebLinkOpenTarget;
  toggleSidebarHotkeyLabel: string;
  workspaceOpenTargets: TitlebarOpenTargetsSettings;
  isFocusModeActive?: boolean;
  promptEditorOpen?: boolean;
  updateAvailable: boolean;
  updateDownloadProgress: number | null;
  updateDownloading: boolean;
};

export type ResourceProcess = {
  command: string;
  cpu: number;
  pid: number;
  ppid: number;
  rssMb: number;
};

export type ResourceListeningServer = {
  commandName: string;
  cwd?: string;
  host: string;
  pid: number;
  port: number;
  url: string;
};

export type ResourcePortlessServerPresentation = {
  hostname: string;
  isSetupActive: boolean;
  protocol: SidebarPortlessState["health"]["protocol"];
  setupAction?: NativePortlessAdminInstallAction;
  setupActionLabel: string;
  setupStatusLabel: string;
};

export type ResourceProcessBundle = {
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

export type ResourceProcessTotals = {
  cpu: number;
  memoryMb: number;
  processCount: number;
};

export type ResourceGroupView = {
  bundles: ResourceProcessBundle[];
  group: TitlebarResourceGroup;
};

export type NativeTitlebarCommand =
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

export type ResolvedOpenTarget =
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

/*
CDXC:TitlebarHostBootstrap 2026-08-20-12:40:
The host bootstrap object used to be declared by the removed macOS
`native-sidebar.tsx`. GPUI injects only `codeServerRuntime.port` into it (see
`render_titlebar`'s bootstrap script in apps/desktop/src/app/render/mode_switcher_and_titlebar.rs), so this host owns the
declaration for the fields it reads. Keep it a type alias rather than an
interface: `App` forwards the bootstrap to `createInitialProjectState`, which
takes `Record<string, unknown>`.
*/
export type TitlebarNativeBootstrap = {
  bundleIdentifier?: string;
  codeServerRuntime?: {
    host?: string;
    origin?: string;
    ownerId?: string;
    port?: number;
    storageName?: string;
  };
  cwd?: string;
  ghostexHomeDir?: string;
  homeDir?: string;
  sharedSidebarStorage?: {
    settings?: string;
  };
  updateAvailable?: boolean;
  workspaceName?: string;
};

declare global {
  interface Window {
    __ghostex_NATIVE_HOST__?: TitlebarNativeBootstrap;
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

export type KeepAwakeRuntimeState = {
  durationMinutes: KeepAwakeDurationMinutes;
  fireAtMs?: number;
  pid: number;
  source: "automatic" | "manual";
  startedAtMs: number;
};

export type KeepAwakeRuntimeSyncState = {
  runtime?: KeepAwakeRuntimeState | null;
  suppressAutoStart: boolean;
};

export type TitlebarKeepAwakeCommand =
  | { action: "start"; durationMinutes?: KeepAwakeDurationMinutes }
  | { action: "stop" };

export type ResourceItemCollapseTarget = {
  collapsedWhenKeyPresent: boolean;
  key: string;
};

export type TitlebarRgbColor = {
  blue: number;
  green: number;
  red: number;
};
