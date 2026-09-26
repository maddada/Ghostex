import { type SidebarThemeSetting } from '../session-grid-contract-core';
import type { ContentThemeSetting } from '../appearance';
import type { DarkThemePreset, LightThemePreset } from './titlebar-color';
import { DEFAULT_COMMANDS_PANEL_HEIGHT_PX } from '../session-grid-contract-session';
import { type SessionChatThemeSetting } from '../session-chat';
import { type CompletionSoundPreference, type CompletionSoundSetting } from '../completion-sound';
import { type ghostexHotkeySettings } from '../ghostex-hotkeys';
import { type CustomWorkspaceOpenTarget, type WorkspaceOpenTargetAvailability } from '../workspace-open-targets';
import { type PetId } from '../pets';
import { type SidebarSessionTagListItem } from '../session-tags';
import { type SessionCardHoverButtonItem } from '../session-card-hover-actions';
import type { ProjectViewTemplate } from './project-views';
import type { ProjectWebsiteSettings } from './project-websites';
import { type GhostexCustomView } from './custom-views';
import { type GhostexViewScopes } from './view-scopes';
import { type DiagnosticLoggingSettings } from './diagnostic-logging';
import { type RemoteMachineSettings } from './remote-machines';
import { type SettingsModalNavigationState } from './settings-modal-navigation';

export type GhosttyConfirmCloseSurface = 'false' | 'true' | 'always';
export type GhosttyCopyOnSelect = 'false' | 'true' | 'clipboard';
export type GhosttyScrollbar = 'system' | 'never';
export type TerminalCursorStyle = 'bar' | 'block' | 'underline';
export type TerminalBackgroundImageFit = 'cover' | 'contain' | 'stretch' | 'natural';
export type TerminalViewWidthMode = 'full' | 'match-chat' | 'custom';
export type PortlessProtocol = 'https' | 'http';
/**
 * CDXC:Navigation 2026-08-19:
 * One answer to "where does a web link Ghostex opens land". Command-clicked
 * terminal links, session chat links, and detected dev-server rows all read
 * this single target instead of the old split between a Browser toggle and a
 * Dev Servers dropdown, which could disagree with each other.
 */
export type WebLinkOpenTarget = 'internal-browser' | 'system-default-browser';
export type ChatFileOpenView = 'docs' | 'code';
export type DefaultEditorCommand =
  'code' | 'code-insiders' | 'zed' | 'zeditor' | 'cursor' | 'windsurf' | 'codium' | 'subl' | 'other';
export type CommandsPanelSide = 'bottom' | 'right';
export type WindowGlassMode = 'auto' | 'frosted' | 'opaque';
export type WindowGlassSource = 'wallpaper' | 'desktopAndWindows' | 'customImage' | 'video' | 'live';

/** The animated styles Live glass draws (`live_<id>` in the GPUI macOS crate's live_backdrop.metal). */
export type WindowGlassLiveStyle = 'aurora' | 'ink' | 'drift' | 'nebula' | 'silk' | 'bokeh' | 'waves' | 'mesh';
export type WindowGlassImagePlacement = 'static' | 'desktop';
export type PanelAnimationSpeed = 'off' | 'slow' | 'normal' | 'fast';
export const MIN_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT = 40;
export const MAX_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT = 100;
export const MIN_WINDOW_GLASS_WORK_AREA_TINT_PERCENT = 0;
export const MAX_WINDOW_GLASS_WORK_AREA_TINT_PERCENT = 100;
/**
 * CDXC:Theming 2026-09-25 DECISION:
 * User picked "Default to 20" for new users' Transparency strength, so a fresh install lands on a point of the
 * strength slider (sidebar 88 / work area 81 in dark, 93 / 86 in light; `transparencyStrengthPatch(20)`) instead of
 * showing Custom. Supersedes the sidebar 80 / work area 88 defaults. SEE-ALSO: the matching constants in
 * apps/desktop/src/app/helpers/window_glass.rs.
 */
export const DEFAULT_WINDOW_GLASS_SIDEBAR_OPACITY_DARK_PERCENT = 88;
export const DEFAULT_WINDOW_GLASS_SIDEBAR_OPACITY_LIGHT_PERCENT = 93;
export const DEFAULT_WINDOW_GLASS_WORK_AREA_TINT_DARK_PERCENT = 81;
export const DEFAULT_WINDOW_GLASS_WORK_AREA_TINT_LIGHT_PERCENT = 86;

export function clampWindowGlassSidebarOpacityPercent(value: number, fallback: number): number {
  if (!Number.isFinite(value)) return fallback;
  return Math.round(
    Math.min(MAX_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT, Math.max(MIN_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT, value))
  );
}

export function clampWindowGlassWorkAreaTintPercent(value: number, fallback: number): number {
  if (!Number.isFinite(value)) return fallback;
  return Math.round(
    Math.min(MAX_WINDOW_GLASS_WORK_AREA_TINT_PERCENT, Math.max(MIN_WINDOW_GLASS_WORK_AREA_TINT_PERCENT, value))
  );
}

/**
 * The work area's own tint for a user who set the retired extra-layer slider
 * (`windowGlassMainOpacity*`), which lay over the sidebar tint: the coverage the two layers
 * added up to, so the work area looks the same after the tints were made independent.
 */
export function migrateWindowGlassWorkAreaTintPercent(sidebarPercent: number, extraPercent: number): number {
  const sidebar = Math.min(100, Math.max(0, sidebarPercent)) / 100;
  const extra = Math.min(100, Math.max(0, extraPercent)) / 100;
  return Math.round((1 - (1 - sidebar) * (1 - extra)) * 100);
}
export type SidebarSpaceSwitchBehavior = 'restore' | 'keep';
export type SidebarVisibilityMemory = 'shared' | 'perView';
export const MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS = 0;
export const MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS = 1000;
export const SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS = 100;
export const DEFAULT_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS = 400;
export const MIN_SIDEBAR_TOOLTIP_DELAY_MS = 0;
export const MAX_SIDEBAR_TOOLTIP_DELAY_MS = 2_000;
export const SIDEBAR_TOOLTIP_DELAY_STEP_MS = 100;
export const DEFAULT_SIDEBAR_TOOLTIP_DELAY_MS = 600;
/** CDXC:SessionChat 2026-09-13 DECISION:
 * User: Settings > Chat offers a default chat zoom from 70% to 200% in 5% steps.
 */
export const MIN_SESSION_CHAT_ZOOM_PERCENT = 70;
export const MAX_SESSION_CHAT_ZOOM_PERCENT = 200;
export const SESSION_CHAT_ZOOM_PERCENT_STEP = 5;
export const DEFAULT_SESSION_CHAT_ZOOM_PERCENT = 100;

export function clampSessionChatZoomPercent(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_SESSION_CHAT_ZOOM_PERCENT;
  const clamped = Math.min(MAX_SESSION_CHAT_ZOOM_PERCENT, Math.max(MIN_SESSION_CHAT_ZOOM_PERCENT, value));
  return Math.round(clamped / SESSION_CHAT_ZOOM_PERCENT_STEP) * SESSION_CHAT_ZOOM_PERCENT_STEP;
}

export const MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT = 50;
export const MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT = 100;
export const SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP = 5;
export const DEFAULT_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT = 75;
export const MIN_TERMINAL_VIEW_WIDTH_PERCENT = MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT;
export const MAX_TERMINAL_VIEW_WIDTH_PERCENT = MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT;
export const TERMINAL_VIEW_WIDTH_PERCENT_STEP = SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP;
export const DEFAULT_TERMINAL_VIEW_WIDTH_PERCENT = DEFAULT_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT;
export const DEFAULT_TERMINAL_VIEW_WIDTH_MODE: TerminalViewWidthMode = 'full';
export const TERMINAL_VIEW_WIDTH_MODE_OPTIONS: readonly { label: string; value: TerminalViewWidthMode }[] = [
  { label: 'Full', value: 'full' },
  { label: 'Match Chat', value: 'match-chat' },
  { label: 'Custom', value: 'custom' },
];

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

export function clampTerminalViewWidthPercent(value: number): number {
  return clampSessionChatTranscriptWidthPercent(value);
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

export function clampSidebarTooltipDelayMs(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_SIDEBAR_TOOLTIP_DELAY_MS;
  }
  const clamped = Math.min(MAX_SIDEBAR_TOOLTIP_DELAY_MS, Math.max(MIN_SIDEBAR_TOOLTIP_DELAY_MS, value));
  return Math.round(clamped / SIDEBAR_TOOLTIP_DELAY_STEP_MS) * SIDEBAR_TOOLTIP_DELAY_STEP_MS;
}
/** The surface shown first when a newly launched agent supports Session Chat. */
export type PreferredAgentInterface = 'terminal' | 'chat';
export type SidebarSettingsPresetId = 'codex' | 'minimal' | 'detailed' | 'recommended';
export type PromptEditorBackend = 'inherit' | 'monaco';
export type SessionTitleGenerationAgent = 'codex' | 'cursor' | 'claude' | 'grok' | 'pi' | 'antigravity' | 'custom';
export type AppShotsHotkey = 'both-command' | 'both-shift' | 'both-option' | 'double-left-shift' | 'double-left-option';
export type KeepAwakeDurationMinutes = 0 | 120 | 300;
export type AutoSleepIdleMinutes = 0 | 5 | 10 | 15 | 30 | 60 | 120 | 300;
export const DEFAULT_TERMINAL_PANE_HORIZONTAL_PADDING_PX = 16;
export const DEFAULT_TERMINAL_PANE_PADDING_PX = 0;
export const MIN_TERMINAL_PANE_PADDING_PX = 0;
export const MAX_TERMINAL_PANE_PADDING_PX = 64;
export const MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX = 40;
export const MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX = 600;
export const DEFAULT_SIDEBAR_DEFAULT_WIDTH_PX = 275;
export const MIN_SIDEBAR_DEFAULT_WIDTH_PX = 150;
export const MAX_SIDEBAR_DEFAULT_WIDTH_PX = 520;
export const DEFAULT_PROJECT_SWITCH_KEEP_ALIVE_MINUTES = 10;
export const MIN_PROJECT_SWITCH_KEEP_ALIVE_MINUTES = 0;
export const MAX_PROJECT_SWITCH_KEEP_ALIVE_MINUTES = 60;
export const DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT = 13;
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

export function clampProjectSwitchKeepAliveMinutes(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_PROJECT_SWITCH_KEEP_ALIVE_MINUTES;
  }
  return Math.min(
    MAX_PROJECT_SWITCH_KEEP_ALIVE_MINUTES,
    Math.max(MIN_PROJECT_SWITCH_KEEP_ALIVE_MINUTES, Math.round(value))
  );
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
 * CDXC:Icons 2026-05-12-07:35
 * Public app copy uses Ghostex, and public terminal commands use `ghostex`
 * with `gx` as the short alias. The codebase can keep ghostex in type names,
 * storage/protocol keys, file paths, and implementation identifiers.
 *
 * CDXC:Icons 2026-05-26-15:11
 * New installs should expose `gx` instead of the older `gtx` command, and setup
 * should not claim `gx` when another tool already owns that binary name.
 *
 * CDXC:Icons 2026-05-15-11:54
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
   * CDXC:AgentProviders 2026-06-02-22:23:
   * This field is the sidebar render cache for gxserver-owned global agent approval
   * settings. Settings UI can display and edit it, but gxserver persists the
   * canonical value and applies each agent's runtime permission-bypass mode.
   *
   * CDXC:AgentProviders 2026-06-09-14:22:
   * OpenCode's run-without-asking mode is runtime config rather than a CLI flag, so settings
   * copy and storage must describe the policy without promising flag insertion.
   */
  agentAcceptAllEnabled: boolean;
  agentManagerZoomPercent: number;
  /**
   * CDXC:AgentLauncher 2026-05-28-07:15:
   * Automated prompt flows such as Git helper prompts, project board Start Work,
   * and worktree first prompts need one user-selected default agent instead of
   * hardcoding Codex in each launcher.
   *
   * CDXC:AgentProviders 2026-06-19-08:58:
   * gxserver now owns the canonical Default Prompt Agent alongside global
   * agent approval policy. Keep this field as the sidebar's synchronous render cache so
   * Settings can draw immediately from startup snapshots and gxserver update
   * responses without localStorage becoming a competing source of truth.
   */
  defaultPromptAgentId: string;
  /**
   * CDXC:SessionTitles 2026-06-04-08:24:
   * First-prompt session-title generation is gxserver-owned, but Settings owns
   * which headless agent command should produce those titles. Keep this scoped
   * away from Default Prompt Agent so changing title generation does not alter
   * Git prompts, worktree starts, or project-board prompts.
   *
   * CDXC:SessionTitles 2026-06-04-22:44:
   * The selector includes Grok Build and its Composer 2.5 command preview, so
   * users can see the exact headless CLI command Ghostex will send before
   * automatic first-prompt session naming runs.
   */
  sessionTitleGenerationAgent: SessionTitleGenerationAgent;
  customSessionTitleGenerationCommand: string;
  /**
   * CDXC:Navigation 2026-07-02-13:05:
   * Command-clicked http/https terminal links open as tabs in the project
   * Browser view by default. Pointing this at the system default browser
   * restores handing web links to it. File paths and non-web schemes always
   * keep the external NSWorkspace route regardless of this setting.
   *
   * CDXC:SessionChat 2026-08-18:
   * Web links clicked in session chat follow the same switch, so this is the
   * single answer to "where do agent-sent web links open". Chat file links
   * still open in Docs or Code, and Shift+click still forces the system
   * default browser while the target is the internal browser.
   *
   * CDXC:Navigation 2026-08-19:
   * Detected dev-server rows read this too, replacing the separate Dev Servers
   * open-target dropdown. Migrated from the legacy openTerminalLinksInApp
   * boolean, which wins over the legacy dev-server target when both persist,
   * because it is the switch users actually flipped.
   */
  webLinkOpenTarget: WebLinkOpenTarget;
  /** Preferred workarea for Markdown file links clicked in session chat. */
  markdownFileOpenView: ChatFileOpenView;
  /** Preferred workarea for HTML file links clicked in session chat. */
  htmlFileOpenView: ChatFileOpenView;
  /**
   * CDXC:Settings 2026-06-28-08:01:
   * Show Advanced is a persisted Settings browsing preference. When users enable
   * advanced rows, keep that density enabled across app restarts until they
   * explicitly turn it off again.
   */
  showAdvancedSettings: boolean;
  /**
   * CDXC:Settings 2026-06-29-17:54:
   * Closing Settings should persist the user's current Settings page and
   * scroll offsets so relaunching the macOS app can reopen Settings at the
   * exact spot they left, while explicit deep links can still override it.
   *
   * CDXC:Settings 2026-06-30-04:47:
   * Persist page navigation as the user moves through Settings because the
   * native AppKit close button can tear down the child window before React's
   * dialog-close callback runs.
   */
  settingsModalNavigation: SettingsModalNavigationState;
  /**
   * CDXC:Settings 2026-06-28-07:41:
   * Enable Experimental Features is the user-facing name for this persisted
   * showBetaFeatures key. Experimental surfaces stay hidden by default, while
   * Agents Hub remains outside this gate and visible in the sidebar.
   *
   * CDXC:Automations 2026-09-17:
   * All Automations and project Automate are experimental macOS surfaces.
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
  terminalViewTabHidden: boolean;
  storybookViewTabHidden: boolean;
  linearViewTabHidden: boolean;
  jiraViewTabHidden: boolean;
  githubViewTabHidden: boolean;
  sentryViewTabHidden: boolean;
  figmaViewTabHidden: boolean;
  vercelViewTabHidden: boolean;
  supabaseViewTabHidden: boolean;
  githubActionsViewTabHidden: boolean;
  posthogViewTabHidden: boolean;
  customWebsiteViewTabHidden: boolean;
  projectWebsiteViews: ProjectWebsiteSettings;
  /**
   * Quick-access switches affect only the matching right-side titlebar button.
   * The menus and commands remain available through their other entry points.
   */
  tipsAndTricksTitlebarButtonHidden: boolean;
  notificationsTitlebarButtonHidden: boolean;
  helpTitlebarButtonHidden: boolean;
  resourcesTitlebarButtonHidden: boolean;
  devServersTitlebarButtonHidden: boolean;
  extensionsTitlebarButtonHidden: boolean;
  gitActionsTitlebarButtonHidden: boolean;
  quickActionsTitlebarButtonHidden: boolean;
  openInTitlebarButtonHidden: boolean;
  codeServerLinkVscodeUserConfig: boolean;
  codeServerUseVscodeInsidersUserConfig: boolean;
  customDefaultEditorCommand: string;
  /**
   * CDXC:Icons 2026-06-25-21:50: Persisted id of the selected Dock /
   * app-switcher icon. Empty string means the default bundled app icon. The
   * value is a filename living in the native icons folder; native confirms the
   * selection via an appIconState ok event before the sidebar persists it.
   */
  appIconSourceId: string;
  defaultEditorCommand: DefaultEditorCommand;
  hideProjectHeaderDiffStats: boolean;
  /**
   * CDXC:Docs 2026-06-30-19:47:
   * The Docs sidebar scans ./docs, ./artifacts, ./ai, and ./tmp recursively, plus those same folder names one level down, plus root artifacts by default. Users can add comma-separated project-relative folder roots from global Projects settings. Trim spaces around each folder name while preserving spaces inside names such as "my documents".
   *
   * CDXC:Docs 2026-08-09:
   * These folders are project-root-relative, always. A configured Docs directory is mounted as an ADDITIONAL top-level folder that always shows its whole tree, so it is never narrowed by this list (round 2 briefly made it a narrowing control for that root; additive mounting replaced that).
   */
  manageAdditionalDocsFolders: string;
  /**
   * CDXC:Projects 2026-08-02:
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
   * CDXC:Docs 2026-08-09:
   * Absolute folder Docs shows IN ADDITION to the project's own docs when a
   * project sets no Docs directory of its own.
   *
   * CDXC:Docs 2026-08-09:
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
  completionSound: CompletionSoundPreference;
  /** Play the short copy sound whenever something is copied to the clipboard. */
  copySound: boolean;
  showNotificationOnTerminalBell: boolean;
  createSessionOnSidebarDoubleClick: boolean;
  /**
   * CDXC:Hotkeys 2026-09-19 DECISION:
   * User: Previous/Next Session gets a Settings toggle to jump over sleeping sessions, but sleeping sessions are not skipped by default.
   */
  sidebarSessionCycleSkipsSleeping: boolean;
  /**
   * Enables the Park session action and the collapsible Parked section at the
   * bottom of each project's session list. Parking is durable session state;
   * disabling this preference only hides the organization feature and renders
   * parked sessions in the ordinary Sessions section again.
   */
  enableSessionParking: boolean;
  /**
   * Sleeps a session through its normal lifecycle immediately after the user
   * parks it. This remains opt-in so parking can continue to be organization
   * only for users who want parked sessions to keep running.
   */
  sleepSessionWhenParking: boolean;
  /**
   * Opens the Tag as menu when the user parks or snoozes a session, so the
   * session can be tagged in the same gesture. Shown in Settings as
   * "Park & Snooze with tags"; the key keeps its original name so saved
   * settings files carry over.
   */
  showTagMenuWhenParking: boolean;
  /**
   * Unparks a parked session when the user sends it an actual message, from
   * chat or by typing into its terminal. gxserver applies it, reading this key
   * from native-sidebar-settings.json; an absent key means on.
   */
  unparkAfterSendingMessage: boolean;
  /**
   * CDXC:Telemetry 2026-08-26:
   * File-level opt-out for the anonymous PostHog usage analytics gxserver
   * sends. Default true. gxserver reads this key straight out of
   * native-sidebar-settings.json and treats an absent key as enabled. There is
   * no Settings UI for this; `GHOSTEX_TELEMETRY_DISABLED` and `DO_NOT_TRACK`
   * are the supported switches. Turning it off stops capture and drops the
   * queue.
   */
  analyticsEnabled: boolean;
  debuggingMode: boolean;
  /**
   * CDXC:Diagnostics 2026-06-27-22:07:
   * Debugging Mode no longer acts as the broad disk-logging switch. Routine
   * persistent diagnostics are controlled by explicit scenario ids so users can
   * enable one repro area, such as GPUI app modals or macOS terminal focus,
   * without turning on every noisy log writer.
   */
  diagnosticLogging: DiagnosticLoggingSettings;
  renameSessionOnDoubleClick: boolean;
  /** Show project artwork or the first-letter square beside project names. */
  showProjectIcons: boolean;
  hideSessionAgentIconUntilHover: boolean;
  hideBrowserFaviconUntilHover: boolean;
  /**
   * CDXC:Sessions 2026-09-12 DECISION:
   * User: the buttons a session card reveals on hover are an ordered strip chosen in Settings, with the chevron as one of its draggable items: buttons right of the chevron always show, buttons left of it hide until the chevron is clicked, and each project remembers whether it is revealed across restarts.
   * Replaces the `showCloseButtonOnSessionCards` toggle and the "Show Close option in context menu" setting; the first is migrated into this list, the second is dropped because the list now decides where Close lives.
   */
  sessionCardHoverButtons: readonly SessionCardHoverButtonItem[];
  /**
   * CDXC:Sessions 2026-09-15 DECISION:
   * User: the enabled hover buttons also appear in the session context menu by default, leading the everyday rows in the card's right-to-left order. Close is the exception and never appears in the menu while it is on the card. Turning this off restores the 2026-09-12 rule where every enabled hover button leaves the context menu.
   */
  showSessionCardHoverButtonsInContextMenu: boolean;
  hideLastActiveTimeOnSessionCards: boolean;
  hideAccountEmails: boolean;
  /**
   * CDXC:Sessions 2026-06-13-17:50:
   * Settings owns the sidebar tag-filter presentation list: users can reorder
   * tags, move separators, hide rows, or disable selectable tag filters without
   * changing the durable session tag values stored on sessions.
   */
  sidebarSessionTagListItems: readonly SidebarSessionTagListItem[];
  /**
   * CDXC:SessionSleep 2026-05-28-08:06:
   * Auto Sleep is a settings-owned policy for retiring idle VS Code, Git,
   * Project, Manage, browser, and agent sessions through their native sleep paths.
   * Keep each surface independently configurable so users can preserve existing
   * editor behavior while opting agent terminals in separately.
   */
  autoSleepAgentIdleMinutes: AutoSleepIdleMinutes;
  autoSleepBrowserIdleMinutes: AutoSleepIdleMinutes;
  autoSleepCodeEditorIdleMinutes: AutoSleepIdleMinutes;
  autoSleepGitEditorIdleMinutes: AutoSleepIdleMinutes;
  autoSleepProjectEditorIdleMinutes: AutoSleepIdleMinutes;
  autoSleepRequireAgentResumeCommand: boolean;
  autoSleepFavoriteAgentSessions: boolean;
  keepAwakeActivateOnExternalDisplay: boolean;
  keepAwakeActivateOnLaunch: boolean;
  keepAwakeAllowDisplaySleep: boolean;
  keepAwakeBatteryThresholdPercent: number;
  keepAwakeDeactivateOnLowPowerMode: boolean;
  keepAwakeDeactivateOnUserSwitch: boolean;
  keepAwakeDefaultDurationMinutes: KeepAwakeDurationMinutes;
  /**
   * CDXC:KeepAwake 2026-06-23-08:20:
   * Users can opt into a Mac power hold while any session is Working, with the titlebar runtime extending that hold for a short reply window after work stops.
   */
  keepAwakeWhileWorkingSessions: boolean;
  keepAwakePreventLidSleep: boolean;
  hideKeepAwakeTitlebarControl: boolean;
  /**
   * CDXC:AgentLauncher 2026-08-01:
   * The Agents tab strip ships New Terminal and New Browser Tab buttons. Users
   * who run those from Global Actions or hotkeys can hide either one to make
   * room in the strip. The pane overflow button is deliberately not hideable —
   * it is the only way to reach the rest of the pane actions.
   */
  hideTabStripNewTerminalButton: boolean;
  hideTabStripNewBrowserButton: boolean;
  showMacOSAttentionNotifications: boolean;
  hideMenuBarSessionStatusIndicators: boolean;
  petOverlayEnabled: boolean;
  selectedPetId: PetId;
  showQuickModelPickerInTerminal: boolean;
  showSessionIdInTerminalPanes: boolean;
  /** Newly launched supported agents still start a terminal, then show this surface first. */
  preferredAgentInterface: PreferredAgentInterface;
  /**
   * Per-agent override of `preferredAgentInterface`, keyed by agent id.
   *
   * A missing key means "inherit the global Default Agent View", so inherit is
   * stored as key-absent rather than as a third value: that keeps the global
   * setting live for every agent the user never touched, and lets a user undo
   * an override without leaving a stale value behind. Only agents that support
   * Ghostex's Chat View can carry an override; every other agent can only run
   * in the terminal, so an entry for one is meaningless and normalization is
   * free to keep it (it simply never resolves anything different).
   *
   * Values are the wire-contract spellings `"chat"` and `"terminal"`. Unknown
   * values and malformed entries are dropped by normalization rather than
   * defaulting, so a hand-edited settings file cannot force an agent into a
   * view the user never chose.
   */
  preferredAgentInterfaceOverrides: Readonly<Record<string, PreferredAgentInterface>>;
  /** Duration for sidebar section, group, and project disclosure animations, and for the floating sidebar's slide. */
  sidebarCollapseAnimationDurationMs: number;
  /**
   * CDXC:Workarea 2026-09-23 SEE-ALSO: how fast the sidebar, the side panel, the Agents Panel and
   * the command pane slide open and shut; the user's decision and the durations live in
   * `apps/desktop/src/app/panel_motion.rs`. macOS Reduce Motion always snaps.
   */
  panelAnimationSpeed: PanelAnimationSpeed;
  /**
   * CDXC:Workarea 2026-09-26 SEE-ALSO: closing the last tab in the side panel closes the panel
   * instead of showing the "Open a view" picker; the user's decision lives on `close_view_tab` in
   * `apps/desktop/src/app/view_panel.rs`.
   */
  closeSidePanelWithLastTab: boolean;
  /** Delay before sidebar hover tooltips appear. */
  sidebarTooltipDelayMs: number;
  /**
   * CDXC:Sidebar 2026-06-05-04:40:
   * The sidebar default width is the reset target for a double-click on the
   * sidebar drag handle in Electron and native macOS. Restart hydration must
   * continue using the last persisted sidebarWidth so changing this default
   * does not erase the user's last manual resize.
   */
  sidebarDefaultWidthPx: number;
  /**
   * CDXC:Projects 2026-09-12 DECISION:
   * User: a Compact project list shows 13 rows by default, configurable up to 50, before its "Show all" row. The key keeps its 2026-06-13 name (then the Show less count, default ten) so saved settings carry over.
   */
  projectSessionListCollapsedCount: number;
  /**
   * CDXC:Spaces 2026-08-28:
   * Spaces (saved per-gxserver sidebar filters) are opt-in. gxserver keeps
   * owning the Space document regardless, so this only decides whether the
   * sidebar renders the Space row, filters by a Space, and offers the Spaces
   * membership submenus.
   */
  sidebarSpacesEnabled: boolean;
  /**
   * CDXC:Spaces 2026-09-11 DECISION:
   * User: switching Spaces should bring back the state each Space was last in so context switches are faster.
   * "Restore the Space's projects" (the default) reopens the session last active in that Space, in its project's remembered view, walks back to earlier sessions when the latest one is closed, and opens the Space's first project when nothing is remembered.
   * "Don't switch projects" keeps the old behaviour: only the sidebar filter changes.
   * Only meaningful while `sidebarSpacesEnabled` is on.
   */
  sidebarSpaceSwitchBehavior: SidebarSpaceSwitchBehavior;
  /**
   * CDXC:Workarea 2026-09-19 DECISION:
   * User: switching Spaces (which switches projects) must keep the previous project live in the background instead of closing and relaunching it every swap, with a slider for how long.
   * What was visible when the project was left (its terminals, chat pages, and the open view page) stays running for this many minutes; 0 releases it right away as before.
   */
  projectSwitchKeepAliveMinutes: number;
  /**
   * CDXC:Spaces 2026-09-11 DECISION:
   * User: add a toggle, off by default for now, that switches the selected Space to the one owning a session activated from outside it (Back/Forward, Search by Prompt, notifications, Previous Sessions).
   * The user may remove it after testing, so it stays a plain boolean under the Spaces switch.
   */
  sidebarSpaceFollowActiveSession: boolean;
  /**
   * CDXC:Workarea 2026-09-12 DECISION:
   * User: pane visibility is remembered for the whole app, never per project, so switching projects cannot toggle the sidebar on its own.
   * The companion and the Commands pane always follow the kind of view (Agents versus the wide views: Browser, Code, Docs, Kanban, Automate, extensions).
   * Whether the sessions sidebar also follows the view is this advanced dropdown; the default keeps one sidebar state everywhere while the per-view model is evaluated.
   * This supersedes the 2026-09-09 decision to remember the sidebar, companion and Commands pane per project and view.
   *
   * CDXC:Workarea 2026-09-20 WHY:
   * The companion clause has no object any more: the sessions column replaced it and is never
   * hidden. The two layouts survive with their meaning re-read rather than changed — "Agents" is a
   * window with no view open, "wide" a window with a view panel beside the sessions — which keeps
   * the decision's own rule intact, because a project switch still cannot move the sidebar.
   */
  sidebarVisibilityMemory: SidebarVisibilityMemory;
  /**
   * CDXC:Hotkeys 2026-06-15-11:12:
   * Jump to Project shortcuts should reveal the target project row when it was collapsed, because the keyboard action is also a navigation intent in the visible Projects sidebar area.
   */
  expandCollapsedProjectsOnJump: boolean;
  /**
   * CDXC:Hotkeys 2026-06-15-11:12:
   * Some users want a project jump to reveal only the target project header plus the configured Show less slice after auto-expanding a collapsed project. Keep that secondary behavior opt-in and only meaningful when auto-expand is enabled.
   */
  showLessForExpandedProjectJumps: boolean;
  sidebarTheme: SidebarThemeSetting;
  /** Follow the app theme, or override chat with Light, Dark, or System. */
  sessionChatTheme: SessionChatThemeSetting;
  /** CSS font-family used by chat messages and the prompt composer. */
  sessionChatFontFamily: string;
  /** Default zoom percentage for the desktop chat interface. */
  sessionChatZoomPercent: number;
  /** Whether the transcript departs from the prompt composer's 48rem column. */
  sessionChatCustomTranscriptWidthEnabled: boolean;
  /** Centered transcript width as a percentage of wide chat panes. */
  sessionChatTranscriptWidthPercent: number;
  /**
   * Reveal thinking-owned tool calls by default in Session Chat. Chats that
   * use the composer's Verbose pill store their own value and stop following
   * this (the `verbose` store, packages/gx-chat-core/src/composer/storage.rs).
   */
  sessionChatVerboseMode: boolean;
  /** CDXC:SessionChat 2026-09-13 DECISION:
   * User: Simple mode applies to all chats, and the chat menu and Settings > Chat reflect the same global toggle.
   */
  sessionChatSimpleMode: boolean;
  /** CDXC:SessionChat 2026-09-09 DECISION:
   * User: file edits default to a single collapsed row; Chat settings can opt into seven-line previews.
   */
  sessionChatFileEditPreviews: boolean;
  /** CDXC:SessionChat 2026-09-23 DECISION:
   * User: add a setting that stops the GPUI chat box from animating up and down. While on, scrolling the transcript never collapses the desktop chat box. It shipped on by default while the collapse bounced near the end of the transcript; once that bounce was fixed the user asked for auto collapse to be enabled by default, so the setting now defaults to off.
   */
  sessionChatKeepComposerExpanded: boolean;
  /**
   * CDXC:Theming 2026-06-15-11:24:
   * Custom chrome colors are scoped to the sidebar and native titlebar only.
   * Keep these separate from theme tokens so modals, dropdowns, and the
   * disabled theme selector keep using Dark Gray/Dark 2 defaults.
   *
   * CDXC:Theming 2026-06-15-13:22:
   * Settings still carries a foreground field for compatibility with native
   * layout payloads and older stored settings, but normalization derives it
   * from the background instead of preserving user-entered foreground values.
   *
   * CDXC:Theming 2026-06-15-13:45:
   * Users now tune the custom sidebar/titlebar background through a contrast
   * slider. Keep the background color field as the computed dark protocol
   * value, not as a user-editable setting.
   *
   * CDXC:Theming 2026-06-15-15:15:
   * The user-facing Settings control is named Contrast, but this protocol keeps
   * its darkness key so stored settings and native payloads remain compatible.
   *
   * CDXC:Theming 2026-06-15-15:28:
   * Tint is stored as a separate web-picker color and folded into the computed
   * background hex. The native/sidebar consumers still receive one final
   * background color, preserving their existing contract.
   *
   * CDXC:Theming 2026-06-15-21:35:
   * The old custom sidebar/titlebar contrast toggle is retired from Settings.
   * Keep this compatibility field enabled after normalization so visible
   * Theming controls apply without a hidden or experimental gate.
   */
  customSidebarTitlebarForegroundColor: string;
  customSidebarTitlebarBackgroundTintColor: string;
  customSidebarTitlebarBackgroundDarknessPercent: number;
  customSidebarTitlebarBackgroundColor: string;
  /**
   * CDXC:Theming 2026-09-22 DECISION:
   * User: add contrast and tint for light mode like dark mode, and show each pair only when its new
   * Light theme / Dark theme dropdown is set to Custom; the dropdowns otherwise offer preset themes.
   * The dark preset field defaults to Gray, but a saved settings file without it that already carries
   * non-default contrast or tint values migrates to Custom so the chrome it produced does not change.
   * The two background color fields hold the effective (preset or custom) chrome for each appearance.
   */
  darkThemePreset: DarkThemePreset;
  lightThemePreset: LightThemePreset;
  /** CDXC:Theming 2026-09-23 SEE-ALSO: `readThemeContrastPoints` in titlebar-color.ts and `theme_contrast_points` in apps/desktop/src/app/helpers/titlebar.rs. */
  themeSidebarContrast: number;
  themeWorkAreaContrast: number;
  customSidebarTitlebarLightBackgroundTintColor: string;
  customSidebarTitlebarLightBackgroundLightnessPercent: number;
  customSidebarTitlebarLightBackgroundColor: string;
  terminalCursorStyle: TerminalCursorStyle;
  terminalCursorStyleBlink: boolean;
  /**
   * Windows selects native PowerShell projects or a WSL2 workspace.
   * PowerShell is the default; an explicitly saved WSL selection is preserved.
   */
  windowsTerminalBackend: 'wsl' | 'powershell';
  windowsWslDistribution: string;
  terminalFontFamily: string;
  terminalFontSize: number;
  terminalFontWeight: number;
  terminalColorScheme: ContentThemeSetting;
  terminalGhosttyLightTheme: string;
  terminalGhosttyTheme: string;
  terminalBackgroundImage: string;
  terminalBackgroundImageOpacity: number;
  terminalBackgroundImageFit: TerminalBackgroundImageFit;
  terminalLetterSpacing: number;
  terminalLineHeight: number;
  /** How wide terminal content is relative to its pane and the chat transcript. */
  terminalViewWidthMode: TerminalViewWidthMode;
  /** Width of a centered terminal body while custom terminal width is selected. */
  terminalViewWidthPercent: number;
  /** Apply the narrower terminal width to command pane terminals too. */
  terminalWidthApplyToCommandPaneTerminals: boolean;
  /**
   * CDXC:Terminal 2026-06-25-21:27:
   * Terminal pane padding is app layout, not a Ghostty config key. Store
   * separate horizontal and vertical pixel values so Settings can inset every
   * native terminal surface while preserving the pane titlebar, borders,
   * splitters, and Ghostty background color.
   */
  terminalPaneHorizontalPaddingPx: number;
  terminalPaneVerticalPaddingPx: number;
  terminalMouseScrollMultiplierDiscrete: number;
  terminalMouseScrollMultiplierPrecision: number;
  terminalScrollToBottomWhenTyping: boolean;
  terminalScrollbackLimitMb: number;
  terminalCopyOnSelect: GhosttyCopyOnSelect;
  terminalConfirmCloseSurface: GhosttyConfirmCloseSurface;
  terminalClipboardTrimTrailingSpaces: boolean;
  terminalClipboardPasteProtection: boolean;
  /**
   * CDXC:Clipboard 2026-06-08-13:32:
   * Terminal image paste is app-owned behavior, not a Ghostty config key. Keep a
   * default-on setting so users can opt out of Cmd+V/Ctrl+V converting clipboard
   * images into previewable Markdown links that also render in Cmd-hover terminal
   * previews and the Ctrl+G Rich Prompt Editor.
   */
  terminalPastePreviewableImages: boolean;
  terminalMouseHideWhileTyping: boolean;
  terminalScrollbar: GhosttyScrollbar;
  /**
   * CDXC:Resources 2026-06-23-19:22:
   * Dev-server discovery is app-owned terminal behavior, not a terminal emulator config key. Persist detection, a single open-target choice, and ignored ports with the main settings contract so Terminal settings stay focused on opening in the user's system browser or the internal browser instead of exposing per-browser checkboxes.
   */
  terminalDevServerDetectionEnabled: boolean;
  terminalDevServerIgnoredPortRules: readonly string[];
  /**
   * CDXC:Portless 2026-06-22-22:35:
   * Portless is a global app contract, not project state. Keep one default-on toggle and one protocol setting so every project/worktree shares the same local proxy mode without per-project enablement keys.
   */
  portlessEnabled: boolean;
  portlessProtocol: PortlessProtocol;
  promptEditorBackend: PromptEditorBackend;
  hotkeys: ghostexHotkeySettings;
  /**
   * CDXC:Settings 2026-09-06 DECISION:
   * User: the active pane outline is optional advanced chrome, since most users do not need it.
   * Its advanced color picker belongs directly below the outline toggle and appears only while the outline is enabled.
   */
  showActivePaneOutline: boolean;
  workspaceActivePaneBorderColor: string;
  workspaceBackgroundColor: string;
  /**
   * CDXC:Theming 2026-09-23 SEE-ALSO:
   * The desktop resolves and paints this in apps/desktop/src/app/helpers/window_glass.rs, which holds the user's decision on what Automatic means.
   */
  windowGlass: WindowGlassMode;
  /**
   * CDXC:Theming 2026-09-23 SEE-ALSO:
   * What the glass blurs: only the desktop wallpaper, or everything behind the window. window_glass.rs holds the user's decision and the GPUI macOS window paints it.
   */
  windowGlassSource: WindowGlassSource;
  /**
   * CDXC:Theming 2026-09-23 SEE-ALSO:
   * Absolute paths of the pictures Custom image glass shows in dark and light mode (empty: none chosen, the live blur shows); window_glass.rs holds the user's decision and the GPUI macOS window paints them.
   */
  windowGlassImageDark: string;
  windowGlassImageLight: string;
  /**
   * CDXC:Theming 2026-09-23 SEE-ALSO:
   * The videos Video glass plays in dark and light mode: `aerial:<id>` for an aerial wallpaper macOS has downloaded, or the absolute path of a .mov/.mp4/.m4v file (empty: none chosen, the live blur shows). `windowGlassVideoOnlyOnPower` pauses them on battery. window_glass.rs holds the user's decision and the GPUI macOS window plays them.
   */
  windowGlassVideoDark: string;
  windowGlassVideoLight: string;
  /** Pauses the glass video, and the Live style's motion, while the computer runs on battery. */
  windowGlassVideoOnlyOnPower: boolean;
  /**
   * CDXC:Theming 2026-09-26 SEE-ALSO:
   * The animated style Live glass draws in dark and light mode and how fast it moves (0.25 to 2); window_glass_live.rs holds the user's decision and the GPUI macOS window draws it in the theme's colours.
   */
  windowGlassLiveStyleDark: WindowGlassLiveStyle;
  windowGlassLiveStyleLight: WindowGlassLiveStyle;
  windowGlassLiveSpeed: number;
  /** How bright the Live style is drawn, 10 to 100 percent (default 45). */
  windowGlassLiveBrightness: number;
  /**
   * CDXC:Theming 2026-09-23 SEE-ALSO:
   * Whether the Wallpaper only or Custom image picture covers the window and moves with it (static) or stays still against the screen (desktop); window_glass.rs holds the user's decision and the GPUI macOS window places it.
   */
  windowGlassImagePlacement: WindowGlassImagePlacement;
  /**
   * CDXC:Theming 2026-09-23 SEE-ALSO:
   * Percent of the desktop each area's own glass tint hides, per appearance: the sidebar and the work area are independent layers, so either can be the darker one. window_glass.rs holds the user's decision and paints them; normalize.ts migrates the retired `windowGlassMainOpacity*` extra layer.
   */
  windowGlassSidebarOpacityDark: number;
  windowGlassWorkAreaTintDark: number;
  windowGlassSidebarOpacityLight: number;
  windowGlassWorkAreaTintLight: number;
  /**
   * CDXC:SessionSleep 2026-06-13-01:44:
   * Sleeping native pane tabs should select their original split pane without
   * starting Ghostty immediately. Keep click-to-wake enabled by default so
   * users can inspect stable black placeholders and wake only by clicking the
   * pane body.
   */
  clickToWakeSleepingSessions: boolean;
  customViews: GhostexCustomView[];
  customViewTemplates: ProjectViewTemplate[];
  /**
   * CDXC:Extensions 2026-09-20 DECISION:
   * User (ruling 3A): each built-in view and extension carries a default of shown or hidden plus per-project and
   * per-space overrides, resolved project then space then default, so a view can be hidden in one project without
   * listing every other one. Supersedes the 2026-09-18 "Available in" allow-list. Keyed by `officialViewScopeKey` /
   * `extensionViewScopeKey`; a view with no entry is shown everywhere.
   */
  viewScopes: GhostexViewScopes;
  titlebarViewOrder: string[];
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
   * CDXC:RemotePairing 2026-09-03 DECISION:
   * User: "please add a toggle for tailscale. if tailscale is disabled" (the sentence was cut off).
   * Whether the Tailscale pairing path is offered at all. Easy Connect's switch talks to gxserver (`/api/updateTailcatState`) because the daemon runs the sidecar; Tailscale is only a checklist and a QR, so its switch is a settings-only client flag with no server counterpart.
   * Off: the Settings → Remote card stays collapsed and dimmed with an Off badge, its QR is not rendered, and the Remote Setup modal hides the Tailscale option.
   */
  remoteTailscaleEnabled: boolean;
  /**
   * CDXC:CommandPane 2026-05-30-10:05:
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
  commandsPanelAutoMinimize: boolean;
  commandsPanelAutoMinimizeDelaySeconds: number;
};

export type ghostexSettingsPatch = Partial<ghostexSettings>;

export type ghostexSettingsUpdateSource =
  | 'cli:settings'
  | 'firstLaunch:preferences'
  | 'settings:bulk'
  | 'settings:control'
  | 'settings:navigation'
  | 'settings:remoteMachines'
  | 'sidebar:remoteMachineOrder';

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
  'hideLastActiveTimeOnSessionCards',
  'hideProjectHeaderDiffStats',
  'showProjectEditorDiffFileCount',
  'hideMenuBarSessionStatusIndicators',
] as const satisfies ReadonlyArray<keyof ghostexSettings>;

export type SidebarSettingsPresetKey = (typeof SIDEBAR_SETTINGS_PRESET_KEYS)[number];
export type SidebarSettingsPresetSettings = Pick<ghostexSettings, SidebarSettingsPresetKey>;
