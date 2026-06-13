import type { CompletionSoundSetting } from "./completion-sound";
import type { AgentAcceptAllMode } from "./sidebar-agent-accept-all";
import type { SidebarAgentButton, SidebarAgentIcon } from "./sidebar-agents";
import type { SidebarCommandIcon } from "./sidebar-command-icons";
import type { WorkspaceProjectIcon } from "./workspace-project-appearance";
import type {
  SidebarActionType,
  SidebarCommandButton,
  SidebarCommandRunMode,
} from "./sidebar-commands";
import type { SidebarGitAction, SidebarGitChangedFile, SidebarGitState } from "./sidebar-git";
import type { SidebarProjectDiffStats } from "./project-diff-stats";
import type { ghostexSettings } from "./ghostex-settings";
import type { ghostexHotkeyActionId } from "./ghostex-hotkeys";
import type { WorkspaceIdeTargetApp } from "./workspace-open-targets";
import type { SidebarPinnedPrompt } from "./sidebar-pinned-prompts";
import type { SidebarSessionTag } from "./session-tags";
import type {
  SessionLifecycleState,
  SessionGridSnapshot,
  SessionRecord,
  SidebarTheme,
  TerminalSessionPersistenceProvider,
  TerminalViewMode,
  VisibleSessionCount,
} from "./session-grid-contract-core";

export type SidebarActiveSessionsSortMode = "manual" | "lastActivity";

export type SidebarTitleObservationState = {
  failureCount?: number;
  lastFailedAt?: string;
  lastObservedAt?: string;
  lastStartedAt?: string;
  nextRetryAt?: string;
  status: "active" | "failed" | "retrying" | "starting";
};

export type AgentsHubTab = "mds" | "skills" | "hooks" | "configs";

export type AgentsHubProfile = {
  agentIcon: SidebarAgentIcon;
  filePath: string;
  label: string;
  profilePath: string;
  targetPath?: string;
};

export type AgentsHubFile = {
  content?: string;
  id: string;
  language: string;
  name: string;
  path: string;
};

export type AgentsHubGroup = {
  description: string;
  files: AgentsHubFile[];
  id: string;
  name: string;
  path: string;
  profiles: AgentsHubProfile[];
};

export type AgentsHubCatalogMessage = {
  generatedAt: string;
  groupsByTab: Record<AgentsHubTab, AgentsHubGroup[]>;
  type: "agentsHubCatalog";
};

export type AgentsHubFileContentMessage = {
  content?: string;
  errorMessage?: string;
  filePath: string;
  requestId: string;
  type: "agentsHubFileContent";
};

export type SidebarAgentHookStatus = "installed" | "missing" | "cliMissing" | "notRequired" | "updateRequired";

export type SidebarAgentHookStatusItem = {
  agentId: string;
  cliCommand: string;
  cliInstalled: boolean;
  detail: string;
  hookInstalled: boolean;
  paths: string[];
  status: SidebarAgentHookStatus;
};

/**
 * CDXC:AgentHookSettings 2026-05-23-10:05:
 * Settings -> Agents shows machine-local hook setup status for the same reliable-resume agents Ghostex installs at startup. Native owns filesystem inspection and returns only normalized status rows so the modal host can render the result without direct filesystem access.
 */
export type SidebarAgentHookStatusMessage = {
  agents: SidebarAgentHookStatusItem[];
  errorMessage?: string;
  generatedAt: string;
  hookStateDirectory: string;
  notifyHookPath: string;
  type: "agentHookStatus";
};

export type SidebarGhostexCliStatusMessage = {
  /**
   * CDXC:BrowserAgentControl 2026-05-26-22:17:
   * First-launch CLI setup treats the Ghostex Browser Use skill as part of the
   * installed CLI experience because agents need both the executable and the
   * skill instructions before they can inspect embedded CEF logs and pages.
   *
   * CDXC:IntegrationsSetup 2026-05-27-04:17:
   * Settings -> Integrations and the first-launch flow need one native-owned
   * status payload for CLI, Ghostex Browser Use, and Ghostex Computer Use. Native owns
   * PATH and app-bundle checks so React can warn without guessing from UI state.
   *
   * CDXC:ComputerAgentControl 2026-05-27-06:58:
   * Desktop Control readiness includes the `$ghostex-computer-use` wrapper
   * skill, because Cua Driver alone does not teach agents the Ghostex-named
   * computer-use workflow.
   *
   * CDXC:AgentSkills 2026-05-31-09:18:
   * First launch and Settings must show each bundled Ghostex skill as an
   * explicit install item. Carry per-skill status for Browser Use, Computer Use,
   * Agent Orchestration, and Generate Title instead of only exposing the skills
   * that also have standalone guide pages.
   *
   * CDXC:CuaPermissions 2026-05-29-06:00:
   * The Cua Permissions row must report Cua Driver's own macOS privacy grants,
   * not Ghostex's Accessibility grant. Carry both Accessibility and Screen
   * Recording from `cua-driver check_permissions` in the setup status payload.
   *
   * CDXC:T3CodePackaging 2026-06-06-05:50:
   * Settings -> Integrations must expose whether T3 Code is actually bundled in
   * this app build, because T3 panes are a core visible feature and missing
   * packaged runtime state should be repairable before users hit a web-pane
   * network-style startup error.
   */
  browserSkillInstalled: boolean;
  browserSkillPath?: string;
  computerUseSkillInstalled: boolean;
  computerUseSkillPath?: string;
  agentOrchestrationSkillInstalled: boolean;
  agentOrchestrationSkillPath?: string;
  generateTitleSkillInstalled: boolean;
  generateTitleSkillPath?: string;
  cuaDriverAccessibilityPermissionGranted?: boolean;
  cuaAppInstalled: boolean;
  cuaDriverInstalled: boolean;
  cuaDriverPermissionDetail?: string;
  cuaDriverPath?: string;
  cuaDriverScreenRecordingPermissionGranted?: boolean;
  detail: string;
  generatedAt: string;
  ghostexPath?: string;
  gxBlockedByExistingCommand: boolean;
  gxPath?: string;
  gxUsable: boolean;
  installed: boolean;
  t3RuntimeDetail?: string;
  t3RuntimeInstalled?: boolean;
  t3RuntimeSource?: "bundled" | "development" | "missing" | "unavailable";
  type: "ghostexCliStatus";
};

export type SidebarOSIntegrationStatusMessage = {
  /**
   * CDXC:OSIntegration 2026-05-27-18:06:
   * Settings -> OS Integration shows native Launch Services diagnostics so the
   * user can tell whether Ghostex is merely available in Open With or is the
   * current default for editor, terminal-link, and script-runner roles.
   */
  bundleIdentifier: string;
  editorDefaults: Record<string, string>;
  generatedAt: string;
  registeredEditableFiles: boolean;
  registeredGhostexURLScheme: boolean;
  registeredScriptRunner: boolean;
  scriptDefaults: Record<string, string>;
  terminalLinkDefaultBundleId?: string;
  type: "osIntegrationStatus";
};

export type SidebarSessionItem = {
  kind?: "browser" | "workspace";
  sessionKind?: "browser" | "terminal" | "t3";
  activity: "idle" | "working" | "attention";
  activityLabel?: string;
  agentIcon?: SidebarAgentIcon;
  /**
   * CDXC:SessionRestore 2026-05-22-23:59:
   * Agent CLI hook installs capture the stable provider session id separately from Ghostex's visible session id. Sidebar cards carry that value so hover tooltips can show the exact resume target while title-based restore remains a backup.
   */
  agentSessionId?: string;
  faviconDataUrl?: string;
  firstUserMessage?: string;
  isGeneratingFirstPromptTitle?: boolean;
  isReloading?: boolean;
  lifecycleState?: SessionLifecycleState;
  isFavorite?: boolean;
  /**
   * CDXC:SessionTags 2026-06-05-12:30:
   * Sidebar rows carry the expanded tag marker separately from legacy
   * `isFavorite`. Renderers use this for the leading icon, tag filters, and
   * tooltip prefix while older Favorite-only rows still project as Favorite.
   */
  sessionTag?: SidebarSessionTag;
  /**
   * CDXC:PinnedSessions 2026-05-28-12:04:
   * Sidebar rows carry project-local pin state so the React display sorter can
   * keep pinned sessions at the top of their project and render pin chrome
   * without overloading Favorite.
   */
  isPinned?: boolean;
  lastInteractionAt?: string;
  sessionId: string;
  /**
   * CDXC:SessionTooltips 2026-05-31-06:25:
   * macOS gxserver sessions need their full routed identity in hover tooltips
   * instead of the legacy two-digit display number, because the short display
   * number does not identify the server/project/session being restored.
   */
  sessionRoutingId?: string;
  sessionNumber?: string;
  sessionPersistenceName?: string;
  sessionPersistenceProvider?: TerminalSessionPersistenceProvider;
  /**
   * CDXC:GxserverSessionTitles 2026-06-07-09:33:
   * gxserver-owned rows carry the final visible title string. Sidebar clients render this directly so platform adapters do not duplicate terminal-title trust, placeholder, or unsynced-marker rules.
   */
  displayTitle?: string;
  displayTitleTooltip?: string;
  primaryTitle?: string;
  isPrimaryTitleTerminalTitle?: boolean;
  terminalTitle?: string;
  /**
   * CDXC:SessionStatus 2026-06-07-00:30:
   * Sidebar Auto Sleep must not interpret an idle activity value as reliable while gxserver's zmx title observer is starting or retrying. Carry only coarse observer health so the UI can defer sleep decisions without exposing terminal titles or user-owned terminal content.
   */
  titleObservation?: SidebarTitleObservationState;
  alias: string;
  shortcutLabel: string;
  row: number;
  column: number;
  isFocused: boolean;
  /**
   * CDXC:SessionLifecycle 2026-05-29-09:20:
   * Session lifecycle uses resource-specific state names so UI and batch
   * actions do not infer provider session existence from the legacy `isSleeping` and
   * `isRunning` booleans. A native pane can be unmounted while a zmx/tmux/zellij
   * provider session still exists, so both resource states are carried
   * explicitly and `isLive` is derived from them.
   *
   * CDXC:SessionLifecycle 2026-05-29-06:29:
   * Persistence-disabled terminal sessions must report `providerSessionState:
   * "persistence-disabled"` instead of `unknown`. Unknown is reserved for configured
   * providers whose existence check has not completed or failed.
   *
   * CDXC:SessionLifecycle 2026-05-29-07:19:
   * Name the providerless state `persistence-disabled` so payloads make it
   * clear the terminal provider is absent because persistence is off, not
   * because some unrelated disabled flag was set.
   */
  nativePaneState?: "mounted" | "mounting" | "unmounted";
  providerSessionState?: "exists" | "missing" | "persistence-disabled" | "unknown";
  isLive?: boolean;
  /** @deprecated Use nativePaneState/providerSessionState plus isLive. */
  isSleeping?: boolean;
  isVisible: boolean;
  /** @deprecated Use isLive for runtime liveness and activity for work state. */
  isRunning: boolean;
  detail?: string;
  /**
   * CDXC:DelayedSend 2026-05-17-03:14
   * Delayed Send timers must be visible before they fire. Carry both the
   * absolute deadline and the display countdown so sidebar cards, titlebar
   * resources, and tooltips can show the same remaining time.
   */
  delayedSendDeadlineAt?: string;
  delayedSendRemainingLabel?: string;
  delayedSendRemainingMs?: number;
  /**
   * CDXC:PanePopOut 2026-05-19-10:15:
   * Sidebar session context menus need the live pop-out presentation flag so
   * browser and agent cards can offer Pop Out Pane versus Restore Pane without
   * re-querying native chrome state.
   */
  isPoppedOut?: boolean;
};

export function getSidebarSessionLifecycleState(
  session: Pick<
    SidebarSessionItem,
    "isLive" | "isRunning" | "isSleeping" | "lifecycleState" | "nativePaneState" | "providerSessionState"
  >,
): SessionLifecycleState {
  if (session.lifecycleState) {
    return session.lifecycleState;
  }

  if (session.isLive === true) {
    return "running";
  }

  if (session.nativePaneState === "mounted" || session.providerSessionState === "exists") {
    return "running";
  }

  if (session.isSleeping) {
    return "sleeping";
  }

  return session.isRunning ? "running" : "done";
}

export type SidebarPreviousSessionItem = SidebarSessionItem & {
  closedAt: string;
  groupId?: string;
  historyId: string;
  isGeneratedName: boolean;
  isRestorable: boolean;
  /**
   * CDXC:PreviousSessions 2026-05-05-05:30
   * Restoring from Previous Sessions must recreate the archived agent session,
   * not only its card title. Store the normalized session record and source
   * project/group metadata so native restore can preserve agent identity,
   * first-message metadata, title provenance, and resumable session details.
   */
  projectId?: string;
  projectName?: string;
  projectPath?: string;
  sessionRecord?: SessionRecord;
  sidebarOrder?: number;
};

export type SidebarSessionGroup = {
  kind?: "browser" | "workspace";
  groupId: string;
  isActive: boolean;
  /**
   * CDXC:Chats 2026-05-04-09:41
   * Native Combined mode renders all chat folders under one synthetic Chats
   * header. Mark it explicitly so the React sidebar can keep it non-draggable
   * and route its add button to creating a new chat folder.
   */
  isChatCollection?: boolean;
  /**
   * CDXC:SessionFocusMode 2026-05-28-12:52:
   * Focus is a split-pane zoom, not a tab selector. Sidebar groups must carry actual pane topology so session context menus can hide Focus when a project has only one pane, even if that pane has multiple tabs.
   *
   * CDXC:SessionFocusMode 2026-05-28-15:35:
   * The topology signal must reflect awake rendered pane owners, not only persisted paneLayout children, so sleeping-only split panes do not leave Focus visible while the user sees one native pane.
   */
  canFocusMode?: boolean;
  isFocusModeActive: boolean;
  layoutVisibleCount: VisibleSessionCount;
  projectContext?: {
    canRemoveProject: boolean;
    path: string;
    /**
     * CDXC:EditorPanes 2026-05-06-14:21
     * Combined project cards expose one project-owned code editor surface.
     * The editor is not a split session, so sidebar state carries it through
     * project context instead of mixing it into session card records.
     */
    editor: {
      diffStats: SidebarProjectDiffStats;
      /**
       * CDXC:EditorPanes 2026-05-09-17:24
       * Project editor rows represent attempted/running editor surfaces, not
       * only focused panes. Carry load status so the sidebar can keep the row
       * visible through startup failures and show timeout diagnostics.
       */
      errorMessage?: string;
      isOpen: boolean;
      isSleeping: boolean;
      projectId: string;
      status: "idle" | "opening" | "running" | "error";
    };
    theme?: SidebarTheme;
    themeColor?: string;
    worktree?: SidebarProjectWorktreeMetadata;
  };
  remoteMachineContext?: {
    machineId: string;
    machineName: string;
  };
  sessions: SidebarSessionItem[];
  title: string;
  viewMode: TerminalViewMode;
  visibleCount: VisibleSessionCount;
};

export type SidebarProjectWorktreeMetadata = {
  branch: string;
  createdAt?: string;
  name: string;
  parentProjectId: string;
  parentProjectName: string;
  parentProjectPath: string;
};

export type SidebarProjectWorktree = {
  branch?: string;
  directory: string;
  name: string;
};

export type SidebarProjectSettingsItem = {
  beadsDisplayKey?: string;
  name: string;
  path: string;
  projectId: string;
  worktreeCommand?: string;
};

export type SidebarRecentProject = {
  icon?: WorkspaceProjectIcon;
  iconDataUrl?: string;
  path: string;
  projectId: string;
  recentClosedAt?: string;
  sessionCount: number;
  theme?: SidebarTheme;
  themeColor?: string;
  title: string;
};

export type SidebarCommandSessionIndicator = {
  commandId: string;
  isActive?: boolean;
  sessionId: string;
  status: "idle" | "running" | "error";
  title?: string;
};

export type SidebarHudState = {
  activeSessionsSortMode: SidebarActiveSessionsSortMode;
  /**
   * CDXC:AgentHooks 2026-06-07-08:51:
   * Tips & Tricks and Settings consume gxserver-owned hook status from shared HUD state so every client can warn about unreliable agent statuses without probing local hook files or owning installer logic.
   */
  agentHookStatus?: SidebarAgentHookStatusMessage;
  agentManagerZoomPercent: number;
  agents: SidebarAgentButton[];
  buildStamp?: string;
  commands: SidebarCommandButton[];
  commandSessionIndicators: SidebarCommandSessionIndicator[];
  completionBellEnabled: boolean;
  completionSound: CompletionSoundSetting;
  completionSoundLabel: string;
  /**
   * CDXC:WorkspaceTheme 2026-05-05-02:58
   * The active workspace can override the preset sidebar theme with a custom
   * validated color. Keep the preset `theme` as the fallback, and send this
   * color separately so CSS can derive app-level theme variables.
   */
  customThemeColor?: string;
  debuggingMode: boolean;
  focusedSessionTitle?: string;
  git: SidebarGitState;
  isFocusModeActive: boolean;
  pendingAgentIds: string[];
  /**
   * CDXC:Worktrees 2026-05-18-23:07:
   * The Worktrees settings surface needs the same project id/name/path projection as native workspace storage, plus an optional per-project command override for creating worktrees.
   */
  projectSettingsProjects?: SidebarProjectSettingsItem[];
  /**
   * CDXC:RecentProjects 2026-05-04-14:25
   * Combined sidebar hides projects without active/sleeping sessions in a
   * bottom Recent Projects drawer. The drawer receives a compact, sorted
   * projection so React can restore projects without owning native session
   * storage.
   */
  recentProjects: SidebarRecentProject[];
  projectWorktrees?: SidebarProjectWorktree[];
  settings?: ghostexSettings;
  createSessionOnSidebarDoubleClick: boolean;
  renameSessionOnDoubleClick: boolean;
  showCloseButtonOnSessionCards: boolean;
  theme:
    | "plain-dark"
    | "plain-light"
    | "dark-green"
    | "dark-blue"
    | "dark-red"
    | "dark-pink"
    | "dark-orange"
    | "light-blue"
    | "light-green"
    | "light-pink"
    | "light-orange";
  highlightedVisibleCount: VisibleSessionCount;
  visibleCount: VisibleSessionCount;
  visibleSlotLabels: string[];
  viewMode: TerminalViewMode;
};

export type SidebarHydrateMessage = {
  groups: SidebarSessionGroup[];
  pinnedPrompts: SidebarPinnedPrompt[];
  previousSessions: SidebarPreviousSessionItem[];
  revision: number;
  scratchPadContent: string;
  type: "hydrate";
  hud: SidebarHudState;
};

export type SidebarSessionStateMessage = {
  groups: SidebarSessionGroup[];
  pinnedPrompts: SidebarPinnedPrompt[];
  previousSessions: SidebarPreviousSessionItem[];
  revision: number;
  scratchPadContent: string;
  type: "sessionState";
  hud: SidebarHudState;
};

export type SidebarSessionPresentationChangedMessage = {
  revision?: number;
  session: SidebarSessionItem;
  type: "sessionPresentationChanged";
};

export type SidebarGroupsChangedMessage = {
  groupOrder: string[];
  groups: SidebarSessionGroup[];
  removedGroupIds?: string[];
  removedSessionIds?: string[];
  revision: number;
  /*
  CDXC:SidebarHydration 2026-06-09-23:01:
  Routine gxserver presentation changes must patch the React sidebar tree instead of posting a full hydrate. Carry changed groups, removals, and authoritative order so session add/remove/reorder/project deltas update visible rows without replacing unrelated sidebar state or letting WKWebView refreshes steal terminal focus.
  */
  type: "sidebarGroupsChanged";
};

export type SidebarHudChangedMessage = {
  hud: SidebarHudState;
  revision: number;
  /*
  CDXC:SidebarHydration 2026-06-09-23:01:
  Live presentation patches still need HUD-derived chrome such as focused title, counts, and command indicators. Send HUD as its own patch so gxserver deltas do not force a session-tree hydrate just to keep non-row controls current.
  */
  type: "sidebarHudChanged";
};

export type SidebarPlayCompletionSoundMessage = {
  sound: CompletionSoundSetting;
  sessionId?: string;
  type: "playCompletionSound";
};

export type SidebarOrderSyncKind = "agent" | "command";

export type SidebarOrderSyncResultMessage = {
  itemIds: string[];
  kind: SidebarOrderSyncKind;
  requestId: string;
  status: "error" | "success";
  type: "sidebarOrderSyncResult";
};

export type SidebarCommandRunState = "error" | "running" | "success";

export type SidebarCommandRunStateChangedMessage = {
  commandId: string;
  runId: string;
  state: SidebarCommandRunState;
  type: "sidebarCommandRunStateChanged";
};

export type SidebarCommandRunStateClearedMessage = {
  commandId: string;
  type: "sidebarCommandRunStateCleared";
};

export type SidebarDaemonInfo = {
  pid: number;
  port: number;
  protocolVersion: number;
  startedAt: string;
};

export type SidebarDaemonSessionItem = {
  agentName?: string;
  agentStatus: "idle" | "working" | "attention";
  cols: number;
  cwd: string;
  endedAt?: string;
  errorMessage?: string;
  exitCode?: number;
  isCurrentWorkspace: boolean;
  isLocalOnly?: boolean;
  /**
   * CDXC:SessionInventoryOwnership 2026-06-02-17:19:
   * Running Sessions may show gxserver-backed terminal rows and macOS-local panes in one modal. Carry ownership on the contract so the UI and external consumers can label local-only rows instead of treating every row as shared daemon state.
   */
  ownership?: "gxserver" | "local";
  restoreState: "live" | "replayed";
  rows: number;
  sessionId: string;
  shell: string;
  startedAt: string;
  status: "starting" | "running" | "exited" | "error" | "disconnected";
  title?: string;
  workspaceId: string;
};

export type SidebarDaemonSessionsStateMessage = {
  daemon?: SidebarDaemonInfo;
  errorMessage?: string;
  sessions: SidebarDaemonSessionItem[];
  t3Server?: SidebarT3ServerInfo;
  t3Sessions: SidebarT3SessionItem[];
  type: "daemonSessionsState";
};

export type SidebarT3ServerInfo = {
  pid: number;
  port: number;
  startedAt?: string;
};

export type SidebarT3SessionItem = {
  activity: "idle" | "working" | "attention";
  detail?: string;
  isCurrentWorkspace: boolean;
  isFocused: boolean;
  isLocalOnly?: boolean;
  isRunning: boolean;
  isSleeping: boolean;
  lastInteractionAt?: string;
  ownership?: "local";
  sessionId: string;
  threadId?: string;
  title?: string;
  workspaceId: string;
  workspaceRoot?: string;
};

export type SidebarPromptGitCommitMessage = {
  /**
   * CDXC:PromptAgents 2026-05-29-10:53:
   * Git commit review, Multiple Commits, Release, and generated rename/title flows
   * must carry the user-selected prompt agent explicitly. Modal-specific choices
   * are remembered by the modal host, while Settings default-agent changes clear
   * those remembered choices so every modal returns to the new default.
   */
  action: SidebarGitAction;
  agentId?: string;
  branch?: string | null;
  changedFiles?: SidebarGitChangedFile[];
  confirmLabel: string;
  deleteWorktreeAfterDefault?: boolean;
  description: string;
  isWorktree?: boolean;
  isDefaultRef?: boolean;
  requestId: string;
  showCommitMessage?: boolean;
  suggestedBody?: string;
  suggestedSubject: string;
  type: "promptGitCommit";
  worktreeName?: string;
};

export type SidebarT3BrowserAccessMode = "external" | "local-network" | "local-only" | "tailscale";

export type SidebarShowT3BrowserAccessMessage = {
  endpointUrl: string;
  localUrl: string;
  mode: SidebarT3BrowserAccessMode;
  note: string;
  sessionId: string;
  sessionTitle: string;
  tailscaleEnabled: boolean;
  type: "showT3BrowserAccess";
};

export type SidebarGhostexFolderStat = {
  name: string;
  path: string;
  sizeBytes: number;
};

/**
 * CDXC:SettingsStorage 2026-05-09-15:25
 * Settings exposes ~/.ghostex disk usage only after the user scrolls to the
 * bottom of the modal. The native sidebar sends per-folder byte counts back as
 * a sidebar message so the full-window modal can render stats without owning
 * filesystem access or accepting client-provided paths.
 */
export type SidebarGhostexFolderStatsMessage = {
  errorMessage?: string;
  folderPath: string;
  folders: SidebarGhostexFolderStat[];
  generatedAt: string;
  totalBytes: number;
  type: "ghostexFolderStats";
};

/**
 * CDXC:AppModals 2026-04-28-16:18
 * User-input flows must not use VS Code input boxes, quick picks, or modal
 * editors. Extension-initiated prompts are represented as sidebar messages so
 * the existing React modal host owns rendering and styling.
 */
export type SidebarShowSessionRenameModalMessage = {
  initialTitle: string;
  sessionId: string;
  type: "showSessionRenameModal";
};

export type SidebarShowT3ThreadIdModalMessage = {
  currentThreadId: string;
  sessionId: string;
  type: "showT3ThreadIdModal";
};

export type SidebarPreviousSessionsResultMessage = {
  previousSessions: SidebarPreviousSessionItem[];
  query?: string;
  requestId: string;
  type: "previousSessionsResult";
};

export type SidebarRemoteMachineStatusMessage = {
  machineId: string;
  state:
    | "connecting"
    | "connected"
    | "disconnected"
    | "installApprovalRequired"
    | "installing"
    | "failed";
  type: "remoteMachineStatus";
};

export type SidebarNativeHotkeyMessage = {
  /**
   * CDXC:Hotkeys 2026-06-05-20:53:
   * AppKit owns Cmd+number while terminal panes have focus, then forwards the shared hotkey action id into the sidebar so React can resolve session slots from the currently rendered row order, including collapsed-project filtering.
   */
  actionId: ghostexHotkeyActionId;
  type: "nativeHotkey";
};

export type ExtensionToSidebarMessage =
  | SidebarHydrateMessage
  | SidebarSessionStateMessage
  | SidebarNativeHotkeyMessage
  | AgentsHubCatalogMessage
  | AgentsHubFileContentMessage
  | SidebarSessionPresentationChangedMessage
  | SidebarGroupsChangedMessage
  | SidebarHudChangedMessage
  | SidebarPlayCompletionSoundMessage
  | SidebarOrderSyncResultMessage
  | SidebarCommandRunStateChangedMessage
  | SidebarCommandRunStateClearedMessage
  | SidebarDaemonSessionsStateMessage
  | SidebarPromptGitCommitMessage
  | SidebarShowT3BrowserAccessMessage
  | SidebarGhostexFolderStatsMessage
  | SidebarAgentHookStatusMessage
  | SidebarGhostexCliStatusMessage
  | SidebarOSIntegrationStatusMessage
  | SidebarShowSessionRenameModalMessage
  | SidebarShowT3ThreadIdModalMessage
  | SidebarPreviousSessionsResultMessage
  | SidebarRemoteMachineStatusMessage;

export type SidebarToExtensionMessage =
  | {
      /**
       * CDXC:GxserverBootstrap 2026-05-31-03:56:
       * The gxserver failure toast needs a Retry action that returns to the
       * trusted sidebar command router, then native performs the daemon restart.
       */
      type: "retryGxserverStart";
    }
  | {
      type: "openSettings";
    }
  | {
      /**
       * CDXC:SidebarDiscord 2026-05-27-05:04:
       * Sidebar surfaces can link to the public Ghostex Discord for support,
       * questions, and contributors. Native owns URL opening so the sidebar
       * does not depend on webview popup behavior.
       */
      type: "openExternalUrl";
      url: string;
    }
  | {
      /**
       * CDXC:SettingsStorage 2026-05-09-15:25
       * The settings modal can request ~/.ghostex folder stats lazily, but native
       * resolves the folder path itself and never trusts a path from React.
       */
      type: "requestGhostexFolderStats" | "openGhostexFolder";
    }
  | {
      /**
       * CDXC:AgentHookSettings 2026-05-23-10:05:
       * Settings -> Agents can refresh hook status and trigger the existing hook installer, but native remains the owner of config paths, executable checks, and hook-file mutation.
       *
       * CDXC:FirstLaunchSetup 2026-05-26-17:12:
       * First launch CLI setup must distinguish a missing CLI from an app that
       * was already installed through Homebrew. Native owns PATH inspection so
       * the production modal and Storybook mock can share the same UI contract.
       */
      type: "requestAgentHookStatus" | "installAgentHooks" | "requestGhostexCliStatus";
    }
  | {
      /**
       * CDXC:IntegrationsSetup 2026-05-27-04:17:
       * First launch and Settings -> Integrations expose one-click install
       * actions for optional integrations. Native runs the actual commands and
       * refreshes the shared integration status afterward.
       */
      type:
        | "installGhostexCli"
        | "installBrowserControl"
        | "installComputerUseSkill"
        | "installAgentOrchestrationSkill"
        | "installGenerateTitleSkill"
        | "installCuaDriver";
    }
  | {
      /**
       * CDXC:OSIntegration 2026-05-27-18:06:
       * Settings exposes explicit OS default actions. Installing Ghostex only
       * registers it as an available handler; default editor, terminal-link,
       * and script-runner ownership changes happen only through this command.
       */
      target: "editor" | "terminalLinks" | "scriptRunner" | "all";
      type: "setOSIntegrationDefaults";
    }
  | {
      type: "requestOSIntegrationStatus";
    }
  | {
      settings: ghostexSettings;
      type: "updateSettings";
    }
  | {
      /**
       * CDXC:RemoteMachines 2026-06-09-18:23:
       * Remote settings can save an SSH password, but the password must cross
       * the webview boundary only as an explicit user action. Native writes it
       * to macOS Keychain and settings store only the saved-password marker.
       */
      password: string;
      remoteMachineId: string;
      type: "saveRemoteMachinePassword";
    }
  | {
      /**
       * CDXC:GhosttySettings 2026-04-30-01:48
       * The settings modal exposes Ghostty-specific actions that are not plain
       * ghostex preference changes: reset managed config keys, apply the
       * recommended config block, open docs, and open the platform config file.
       *
       * CDXC:AccessibilityPermissions 2026-05-08-13:08
       * The same modal action channel also carries a direct open-settings
       * command for macOS Accessibility status. It does not enable attachment
       * or trigger the permission prompt by itself.
       */
      type:
        | "applyRecommendedGhosttySettings"
        | "openAccessibilityPreferences"
        | "openScreenRecordingPreferences"
        | "requestMacOSNotificationPermission"
        | "openMacOSNotificationSettings"
        | "openGhosttyConfigFile"
        | "openGhosttySettingsDocs"
        | "installGte"
        | "resetGhosttySettingsToDefault";
    }
  | {
      /**
       * CDXC:SessionAttentionNotifications 2026-05-11-01:14
       * Settings' test button should exercise the same native attention
       * completion flow as a real agent task without mutating any session.
       */
      type: "testAgentTaskCompletion";
    }
  | {
      /**
       * CDXC:Settings 2026-05-11-02:06
       * Settings sound dropdown preview buttons play only the selected sound,
       * using the same native audio path as real completion alerts while
       * avoiding notification side effects.
       */
      sound: CompletionSoundSetting;
      type: "playCompletionSoundPreview";
    }
  | {
      type: "toggleCompletionBell";
    }
  | {
      type: "cycleSessionPersistenceProvider";
    }
  | {
      delta: -1 | 1;
      type: "adjustTerminalFontSize";
    }
  | {
      type: "refreshDaemonSessions";
    }
  | {
      type: "killTerminalDaemon";
    }
  | {
      type: "killT3RuntimeServer";
    }
  | {
      type: "killDaemonSession";
      sessionId: string;
      workspaceId: string;
    }
  | {
      type: "killT3RuntimeSession";
      sessionId: string;
    }
  | {
      type: "moveSidebarToOtherSide";
    }
  | {
      /**
       * CDXC:SidebarCollapse 2026-06-12-02:23:
       * Cmd+B and command-palette execution need a native chrome command that
       * collapses the whole AppKit sidebar, not only React sidebar content.
       */
      type: "toggleSidebarCollapsed";
    }
  | {
      /**
       * CDXC:SidebarContextMenu 2026-05-20-13:05:
       * Session and project context menus notify native when open so clicks on
       * terminal, titlebar, and other non-sidebar surfaces dismiss the menu
       * while the original AppKit click still reaches its target.
       */
      type: "sidebarContextMenuOpened";
    }
  | {
      type: "sidebarContextMenuClosed";
    }
  | {
      /**
       * CDXC:CommandPalette 2026-05-16-08:18:
       * The full-window command palette needs a pet wake/sleep action that
       * reuses the sidebar settings owner instead of mutating pet visibility
       * inside the detached modal host.
       */
      type: "togglePetOverlay";
    }
  | {
      type: "createSession";
    }
  | {
      /**
       * CDXC:CommandPalette 2026-05-15-20:38:
       * Palette selections for built-in Ghostex commands should execute through
       * the same native hotkey action dispatcher as physical shortcuts so the
       * available command list cannot drift from actual app behavior.
       */
      actionId: ghostexHotkeyActionId;
      type: "runGhostexHotkeyAction";
    }
  | {
      /**
       * CDXC:PaneTabs 2026-05-11-11:51
       * The combined sidebar Settings row has a legacy-named secondary terminal
       * action. It targets the currently active project and creates the new
       * terminal as the selected tab in the focused session's tab group so pane
       * sizes and tab groupings remain unchanged.
       */
      type: "createFullWidthTerminalPane";
    }
  | {
      /**
       * CDXC:Chats 2026-05-04-09:30
       * Chats are projectless AI work areas. The native sidebar owns chat
       * folder creation and then opens a normal empty terminal there so agent
       * title/icon detection stays identical to project sessions.
       */
      title?: string;
      type: "createChat";
    }
  | {
      /**
       * CDXC:Plugins 2026-05-08-10:44
       * The top-sidebar Plugins entry opens the skills directory as a Chromium
       * browser pane under Chats, not inside the active project. Keep this
       * separate from generic browser actions because its destination is fixed.
       */
      type: "openPluginsBrowserChat";
    }
  | {
      /**
       * CDXC:AgentsHub 2026-05-12-09:21
       * Agents Hub runs in the full-window modal host, but profile/file actions
       * still need native filesystem affordances from the sidebar bridge.
       */
      path: string;
      type: "openAgentsHubPathInFinder";
    }
  | {
      filePath: string;
      type: "openAgentsHubFileInDefaultEditor";
    }
  | {
      /**
       * CDXC:AgentsHub 2026-05-14-08:29:
       * Agents Hub must show the real files installed on the user's machine, including files owned by Claude/Codex profiles and plugin caches.
       * The modal host requests a fresh native filesystem catalog whenever the Hub opens instead of relying on a bundled placeholder list.
       *
       * CDXC:AgentsHub 2026-06-12-02:53:
       * Catalog requests return metadata only so large profile/plugin trees can
       * paint the Hub immediately without pushing every file buffer through the
       * native process-result bridge.
       */
      type: "requestAgentsHubCatalog";
    }
  | {
      /**
       * CDXC:AgentsHub 2026-06-12-02:53:
       * Agents Hub reads file contents only after selection because the left
       * tree needs metadata, while editor buffers can be large enough to block
       * the modal bridge when loaded for every file at once.
       */
      filePath: string;
      requestId: string;
      type: "requestAgentsHubFileContent";
    }
  | {
      /**
       * CDXC:AgentsHub 2026-05-14-08:27:
       * The Hub modal edits real agent instruction/config files and enables Save only after text changes.
       * Persist the current editor buffer through the native sidebar command contract so the modal host keeps using the same filesystem bridge as the external-editor action.
       */
      content: string;
      filePath: string;
      type: "saveAgentsHubFile";
    }
  | {
      /**
       * CDXC:Chats 2026-05-08-11:53
       * The reference-style Chats section header has a hover-only browser
       * action beside New Chat. It creates a new projectless chat and opens a
       * browser pane there, without requiring a concrete chat group id.
       */
      type: "openBrowserChat";
    }
  | {
      type: "openBrowser";
    }
  | {
      /**
       * CDXC:ChromiumBrowserPanes 2026-05-27-07:24
       * Browser actions always create in-workspace browser panes now that the
       * legacy Chrome Canary attachment route has been removed.
       */
      url?: string;
      type: "openBrowserPane";
    }
  | {
      /**
       * CDXC:ProjectGroups 2026-05-06-18:42
       * Project headers expose New Browser beside the create-session control.
       * Carry the group id so native can focus that project/group before
       * creating the browser pane.
       */
      groupId: string;
      type: "openBrowserPaneInGroup";
    }
  | {
      type: "openWorkspaceWelcome";
    }
  | {
      /**
       * CDXC:NativeWorkspacePicker 2026-05-08-18:45
       * The reference Projects header add button should open the trusted native
       * folder picker.
       */
      type: "pickWorkspaceFolder";
    }
  | {
      /**
       * CDXC:RemoteMachines 2026-06-02-23:47:
       * Disconnected Remote sidebar sections stay visible and expose only Reload. Native owns the SSH reconnect/start/install gxserver flow, so React sends the saved machine id instead of handling SSH details in the sidebar.
       *
       * CDXC:RemoteMachines 2026-06-02-23:38:
       * Missing gxserver installation requires explicit React modal approval.
       * The approval flag is carried back through the same reconnect command
       * so native can upload/install only after the user accepts.
       */
      installApproved?: boolean;
      remoteMachineId: string;
      type: "reconnectRemoteMachine";
    }
  | {
      /**
       * CDXC:RemoteProjectPicker 2026-06-02-23:22:
       * Remote Add Project uses a T3 Code-style directory picker, but every
       * browse request is machine-scoped. Native must route it to that
       * machine's gxserver after SSH reconnect/token setup instead of exposing
       * local filesystem browsing for remote machines.
       */
      partialPath: string;
      remoteMachineId: string;
      requestId: string;
      type: "browseRemoteProjectDirectories";
    }
  | {
      /**
       * CDXC:RemoteProjects 2026-06-03-00:18:
       * Adding a remote project is not the local Add Project command. Carry the
       * remote machine id with the selected path so native can add the project
       * through that machine's gxserver and later render it under that machine's
       * sidebar section.
       */
      path: string;
      remoteMachineId: string;
      requestId: string;
      type: "addRemoteProjectPath";
    }
  | {
      /**
       * CDXC:RemoteClone 2026-06-02-23:38:
       * Connected Remote machine headers expose Clone Repository beside Add
       * Project, but the command must stay machine-scoped. Do not route this
       * through the local clone modal without a remote gxserver target.
       */
      remoteMachineId: string;
      type: "openRemoteCloneRepository";
    }
  | {
      /**
       * CDXC:AddRepository 2026-06-01-10:28:
       * Reference-only repository clones can request main-only and shallow Git
       * options from the modal. Keep both flags explicit in the native bridge
       * contract so the UI state determines the exact clone command.
       *
       * CDXC:AddRepository 2026-06-02-13:41:
       * The full-window Clone Repository modal sends clone requests through the
       * native sidebar UI bridge, but gxserver owns preview, git clone execution,
       * cancellation, and the canonical project returned after clone success.
       *
       * CDXC:AddRepository 2026-06-07-16:06:
       * Clone Repository carries an optional branch name through the same bridge.
       * Empty means gxserver lets Git use the repository default branch; typed
       * names are validated server-side before Git starts.
       */
      branchName?: string;
      cloneMainOnly?: boolean;
      folderPath: string;
      newFolderName?: string;
      remoteMachineId?: string;
      repositoryInput: string;
      requestId: string;
      shallowClone?: boolean;
      type: "cloneRepository";
    }
  | {
      /**
       * CDXC:AddRepository 2026-06-01-11:18:
       * Repository clone destination preview is routed through gxserver so the
       * modal can warn about an existing default folder without reimplementing
       * filesystem and repository parsing logic in the macOS UI layer.
       */
      folderPath: string;
      newFolderName?: string;
      remoteMachineId?: string;
      repositoryInput: string;
      requestId: string;
      type: "previewRepositoryClone";
    }
  | {
      /**
       * CDXC:AddRepository 2026-06-02-13:41:
       * Repository clone progress moved from the modal to a persistent toast.
       * The toast Cancel action must target the active clone request instead of
       * only dismissing UI, so the native sidebar can ask gxserver to cancel the
       * corresponding clone job.
      */
      remoteMachineId?: string;
      requestId: string;
      type: "cancelRepositoryClone";
    }
  | {
      type: "createSessionInGroup";
      groupId: string;
    }
  | {
      type: "focusGroup";
      groupId: string;
    }
  | {
      type: "toggleFullscreenSession";
    }
  | {
      type: "focusSession";
      sessionId: string;
    }
  | {
      /**
       * CDXC:SessionFocusMode 2026-05-23-09:28:
       * Session-card and pane-tab Focus is a reversible zoom for the clicked
       * session's pane tab group. The native/sidebar controller owns this
       * command because it must also switch from Code/Git/Project surfaces back
       * to Agents while remembering the prior surface for unfocus.
       */
      type: "focusSessionMode";
      sessionId: string;
    }
  | {
      type: "promptRenameSession";
      sessionId: string;
    }
  | {
      type: "restartSession";
      sessionId: string;
    }
  | {
      type: "renameSession";
      agentId?: string;
      sessionId: string;
      title: string;
      /**
       * CDXC:SessionNaming 2026-05-09-17:25
       * Generate Title reuses renameSession with the saved 1st user message,
       * but must force controller-side title generation even when that message
       * is shorter than the rename modal's 70-character Generate Name threshold.
       */
      shouldGenerateTitle?: boolean;
    }
  | {
      sessionId: string;
      threadId: string;
      type: "setT3SessionThreadId";
    }
  | {
      type: "renameGroup";
      groupId: string;
      title: string;
    }
  | {
      /**
       * CDXC:WorktreeDelete 2026-05-28-07:46:
       * Combined project rows render worktrees as project headers, so project-name edits and delete confirmation prompts must route through trusted group ids instead of trusting DOM-provided paths.
       */
      type: "renameWorkspaceProjectForGroup";
      groupId: string;
      title: string;
    }
  | {
      type: "copyWorkspaceProjectPathForGroup";
      groupId: string;
    }
  | {
      type: "restoreRecentProject";
      projectId: string;
    }
  | {
      /**
       * CDXC:RecentProjects 2026-05-27-07:04:
       * Recent Projects rows have their own right-click menu because they are
       * parked projects without a rendered project group id. Route filesystem
       * and removal actions by trusted project id so the sidebar does not send
       * raw paths back to native.
       */
      type:
        | "copyRecentProjectPath"
        | "openRecentProjectInFinder"
        | "removeRecentProject";
      projectId: string;
    }
  | {
      /**
       * CDXC:WorkspaceActions 2026-05-04-08:22
       * Combined-mode project cards expose native open actions from the
       * right-click menu. The native sidebar resolves the group id to its
       * trusted stored workspace path instead of accepting a client path.
       */
      type: "openWorkspaceProjectInFinderForGroup" | "openWorkspaceProjectInIdeForGroup";
      groupId: string;
    }
  | {
      /**
       * CDXC:EditorPanes 2026-05-06-14:21
       * Project editor buttons are trusted group-scoped commands. Native
       * resolves the group id to its stored project path before launching the
       * embedded code-server editor or refreshing its diff stats.
       *
       * CDXC:EditorPanes 2026-05-06-18:55
       * The editor card also accepts middle-click close, but the editor is not a
       * session; route close through the same trusted project/group resolver.
       */
      type:
        | "closeWorkspaceProjectEditorForGroup"
        | "openWorkspaceProjectEditorForGroup"
        | "refreshWorkspaceProjectDiffForGroup";
      groupId: string;
    }
  | {
      /**
       * CDXC:SidebarActions 2026-05-05-02:47
       * Sidebar Open In dropdowns know the active project but not a group id.
       * Route these commands through the native sidebar so stored workspace
       * paths remain trusted on the app side instead of being accepted from DOM.
       */
      type: "openActiveWorkspaceProjectInFinder";
    }
  | {
      /**
       * CDXC:SidebarActions 2026-05-05-03:11
       * The sidebar Open In dropdown lists explicit IDE targets. The selected
       * target must travel with the active-project open command instead of
       * being inferred from Settings, so choosing VS Code or Zed immediately
       * opens the project in that exact app.
       */
      targetApp: Extract<WorkspaceIdeTargetApp, "vscode" | "zed">;
      type: "openActiveWorkspaceProjectInIde";
    }
  | {
      /**
       * CDXC:WorkspaceTheme 2026-05-05-05:01
       * Preset theme selection must actively clear a previous Custom color.
       * `themeColor: null` is the sidebar-to-native signal that the custom
       * override is being removed, so icon and project-header tinting cannot
       * keep using stale custom CSS variables after a preset is selected.
       */
      type: "setWorkspaceProjectThemeForGroup";
      groupId: string;
      theme?: SidebarTheme;
      themeColor?: string | null;
    }
  | {
      /**
       * CDXC:RecentProjects 2026-05-04-14:25
       * Combined project context menus close projects into the Recent Projects
       * drawer instead of deleting their stored sessions. Remove remains the
       * explicit project-delete path.
       */
      type: "closeWorkspaceProjectForGroup" | "removeWorkspaceProjectForGroup";
      groupId: string;
    }
  | {
      /**
       * CDXC:WorktreeDelete 2026-06-02-13:41:
       * Delete Worktree first asks gxserver for a fresh Git status summary,
       * then the native sidebar opens the full-window confirmation modal before
       * any checkout directory is removed.
       */
      type: "promptDeleteWorktreeForGroup";
      groupId: string;
    }
  | {
      type: "closeGroup";
      groupId: string;
    }
  | {
      type: "closeSession";
      sessionId: string;
    }
  | {
      type: "closeSessions";
      sessionIds: string[];
    }
  | {
      type: "setSessionSleeping";
      sessionId: string;
      sleeping: boolean;
    }
  | {
      type: "setSessionsSleeping";
      sessionIds: string[];
      sleeping: boolean;
    }
  | {
      favorite: boolean;
      type: "setSessionFavorite";
      sessionId: string;
    }
  | {
      sessionId: string;
      sessionTag?: SidebarSessionTag | null;
      type: "setSessionTag";
    }
  | {
      pinned: boolean;
      type: "setSessionPinned";
      sessionId: string;
    }
  | {
      type: "setGroupSleeping";
      groupId: string;
      sleeping: boolean;
    }
  | {
      /**
       * CDXC:ProjectSleep 2026-05-27-01:50:
       * Combined project rows do not map to one native workspace group. Their
       * context-menu sleep action must be project-scoped and must only sleep
       * inactive sessions so running, working, and attention sessions stay
       * awake.
       */
      type: "sleepInactiveProjectSessions";
      groupId: string;
    }
  | {
      /**
       * CDXC:ProjectClose 2026-06-04-23:40:
       * Combined project-row Close inactive is project-scoped, not group-scoped.
       * It closes idle terminal sessions while preserving working and attention
       * sessions, and it must not park the whole project in Recent Projects.
       */
      type: "closeInactiveProjectSessions";
      groupId: string;
    }
  | {
      /**
       * CDXC:ProjectSleep 2026-05-27-02:18:
       * Combined project-row Wake must wake sleeping terminal sessions across
       * every workspace group because the row does not carry a concrete native
       * workspace group id.
       */
      type: "wakeProjectSleepingSessions";
      groupId: string;
    }
  | {
      type: "copyResumeCommand";
      sessionId: string;
    }
  | {
      type: "copyAttachCommand";
      sessionId: string;
    }
  | {
      /**
       * CDXC:SidebarContextMenu 2026-06-11-23:08:
       * The React sidebar builds Copy details text from its rendered session row
       * and sends only that user-requested clipboard payload to native.
       */
      type: "copySessionDetails";
      detailsText: string;
      sessionId: string;
    }
  | {
      /**
       * CDXC:DelayedSend 2026-05-11-11:56
       * Delayed Send schedules an Enter keypress for an already-staged terminal
       * command. The sidebar/modal sends only the trusted session id and delay;
       * native resolves the terminal and uses the existing Enter-key path.
       */
      delayMs: number;
      sessionId: string;
      type: "scheduleDelayedSend";
    }
  | {
      /**
       * CDXC:DelayedSend 2026-05-17-03:14
       * Users must be able to cancel a scheduled delayed send from the same
       * modal/sidebar affordance that shows the remaining countdown.
       */
      sessionId: string;
      type: "cancelDelayedSend";
    }
  | {
      type: "forkSession";
      sessionId: string;
    }
  | {
      type: "fullReloadSession";
      sessionId: string;
    }
  | {
      /**
       * CDXC:PanePopOut 2026-05-19-10:15:
       * Browser and agent session cards expose Pop Out Pane in the sidebar
       * context menu. The controller toggles pop-out presentation from the
       * current session record, matching the focused-pane hotkey behavior.
       */
      sessionId: string;
      type: "popOutPane";
    }
  | {
      type: "fullReloadGroup";
      groupId: string;
    }
  | {
      /**
       * CDXC:ProjectReload 2026-05-27-02:18:
       * Combined project rows need a project-scoped full reload because their
       * sidebar group id is synthetic. Reload only idle attached zmx terminals
       * so project-level reload never interrupts working or attention sessions
       * and never tries to restore sleeping/detached history records.
       */
      type: "fullReloadProjectZmxSessions";
      groupId: string;
    }
  | {
      type: "requestT3SessionBrowserAccess";
      sessionId: string;
    }
  | {
      type: "openT3SessionBrowserAccessLink";
      url: string;
    }
  | {
      /**
       * CDXC:BrowserPanes 2026-05-02-06:35
       * Browser session cards expose pane-specific controls copied from the
       * native browser workflow: DevTools, the Settings-selected feedback tool,
       * profile selection, and browser-data import. The native host owns the
       * macOS UI and WebKit/CEF work.
       */
      action: "devtools" | "feedback-tool" | "profile-picker" | "import-settings";
      sessionId: string;
      type: "runBrowserPaneAction";
    }
  | {
      /**
       * CDXC:GxserverPresentationSearch 2026-06-01-15:08:
       * Previous Sessions is loaded on demand from gxserver after the presentation hard cutover. React sends debounced metadata queries through native so startup no longer hydrates all previous-session history into the sidebar store.
       */
      limit?: number;
      query?: string;
      requestId: string;
      sessionTags?: SidebarSessionTag[];
      type: "requestPreviousSessions";
    }
  | {
      historyId: string;
      type: "restorePreviousSession";
    }
  | {
      historyId: string;
      type: "deletePreviousSession";
    }
  | {
      /**
       * CDXC:PreviousSessions 2026-05-29-12:36:
       * Previous Sessions needs a direct text-search launcher. Keep it as an
       * explicit sidebar command so the Search row can start a fresh terminal
       * running `gx f`.
       *
       * CDXC:PreviousSessions 2026-05-29-20:32:
       * Search by Text must create that terminal in the currently active
       * project, not in the Quick/projectless terminal area.
       *
       * CDXC:PreviousSessions 2026-06-13-01:09:
       * The Previous Sessions modal no longer renders launch buttons, and the
       * agent-based previous-session prompt path has been removed. This command
       * remains the direct Search row launcher only.
       */
      type: "searchPreviousSessionsByText";
    }
  | {
      content: string;
      type: "saveScratchPad";
    }
  | {
      content: string;
      promptId?: string;
      title: string;
      type: "savePinnedPrompt";
    }
  | {
      type: "moveSessionToGroup";
      groupId: string;
      sessionId: string;
      targetIndex?: number;
    }
  | {
      type: "sidebarDebugLog";
      event: string;
      details?: unknown;
    }
  | {
      type: "createGroupFromSession";
      sessionId: string;
    }
  | {
      type: "createGroup";
    }
  | {
      type: "setVisibleCount";
      visibleCount: VisibleSessionCount;
      groupId?: string;
    }
  | {
      type: "setViewMode";
      viewMode: TerminalViewMode;
    }
  | {
      type: "toggleActiveSessionsSortMode";
    }
  | {
      manualSessionIdsByGroup?: Record<string, string[]>;
      sortMode: SidebarActiveSessionsSortMode;
      type: "setActiveSessionsSortMode";
    }
  | {
      type: "syncSessionOrder";
      groupId: string;
      sessionIds: string[];
    }
  | {
      type: "syncGroupOrder";
      groupIds: string[];
    }
  | {
      type: "runSidebarCommand";
      commandId: string;
      runMode?: SidebarCommandRunMode;
      worktreePath?: string;
    }
  | {
      type: "endSidebarCommandRun";
      commandId: string;
    }
  | {
      action: SidebarGitAction;
      groupId?: string;
      type: "runSidebarGitAction";
    }
  | {
      action: SidebarGitAction;
      type: "setSidebarGitPrimaryAction";
    }
  | {
      type: "refreshGitState";
    }
  | {
      enabled: boolean;
      type: "setSidebarGitCommitConfirmationEnabled";
    }
  | {
      enabled: boolean;
      type: "setSidebarGitGenerateCommitBodyEnabled";
    }
  | {
      commitOnNewRef?: boolean;
      deleteWorktreeAfter?: boolean;
      agentId?: string;
      filePaths?: string[];
      message: string;
      requestId: string;
      type: "confirmSidebarGitCommit";
    }
  | {
      deleteWorktreeAfter?: boolean;
      agentId?: string;
      filePaths?: string[];
      message: string;
      requestId: string;
      type: "confirmSidebarGitDirectMerge";
    }
  | {
      agentId?: string;
      requestId: string;
      type: "runSidebarGitMultipleCommits";
    }
  | {
      filePath: string;
      type: "openSidebarGitChangedFile";
    }
  | {
      filePath: string;
      requestId?: string;
      type: "openSidebarGitChangedFileDiff";
    }
  | {
      requestId: string;
      type: "cancelSidebarGitCommit";
    }
  | {
      /**
       * CDXC:WorktreeDelete 2026-06-10-22:56:
       * Delete Worktree confirmation may request branch cleanup after the
       * checkout is removed. Keep only boolean user choices in the sidebar
       * bridge message; native re-resolves branch names before mutating Git.
       */
      deleteLocalBranch?: boolean;
      deleteRemoteBranch?: boolean;
      projectId: string;
      type: "confirmDeleteWorktree";
    }
  | {
      groupId: string;
      type: "commitWorktreeBeforeDelete";
    }
  | {
      type: "saveSidebarCommand";
      actionType: SidebarActionType;
      closeTerminalOnExit: boolean;
      commandId?: string;
      icon?: SidebarCommandIcon;
      iconColor?: string;
      name: string;
      playCompletionSound: boolean;
      command?: string;
      url?: string;
    }
  | {
      type: "deleteSidebarCommand";
      commandId: string;
    }
  | {
      requestId: string;
      type: "syncSidebarCommandOrder";
      commandIds: string[];
    }
  | {
      type: "runSidebarAgent";
      agentId: string;
      groupId?: string;
    }
  | {
      type: "createProjectWorktree";
      agentId?: string;
      existingWorktreePath?: string;
      mode?: "create" | "openExisting";
      prompt?: string;
      projectId?: string;
      projectPath?: string;
      remoteMachineId?: string;
    }
  | {
      type: "requestProjectWorktrees";
      projectId?: string;
      projectPath?: string;
      requestId: string;
      remoteMachineId?: string;
    }
  | {
      type: "setProjectWorktreeCommand";
      command: string;
      projectId: string;
    }
  | {
      type: "setProjectBeadsDisplayKey";
      displayKey: string;
      projectId: string;
    }
  | {
      acceptAllMode?: AgentAcceptAllMode;
      type: "saveSidebarAgent";
      agentId?: string;
      command: string;
      icon?: SidebarAgentIcon;
      name: string;
    }
  | {
      type: "deleteSidebarAgent";
      agentId: string;
    }
  | {
      requestId: string;
      type: "syncSidebarAgentOrder";
      agentIds: string[];
    };

export type SidebarHudSnapshot = Pick<
  SessionGridSnapshot,
  | "focusedSessionId"
  | "fullscreenRestoreVisibleCount"
  | "sessions"
  | "visibleCount"
  | "visibleSessionIds"
  | "viewMode"
>;
