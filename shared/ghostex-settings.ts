import {
  DEFAULT_AGENT_MANAGER_ZOOM_PERCENT,
  type SidebarThemeSetting,
  type TerminalEngine,
} from "./session-grid-contract-core";
import {
  clampAgentManagerZoomPercent,
  clampSidebarThemeSetting,
  DEFAULT_COMMANDS_PANEL_HEIGHT_PX,
  normalizeTerminalEngine,
} from "./session-grid-contract-session";
import {
  clampCompletionSoundSetting,
  DEFAULT_COMPLETION_SOUND,
  type CompletionSoundSetting,
} from "./completion-sound";
import {
  getGhosttyFontFamilyForPreset,
  getTerminalFontFamilyForPreset,
  normalizeTerminalFontPreset,
} from "./terminal-font-preset";
import {
  DEFAULT_ghostex_HOTKEYS,
  normalizeghostexHotkeySettings,
  type ghostexHotkeySettings,
} from "./ghostex-hotkeys";
import { GHOSTTY_THEME_OPTIONS } from "./ghostty-theme-options";
import {
  DEFAULT_WORKSPACE_OPEN_TARGET_AVAILABILITY,
  normalizeCustomWorkspaceOpenTargets,
  normalizeWorkspaceOpenTargetAvailability,
  normalizeWorkspaceOpenTargetHiddenIds,
  type CustomWorkspaceOpenTarget,
  type WorkspaceOpenTargetAvailability,
} from "./workspace-open-targets";
import { DEFAULT_PET_ID, normalizePetId, type PetId } from "./pets";

export type GhosttyConfirmCloseSurface = "false" | "true" | "always";
export type GhosttyCopyOnSelect = "false" | "true" | "clipboard";
export type GhosttyScrollbar = "system" | "never";
export type TerminalCursorStyle = "bar" | "block" | "underline";
export type BrowserOpenMode = "browser-pane";
export type BrowserFeedbackTool = "react-grab" | "agentation";
export type DefaultEditorCommand =
  | "code"
  | "code-insiders"
  | "zed"
  | "zeditor"
  | "cursor"
  | "windsurf"
  | "codium"
  | "subl"
  | "other";
export type SessionPersistenceProvider = "off" | "tmux" | "zmx" | "zellij";
export type SessionStatusIndicatorSize = "small" | "medium" | "large" | "x-large";
export type SidebarSide = "left" | "right";
export type SidebarSettingsPresetId = "codex" | "minimal" | "detailed" | "recommended";
export type PromptEditorBackend = "inherit" | "monaco" | "gte" | "custom";
export type SessionTitleGenerationAgent = "codex" | "cursor" | "claude" | "grok" | "custom";
export type AppShotsHotkey = "both-command" | "double-left-shift" | "double-left-option";
export type KeepAwakeDurationMinutes = 0 | 120 | 300;
export type AutoSleepIdleMinutes = 5 | 10 | 15 | 30 | 60 | 120 | 300;
export type RemoteMachineSettings = {
  id: string;
  name: string;
  sshHost: string;
  sshIdentityFile?: string;
  sshPasswordSaved?: boolean;
  sshPort?: number;
  sshUser?: string;
};
const MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER = 0.25;
const MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER = 8;
const MIN_GHOSTTY_SCROLLBACK_LIMIT_MB = 1;
const MAX_GHOSTTY_SCROLLBACK_LIMIT_MB = 200;
export const MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX = 40;
export const MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX = 600;
export const DEFAULT_SIDEBAR_DEFAULT_WIDTH_PX = 235;
export const MIN_SIDEBAR_DEFAULT_WIDTH_PX = 150;
export const MAX_SIDEBAR_DEFAULT_WIDTH_PX = 520;
export const DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT = 10;
export const MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT = 1;
export const MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT = 50;
export const SESSION_TITLE_GENERATION_AGENT_OPTIONS: ReadonlyArray<{
  label: string;
  value: SessionTitleGenerationAgent;
}> = [
  { label: "Codex", value: "codex" },
  { label: "Cursor CLI", value: "cursor" },
  { label: "Claude", value: "claude" },
  { label: "Grok Build", value: "grok" },
  { label: "Custom", value: "custom" },
];
export const SESSION_TITLE_GENERATION_PROMPT_PLACEHOLDER = "<title generation prompt>";

export function clampCommandsPanelDefaultHeightPx(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_COMMANDS_PANEL_HEIGHT_PX;
  }
  return Math.min(
    MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX,
    Math.max(MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX, Math.round(value)),
  );
}

export function clampSidebarDefaultWidthPx(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_SIDEBAR_DEFAULT_WIDTH_PX;
  }
  return Math.min(
    MAX_SIDEBAR_DEFAULT_WIDTH_PX,
    Math.max(MIN_SIDEBAR_DEFAULT_WIDTH_PX, Math.round(value)),
  );
}

export function clampProjectSessionListCollapsedCount(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT;
  }
  return Math.min(
    MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
    Math.max(MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT, Math.round(value)),
  );
}

/**
 * CDXC:Branding 2026-05-12-07:35
 * Public app copy uses Ghostex, and public terminal commands use `ghostex`
 * with `gx` as the short alias. The codebase can keep ghostex in type names,
 * storage/protocol keys, file paths, and implementation identifiers.
 *
 * CDXC:Branding 2026-05-26-15:11
 * New installs should expose `gx` instead of the older `gtx` command, and setup
 * should not claim `gx` when another tool already owns that binary name.
 *
 * CDXC:Branding 2026-05-15-11:54
 * The project rename now applies to source-facing identifiers, docs, scripts,
 * config, release metadata, and native project paths. Preserve each existing
 * casing style while using Ghostex, ghostex, or GHOSTEX consistently.
 */
export type ghostexSettings = {
  actionCompletionSound: CompletionSoundSetting;
  appShotsEnabled: boolean;
  appShotsHotkey: AppShotsHotkey;
  /**
   * CDXC:GxserverAgentSettings 2026-06-02-22:23:
   * This field is the sidebar render cache for gxserver-owned global Accept All
   * settings. Settings UI can display and edit it, but gxserver persists the
   * canonical value and applies each agent's runtime permission-bypass mode.
   *
   * CDXC:GxserverAgentSettings 2026-06-09-14:22:
   * OpenCode Accept All is runtime config rather than a CLI flag, so settings
   * copy and storage must describe the policy without promising flag insertion.
   */
  agentAcceptAllEnabled: boolean;
  agentManagerZoomPercent: number;
  /**
   * CDXC:PromptAgents 2026-05-28-07:15:
   * Automated prompt flows such as Git helper prompts, project board Start Work,
   * and worktree first prompts need one user-selected default agent instead of
   * hardcoding Codex in each launcher.
   */
  defaultPromptAgentId: string;
  /**
   * CDXC:GxserverSessionTitle 2026-06-04-08:24:
   * First-prompt session-title generation is gxserver-owned, but Settings owns
   * which headless agent command should produce those titles. Keep this scoped
   * away from Default Prompt Agent so changing title generation does not alter
   * Git prompts, worktree starts, or project-board prompts.
   *
   * CDXC:GxserverSessionTitle 2026-06-04-22:44:
   * The selector includes Grok Build and its Composer 2.5 command preview, so
   * users can see the exact headless CLI command Ghostex will send before
   * automatic first-prompt session naming runs.
   */
  sessionTitleGenerationAgent: SessionTitleGenerationAgent;
  customSessionTitleGenerationCommand: string;
  browserFeedbackTool: BrowserFeedbackTool;
  browserOpenMode: BrowserOpenMode;
  codeServerLinkVscodeUserConfig: boolean;
  codeServerUseVscodeInsidersUserConfig: boolean;
  customDefaultEditorCommand: string;
  defaultEditorCommand: DefaultEditorCommand;
  hideProjectHeaderDiffStats: boolean;
  showProjectEditorDiffFileCount: boolean;
  showUntrackedProjectDiffWhenNoTrackedChanges: boolean;
  completionBellEnabled: boolean;
  completionSound: CompletionSoundSetting;
  createSessionOnSidebarDoubleClick: boolean;
  debuggingMode: boolean;
  renameSessionOnDoubleClick: boolean;
  hideSessionAgentIconUntilHover: boolean;
  hideBrowserFaviconUntilHover: boolean;
  showCloseButtonOnSessionCards: boolean;
  hideLastActiveTimeOnSessionCards: boolean;
  /**
   * CDXC:SidebarContextMenu 2026-06-10-13:58:
   * The destructive single-session Close context-menu item is advanced chrome.
   * Hide it by default and expose it through an explicit Session Cards setting
   * so context menus stay focused unless users opt into close-from-menu actions.
   */
  showSessionCloseContextMenuAction: boolean;
  /**
   * CDXC:SidebarContextMenu 2026-06-09-23:17:
   * Session context menus should hide Copy resume and Copy attach command by default because they expose raw shell-command utilities. Settings owns a single opt-in that reveals both actions for users who intentionally copy commands into external terminals.
   */
  showSessionCommandCopyActions: boolean;
  /**
   * CDXC:SidebarContextMenu 2026-06-11-23:08:
   * Copy details is an explicit session-card context-menu opt-in. Keep it hidden
   * by default because it copies project/session metadata, including paths and
   * provider ids, into the system clipboard.
   */
  showSessionDetailsCopyAction: boolean;
  /**
   * CDXC:AutoSleep 2026-05-28-08:06:
   * Auto Sleep is a settings-owned policy for retiring idle VS Code, Git,
   * Project, browser, and agent sessions through their native sleep paths.
   * Keep each surface independently configurable so users can preserve existing
   * editor behavior while opting agent terminals in separately.
   */
  autoSleepAgentSessionsEnabled: boolean;
  autoSleepAgentIdleMinutes: AutoSleepIdleMinutes;
  autoSleepBrowserSessionsEnabled: boolean;
  autoSleepBrowserIdleMinutes: AutoSleepIdleMinutes;
  autoSleepCodeEditorEnabled: boolean;
  autoSleepCodeEditorIdleMinutes: AutoSleepIdleMinutes;
  autoSleepGitEditorEnabled: boolean;
  autoSleepGitEditorIdleMinutes: AutoSleepIdleMinutes;
  autoSleepProjectEditorEnabled: boolean;
  autoSleepProjectEditorIdleMinutes: AutoSleepIdleMinutes;
  autoSleepRequireAgentResumeCommand: boolean;
  autoSleepFavoriteAgentSessions: boolean;
  keepAwakeActivateOnExternalDisplay: boolean;
  keepAwakeActivateOnLaunch: boolean;
  keepAwakeAllowDisplaySleep: boolean;
  keepAwakeBatteryThresholdPercent: number;
  keepAwakeDeactivateBelowBatteryThreshold: boolean;
  keepAwakeDeactivateOnLowPowerMode: boolean;
  keepAwakeDeactivateOnUserSwitch: boolean;
  keepAwakeDefaultDurationMinutes: KeepAwakeDurationMinutes;
  keepAwakePreventLidSleep: boolean;
  hideKeepAwakeTitlebarControl: boolean;
  showMacOSAttentionNotifications: boolean;
  hideFloatingSessionStatusIndicators: boolean;
  hideMenuBarSessionStatusIndicators: boolean;
  petOverlayEnabled: boolean;
  selectedPetId: PetId;
  sessionStatusIndicatorSize: SessionStatusIndicatorSize;
  sessionPersistenceProvider: SessionPersistenceProvider;
  showSessionIdInTerminalPanes: boolean;
  sidebarSide: SidebarSide;
  /**
   * CDXC:SidebarChrome 2026-06-05-04:40:
   * The sidebar default width is the reset target for a double-click on the
   * sidebar drag handle in Electron and native macOS. Restart hydration must
   * continue using the last persisted sidebarWidth so changing this default
   * does not erase the user's last manual resize.
   */
  sidebarDefaultWidthPx: number;
  /**
   * CDXC:ProjectSessionLists 2026-06-13-01:06:
   * The project header Show less action keeps a configurable number of project sessions visible. Default to ten visible sessions so active projects stay scannable before switching back to Show more.
   */
  projectSessionListCollapsedCount: number;
  sidebarTheme: SidebarThemeSetting;
  terminalCursorStyle: TerminalCursorStyle;
  terminalCursorStyleBlink: boolean;
  terminalEngine: TerminalEngine;
  terminalFontFamily: string;
  terminalFontSize: number;
  terminalFontWeight: number;
  terminalGhosttyTheme: string;
  terminalLetterSpacing: number;
  terminalLineHeight: number;
  terminalMouseScrollMultiplierDiscrete: number;
  terminalMouseScrollMultiplierPrecision: number;
  tmuxMode: boolean;
  terminalScrollToBottomWhenTyping: boolean;
  terminalScrollbackLimitMb: number;
  terminalCopyOnSelect: GhosttyCopyOnSelect;
  terminalConfirmCloseSurface: GhosttyConfirmCloseSurface;
  terminalClipboardTrimTrailingSpaces: boolean;
  terminalClipboardPasteProtection: boolean;
  /**
   * CDXC:TerminalImagePaste 2026-06-08-13:32:
   * Terminal image paste is app-owned behavior, not a Ghostty config key. Keep a
   * default-on setting so users can opt out of Cmd+V/Ctrl+V converting clipboard
   * images into previewable Markdown links that also render in Cmd-hover terminal
   * previews and the Ctrl+G Rich Prompt Editor.
   */
  terminalPastePreviewableImages: boolean;
  terminalMouseHideWhileTyping: boolean;
  terminalScrollbar: GhosttyScrollbar;
  promptEditorBackend: PromptEditorBackend;
  customPromptEditorCommand: string;
  richPromptEditingWithGte: boolean;
  useGteForCtrlGPromptEditing: boolean;
  hotkeys: ghostexHotkeySettings;
  workspaceActivePaneBorderColor: string;
  workspaceBackgroundColor: string;
  /**
   * CDXC:SleepingPanePlaceholders 2026-06-13-01:44:
   * Sleeping native pane tabs should select their original split pane without
   * starting Ghostty immediately. Keep click-to-wake enabled by default so
   * users can inspect stable black placeholders and wake only by clicking the
   * pane body.
   */
  clickToWakeSleepingSessions: boolean;
  customWorkspaceOpenTargets: CustomWorkspaceOpenTarget[];
  workspaceOpenTargetAvailability: WorkspaceOpenTargetAvailability;
  workspaceOpenTargetHiddenIds: string[];
  workspacePaneGap: number;
  /**
   * CDXC:RemoteMachines 2026-06-02-23:47:
   * Settings owns the saved Remote machine list and its sidebar section order. Each machine requires a user-visible name and SSH host; live connection state, projects, sessions, and gxserver tokens stay outside settings so reconnect/start/install flows refresh from the remote daemon.
   */
  remoteMachines: RemoteMachineSettings[];
  /**
   * CDXC:CommandsPanel 2026-05-30-10:05:
   * Opening the command pane (F12, sidebar button) and double-clicking its top
   * resize rail must restore this pixel height, clamped to the same 5%-90%
   * workspace limits enforced during drag resize.
   */
  commandsPanelDefaultHeightPx: number;
};

export const SIDEBAR_SETTINGS_PRESET_KEYS = [
  "hideSessionAgentIconUntilHover",
  "hideBrowserFaviconUntilHover",
  "showCloseButtonOnSessionCards",
  "hideLastActiveTimeOnSessionCards",
  "hideProjectHeaderDiffStats",
  "showProjectEditorDiffFileCount",
  "hideFloatingSessionStatusIndicators",
  "hideMenuBarSessionStatusIndicators",
] as const satisfies ReadonlyArray<keyof ghostexSettings>;

export type SidebarSettingsPresetKey = (typeof SIDEBAR_SETTINGS_PRESET_KEYS)[number];
export type SidebarSettingsPresetSettings = Pick<ghostexSettings, SidebarSettingsPresetKey>;

/**
 * CDXC:SidebarSettingsPresets 2026-05-16-10:11:
 * The Settings top row exposes Codex, Minimal, Detailed, and Recommended sidebar UI presets as toggle buttons.
 * Preset state is derived from the controlled sidebar settings instead of persisted separately, so manual deviations show Custom without adding another source of truth.
 *
 * CDXC:SidebarSettingsPresets 2026-06-12-07:10:
 * Recommended matches Detailed chrome but keeps session agent icons hover-only so dense sidebars stay readable without losing icon access on demand.
 *
 * CDXC:SidebarSettingsPresets 2026-06-13-01:06:
 * Recommended is the first-run sidebar preset and the leftmost Settings preset button. Defaults should expose detailed sidebar status chrome while keeping agent identity hover-only.
 */
export const SIDEBAR_SETTINGS_PRESET_SETTINGS = {
  codex: {
    hideSessionAgentIconUntilHover: true,
    hideBrowserFaviconUntilHover: false,
    showCloseButtonOnSessionCards: true,
    hideLastActiveTimeOnSessionCards: false,
    hideProjectHeaderDiffStats: true,
    showProjectEditorDiffFileCount: false,
    hideFloatingSessionStatusIndicators: true,
    hideMenuBarSessionStatusIndicators: true,
  },
  minimal: {
    hideSessionAgentIconUntilHover: true,
    hideBrowserFaviconUntilHover: true,
    showCloseButtonOnSessionCards: true,
    hideLastActiveTimeOnSessionCards: true,
    hideProjectHeaderDiffStats: true,
    showProjectEditorDiffFileCount: false,
    hideFloatingSessionStatusIndicators: true,
    hideMenuBarSessionStatusIndicators: true,
  },
  detailed: {
    hideSessionAgentIconUntilHover: false,
    hideBrowserFaviconUntilHover: false,
    showCloseButtonOnSessionCards: false,
    hideLastActiveTimeOnSessionCards: false,
    hideProjectHeaderDiffStats: false,
    showProjectEditorDiffFileCount: false,
    hideFloatingSessionStatusIndicators: false,
    hideMenuBarSessionStatusIndicators: false,
  },
  recommended: {
    hideSessionAgentIconUntilHover: true,
    hideBrowserFaviconUntilHover: false,
    showCloseButtonOnSessionCards: false,
    hideLastActiveTimeOnSessionCards: false,
    hideProjectHeaderDiffStats: false,
    showProjectEditorDiffFileCount: false,
    hideFloatingSessionStatusIndicators: false,
    hideMenuBarSessionStatusIndicators: false,
  },
} as const satisfies Record<SidebarSettingsPresetId, SidebarSettingsPresetSettings>;

export const SIDEBAR_SETTINGS_PRESETS: ReadonlyArray<{
  id: SidebarSettingsPresetId;
  label: string;
  settings: SidebarSettingsPresetSettings;
}> = [
  {
    id: "recommended",
    label: "Recommended",
    settings: SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended,
  },
  { id: "codex", label: "Codex", settings: SIDEBAR_SETTINGS_PRESET_SETTINGS.codex },
  { id: "minimal", label: "Minimal", settings: SIDEBAR_SETTINGS_PRESET_SETTINGS.minimal },
  { id: "detailed", label: "Detailed", settings: SIDEBAR_SETTINGS_PRESET_SETTINGS.detailed },
];

export const DEFAULT_ghostex_SETTINGS: ghostexSettings = {
  /**
   * CDXC:CompletionSounds 2026-05-29-12:00:
   * Action-completion feedback should use the plain shamisen sound by default;
   * shamisen reverb remains available from Settings for users who prefer it.
   */
  actionCompletionSound: "shamisen",
  appShotsEnabled: true,
  appShotsHotkey: "both-command",
  /**
   * CDXC:GxserverAgentSettings 2026-06-02-22:23:
   * New installs should start with gxserver-owned Accept All enabled so built-in
   * and custom agent launches inherit permission-bypass mode unless the user
   * turns it off.
   */
  agentAcceptAllEnabled: true,
  agentManagerZoomPercent: DEFAULT_AGENT_MANAGER_ZOOM_PERCENT,
  defaultPromptAgentId: "codex",
  sessionTitleGenerationAgent: "codex",
  customSessionTitleGenerationCommand: "",
  /**
   * CDXC:BrowserFeedbackTools 2026-05-22-09:18:
   * Browser panes can inject either React Grab or Agentation for visual
   * feedback.
   *
   * CDXC:BrowserFeedbackTools 2026-05-22-09:18:
   * Agentation is the default browser feedback tool so browser panes open the
   * structured annotation workflow unless a user explicitly switches back to
   * React Grab in Settings.
   */
  browserFeedbackTool: "agentation",
  /**
   * CDXC:BrowserPanes 2026-05-27-07:24
   * Browser actions should no longer expose or route through Chrome Canary attachment.
   * Normalize all browser-action launches to in-workspace browser panes so Settings and native startup do not preserve the old external Canary path.
   */
  browserOpenMode: "browser-pane",
  /**
   * CDXC:EditorPanes 2026-05-06-15:00
   * Embedded code-server editor panes can reuse the user's local VS Code
   * user settings. A separate Insiders toggle switches the linked source
   * directory without disabling the shared project editor runtime.
   *
   * CDXC:EditorPanes 2026-06-08-20:12:
   * New installs should use Ghostex-owned bundled editor settings by default
   * so the embedded VS Code surface starts on Dark 2026. Users can still opt
   * into local VS Code settings explicitly from Settings.
   */
  codeServerLinkVscodeUserConfig: false,
  codeServerUseVscodeInsidersUserConfig: false,
  /**
   * CDXC:AgentsHub 2026-05-12-09:22
   * Agents Hub file-edit actions should use one Settings-owned editor command.
   * Start with VS Code because its `code <file>` command is the most common
   * cross-project default, while Settings exposes Zed, Cursor, and custom
   * commands for users who prefer a different editor.
   */
  customDefaultEditorCommand: "",
  defaultEditorCommand: "code",
  /**
   * CDXC:ProjectDiffStats 2026-05-16-08:46:
   * Users can hide the project-header +added/-removed git summary completely
   * when they want project names to stay visually quiet. This is independent
   * from the existing changed-file count preference.
   *
   * CDXC:SidebarSettingsPresets 2026-06-13-01:06:
   * Recommended is the default sidebar preset, so new settings show project-header
   * git stats while keeping the changed-file count off unless the user enables it.
   */
  hideProjectHeaderDiffStats:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideProjectHeaderDiffStats,
  /**
   * CDXC:ProjectDiffStats 2026-05-15-14:33:
   * Project-header git stats should hide the changed-file count by default and
   * show only added/removed line counts. Users can opt back into the file
   * number from Settings when they want the full diff summary.
   */
  showProjectEditorDiffFileCount:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.codex.showProjectEditorDiffFileCount,
  /**
   * CDXC:ProjectDiffStats 2026-05-27-09:25:
   * Match Starship-style tracked line counts by default. Users can opt in to
   * show untracked line totals only when tracked `git diff --numstat HEAD` is
   * +0 -0.
   */
  showUntrackedProjectDiffWhenNoTrackedChanges: false,
  /**
   * CDXC:CompletionSounds 2026-05-29-12:00:
   * The completion bell should be enabled by default so finished agent work is
   * audible without requiring users to discover the Sounds setting first.
   */
  completionBellEnabled: true,
  completionSound: DEFAULT_COMPLETION_SOUND,
  createSessionOnSidebarDoubleClick: false,
  debuggingMode: false,
  renameSessionOnDoubleClick: false,
  /**
   * CDXC:SidebarSessions 2026-05-16-08:46:
   * Agent identity remains configurable in Settings through an explicit
   * hover-only mode for quieter session lists.
   *
   * CDXC:SidebarSettingsPresets 2026-06-13-01:06:
   * Recommended is the first-run preset and keeps session agent icons hover-only
   * while showing detailed sidebar status chrome.
   */
  hideSessionAgentIconUntilHover:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideSessionAgentIconUntilHover,
  /**
   * CDXC:BrowserPanes 2026-05-28-07:38:
   * Browser page favicons are page identity, not agent chrome. Keep them
   * visible in the default Codex and Detailed presets even when agent icons are
   * hover-only, while Minimal can hide favicons until hover for a quieter list.
   */
  hideBrowserFaviconUntilHover:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideBrowserFaviconUntilHover,
  /**
   * CDXC:SidebarSessions 2026-05-09-17:00
   * Session-card close controls should be available out of the box. Users can
   * still turn the hover chrome off from Settings when they want quieter cards.
   */
  showCloseButtonOnSessionCards:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.showCloseButtonOnSessionCards,
  /**
   * CDXC:SidebarSessions 2026-05-15-08:57
   * Session-card Last Active timestamps stay visible by default for existing
   * users, but Settings owns an explicit hide toggle for quieter title rows.
   * This setting applies only to session-card timestamps and must not affect
   * project-header git diff stats.
   */
  hideLastActiveTimeOnSessionCards:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideLastActiveTimeOnSessionCards,
  showSessionCloseContextMenuAction: false,
  showSessionCommandCopyActions: false,
  showSessionDetailsCopyAction: false,
  /**
   * CDXC:AutoSleep 2026-05-28-08:06:
   * Background VS Code, Project, and Git panes auto-sleep after fifteen minutes
   * of idle time by default. Browser and agent auto-sleep start opt-in because
   * they close live user-created session surfaces.
   *
   * CDXC:AutoSleep 2026-06-07-00:53:
   * Agent auto-sleep keeps its opt-in policy, but the default idle threshold is
   * now fifteen minutes so enabled agent sessions retire on the same window as
   * editor surfaces.
   *
   * CDXC:AutoSleep 2026-06-07-00:56:
   * Focused agent sessions must never auto-sleep and no longer have a Settings
   * override because sleeping the active conversation is not a supported UX.
   */
  autoSleepAgentSessionsEnabled: false,
  autoSleepAgentIdleMinutes: 15,
  autoSleepBrowserSessionsEnabled: false,
  autoSleepBrowserIdleMinutes: 30,
  autoSleepCodeEditorEnabled: true,
  autoSleepCodeEditorIdleMinutes: 15,
  autoSleepGitEditorEnabled: true,
  autoSleepGitEditorIdleMinutes: 15,
  autoSleepProjectEditorEnabled: true,
  autoSleepProjectEditorIdleMinutes: 15,
  autoSleepRequireAgentResumeCommand: true,
  autoSleepFavoriteAgentSessions: false,
  keepAwakeActivateOnExternalDisplay: false,
  keepAwakeActivateOnLaunch: false,
  keepAwakeAllowDisplaySleep: false,
  keepAwakeBatteryThresholdPercent: 20,
  keepAwakeDeactivateBelowBatteryThreshold: false,
  keepAwakeDeactivateOnLowPowerMode: false,
  keepAwakeDeactivateOnUserSwitch: false,
  keepAwakeDefaultDurationMinutes: 0,
  /**
   * CDXC:TitlebarKeepAwake 2026-05-28-19:28:
   * Closing a MacBook lid is not covered by the standard caffeinate idle-sleep assertion.
   * Keep lid-close sleep prevention as an explicit opt-in because it changes the system-wide `pmset disablesleep` policy with administrator approval.
   */
  keepAwakePreventLidSleep: false,
  /**
   * CDXC:TitlebarKeepAwake 2026-05-27-07:32:
   * The titlebar keep-awake affordance is optional chrome. Keep it visible by
   * default, but persist a Power setting that can remove the titlebar control
   * completely for users who do not use Mac sleep management from Ghostex.
   */
  hideKeepAwakeTitlebarControl: false,
  /**
   * CDXC:SessionAttentionNotifications 2026-05-10-16:46
   * macOS attention notifications are enabled by default so a background
   * session that transitions into attention can surface itself without relying
   * on persistent status badges or completion sounds.
   *
   * CDXC:SessionAttentionNotifications 2026-05-11-01:14
   * Keep this default-on even after adding macOS permission prompts and test
   * controls; users should opt out explicitly when they do not want banners.
   */
  showMacOSAttentionNotifications: true,
  /**
   * CDXC:SessionStatusIndicators 2026-05-09-17:30
   * Floating and menu bar desktop status badges are visible under the default
   * Recommended preset. Keep separate hide toggles so users can hide either
   * surface without coupling their visibility.
   */
  hideFloatingSessionStatusIndicators:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideFloatingSessionStatusIndicators,
  hideMenuBarSessionStatusIndicators:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideMenuBarSessionStatusIndicators,
  petOverlayEnabled: false,
  selectedPetId: DEFAULT_PET_ID,
  /**
   * CDXC:SessionStatusIndicators 2026-05-07-18:20
   * The AppKit floating session indicator defaults to Medium, which is half of
   * the approved X-Large visual size. Persist the named size now so Settings
   * can later tune the same scalable drawing metrics without changing native
   * command shape again.
   */
  sessionStatusIndicatorSize: "medium",
  /**
   * CDXC:SessionPersistence 2026-05-05-07:28
   * Terminal persistence is provider-selected. Off preserves the direct
   * Ghostty launch path; tmux, zmx, and zellij wrap new terminal/agent
   * sessions in a named persistence session so app restart can reattach or
   * recreate+resume.
   *
   * CDXC:SessionPersistence 2026-05-06-03:43
   * zellij uses the same durable session name contract as tmux/zmx for restart
   * attach and missing-session recreate+resume behavior even when hidden from
   * the current Settings dropdown.
   *
   * CDXC:SessionPersistence 2026-05-26-13:41:
   * New installs should start with zmx persistence enabled by default because zmx is the recommended provider for continuing Ghostex-created sessions from other devices.
   */
  sessionPersistenceProvider: "zmx",
  /**
   * CDXC:SessionPersistence 2026-05-23-00:50:
   * The session-id pane overlay preference is configurable, and the
   * native label itself must still render only for terminal panes that carry
   * zmx/tmux/zellij persistence metadata.
   *
   * CDXC:SessionPersistence 2026-06-06-05:47:
   * Provider session ids in terminal panes are opt-in chrome. Keep the setting
   * disabled for default settings so new users do not see top-right provider
   * identifiers unless they explicitly enable the pane overlay.
   */
  showSessionIdInTerminalPanes: false,
  /**
   * CDXC:SidebarPlacement 2026-05-06-17:32
   * Sidebar side is a first-class setting so users can choose left or right
   * placement from Settings instead of relying on sidebar placement shortcuts.
   *
   * CDXC:SidebarCollapse 2026-06-12-02:23:
   * Cmd+B is reserved for complete sidebar collapse, so sidebar side placement
   * should remain an explicit setting or user-assigned command.
   */
  sidebarSide: "left",
  /**
   * CDXC:SidebarChrome 2026-06-05-04:40:
   * First-run reset target remains 235px, but users can change this Settings
   * value for explicit sidebar-handle double-click resets without changing the
   * last-width restore path used at app restart.
   */
  sidebarDefaultWidthPx: DEFAULT_SIDEBAR_DEFAULT_WIDTH_PX,
  projectSessionListCollapsedCount: DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  /**
   * CDXC:SidebarTheme 2026-05-08-11:14
   * Dark Gray is the only active user-facing sidebar theme while the broader
   * theme picker is hidden. New installs must start on the persisted "plain"
   * value so the resolved chrome is the dark gray palette immediately.
   */
  sidebarTheme: "plain",
  /**
   * CDXC:GhosttyDefaults 2026-05-22-12:29:
   * New Ghostex terminals should default to the requested GitHub Dark terminal
   * profile: JetBrains Mono 13pt, bar cursor with blink, wght=300, 20% cell
   * height expansion, 15 MB scrollback, no copy-on-select, and one-to-one
   * precision/discrete mouse scrolling.
   */
  terminalCursorStyle: "bar",
  terminalCursorStyleBlink: true,
  terminalEngine: "ghostty-native",
  terminalFontFamily: "JetBrains Mono",
  terminalFontSize: 13,
  terminalFontWeight: 300,
  terminalGhosttyTheme: "GitHub Dark",
  terminalLetterSpacing: 0,
  terminalLineHeight: 1.2,
  terminalMouseScrollMultiplierDiscrete: 1,
  terminalMouseScrollMultiplierPrecision: 1,
  /**
   * CDXC:SessionPersistence 2026-05-05-07:28
   * tmuxMode remains as a compatibility mirror for older persisted settings and
   * legacy UI code. New launch behavior reads sessionPersistenceProvider so
   * zmx and zellij can follow the same persistence semantics as tmux.
   */
  tmuxMode: false,
  terminalScrollToBottomWhenTyping: true,
  terminalScrollbackLimitMb: 15,
  terminalCopyOnSelect: "false",
  terminalConfirmCloseSurface: "true",
  terminalClipboardTrimTrailingSpaces: true,
  terminalClipboardPasteProtection: true,
  terminalPastePreviewableImages: true,
  terminalMouseHideWhileTyping: false,
  terminalScrollbar: "system",
  /**
   * CDXC:PromptEditorBackend 2026-05-13-15:58
   * Ctrl+G rich prompt editing originally defaulted to the floating Monaco editor. Preserve explicit gte choices, but keep new and invalid settings on the current built-in backend.
   *
   * CDXC:PromptEditorBackend 2026-05-22-09:56
   * The terminal prompt editor is named gte for Ghostex Terminal Editor. Settings, launch commands, and install copy must use gte consistently across the app.
   *
   * CDXC:PromptEditorBackend 2026-05-22-10:16
   * Monaco is popup-backed, but gte is terminal-native. A gte backend selection must resolve to the plain `gte` command so Ctrl+G edits inside the terminal that launched the editor.
   *
   * CDXC:PromptEditorBackend 2026-05-25-11:31:
   * Monaco is the out-of-the-box Ctrl+G prompt editor again. New installs should open the floating Monaco editor for local app terminals, while the native runtime resolves Monaco-over-SSH to gte because remote terminals cannot use the local overlay.
   */
  promptEditorBackend: "monaco",
  customPromptEditorCommand: "code --wait",
  /**
   * CDXC:GtePromptEditing 2026-05-22-09:56
   * The boolean mirrors keep the Ctrl+G prompt-editor setting easy to search while promptEditorBackend remains the source of truth for launch behavior.
   *
   * CDXC:GtePromptEditing 2026-05-25-11:31:
   * First-run settings mirror the default Monaco backend so older call sites that still read these booleans do not enable gte unless Settings or legacy persisted keys explicitly request it.
   */
  richPromptEditingWithGte: false,
  useGteForCtrlGPromptEditing: false,
  hotkeys: DEFAULT_ghostex_HOTKEYS,
  workspaceActivePaneBorderColor: "#3b82f6",
  /**
   * CDXC:WorkspaceLayout 2026-06-07-16:53:
   * Black is the fallback workspace background when Ghostty has no readable terminal background. Native layout sync treats this default as automatic so the macOS workarea can use the loaded Ghostty `background` color instead of forcing a separate app gray.
   */
  workspaceBackgroundColor: "#000000",
  clickToWakeSleepingSessions: true,
  /**
   * CDXC:TitlebarOpenIn 2026-05-11-00:22
   * The titlebar Open In menu is configurable: built-in editor targets can be
   * hidden and user-defined command targets can be appended without changing
   * the t3code-derived default editor catalog.
   */
  customWorkspaceOpenTargets: [],
  /**
   * CDXC:TitlebarOpenIn 2026-05-11-02:03
   * First launch starts with only ghostex/Open Folder until the native sidebar performs
   * its one startup installed-target scan and persists the detected IDE list.
   *
   * CDXC:TitlebarOpenIn 2026-06-04-13:39:
   * The default folder target should be described with OS-agnostic Open Folder copy even though the persisted target id remains finder for compatibility.
   */
  workspaceOpenTargetAvailability: DEFAULT_WORKSPACE_OPEN_TARGET_AVAILABILITY,
  workspaceOpenTargetHiddenIds: [],
  /**
   * CDXC:WorkspaceLayout 2026-05-30-07:24:
   * The macOS app no longer exposes Pane Gap as a user setting. Keep the
   * persisted field for settings compatibility, but normalize it to zero so
   * native panes always render without configurable spacing.
   */
  workspacePaneGap: 0,
  remoteMachines: [],
  commandsPanelDefaultHeightPx: DEFAULT_COMMANDS_PANEL_HEIGHT_PX,
};

export const SIDEBAR_THEME_SETTING_OPTIONS: ReadonlyArray<{
  label: string;
  value: SidebarThemeSetting;
}> = [
  /**
   * CDXC:SidebarTheme 2026-05-08-11:14
   * Hide Auto and the other theme presets from Settings until theme selection
   * returns. Keep only Dark Gray visible so the UI matches the active default.
   *
   * CDXC:SidebarTheme 2026-04-26-21:32: Keep the persisted value as "plain"
   * for compatibility, but present it as Dark Gray because the option now
   * always selects the dark gray sidebar palette.
   */
  { label: "Dark Gray", value: "plain" },
];

export const TERMINAL_ENGINE_SETTING_OPTIONS: ReadonlyArray<{
  label: string;
  value: TerminalEngine;
}> = [{ label: "Ghostty Native", value: "ghostty-native" }];

export const BROWSER_OPEN_MODE_OPTIONS: ReadonlyArray<{
  label: string;
  value: BrowserOpenMode;
}> = [{ label: "Browser Panes", value: "browser-pane" }];

export const BROWSER_FEEDBACK_TOOL_OPTIONS: ReadonlyArray<{
  label: string;
  value: BrowserFeedbackTool;
}> = [
  { label: "React Grab", value: "react-grab" },
  { label: "Agentation", value: "agentation" },
];

export const APP_SHOTS_HOTKEY_OPTIONS: ReadonlyArray<{
  label: string;
  value: AppShotsHotkey;
}> = [
  { label: "Both Command keys", value: "both-command" },
  { label: "Double-tap Left Shift", value: "double-left-shift" },
  { label: "Double-tap Left Option", value: "double-left-option" },
];

export const DEFAULT_EDITOR_COMMAND_OPTIONS: ReadonlyArray<{
  label: string;
  value: DefaultEditorCommand;
}> = [
  { label: "VS Code (code)", value: "code" },
  { label: "VS Code Insiders (code-insiders)", value: "code-insiders" },
  { label: "Zed (zed)", value: "zed" },
  { label: "Zed alternate (zeditor)", value: "zeditor" },
  { label: "Cursor (cursor)", value: "cursor" },
  { label: "Windsurf (windsurf)", value: "windsurf" },
  { label: "VSCodium (codium)", value: "codium" },
  { label: "Sublime Text (subl)", value: "subl" },
  { label: "Other", value: "other" },
];

export const SESSION_PERSISTENCE_PROVIDER_OPTIONS: ReadonlyArray<{
  label: string;
  value: SessionPersistenceProvider;
}> = [
  /**
   * CDXC:SessionPersistence 2026-05-26-13:41:
   * Settings should recommend zmx and keep tmux/zellij out of the provider dropdown while code still accepts those persisted providers for existing sessions and internal launch paths.
   */
  { label: "Off", value: "off" },
  { label: "zmx (recommended)", value: "zmx" },
];

export const SIDEBAR_SIDE_OPTIONS: ReadonlyArray<{
  label: string;
  value: SidebarSide;
}> = [
  { label: "Left", value: "left" },
  { label: "Right", value: "right" },
];

export const SESSION_STATUS_INDICATOR_SIZE_OPTIONS: ReadonlyArray<{
  label: string;
  value: SessionStatusIndicatorSize;
}> = [
  { label: "X-Large", value: "x-large" },
  { label: "Large", value: "large" },
  { label: "Medium", value: "medium" },
  { label: "Small", value: "small" },
];

export const KEEP_AWAKE_DURATION_OPTIONS: ReadonlyArray<{
  label: string;
  value: KeepAwakeDurationMinutes;
}> = [
  /**
   * CDXC:TitlebarKeepAwake 2026-05-28-19:28:
   * The keep-awake menu should stay intentionally small: indefinite, two hours,
   * five hours, and the runtime Allow Sleep Now action are the complete user-facing duration set.
   */
  { label: "", value: 0 },
  { label: "2 hours", value: 120 },
  { label: "5 hours", value: 300 },
];

export const AUTO_SLEEP_IDLE_MINUTE_OPTIONS: ReadonlyArray<{
  label: string;
  value: AutoSleepIdleMinutes;
}> = [
  { label: "5 minutes", value: 5 },
  { label: "10 minutes", value: 10 },
  { label: "15 minutes", value: 15 },
  { label: "30 minutes", value: 30 },
  { label: "1 hour", value: 60 },
  { label: "2 hours", value: 120 },
  { label: "5 hours", value: 300 },
];

export const GHOSTTY_COPY_ON_SELECT_OPTIONS: ReadonlyArray<{
  label: string;
  value: GhosttyCopyOnSelect;
}> = [
  { label: "Off", value: "false" },
  { label: "Selection clipboard", value: "true" },
  { label: "System and selection clipboard", value: "clipboard" },
];

export const GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS: ReadonlyArray<{
  label: string;
  value: GhosttyConfirmCloseSurface;
}> = [
  { label: "Smart confirmation", value: "true" },
  { label: "Always confirm", value: "always" },
  { label: "Do not confirm", value: "false" },
];

export const GHOSTTY_SCROLLBAR_OPTIONS: ReadonlyArray<{
  label: string;
  value: GhosttyScrollbar;
}> = [
  { label: "System", value: "system" },
  { label: "Never", value: "never" },
];

export const PROMPT_EDITOR_BACKEND_OPTIONS: ReadonlyArray<{
  label: string;
  value: PromptEditorBackend;
}> = [
  { label: "Inherit from system", value: "inherit" },
  { label: "Monaco floating editor", value: "monaco" },
  { label: "gte terminal editor", value: "gte" },
  { label: "Custom", value: "custom" },
];

export const GHOSTTY_THEME_SETTING_OPTIONS: ReadonlyArray<{
  label: string;
  value: string;
}> = [
  /**
   * CDXC:TerminalThemeSettings 2026-04-29-09:32
   * Users may already manage Ghostty themes directly in their Ghostty config.
   * The sentinel value lets ghostex leave any existing `theme` line untouched
   * until the user deliberately chooses a bundled theme from this modal.
  */
  { label: "Use existing Ghostty config", value: "__ghostex_ghostty_theme_unmanaged__" },
  ...GHOSTTY_THEME_OPTIONS.map((theme) => ({ label: theme, value: theme })),
];

export function normalizeghostexSettings(candidate: unknown): ghostexSettings {
  const source = isRecord(candidate) ? candidate : {};
  const promptEditorBackend = normalizePromptEditorBackend(source);
  const sessionPersistenceProvider = normalizeSessionPersistenceProvider(
    readString(
      source,
      "sessionPersistenceProvider",
      readBoolean(source, "tmuxMode", DEFAULT_ghostex_SETTINGS.tmuxMode)
        ? "tmux"
        : DEFAULT_ghostex_SETTINGS.sessionPersistenceProvider,
    ),
  );
  return {
    actionCompletionSound: clampCompletionSoundSetting(
      readString(source, "actionCompletionSound", DEFAULT_ghostex_SETTINGS.actionCompletionSound),
    ),
    appShotsEnabled: readBoolean(
      source,
      "appShotsEnabled",
      DEFAULT_ghostex_SETTINGS.appShotsEnabled,
    ),
    appShotsHotkey: normalizeAppShotsHotkey(
      readString(source, "appShotsHotkey", DEFAULT_ghostex_SETTINGS.appShotsHotkey),
    ),
    agentAcceptAllEnabled: readBoolean(
      source,
      "agentAcceptAllEnabled",
      DEFAULT_ghostex_SETTINGS.agentAcceptAllEnabled,
    ),
    agentManagerZoomPercent: clampAgentManagerZoomPercent(
      readNumber(source, "agentManagerZoomPercent", DEFAULT_ghostex_SETTINGS.agentManagerZoomPercent),
    ),
    /**
     * CDXC:PromptAgents 2026-05-28-07:15:
     * Keep the selected default prompt agent as a plain agent id so built-in,
     * reordered, hidden-restored, and custom agents can all be selected without
     * coupling settings normalization to the runtime agent registry.
     */
    defaultPromptAgentId: normalizeDefaultPromptAgentId(
      readString(source, "defaultPromptAgentId", DEFAULT_ghostex_SETTINGS.defaultPromptAgentId),
    ),
    sessionTitleGenerationAgent: normalizeSessionTitleGenerationAgent(
      readString(
        source,
        "sessionTitleGenerationAgent",
        DEFAULT_ghostex_SETTINGS.sessionTitleGenerationAgent,
      ),
    ),
    customSessionTitleGenerationCommand: normalizeCustomSessionTitleGenerationCommand(
      readString(
        source,
        "customSessionTitleGenerationCommand",
        DEFAULT_ghostex_SETTINGS.customSessionTitleGenerationCommand,
      ),
    ),
    /**
     * CDXC:BrowserFeedbackTools 2026-05-22-09:18:
     * Normalize the browser feedback injector choice so missing or invalid
     * settings use Agentation, while explicit React Grab selections continue
     * to launch the legacy injector.
     */
    browserFeedbackTool: normalizeBrowserFeedbackTool(
      readString(source, "browserFeedbackTool", DEFAULT_ghostex_SETTINGS.browserFeedbackTool),
    ),
    /**
     * CDXC:BrowserPanes 2026-05-27-07:24
     * Existing settings files may still contain the deleted Chrome Canary value.
     * Treat every stored value as Browser Panes so the old attachment route cannot reappear after reload.
     */
    browserOpenMode: normalizeBrowserOpenMode(
      readString(source, "browserOpenMode", DEFAULT_ghostex_SETTINGS.browserOpenMode),
    ),
    /**
     * CDXC:EditorPanes 2026-06-08-20:12:
     * Normalize the code-server VS Code settings-link toggles on every read so
     * missing values use the bundled editor defaults while explicit local VS
     * Code settings choices remain persisted.
     */
    codeServerLinkVscodeUserConfig: readBoolean(
      source,
      "codeServerLinkVscodeUserConfig",
      DEFAULT_ghostex_SETTINGS.codeServerLinkVscodeUserConfig,
    ),
    codeServerUseVscodeInsidersUserConfig: readBoolean(
      source,
      "codeServerUseVscodeInsidersUserConfig",
      DEFAULT_ghostex_SETTINGS.codeServerUseVscodeInsidersUserConfig,
    ),
    defaultEditorCommand: normalizeDefaultEditorCommand(
      readString(source, "defaultEditorCommand", DEFAULT_ghostex_SETTINGS.defaultEditorCommand),
    ),
    customDefaultEditorCommand: normalizeCustomDefaultEditorCommand(
      readString(
        source,
        "customDefaultEditorCommand",
        DEFAULT_ghostex_SETTINGS.customDefaultEditorCommand,
      ),
    ),
    /**
     * CDXC:ProjectDiffStats 2026-05-16-08:46:
     * Missing project-header visibility now follows the Codex preset, which
     * hides git line deltas unless the user selects Detailed or changes this
     * setting directly.
     */
    hideProjectHeaderDiffStats: readBoolean(
      source,
      "hideProjectHeaderDiffStats",
      DEFAULT_ghostex_SETTINGS.hideProjectHeaderDiffStats,
    ),
    /**
     * CDXC:ProjectDiffStats 2026-05-15-14:33:
     * Missing or invalid older settings must keep project-header git stats in
     * the quieter default that hides the changed-file count.
     */
    showProjectEditorDiffFileCount: readBoolean(
      source,
      "showProjectEditorDiffFileCount",
      DEFAULT_ghostex_SETTINGS.showProjectEditorDiffFileCount,
    ),
    showUntrackedProjectDiffWhenNoTrackedChanges: readBoolean(
      source,
      "showUntrackedProjectDiffWhenNoTrackedChanges",
      DEFAULT_ghostex_SETTINGS.showUntrackedProjectDiffWhenNoTrackedChanges,
    ),
    completionBellEnabled: readBoolean(
      source,
      "completionBellEnabled",
      DEFAULT_ghostex_SETTINGS.completionBellEnabled,
    ),
    completionSound: clampCompletionSoundSetting(
      readString(source, "completionSound", DEFAULT_ghostex_SETTINGS.completionSound),
    ),
    createSessionOnSidebarDoubleClick: readBoolean(
      source,
      "createSessionOnSidebarDoubleClick",
      DEFAULT_ghostex_SETTINGS.createSessionOnSidebarDoubleClick,
    ),
    debuggingMode: readBoolean(source, "debuggingMode", DEFAULT_ghostex_SETTINGS.debuggingMode),
    renameSessionOnDoubleClick: readBoolean(
      source,
      "renameSessionOnDoubleClick",
      DEFAULT_ghostex_SETTINGS.renameSessionOnDoubleClick,
    ),
    /**
     * CDXC:SidebarSessions 2026-05-16-08:46:
     * Missing session-card icon visibility now follows the Codex preset, which
     * hides agent icons until hover unless the user selects Detailed or changes
     * this setting directly.
     */
    hideSessionAgentIconUntilHover: readBoolean(
      source,
      "hideSessionAgentIconUntilHover",
      DEFAULT_ghostex_SETTINGS.hideSessionAgentIconUntilHover,
    ),
    /**
     * CDXC:BrowserPanes 2026-05-28-07:38:
     * Missing browser-favicon visibility should follow the sidebar preset
     * independently from the older agent-icon hover-only setting so browser
     * page identity does not disappear just because agent logos are quiet.
     */
    hideBrowserFaviconUntilHover: readBoolean(
      source,
      "hideBrowserFaviconUntilHover",
      DEFAULT_ghostex_SETTINGS.hideBrowserFaviconUntilHover,
    ),
    showCloseButtonOnSessionCards: readBoolean(
      source,
      "showCloseButtonOnSessionCards",
      DEFAULT_ghostex_SETTINGS.showCloseButtonOnSessionCards,
    ),
    /**
     * CDXC:SidebarSessions 2026-05-15-08:57
     * Older settings files should preserve the current session-card timestamp
     * behavior. Explicit true hides only the Last Active label, not the code
     * project header's separate git additions/deletions summary.
     */
    hideLastActiveTimeOnSessionCards: readBoolean(
      source,
      "hideLastActiveTimeOnSessionCards",
      DEFAULT_ghostex_SETTINGS.hideLastActiveTimeOnSessionCards,
    ),
    showSessionCloseContextMenuAction: readBoolean(
      source,
      "showSessionCloseContextMenuAction",
      DEFAULT_ghostex_SETTINGS.showSessionCloseContextMenuAction,
    ),
    showSessionCommandCopyActions: readBoolean(
      source,
      "showSessionCommandCopyActions",
      DEFAULT_ghostex_SETTINGS.showSessionCommandCopyActions,
    ),
    showSessionDetailsCopyAction: readBoolean(
      source,
      "showSessionDetailsCopyAction",
      DEFAULT_ghostex_SETTINGS.showSessionDetailsCopyAction,
    ),
    /**
     * CDXC:AutoSleep 2026-05-28-08:06:
     * Normalize Auto Sleep policy independently from keep-awake so Mac power
     * assertions and Ghostex session retirement can be configured separately.
     */
    autoSleepAgentSessionsEnabled: readBoolean(
      source,
      "autoSleepAgentSessionsEnabled",
      DEFAULT_ghostex_SETTINGS.autoSleepAgentSessionsEnabled,
    ),
    autoSleepAgentIdleMinutes: normalizeAutoSleepIdleMinutes(
      readNumber(
        source,
        "autoSleepAgentIdleMinutes",
        DEFAULT_ghostex_SETTINGS.autoSleepAgentIdleMinutes,
      ),
      DEFAULT_ghostex_SETTINGS.autoSleepAgentIdleMinutes,
    ),
    autoSleepBrowserSessionsEnabled: readBoolean(
      source,
      "autoSleepBrowserSessionsEnabled",
      DEFAULT_ghostex_SETTINGS.autoSleepBrowserSessionsEnabled,
    ),
    autoSleepBrowserIdleMinutes: normalizeAutoSleepIdleMinutes(
      readNumber(
        source,
        "autoSleepBrowserIdleMinutes",
        DEFAULT_ghostex_SETTINGS.autoSleepBrowserIdleMinutes,
      ),
      DEFAULT_ghostex_SETTINGS.autoSleepBrowserIdleMinutes,
    ),
    autoSleepCodeEditorEnabled: readBoolean(
      source,
      "autoSleepCodeEditorEnabled",
      DEFAULT_ghostex_SETTINGS.autoSleepCodeEditorEnabled,
    ),
    autoSleepCodeEditorIdleMinutes: normalizeAutoSleepIdleMinutes(
      readNumber(
        source,
        "autoSleepCodeEditorIdleMinutes",
        DEFAULT_ghostex_SETTINGS.autoSleepCodeEditorIdleMinutes,
      ),
      DEFAULT_ghostex_SETTINGS.autoSleepCodeEditorIdleMinutes,
    ),
    autoSleepGitEditorEnabled: readBoolean(
      source,
      "autoSleepGitEditorEnabled",
      DEFAULT_ghostex_SETTINGS.autoSleepGitEditorEnabled,
    ),
    autoSleepGitEditorIdleMinutes: normalizeAutoSleepIdleMinutes(
      readNumber(
        source,
        "autoSleepGitEditorIdleMinutes",
        DEFAULT_ghostex_SETTINGS.autoSleepGitEditorIdleMinutes,
      ),
      DEFAULT_ghostex_SETTINGS.autoSleepGitEditorIdleMinutes,
    ),
    autoSleepProjectEditorEnabled: readBoolean(
      source,
      "autoSleepProjectEditorEnabled",
      DEFAULT_ghostex_SETTINGS.autoSleepProjectEditorEnabled,
    ),
    autoSleepProjectEditorIdleMinutes: normalizeAutoSleepIdleMinutes(
      readNumber(
        source,
        "autoSleepProjectEditorIdleMinutes",
        DEFAULT_ghostex_SETTINGS.autoSleepProjectEditorIdleMinutes,
      ),
      DEFAULT_ghostex_SETTINGS.autoSleepProjectEditorIdleMinutes,
    ),
    autoSleepRequireAgentResumeCommand: readBoolean(
      source,
      "autoSleepRequireAgentResumeCommand",
      DEFAULT_ghostex_SETTINGS.autoSleepRequireAgentResumeCommand,
    ),
    autoSleepFavoriteAgentSessions: readBoolean(
      source,
      "autoSleepFavoriteAgentSessions",
      DEFAULT_ghostex_SETTINGS.autoSleepFavoriteAgentSessions,
    ),
    keepAwakeActivateOnExternalDisplay: readBoolean(
      source,
      "keepAwakeActivateOnExternalDisplay",
      DEFAULT_ghostex_SETTINGS.keepAwakeActivateOnExternalDisplay,
    ),
    keepAwakeActivateOnLaunch: readBoolean(
      source,
      "keepAwakeActivateOnLaunch",
      DEFAULT_ghostex_SETTINGS.keepAwakeActivateOnLaunch,
    ),
    keepAwakeAllowDisplaySleep: readBoolean(
      source,
      "keepAwakeAllowDisplaySleep",
      DEFAULT_ghostex_SETTINGS.keepAwakeAllowDisplaySleep,
    ),
    keepAwakeBatteryThresholdPercent: clampNumber(
      readNumber(
        source,
        "keepAwakeBatteryThresholdPercent",
        DEFAULT_ghostex_SETTINGS.keepAwakeBatteryThresholdPercent,
      ),
      10,
      90,
      DEFAULT_ghostex_SETTINGS.keepAwakeBatteryThresholdPercent,
    ),
    keepAwakeDeactivateBelowBatteryThreshold: readBoolean(
      source,
      "keepAwakeDeactivateBelowBatteryThreshold",
      DEFAULT_ghostex_SETTINGS.keepAwakeDeactivateBelowBatteryThreshold,
    ),
    keepAwakeDeactivateOnLowPowerMode: readBoolean(
      source,
      "keepAwakeDeactivateOnLowPowerMode",
      DEFAULT_ghostex_SETTINGS.keepAwakeDeactivateOnLowPowerMode,
    ),
    keepAwakeDeactivateOnUserSwitch: readBoolean(
      source,
      "keepAwakeDeactivateOnUserSwitch",
      DEFAULT_ghostex_SETTINGS.keepAwakeDeactivateOnUserSwitch,
    ),
    keepAwakeDefaultDurationMinutes: normalizeKeepAwakeDurationMinutes(
      readNumber(
        source,
        "keepAwakeDefaultDurationMinutes",
        DEFAULT_ghostex_SETTINGS.keepAwakeDefaultDurationMinutes,
      ),
    ),
    keepAwakePreventLidSleep: readBoolean(
      source,
      "keepAwakePreventLidSleep",
      DEFAULT_ghostex_SETTINGS.keepAwakePreventLidSleep,
    ),
    /**
     * CDXC:TitlebarKeepAwake 2026-05-27-07:32:
     * Normalize the hide preference independently from the caffeinate rules so
     * hiding titlebar chrome does not rewrite existing power automation settings.
     */
    hideKeepAwakeTitlebarControl: readBoolean(
      source,
      "hideKeepAwakeTitlebarControl",
      DEFAULT_ghostex_SETTINGS.hideKeepAwakeTitlebarControl,
    ),
    /**
     * CDXC:SessionAttentionNotifications 2026-05-10-16:46
     * Older settings files should opt into macOS attention notifications, and
     * explicit false must be preserved for users who disable system banners.
     */
    showMacOSAttentionNotifications: readBoolean(
      source,
      "showMacOSAttentionNotifications",
      DEFAULT_ghostex_SETTINGS.showMacOSAttentionNotifications,
    ),
    /**
     * CDXC:SessionStatusIndicators 2026-05-09-17:30
     * Visibility is persisted as explicit hide flags: floating is hidden by
     * default, while the menu bar remains visible by default. Normalize missing
     * values to those defaults without coupling either surface to indicator size.
     */
    hideFloatingSessionStatusIndicators: readBoolean(
      source,
      "hideFloatingSessionStatusIndicators",
      DEFAULT_ghostex_SETTINGS.hideFloatingSessionStatusIndicators,
    ),
    hideMenuBarSessionStatusIndicators: readBoolean(
      source,
      "hideMenuBarSessionStatusIndicators",
      DEFAULT_ghostex_SETTINGS.hideMenuBarSessionStatusIndicators,
    ),
    petOverlayEnabled: readBoolean(
      source,
      "petOverlayEnabled",
      DEFAULT_ghostex_SETTINGS.petOverlayEnabled,
    ),
    selectedPetId: normalizePetId(
      readString(source, "selectedPetId", DEFAULT_ghostex_SETTINGS.selectedPetId),
    ),
    /**
     * CDXC:SessionStatusIndicators 2026-05-07-18:20
     * Indicator size is a named UX preference, not raw pixels. Normalize to
     * supported sizes so the native AppKit renderer can apply deterministic
     * scale factors while preserving Medium as the first-install default.
     */
    sessionStatusIndicatorSize: normalizeSessionStatusIndicatorSize(
      readString(
        source,
        "sessionStatusIndicatorSize",
        DEFAULT_ghostex_SETTINGS.sessionStatusIndicatorSize,
      ),
    ),
    sessionPersistenceProvider,
    /**
     * CDXC:SessionPersistence 2026-05-23-00:50:
     * Older settings should normalize the session-id overlay preference from
     * the canonical default while preserving explicit user choices.
     * The native pane still suppresses the actual label unless that terminal is
     * backed by zmx, tmux, or zellij.
     */
    showSessionIdInTerminalPanes: readBoolean(
      source,
      "showSessionIdInTerminalPanes",
      DEFAULT_ghostex_SETTINGS.showSessionIdInTerminalPanes,
    ),
    /**
     * CDXC:SidebarPlacement 2026-05-06-17:32
     * Persist only the supported AppKit chrome sides. Unknown values normalize
     * to the default left placement so the native layout never receives an
     * unsupported sidebar position.
     */
    sidebarSide: normalizeSidebarSide(
      readString(source, "sidebarSide", DEFAULT_ghostex_SETTINGS.sidebarSide),
    ),
    sidebarDefaultWidthPx: clampSidebarDefaultWidthPx(
      readNumber(
        source,
        "sidebarDefaultWidthPx",
        DEFAULT_ghostex_SETTINGS.sidebarDefaultWidthPx,
      ),
    ),
    /**
     * CDXC:ProjectSessionLists 2026-06-13-01:06:
     * Missing settings should use the current ten-session Show less behavior, while explicit numeric values tune how many project sessions remain visible before the header toggle offers Show more.
     */
    projectSessionListCollapsedCount: clampProjectSessionListCollapsedCount(
      readNumber(
        source,
        "projectSessionListCollapsedCount",
        DEFAULT_ghostex_SETTINGS.projectSessionListCollapsedCount,
      ),
    ),
    sidebarTheme: clampSidebarThemeSetting(
      readString(source, "sidebarTheme", DEFAULT_ghostex_SETTINGS.sidebarTheme),
    ),
    terminalCursorStyle: normalizeTerminalCursorStyle(
      readString(source, "terminalCursorStyle", DEFAULT_ghostex_SETTINGS.terminalCursorStyle),
    ),
    terminalCursorStyleBlink: readBoolean(
      source,
      "terminalCursorStyleBlink",
      DEFAULT_ghostex_SETTINGS.terminalCursorStyleBlink,
    ),
    terminalEngine: normalizeTerminalEngine(
      readString(source, "terminalEngine", DEFAULT_ghostex_SETTINGS.terminalEngine),
    ),
    /**
     * CDXC:TerminalTypographySettings 2026-04-29-09:32
     * Font family is a raw Ghostty font-family string so users can type any
     * installed font from `ghostty +list-fonts`. Empty means ghostex leaves an
     * existing Ghostty font-family line or Ghostty's platform default in charge.
     * Legacy preset labels are converted to their Ghostty family name.
     */
    terminalFontFamily: normalizeGhosttyFontFamily(
      readString(source, "terminalFontFamily", DEFAULT_ghostex_SETTINGS.terminalFontFamily),
    ),
    terminalFontSize: clampNumber(
      readNumber(source, "terminalFontSize", DEFAULT_ghostex_SETTINGS.terminalFontSize),
      8,
      32,
      DEFAULT_ghostex_SETTINGS.terminalFontSize,
    ),
    terminalFontWeight: clampNumber(
      readNumber(source, "terminalFontWeight", DEFAULT_ghostex_SETTINGS.terminalFontWeight),
      100,
      900,
      DEFAULT_ghostex_SETTINGS.terminalFontWeight,
    ),
    /**
     * CDXC:TerminalThemeSettings 2026-04-29-09:32
     * Ghostty themes are exact strings. Preserve only bundled theme names from
     * the settings list, or an empty unmanaged value that keeps an existing
     * user-authored Ghostty `theme` line outside ghostex control.
     */
    terminalGhosttyTheme: normalizeGhosttyTheme(
      readString(source, "terminalGhosttyTheme", DEFAULT_ghostex_SETTINGS.terminalGhosttyTheme),
    ),
    terminalLetterSpacing: clampNumber(
      readNumber(source, "terminalLetterSpacing", DEFAULT_ghostex_SETTINGS.terminalLetterSpacing),
      -2,
      8,
      DEFAULT_ghostex_SETTINGS.terminalLetterSpacing,
    ),
    terminalLineHeight: clampNumber(
      readNumber(source, "terminalLineHeight", DEFAULT_ghostex_SETTINGS.terminalLineHeight),
      0.8,
      2,
      DEFAULT_ghostex_SETTINGS.terminalLineHeight,
    ),
    /**
     * CDXC:TerminalScrollSettings 2026-04-29-08:56
     * Ghostty exposes mouse wheel speed through mouse-scroll-multiplier with
     * separate precision and discrete device prefixes. Store both values so
     * trackpads and notched mouse wheels can be tuned independently while
     * matching the settings modal's 0.25-step practical range. Ghostty accepts
     * 0.01..10000, but those extremes are intentionally not exposed because
     * the docs warn they produce a bad experience.
     */
    terminalMouseScrollMultiplierDiscrete: clampNumber(
      readNumber(
        source,
        "terminalMouseScrollMultiplierDiscrete",
        DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierDiscrete,
      ),
      MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
      MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
      DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierDiscrete,
    ),
    terminalMouseScrollMultiplierPrecision: clampNumber(
      readNumber(
        source,
        "terminalMouseScrollMultiplierPrecision",
        DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierPrecision,
      ),
      MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
      MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
      DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierPrecision,
    ),
    tmuxMode: sessionPersistenceProvider === "tmux",
    terminalScrollToBottomWhenTyping: readBoolean(
      source,
      "terminalScrollToBottomWhenTyping",
      DEFAULT_ghostex_SETTINGS.terminalScrollToBottomWhenTyping,
    ),
    /**
     * CDXC:TerminalBehaviorSettings 2026-04-29-09:32
     * Common Ghostty terminal behavior settings are persisted with the same
     * practical UI ranges and enum values that the settings modal exposes,
     * then written as documented Ghostty config keys by the native host.
     */
    terminalScrollbackLimitMb: clampNumber(
      readNumber(
        source,
        "terminalScrollbackLimitMb",
        DEFAULT_ghostex_SETTINGS.terminalScrollbackLimitMb,
      ),
      MIN_GHOSTTY_SCROLLBACK_LIMIT_MB,
      MAX_GHOSTTY_SCROLLBACK_LIMIT_MB,
      DEFAULT_ghostex_SETTINGS.terminalScrollbackLimitMb,
    ),
    terminalCopyOnSelect: normalizeGhosttyCopyOnSelect(
      readString(source, "terminalCopyOnSelect", DEFAULT_ghostex_SETTINGS.terminalCopyOnSelect),
    ),
    terminalConfirmCloseSurface: normalizeGhosttyConfirmCloseSurface(
      readString(
        source,
        "terminalConfirmCloseSurface",
        DEFAULT_ghostex_SETTINGS.terminalConfirmCloseSurface,
      ),
    ),
    /**
     * CDXC:TerminalBehaviorSettings 2026-04-29-09:32
     * Clipboard cleanup/protection and mouse/scrollbar visibility mirror
     * Ghostty's documented defaults unless the user changes them in ghostex.
     */
    terminalClipboardTrimTrailingSpaces: readBoolean(
      source,
      "terminalClipboardTrimTrailingSpaces",
      DEFAULT_ghostex_SETTINGS.terminalClipboardTrimTrailingSpaces,
    ),
    terminalClipboardPasteProtection: readBoolean(
      source,
      "terminalClipboardPasteProtection",
      DEFAULT_ghostex_SETTINGS.terminalClipboardPasteProtection,
    ),
    terminalPastePreviewableImages: readBoolean(
      source,
      "terminalPastePreviewableImages",
      DEFAULT_ghostex_SETTINGS.terminalPastePreviewableImages,
    ),
    terminalMouseHideWhileTyping: readBoolean(
      source,
      "terminalMouseHideWhileTyping",
      DEFAULT_ghostex_SETTINGS.terminalMouseHideWhileTyping,
    ),
    terminalScrollbar: normalizeGhosttyScrollbar(
      readString(source, "terminalScrollbar", DEFAULT_ghostex_SETTINGS.terminalScrollbar),
    ),
    promptEditorBackend,
    customPromptEditorCommand: normalizeCustomPromptEditorCommand(
      readString(
        source,
        "customPromptEditorCommand",
        DEFAULT_ghostex_SETTINGS.customPromptEditorCommand,
      ),
    ),
    /**
     * CDXC:GtePromptEditing 2026-05-10-11:11
     * Keep reading the old opt-in key so older snapshots round-trip cleanly.
     *
     * CDXC:GtePromptEditing 2026-05-23-01:51:
     * Mirror defaults follow the normalized backend so first-run settings and older files without mirror keys still report gte as the active Ctrl+G editor.
     */
    richPromptEditingWithGte: readBoolean(
      source,
      "richPromptEditingWithGte",
      promptEditorBackend === "gte",
    ),
    useGteForCtrlGPromptEditing: readBoolean(
      source,
      "useGteForCtrlGPromptEditing",
      readBoolean(source, "richPromptEditingWithGte", promptEditorBackend === "gte") === true,
    ),
    /**
     * CDXC:Hotkeys 2026-04-28-05:20
     * User-defined app shortcuts are normalized with defaults on every settings
     * read so older settings files gain configurable native hotkeys without a
     * migration or fallback execution path.
     */
    hotkeys: normalizeghostexHotkeySettings(source.hotkeys),
    workspaceActivePaneBorderColor:
      readString(
        source,
        "workspaceActivePaneBorderColor",
        DEFAULT_ghostex_SETTINGS.workspaceActivePaneBorderColor,
      ).trim() || DEFAULT_ghostex_SETTINGS.workspaceActivePaneBorderColor,
    /**
     * CDXC:WorkspaceLayout 2026-04-28-06:08
     * Users can choose the background visible behind terminal panes. Persist a
     * normalized CSS color string so the React workspace and native AppKit
     * workspace render the same color instead of hardcoding dark gray.
     */
    workspaceBackgroundColor:
      readString(source, "workspaceBackgroundColor", DEFAULT_ghostex_SETTINGS.workspaceBackgroundColor)
        .trim() || DEFAULT_ghostex_SETTINGS.workspaceBackgroundColor,
    clickToWakeSleepingSessions: readBoolean(
      source,
      "clickToWakeSleepingSessions",
      DEFAULT_ghostex_SETTINGS.clickToWakeSleepingSessions,
    ),
    /**
     * CDXC:TitlebarOpenIn 2026-05-11-00:22
     * Settings owns which titlebar Open In targets are shown. Normalize on read
     * so the React titlebar can trust the persisted custom commands and hidden
     * built-in ids sent through native layout sync.
     */
    customWorkspaceOpenTargets: normalizeCustomWorkspaceOpenTargets(
      source.customWorkspaceOpenTargets,
    ),
    workspaceOpenTargetAvailability: normalizeWorkspaceOpenTargetAvailability(
      source.workspaceOpenTargetAvailability,
    ),
    workspaceOpenTargetHiddenIds: normalizeWorkspaceOpenTargetHiddenIds(
      source.workspaceOpenTargetHiddenIds,
    ),
    workspacePaneGap: 0,
    remoteMachines: normalizeRemoteMachineSettings(source.remoteMachines),
    commandsPanelDefaultHeightPx: clampCommandsPanelDefaultHeightPx(
      readNumber(
        source,
        "commandsPanelDefaultHeightPx",
        DEFAULT_ghostex_SETTINGS.commandsPanelDefaultHeightPx,
      ),
    ),
  };
}

export function normalizeRemoteMachineSettings(candidate: unknown): RemoteMachineSettings[] {
  if (!Array.isArray(candidate)) {
    return [];
  }
  const seenIds = new Set<string>();
  const normalized: RemoteMachineSettings[] = [];
  for (const item of candidate) {
    if (!isRecord(item)) {
      continue;
    }
    const name = readLooseString(item.name).slice(0, 80);
    const sshHost = readLooseString(item.sshHost).slice(0, 200);
    if (!name || !sshHost) {
      continue;
    }
    let id = normalizeRemoteMachineId(item.id);
    if (!id || seenIds.has(id)) {
      id = `remote-${normalized.length + 1}`;
      while (seenIds.has(id)) {
        id = `remote-${normalized.length + 1}-${seenIds.size + 1}`;
      }
    }
    seenIds.add(id);
    const sshUser = readLooseString(item.sshUser).slice(0, 120);
    const sshIdentityFile = readLooseString(item.sshIdentityFile).slice(0, 500);
    const sshPort = normalizeRemoteMachineSshPort(item.sshPort);
    normalized.push({
      id,
      name,
      sshHost,
      /*
      CDXC:RemoteMachines 2026-06-09-18:23:
      Remote SSH passwords are stored only in macOS Keychain. Settings may keep
      this boolean marker so the UI can show a saved credential state, but raw
      password fields from drafts/imports must be ignored by normalization.
      */
      ...(item.sshPasswordSaved === true ? { sshPasswordSaved: true } : {}),
      ...(sshIdentityFile ? { sshIdentityFile } : {}),
      ...(sshPort ? { sshPort } : {}),
      ...(sshUser ? { sshUser } : {}),
    });
  }
  return normalized;
}

export function getTerminalFontFamilyForghostexSettings(settings: ghostexSettings): string {
  return settings.terminalFontFamily.trim() || getTerminalFontFamilyForPreset("JetBrains Mono");
}

export function getSidebarSettingsPresetId(
  settings: Pick<ghostexSettings, SidebarSettingsPresetKey>,
): SidebarSettingsPresetId | undefined {
  return SIDEBAR_SETTINGS_PRESETS.find((preset) =>
    SIDEBAR_SETTINGS_PRESET_KEYS.every((key) => Object.is(settings[key], preset.settings[key])),
  )?.id;
}

export function applySidebarSettingsPreset(
  settings: ghostexSettings,
  presetId: SidebarSettingsPresetId,
): ghostexSettings {
  return normalizeghostexSettings({
    ...settings,
    ...SIDEBAR_SETTINGS_PRESET_SETTINGS[presetId],
  });
}

function normalizeTerminalCursorStyle(value: string | undefined): TerminalCursorStyle {
  return value === "block" || value === "underline" ? value : "bar";
}

function normalizeBrowserOpenMode(value: string | undefined): BrowserOpenMode {
  return "browser-pane";
}

function normalizeBrowserFeedbackTool(value: string | undefined): BrowserFeedbackTool {
  return value === "react-grab" ? "react-grab" : DEFAULT_ghostex_SETTINGS.browserFeedbackTool;
}

function normalizeAppShotsHotkey(value: string | undefined): AppShotsHotkey {
  return value === "double-left-shift" || value === "double-left-option"
    ? value
    : DEFAULT_ghostex_SETTINGS.appShotsHotkey;
}

function normalizeDefaultEditorCommand(value: string | undefined): DefaultEditorCommand {
  return value === "code-insiders" ||
    value === "zed" ||
    value === "zeditor" ||
    value === "cursor" ||
    value === "windsurf" ||
    value === "codium" ||
    value === "subl" ||
    value === "other"
    ? value
    : DEFAULT_ghostex_SETTINGS.defaultEditorCommand;
}

function normalizeCustomDefaultEditorCommand(value: string | undefined): string {
  return (value ?? "").trim().slice(0, 240);
}

function normalizeDefaultPromptAgentId(value: string | undefined): string {
  return ((value ?? "").trim() || DEFAULT_ghostex_SETTINGS.defaultPromptAgentId).slice(0, 120);
}

function normalizeSessionTitleGenerationAgent(
  value: string | undefined,
): SessionTitleGenerationAgent {
  return value === "cursor" || value === "claude" || value === "grok" || value === "custom"
    ? value
    : DEFAULT_ghostex_SETTINGS.sessionTitleGenerationAgent;
}

function normalizeCustomSessionTitleGenerationCommand(value: string | undefined): string {
  return (value ?? "").trim().slice(0, 240);
}

export function getSessionTitleGenerationCommandPreview(
  agent: SessionTitleGenerationAgent,
  options: { command?: string } = {},
): string {
  const command = readSessionTitleGenerationPreviewCommand(agent, options.command);
  const prompt = SESSION_TITLE_GENERATION_PROMPT_PLACEHOLDER;
  switch (agent) {
    case "codex":
      /*
      CDXC:SessionTitleSettings 2026-06-07-01:57:
      Settings must preview the same internal Codex title-generation command gxserver runs. Include `--ephemeral` so users see that generated titles do not create restorable Codex sessions.
      */
      return createSessionTitleGenerationHereDocPreview(
        `${command} exec --ephemeral --skip-git-repo-check -m gpt-5.4-mini -c 'model_reasoning_effort="low"'`,
        prompt,
      );
    case "cursor":
      return `${command} --print --yolo --trust --output-format text '${prompt}'`;
    case "claude":
      return createSessionTitleGenerationHereDocPreview(`${command} -p --model haiku`, prompt);
    case "grok":
      return `${command} -p --model grok-composer-2.5-fast --output-format plain --no-alt-screen --no-plan --no-subagents --disable-web-search --max-turns 1 '${prompt}'`;
    case "custom":
      return createSessionTitleGenerationHereDocPreview(command, prompt);
  }
}

function readSessionTitleGenerationPreviewCommand(
  agent: SessionTitleGenerationAgent,
  command: string | undefined,
): string {
  const configured = command?.trim();
  if (configured) {
    return configured;
  }
  switch (agent) {
    case "codex":
      return "codex";
    case "cursor":
      return "cursor-agent";
    case "claude":
      return "claude";
    case "grok":
      return "grok";
    case "custom":
      return "<custom command>";
  }
}

function createSessionTitleGenerationHereDocPreview(command: string, prompt: string): string {
  return `${command} <<'PROMPT'\n${prompt}\nPROMPT`;
}

function normalizeCustomPromptEditorCommand(value: string | undefined): string {
  return ((value ?? "").trim() || DEFAULT_ghostex_SETTINGS.customPromptEditorCommand).slice(0, 240);
}

export function getDefaultEditorCommandForSettings(settings: ghostexSettings): string {
  const customCommand = settings.customDefaultEditorCommand.trim();
  return settings.defaultEditorCommand === "other"
    ? customCommand || DEFAULT_ghostex_SETTINGS.defaultEditorCommand
    : settings.defaultEditorCommand;
}

function normalizeSidebarSide(value: string | undefined): SidebarSide {
  return value === "right" ? "right" : DEFAULT_ghostex_SETTINGS.sidebarSide;
}

function normalizeSessionStatusIndicatorSize(
  value: string | undefined,
): SessionStatusIndicatorSize {
  return value === "small" || value === "large" || value === "x-large" ? value : "medium";
}

function normalizeSessionPersistenceProvider(
  value: string | undefined,
): SessionPersistenceProvider {
  return value === "tmux" || value === "zmx" || value === "zellij" ? value : "off";
}

function normalizeKeepAwakeDurationMinutes(value: number): KeepAwakeDurationMinutes {
  return KEEP_AWAKE_DURATION_OPTIONS.some((option) => option.value === value)
    ? (value as KeepAwakeDurationMinutes)
    : DEFAULT_ghostex_SETTINGS.keepAwakeDefaultDurationMinutes;
}

function normalizeAutoSleepIdleMinutes(
  value: number,
  fallback: AutoSleepIdleMinutes,
): AutoSleepIdleMinutes {
  return AUTO_SLEEP_IDLE_MINUTE_OPTIONS.some((option) => option.value === value)
    ? (value as AutoSleepIdleMinutes)
    : fallback;
}

function normalizePromptEditorBackend(source: Record<string, unknown>): PromptEditorBackend {
  const backend = readString(source, "promptEditorBackend", "");
  if (backend === "inherit" || backend === "monaco" || backend === "gte" || backend === "custom") {
    return backend;
  }
  if (
    readBoolean(source, "useGteForCtrlGPromptEditing", false) ||
    readBoolean(source, "richPromptEditingWithGte", false)
  ) {
    return "gte";
  }
  return DEFAULT_ghostex_SETTINGS.promptEditorBackend;
}

function normalizeRemoteMachineId(input: unknown): string | undefined {
  const id = readLooseString(input).slice(0, 80);
  return /^remote-[a-z0-9_-]+$/iu.test(id) ? id : undefined;
}

function normalizeRemoteMachineSshPort(input: unknown): number | undefined {
  if (input === undefined || input === null || input === "") {
    return undefined;
  }
  const value = typeof input === "number" ? input : Number(input);
  if (!Number.isInteger(value) || value < 1 || value > 65535) {
    return undefined;
  }
  return value;
}

function normalizeGhosttyTheme(value: string | undefined): string {
  if (!value || value === "__ghostex_ghostty_theme_unmanaged__") {
    return "";
  }
  return (GHOSTTY_THEME_OPTIONS as readonly string[]).includes(value) ? value : "";
}

function normalizeGhosttyFontFamily(value: string | undefined): string {
  const trimmedValue = (value ?? "").trim();
  if (!trimmedValue) {
    return "";
  }
  const legacyPreset = normalizeTerminalFontPreset(trimmedValue);
  if (legacyPreset === trimmedValue) {
    return getGhosttyFontFamilyForPreset(legacyPreset);
  }
  return trimmedValue;
}

function normalizeGhosttyCopyOnSelect(value: string | undefined): GhosttyCopyOnSelect {
  return value === "true" || value === "clipboard" ? value : DEFAULT_ghostex_SETTINGS.terminalCopyOnSelect;
}

function normalizeGhosttyConfirmCloseSurface(
  value: string | undefined,
): GhosttyConfirmCloseSurface {
  return value === "false" || value === "always" ? value : "true";
}

function normalizeGhosttyScrollbar(value: string | undefined): GhosttyScrollbar {
  return value === "never" ? "never" : "system";
}

function clampNumber(value: number, min: number, max: number, fallback: number): number {
  if (!Number.isFinite(value)) {
    return fallback;
  }

  return Math.min(max, Math.max(min, value));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function readBoolean(
  source: Record<string, unknown>,
  key: keyof ghostexSettings,
  fallback: boolean,
): boolean {
  const value = source[key];
  return typeof value === "boolean" ? value : fallback;
}

function readNumber(
  source: Record<string, unknown>,
  key: keyof ghostexSettings,
  fallback: number,
): number {
  const value = source[key];
  return typeof value === "number" ? value : fallback;
}

function readString(
  source: Record<string, unknown>,
  key: keyof ghostexSettings,
  fallback: string,
): string {
  const value = source[key];
  return typeof value === "string" ? value : fallback;
}

function readLooseString(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}
