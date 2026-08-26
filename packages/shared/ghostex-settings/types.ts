import { type SidebarThemeSetting, type TerminalEngine } from '../session-grid-contract-core';
import { DEFAULT_COMMANDS_PANEL_HEIGHT_PX } from '../session-grid-contract-session';
import { type SessionChatTheme } from '../session-chat';
import { type CompletionSoundSetting } from '../completion-sound';
import { type ghostexHotkeySettings } from '../ghostex-hotkeys';
import { type CustomWorkspaceOpenTarget, type WorkspaceOpenTargetAvailability } from '../workspace-open-targets';
import { type PetId } from '../pets';
import { type SidebarSessionTagListItem } from '../session-tags';
import { type DiagnosticLoggingSettings } from './diagnostic-logging';
import { type RemoteMachineSettings } from './remote-machines';
import { type SettingsModalNavigationState } from './settings-modal-navigation';

export type GhosttyConfirmCloseSurface = 'false' | 'true' | 'always';
export type GhosttyCopyOnSelect = 'false' | 'true' | 'clipboard';
export type GhosttyScrollbar = 'system' | 'never';
export type TerminalCursorStyle = 'bar' | 'block' | 'underline';
export type TerminalBackgroundImageFit = 'cover' | 'contain' | 'stretch' | 'natural';
export type WindowsTerminalBackend = 'wsl';
export type BrowserOpenMode = 'browser-pane';
export type BrowserFeedbackTool = 'agentation';
export type PortlessProtocol = 'https' | 'http';
/**
 * CDXC:WebLinkOpenTarget 2026-08-19:
 * One answer to "where does a web link Ghostex opens land". Command-clicked
 * terminal links, session chat links, and detected dev-server rows all read
 * this single target instead of the old split between a Browser toggle and a
 * Dev Servers dropdown, which could disagree with each other.
 */
export type WebLinkOpenTarget = 'internal-browser' | 'system-default-browser';
export type DefaultEditorCommand =
  'code' | 'code-insiders' | 'zed' | 'zeditor' | 'cursor' | 'windsurf' | 'codium' | 'subl' | 'other';
export type SessionPersistenceProvider = 'off' | 'tmux' | 'zmx' | 'zellij';
export type SessionStatusIndicatorSize = 'small' | 'medium' | 'large' | 'x-large';
export type SidebarSide = 'left' | 'right';
export type CommandsPanelSide = 'bottom' | 'right';
export type SidebarProjectGroupStyle = 'quiet' | 'header' | 'branched';
export const MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS = 0;
export const MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS = 1000;
export const SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS = 100;
export const DEFAULT_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS = 400;
export const MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT = 50;
export const MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT = 100;
export const SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP = 5;
export const DEFAULT_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT = 75;

export function clampSessionChatTranscriptWidthPercent(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT;
  }
  const clamped = Math.min(
    MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
    Math.max(MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT, value)
  );
  return Math.round(clamped / SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP) * SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP;
}

export function clampSidebarCollapseAnimationDurationMs(value: number): number {
  const clamped = Math.min(
    MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
    Math.max(MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS, value)
  );
  return (
    Math.round(clamped / SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS) * SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS
  );
}
/**
 * CDXC:SidebarV2 2026-07-29:
 * Sidebar V2 ("Inbox") is an opt-in presentation layer beside the classic
 * sidebar. The stored version selector rides the normal settings file, so
 * hosts that only persist unknown keys need no change to support it.
 */
export type SidebarVersion = 'v1' | 'v2';
/** The surface shown first when a newly launched agent supports Session Chat. */
export type PreferredAgentInterface = 'terminal' | 'chat';
/**
 * CDXC:SidebarV2 2026-07-29:
 * Sidebar V2 renders collapsible per-project groups by default and can switch
 * to one flat session inbox. Keep the sub-mode as its own key so the layout
 * choice survives switching back and forth between V1 and V2.
 */
export type SidebarV2Layout = 'flat' | 'byProject';
/**
 * CDXC:SidebarV2Worktree 2026-07-29:
 * What the plain "+" does in Sidebar V2: start a session in the project itself
 * ("local", the unchanged instant path) or open the worktree popover
 * pre-filled ("worktree").
 */
export type SidebarNewSessionEnvMode = 'local' | 'worktree';
/**
 * CDXC:SidebarV2LogicalProjects 2026-07-29:
 * How aggressively one checkout merges with other checkouts of the same
 * repository in Sidebar V2. Mirrors
 * `SidebarV2ProjectGroupingMode` in `packages/shared/sidebar-v2-logical-project.ts`
 * one-for-one; the two spellings must stay identical because this settings
 * value is fed straight into that module.
 */
export type SidebarProjectGroupingMode = 'repository' | 'repositoryPath' | 'separate';
export type SidebarSettingsPresetId = 'codex' | 'minimal' | 'detailed' | 'recommended';
export type PromptEditorBackend = 'inherit' | 'monaco';
export type SessionTitleGenerationAgent = 'codex' | 'cursor' | 'claude' | 'grok' | 'custom';
export type AppShotsHotkey = 'both-command' | 'both-shift' | 'both-option' | 'double-left-shift' | 'double-left-option';
export type KeepAwakeDurationMinutes = 0 | 120 | 300;
export type AutoSleepIdleMinutes = 5 | 10 | 15 | 30 | 60 | 120 | 300;
/**
 * CDXC:AccentColor 2026-08-24:
 * The Codex-style redesign paints its accent text (Automate "Active", unread
 * counts, and upcoming modal accents) from a single user-configurable color.
 * The default is the sky tone those surfaces shipped hardcoded.
 */
export const DEFAULT_ACCENT_COLOR = '#38bdf8';
export const DEFAULT_TERMINAL_PANE_HORIZONTAL_PADDING_PX = 21;
export const DEFAULT_TERMINAL_PANE_PADDING_PX = 0;
export const MIN_TERMINAL_PANE_PADDING_PX = 0;
export const MAX_TERMINAL_PANE_PADDING_PX = 64;
export const MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX = 40;
export const MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX = 600;
export const DEFAULT_SIDEBAR_DEFAULT_WIDTH_PX = 275;
export const MIN_SIDEBAR_DEFAULT_WIDTH_PX = 150;
export const MAX_SIDEBAR_DEFAULT_WIDTH_PX = 520;
export const DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT = 10;
export const MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT = 1;
export const MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT = 50;

export function clampCommandsPanelDefaultHeightPx(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_COMMANDS_PANEL_HEIGHT_PX;
  }
  return Math.min(
    MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX,
    Math.max(MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX, Math.round(value))
  );
}

export function clampSidebarDefaultWidthPx(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_SIDEBAR_DEFAULT_WIDTH_PX;
  }
  return Math.min(MAX_SIDEBAR_DEFAULT_WIDTH_PX, Math.max(MIN_SIDEBAR_DEFAULT_WIDTH_PX, Math.round(value)));
}

export function clampProjectSessionListCollapsedCount(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT;
  }
  return Math.min(
    MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
    Math.max(MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT, Math.round(value))
  );
}

export function clampTerminalPanePaddingPx(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_TERMINAL_PANE_PADDING_PX;
  }
  return Math.min(MAX_TERMINAL_PANE_PADDING_PX, Math.max(MIN_TERMINAL_PANE_PADDING_PX, Math.round(value)));
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
  /** GPUI titlebar choices, keyed by canonical main-project id. */
  gpuiTitlebarActionCommandByProject: Record<string, string>;
  gpuiTitlebarOpenTargetByProject: Record<string, string>;
  appShotsEnabled: boolean;
  appShotsHotkey: AppShotsHotkey;
  appShotsMetadataEnabled: boolean;
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
   *
   * CDXC:GxserverAgentSettings 2026-06-19-08:58:
   * gxserver now owns the canonical Default Prompt Agent alongside global
   * Accept All. Keep this field as the sidebar's synchronous render cache so
   * Settings can draw immediately from startup snapshots and gxserver update
   * responses without localStorage becoming a competing source of truth.
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
  /**
   * CDXC:TerminalLinkInAppBrowser 2026-07-02-13:05:
   * Command-clicked http/https terminal links open as tabs in the project
   * Browser view by default. Pointing this at the system default browser
   * restores handing web links to it. File paths and non-web schemes always
   * keep the external NSWorkspace route regardless of this setting.
   *
   * CDXC:GPUISessionChatLinks 2026-08-18:
   * Web links clicked in session chat follow the same switch, so this is the
   * single answer to "where do agent-sent web links open". Chat file links
   * still open in Docs or Code, and Shift+click still forces the system
   * default browser while the target is the internal browser.
   *
   * CDXC:WebLinkOpenTarget 2026-08-19:
   * Detected dev-server rows read this too, replacing the separate Dev Servers
   * open-target dropdown. Migrated from the legacy openTerminalLinksInApp
   * boolean, which wins over the legacy dev-server target when both persist,
   * because it is the switch users actually flipped.
   */
  webLinkOpenTarget: WebLinkOpenTarget;
  /**
   * CDXC:SettingsAdvanced 2026-06-28-08:01:
   * Show Advanced is a persisted Settings browsing preference. When users enable
   * advanced rows, keep that density enabled across app restarts until they
   * explicitly turn it off again.
   */
  showAdvancedSettings: boolean;
  /**
   * CDXC:SettingsNavigation 2026-06-29-17:54:
   * Closing Settings should persist the user's current Settings page and
   * scroll offsets so relaunching the macOS app can reopen Settings at the
   * exact spot they left, while explicit deep links can still override it.
   *
   * CDXC:SettingsNavigation 2026-06-30-04:47:
   * Persist page navigation as the user moves through Settings because the
   * native AppKit close button can tear down the child window before React's
   * dialog-close callback runs.
   */
  settingsModalNavigation: SettingsModalNavigationState;
  /**
   * CDXC:ExperimentalFeatures 2026-06-28-07:41:
   * Enable Experimental Features is the user-facing name for this persisted
   * showBetaFeatures key. Experimental surfaces stay hidden by default, while
   * Agents Hub remains outside this gate and visible in the sidebar.
   *
   * CDXC:Automations 2026-07-01-03:24:
   * Automations Overview and project Automate are experimental macOS surfaces.
   * Keep their real page content behind this gate; disabled users should see
   * only the coming-soon overlay for those pages.
   */
  showBetaFeatures: boolean;
  /**
   * Built-in project workarea switches control titlebar presentation only;
   * they do not stop runtimes, unmount surfaces, or disable hotkeys.
   */
  codeViewTabHidden: boolean;
  browserViewTabHidden: boolean;
  kanbanViewTabHidden: boolean;
  automateViewTabHidden: boolean;
  docsViewTabHidden: boolean;
  /**
   * Quick-access switches affect only the matching right-side titlebar button.
   * The menus and commands remain available through their other entry points.
   */
  tipsAndTricksTitlebarButtonHidden: boolean;
  resourcesTitlebarButtonHidden: boolean;
  extensionsTitlebarButtonHidden: boolean;
  gitActionsTitlebarButtonHidden: boolean;
  quickActionsTitlebarButtonHidden: boolean;
  openInTitlebarButtonHidden: boolean;
  codeServerLinkVscodeUserConfig: boolean;
  codeServerUseVscodeInsidersUserConfig: boolean;
  customDefaultEditorCommand: string;
  /**
   * CDXC:AppIconPicker 2026-06-25-21:50: Persisted id of the selected Dock /
   * app-switcher icon. Empty string means the default bundled app icon. The
   * value is a filename living in the native icons folder; native confirms the
   * selection via an appIconState ok event before the sidebar persists it.
   */
  appIconSourceId: string;
  defaultEditorCommand: DefaultEditorCommand;
  hideProjectHeaderDiffStats: boolean;
  /**
   * CDXC:DocsSidebar 2026-06-30-19:47:
   * The Docs sidebar scans ./docs, ./artifacts, and ./ai recursively plus root artifacts by default. Users can add comma-separated project-relative folder roots from global Projects settings. Trim spaces around each folder name while preserving spaces inside names such as "my documents".
   *
   * CDXC:DocsRootAdditive 2026-08-09:
   * These folders are project-root-relative, always. A configured Docs directory is mounted as an ADDITIONAL top-level folder that always shows its whole tree, so it is never narrowed by this list (round 2 briefly made it a narrowing control for that root; additive mounting replaced that).
   */
  manageAdditionalDocsFolders: string;
  /**
   * CDXC:GlobalProjectDefaults 2026-08-02:
   * Global Defaults for the three per-project fields on the Projects settings
   * page. A project keeps overriding the default whenever its own value is
   * non-empty; an empty project value now falls back here before falling back
   * to the previous built-in behavior. Empty globals therefore preserve the
   * exact pre-existing resolution for every project.
   */
  globalWorktreeCommand: string;
  globalBeadsDisplayKey: string;
  globalBeadsDirectory: string;
  /**
   * CDXC:DocsRootDirectory 2026-08-09:
   * Absolute folder Docs shows IN ADDITION to the project's own docs when a
   * project sets no Docs directory of its own.
   *
   * CDXC:DocsRootAdditive 2026-08-09:
   * It never replaces the project's docs — README.md, CLAUDE.md, docs/, and the
   * configured Docs folders all keep listing, and this folder is added beside
   * them as one top-level node named after itself. Empty adds nothing.
   *
   * A project's own `docsDirectory` takes this one's place in the cascade; it is
   * likewise an addition to that project's docs, never a replacement for them.
   */
  globalDocsDirectory: string;
  showProjectEditorDiffFileCount: boolean;
  showUntrackedProjectDiffWhenNoTrackedChanges: boolean;
  completionBellEnabled: boolean;
  completionSound: CompletionSoundSetting;
  showNotificationOnTerminalBell: boolean;
  createSessionOnSidebarDoubleClick: boolean;
  /**
   * Enables the Park session action and the collapsible Parked section at the
   * bottom of each project's session list. Parking is durable session state;
   * disabling this preference only hides the organization feature and renders
   * parked sessions in the ordinary Sessions section again.
   */
  enableSessionParking: boolean;
  /**
   * CDXC:AnonymousAnalytics 2026-08-26:
   * Opt-out switch for the anonymous PostHog usage analytics gxserver sends.
   * Default true. gxserver reads this key straight out of
   * native-sidebar-settings.json and treats an absent key as enabled, so the
   * app never has to write the file before analytics can be gated. Turning it
   * off stops capture and drops the queue; see docs/ANALYTICS.md.
   */
  analyticsEnabled: boolean;
  debuggingMode: boolean;
  /**
   * CDXC:DiagnosticsSettings 2026-06-27-22:07:
   * Debugging Mode no longer acts as the broad disk-logging switch. Routine
   * persistent diagnostics are controlled by explicit scenario ids so users can
   * enable one repro area, such as GPUI app modals or macOS terminal focus,
   * without turning on every noisy log writer.
   */
  diagnosticLogging: DiagnosticLoggingSettings;
  renameSessionOnDoubleClick: boolean;
  /** Show project artwork or the folder/worktree fallback beside project names. */
  showProjectIcons: boolean;
  hideSessionAgentIconUntilHover: boolean;
  /**
   * CDXC:SidebarSessionAgentIcons 2026-06-29-23:58:
   * Session-card agent logos are monochrome by default for compatibility, but
   * Settings needs an independent toggle for colored brand artwork. Favorite
   * state must not recolor the agent logo to gold.
   */
  useColoredSessionAgentIcons: boolean;
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
   * CDXC:SessionTagFilters 2026-06-13-17:50:
   * Settings owns the sidebar tag-filter presentation list: users can reorder
   * tags, move separators, hide rows, or disable selectable tag filters without
   * changing the durable session tag values stored on sessions.
   */
  sidebarSessionTagListItems: readonly SidebarSessionTagListItem[];
  /**
   * CDXC:AutoSleep 2026-05-28-08:06:
   * Auto Sleep is a settings-owned policy for retiring idle VS Code, Git,
   * Project, Manage, browser, and agent sessions through their native sleep paths.
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
  /**
   * CDXC:TitlebarKeepAwake 2026-06-23-08:20:
   * Users can opt into a Mac power hold while any session is Working, with the titlebar runtime extending that hold for a short reply window after work stops.
   */
  keepAwakeWhileWorkingSessions: boolean;
  keepAwakePreventLidSleep: boolean;
  hideKeepAwakeTitlebarControl: boolean;
  /**
   * CDXC:GlobalActions 2026-08-01:
   * The Agents tab strip ships New Terminal and New Browser Tab buttons. Users
   * who run those from Global Actions or hotkeys can hide either one to make
   * room in the strip. The pane overflow button is deliberately not hideable —
   * it is the only way to reach the rest of the pane actions.
   */
  hideTabStripNewTerminalButton: boolean;
  hideTabStripNewBrowserButton: boolean;
  showMacOSAttentionNotifications: boolean;
  hideFloatingSessionStatusIndicators: boolean;
  hideMenuBarSessionStatusIndicators: boolean;
  petOverlayEnabled: boolean;
  selectedPetId: PetId;
  sessionStatusIndicatorSize: SessionStatusIndicatorSize;
  sessionPersistenceProvider: SessionPersistenceProvider;
  showSessionIdInTerminalPanes: boolean;
  /** Newly launched supported agents still start a terminal, then show this surface first. */
  preferredAgentInterface: PreferredAgentInterface;
  /**
   * CDXC:SidebarV2 2026-07-29:
   * The sidebar version selector is the rollout switch for the Inbox sidebar.
   * V1 stays the default everywhere; V2 is pure opt-in from Settings or the
   * sidebar Sort & Filter menu.
   */
  sidebarVersion: SidebarVersion;
  /**
   * CDXC:SidebarV2 2026-07-29:
   * Group by Project is a V2-only sub-mode. It is stored independently of
   * `sidebarVersion` so returning to V2 restores the last chosen layout.
   */
  sidebarV2Layout: SidebarV2Layout;
  /**
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * Days of inactivity before an untouched session auto-settles onto the Inbox
   * sidebar's Settled shelf. `null` disables inactivity auto-settle entirely.
   *
   * This key is read by BOTH ends: the client predicate in
   * `packages/shared/sidebar-v2-lifecycle.ts` and server, which reads
   * `sidebarAutoSettleAfterDays` straight out of
   * `GHOSTEX_HOME/state/native-sidebar-settings.json` for its auto-settle sweep
   * (`server/src/session_lifecycle.rs`). The spelling is therefore part of
   * the server contract — renaming it silently reverts every user to the
   * 3-day default.
   */
  sidebarAutoSettleAfterDays: number | null;
  /**
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * Per-checkout override for cross-machine logical project grouping in Sidebar
   * V2. The default (an empty record) means every project follows the automatic
   * rule: merge checkouts that share a normalized git `origin` remote, and
   * never merge anything without one.
   *
   * The KEY is the module's physical project key
   * (`deriveSidebarV2ProjectGroupingOverrideKey` in
   * `packages/shared/sidebar-v2-logical-project.ts`), i.e. `<machineId>:<path>`. Keying
   * by the physical checkout rather than by repository is deliberate: setting
   * "keep separate" on this Mac's copy must not silently re-group a colleague's
   * machine, and the key stays stable when a project is renamed.
   *
   * Values are the wire-contract spellings `"repository"` (merge every checkout
   * of the repo), `"repositoryPath"` (merge only checkouts at the same path
   * inside the repo), and `"separate"` (never merge). Unknown values and
   * malformed entries are dropped by normalization rather than defaulting, so a
   * hand-edited settings file cannot invent a grouping mode.
   */
  sidebarProjectGroupingOverrides: Readonly<Record<string, SidebarProjectGroupingMode>>;
  /**
   * CDXC:SidebarV2Worktree 2026-07-29:
   * Default environment for a new session started from Sidebar V2's "+".
   * "local" keeps the instant in-project session; "worktree" makes the same
   * click open the worktree popover pre-filled instead.
   *
   * This is GLOBAL rather than per-project on purpose (see the plan's P4
   * notes): per-project storage would mean a new `gitConfig` field, a new
   * settings message, a new projection, and a Projects-tab control, for a
   * preference users set once. A per-project override can be layered on later
   * without changing this key's meaning.
   */
  newSessionsDefaultEnvMode: SidebarNewSessionEnvMode;
  sidebarSide: SidebarSide;
  /** Duration for sidebar section, group, and project disclosure animations. */
  sidebarCollapseAnimationDurationMs: number;
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
  /** Visual treatment for user-created project groups in the shared sidebar. */
  sidebarProjectGroupStyle: SidebarProjectGroupStyle;
  /**
   * CDXC:ProjectHotkeys 2026-06-15-11:12:
   * Jump to Project shortcuts should reveal the target project row when it was collapsed, because the keyboard action is also a navigation intent in the visible Projects sidebar area.
   */
  expandCollapsedProjectsOnJump: boolean;
  /**
   * CDXC:ProjectHotkeys 2026-06-15-11:12:
   * Some users want a project jump to reveal only the target project header plus the configured Show less slice after auto-expanding a collapsed project. Keep that secondary behavior opt-in and only meaningful when auto-expand is enabled.
   */
  showLessForExpandedProjectJumps: boolean;
  sidebarTheme: SidebarThemeSetting;
  /** Theme for chat content only; the surrounding Ghostex chrome stays dark. */
  sessionChatTheme: SessionChatTheme;
  /** CSS font-family used by chat messages and the prompt composer. */
  sessionChatFontFamily: string;
  /** Transcript width on a 64rem scale; 75% preserves the historical 48rem cap. */
  sessionChatTranscriptWidthPercent: number;
  /**
   * Reveal thinking-owned tool calls by default in Session Chat. Chats that
   * use the composer's Verbose pill store their own value and stop following
   * this (packages/core-ui/chat/session-chat-verbose-override.ts).
   */
  sessionChatVerboseMode: boolean;
  /**
   * CDXC:SidebarTitlebarColors 2026-06-15-11:24:
   * Custom chrome colors are scoped to the sidebar and native titlebar only.
   * Keep these separate from theme tokens so modals, dropdowns, and the
   * disabled theme selector keep using Dark Gray/Dark 2 defaults.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-13:22:
   * Settings still carries a foreground field for compatibility with native
   * layout payloads and older stored settings, but normalization derives it
   * from the background instead of preserving user-entered foreground values.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-13:45:
   * Users now tune the custom sidebar/titlebar background through a contrast
   * slider. Keep the background color field as the computed dark protocol
   * value, not as a user-editable setting.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:15:
   * The user-facing Settings control is named Contrast, but this protocol keeps
   * its darkness key so stored settings and native payloads remain compatible.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:28:
   * Tint is stored as a separate web-picker color and folded into the computed
   * background hex. The native/sidebar consumers still receive one final
   * background color, preserving their existing contract.
   *
   * CDXC:SettingsTheming 2026-06-15-21:35:
   * The old custom sidebar/titlebar contrast toggle is retired from Settings.
   * Keep this compatibility field enabled after normalization so visible
   * Theming controls apply without a hidden or experimental gate.
   */
  customSidebarTitlebarColorsEnabled: boolean;
  customSidebarTitlebarForegroundColor: string;
  customSidebarTitlebarBackgroundTintColor: string;
  customSidebarTitlebarBackgroundDarknessPercent: number;
  customSidebarTitlebarBackgroundColor: string;
  /**
   * CDXC:AccentColor 2026-08-24:
   * Hex accent color published to every React surface as --ghostex-accent.
   */
  accentColor: string;
  terminalCursorStyle: TerminalCursorStyle;
  terminalCursorStyleBlink: boolean;
  terminalEngine: TerminalEngine;
  /**
   * Windows currently runs terminals only through WSL2. The optional
   * distribution override selects an exact initialized distro when automatic
   * discovery cannot choose the intended install.
   */
  windowsTerminalBackend: WindowsTerminalBackend;
  windowsWslDistribution: string;
  terminalFontFamily: string;
  terminalFontSize: number;
  terminalFontWeight: number;
  terminalGhosttyTheme: string;
  terminalBackgroundImage: string;
  terminalBackgroundImageOpacity: number;
  terminalBackgroundImageFit: TerminalBackgroundImageFit;
  terminalLetterSpacing: number;
  terminalLineHeight: number;
  /**
   * CDXC:TerminalPanePadding 2026-06-25-21:27:
   * Terminal pane padding is app layout, not a Ghostty config key. Store
   * separate horizontal and vertical pixel values so Settings can inset every
   * native terminal surface while preserving the pane titlebar, borders,
   * splitters, and Ghostty background color.
   */
  terminalPaneHorizontalPaddingPx: number;
  terminalPaneVerticalPaddingPx: number;
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
  /**
   * CDXC:TerminalDevServers 2026-06-23-19:22:
   * Dev-server discovery is app-owned terminal behavior, not a terminal emulator config key. Persist detection, a single open-target choice, and ignored ports with the main settings contract so Terminal settings stay focused on opening in the user's system browser or the internal browser instead of exposing per-browser checkboxes.
   */
  terminalDevServerDetectionEnabled: boolean;
  terminalDevServerIgnoredPortRules: readonly string[];
  /**
   * CDXC:PortlessSettings 2026-06-22-22:35:
   * Portless is a global app contract, not project state. Keep one default-on toggle and one protocol setting so every project/worktree shares the same local proxy mode without per-project enablement keys.
   */
  portlessEnabled: boolean;
  portlessProtocol: PortlessProtocol;
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
  /**
   * Where the command pane docks in the desktop workspace: below the active
   * workspace (default) or as a column to its right. Terminal Actions and
   * F12 open the pane on the configured side.
   */
  commandsPanelSide: CommandsPanelSide;
};

export type ghostexSettingsPatch = Partial<ghostexSettings>;

export type ghostexSettingsUpdateSource =
  | 'firstLaunch:preferences'
  | 'settings:bulk'
  | 'settings:control'
  | 'settings:navigation'
  | 'settings:remoteMachines'
  | 'sidebar:remoteMachineOrder'
  /**
   * CDXC:SidebarV2 2026-07-29:
   * The sidebar Sort & Filter menu can switch the sidebar version and its
   * Group by Project sub-mode. Those writes come from the sidebar surface, not
   * from the Settings modal, so they carry their own source and must never be
   * treated as a remote-machine-capable save.
   */
  | 'sidebar:sidebarVersion'
  /**
   * CDXC:SidebarV2Worktree 2026-07-29:
   * Sidebar V2's "+" menu can flip the default environment for new sessions.
   * Same reasoning as the version switch above: a sidebar-surface write, never
   * a remote-machine-capable save.
   */
  | 'sidebar:newSessionsDefaultEnvMode'
  /**
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * Sidebar V2's project group header can change how one checkout merges with
   * other checkouts of the same repository. Same reasoning as the two sources
   * above: a sidebar-surface write, never a remote-machine-capable save.
   */
  | 'sidebar:projectGrouping';

export function canSettingsUpdateSourceChangeRemoteMachines(source: ghostexSettingsUpdateSource | undefined): boolean {
  /*
   * CDXC:RemoteMachines 2026-06-30-15:18:
   * Remote machine settings must not be rewritten by broad Settings saves such
   * as tab, scroll, preset, or reset updates. Only explicit remote-machine UI
   * and sidebar ordering operations may replace the saved machine list.
   */
  return source === 'settings:remoteMachines' || source === 'sidebar:remoteMachineOrder';
}

export const SIDEBAR_SETTINGS_PRESET_KEYS = [
  'showProjectIcons',
  'hideSessionAgentIconUntilHover',
  'hideBrowserFaviconUntilHover',
  'showCloseButtonOnSessionCards',
  'hideLastActiveTimeOnSessionCards',
  'hideProjectHeaderDiffStats',
  'showProjectEditorDiffFileCount',
  'hideMenuBarSessionStatusIndicators',
] as const satisfies ReadonlyArray<keyof ghostexSettings>;

export type SidebarSettingsPresetKey = (typeof SIDEBAR_SETTINGS_PRESET_KEYS)[number];
export type SidebarSettingsPresetSettings = Pick<ghostexSettings, SidebarSettingsPresetKey>;
