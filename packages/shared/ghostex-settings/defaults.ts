import { DEFAULT_AGENT_MANAGER_ZOOM_PERCENT } from '../session-grid-contract-core';
import { DEFAULT_COMMANDS_PANEL_HEIGHT_PX } from '../session-grid-contract-session';
import { DEFAULT_COMPLETION_SOUND } from '../completion-sound';
import { DEFAULT_ghostex_HOTKEYS } from '../ghostex-hotkeys';
import { DEFAULT_WORKSPACE_OPEN_TARGET_AVAILABILITY } from '../workspace-open-targets';
import { DEFAULT_PET_ID } from '../pets';
import { DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS } from '../session-tags';
import { DEFAULT_DIAGNOSTIC_LOGGING_SCENARIOS } from './diagnostic-logging';
import { DEFAULT_CHAT_FILE_OPEN_VIEW, DEFAULT_WEB_LINK_OPEN_TARGET } from './option-tables';
import { SIDEBAR_SETTINGS_PRESET_SETTINGS } from './presets';
import { DEFAULT_SETTINGS_MODAL_NAVIGATION_STATE } from './settings-modal-navigation';
import { DEFAULT_TERMINAL_DEV_SERVER_IGNORED_PORT_RULES } from './terminal-dev-servers';
import {
  DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
  DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR,
  DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_FOREGROUND_COLOR,
  getSidebarTitlebarBackgroundForDarkness,
} from './titlebar-color';
import {
  DEFAULT_ACCENT_COLOR,
  DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  DEFAULT_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
  DEFAULT_TERMINAL_VIEW_WIDTH_PERCENT,
  DEFAULT_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
  DEFAULT_SIDEBAR_DEFAULT_WIDTH_PX,
  DEFAULT_TERMINAL_PANE_HORIZONTAL_PADDING_PX,
  DEFAULT_TERMINAL_PANE_PADDING_PX,
  type PromptEditorBackend,
  type WebLinkOpenTarget,
  type ghostexSettings,
} from './types';

export const DEFAULT_ghostex_SETTINGS: ghostexSettings = {
  /**
   * CDXC:CompletionSounds 2026-05-29-12:00:
   * Action-completion feedback should use the plain shamisen sound by default;
   * shamisen reverb remains available from Settings for users who prefer it.
   */
  actionCompletionSound: 'shamisen',
  gpuiTitlebarActionCommandByProject: {},
  gpuiTitlebarOpenTargetByProject: {},
  /**
   * CDXC:AppShots 2026-06-13-19:51:
   * App Shots are a beta workflow and should be opt-in for first-run Settings
   * defaults and missing persisted settings. Keep the hotkey configured so
   * enabling the beta feature is a single explicit toggle.
   */
  appShotsEnabled: false,
  appShotsHotkey: 'both-command',
  /*
   * CDXC:AppShots 2026-06-29-02:59:
   * App Shot prompts should paste only the image link by default. Window metadata is useful for debugging and context-heavy cases, but it must be an explicit Settings opt-in so routine image prompts stay compact.
   */
  appShotsMetadataEnabled: false,
  /**
   * CDXC:GxserverAgentSettings 2026-06-02-22:23:
   * New installs should start with gxserver-owned Accept All enabled so built-in
   * and custom agent launches inherit permission-bypass mode unless the user
   * turns it off.
   */
  agentAcceptAllEnabled: true,
  agentManagerZoomPercent: DEFAULT_AGENT_MANAGER_ZOOM_PERCENT,
  defaultPromptAgentId: 'codex',
  sessionTitleGenerationAgent: 'codex',
  customSessionTitleGenerationCommand: '',
  /**
   * Browser feedback always uses Agentation. Keep the normalized field for
   * compatibility with older host payloads, but do not expose it in Settings.
   */
  browserFeedbackTool: 'agentation',
  /**
   * CDXC:BrowserPanes 2026-05-27-07:24
   * Browser actions should no longer expose or route through Chrome Canary attachment.
   * Normalize all browser-action launches to in-workspace browser panes so Settings and native startup do not preserve the old external Canary path.
   */
  browserOpenMode: 'browser-pane',
  /**
   * CDXC:TerminalLinkInAppBrowser 2026-07-02-13:05:
   * In-app link routing is the default so cmd-clicked web links land in the
   * project Browser view unless the user opts back into the system browser in
   * Settings.
   *
   * CDXC:GPUISessionChatLinks 2026-08-18:
   * Session chat web links share this default for the same reason.
   *
   * CDXC:WebLinkOpenTarget 2026-08-19:
   * Detected dev-server rows now share it as well, so a fresh install answers
   * every web link the same way instead of splitting chat and terminal links
   * from dev-server links.
   */
  webLinkOpenTarget: DEFAULT_WEB_LINK_OPEN_TARGET,
  markdownFileOpenView: DEFAULT_CHAT_FILE_OPEN_VIEW,
  htmlFileOpenView: DEFAULT_CHAT_FILE_OPEN_VIEW,
  /**
   * CDXC:SettingsAdvanced 2026-06-28-08:01:
   * New installs should start with ordinary Settings density, but an explicit
   * Show Advanced toggle is saved so restart hydration preserves the user's
   * last browsing mode.
   */
  showAdvancedSettings: false,
  /**
   * CDXC:SettingsNavigation 2026-06-29-17:54:
   * New installs start at General Settings. Once the user closes Settings, the
   * modal saves only navigation chrome state here, not search text or private
   * setting values beyond the already persisted preferences.
   *
   * CDXC:SettingsNavigation 2026-06-30-04:47:
   * Navigation writes can happen before close so native-window teardown does not
   * lose the last selected Settings page.
   */
  settingsModalNavigation: DEFAULT_SETTINGS_MODAL_NAVIGATION_STATE,
  /**
   * CDXC:ExperimentalFeatures 2026-06-28-07:41:
   * New installs and missing persisted settings should keep experimental
   * surfaces hidden until the user enables Enable Experimental Features from
   * Advanced Settings.
   *
   * CDXC:Automations 2026-07-01-03:24:
   * Automations Overview and project Automate start hidden behind their
   * coming-soon overlay until Enable Experimental Features is on.
   */
  showBetaFeatures: false,
  codeViewTabHidden: false,
  browserViewTabHidden: false,
  kanbanViewTabHidden: false,
  automateViewTabHidden: false,
  docsViewTabHidden: false,
  tipsAndTricksTitlebarButtonHidden: false,
  resourcesTitlebarButtonHidden: false,
  extensionsTitlebarButtonHidden: false,
  gitActionsTitlebarButtonHidden: false,
  quickActionsTitlebarButtonHidden: false,
  openInTitlebarButtonHidden: false,
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
   * Legacy external-IDE preference keys remain normalized so existing settings
   * files and generic Open in IDE actions stay readable. They are no longer
   * exposed in Settings or used by Agents Hub, which opens files in Source.
   */
  customDefaultEditorCommand: '',
  // CDXC:AppIconPicker 2026-06-25-21:50: New installs use the default bundled app icon (empty source id).
  appIconSourceId: '',
  defaultEditorCommand: 'code',
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
  hideProjectHeaderDiffStats: SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideProjectHeaderDiffStats,
  /**
   * CDXC:DocsSidebar 2026-06-30-19:47:
   * Additional Docs scan folders remain opt-in beyond the built-in ./docs,
   * ./artifacts, and ./ai roots plus root Markdown, HTML, and Excalidraw files.
   * A configured Docs directory adds its own tree on top of whatever this
   * lists (CDXC:DocsRootAdditive).
   */
  manageAdditionalDocsFolders: '',
  /**
   * CDXC:GlobalProjectDefaults 2026-08-02:
   * New installs ship every Global Default empty so project resolution stays
   * byte-for-byte identical to the pre-feature behavior until a user fills one in.
   */
  globalWorktreeCommand: '',
  globalBeadsDisplayKey: '',
  globalBeadsDirectory: '',
  globalDocsDirectory: '',
  /**
   * CDXC:ProjectDiffStats 2026-05-15-14:33:
   * Project-header git stats should hide the changed-file count by default and
   * show only added/removed line counts. Users can opt back into the file
   * number from Settings when they want the full diff summary.
   */
  showProjectEditorDiffFileCount: SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.showProjectEditorDiffFileCount,
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
  /**
   * CDXC:TerminalBellAttention 2026-07-01-01:13:
   * Plain terminal BEL events include ordinary shell feedback such as zsh
   * completion misses. Keep terminal-bell attention notifications opt-in so
   * Monaco prompt editing and agent completion alerts remain independent from
   * noisy terminal-emulator bells.
   */
  showNotificationOnTerminalBell: false,
  createSessionOnSidebarDoubleClick: false,
  enableSessionParking: false,
  /**
   * CDXC:AnonymousAnalytics 2026-08-26:
   * Anonymous usage analytics are on by default and opt-out. Nothing personal,
   * no prompts, no paths, no project names.
   */
  analyticsEnabled: true,
  debuggingMode: false,
  diagnosticLogging: {
    scenarios: DEFAULT_DIAGNOSTIC_LOGGING_SCENARIOS,
    version: 1,
  },
  renameSessionOnDoubleClick: false,
  showProjectIcons: SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.showProjectIcons,
  /**
   * CDXC:SidebarSessions 2026-05-16-08:46:
   * Agent identity remains configurable in Settings through an explicit
   * hover-only mode for quieter session lists.
   *
   * CDXC:SidebarSettingsPresets 2026-06-13-01:06:
   * Superseded by CDXC:SidebarSettingsPresets 2026-06-30-22:29.
   *
   * CDXC:SidebarSettingsPresets 2026-06-30-22:29:
   * Recommended is the first-run preset and keeps session agent icons visible
   * while showing detailed sidebar status chrome.
   */
  hideSessionAgentIconUntilHover: SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideSessionAgentIconUntilHover,
  /**
   * CDXC:SidebarSessionAgentIcons 2026-06-29-23:58:
   * New installs use colored agent logos so session identity stays visually
   * distinct without requiring a separate opt-in.
   *
   * CDXC:SidebarSessionAgentIcons 2026-06-30-22:40:
   * The same setting colors the selected agent launcher icon in project and
   * Quick headers, so the visible picker identity matches session cards.
   */
  useColoredSessionAgentIcons: true,
  /**
   * CDXC:BrowserPanes 2026-05-28-07:38:
   * Browser page favicons are page identity, not agent chrome. Keep them
   * visible in the default Codex and Detailed presets even when agent icons are
   * hover-only, while Minimal can hide favicons until hover for a quieter list.
   */
  hideBrowserFaviconUntilHover: SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideBrowserFaviconUntilHover,
  /**
   * CDXC:SidebarSessions 2026-05-09-17:00
   * Session-card close controls should be available out of the box. Users can
   * still turn the hover chrome off from Settings when they want quieter cards.
   */
  showCloseButtonOnSessionCards: SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.showCloseButtonOnSessionCards,
  /**
   * CDXC:SidebarSessions 2026-06-13-15:42
   * Recommended is the default sidebar style and hides session-card Last Active
   * timestamps by default. Settings still owns an explicit toggle for users who
   * want the timestamp back, and the setting must not affect project-header git
   * diff stats.
   */
  hideLastActiveTimeOnSessionCards: SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideLastActiveTimeOnSessionCards,
  showSessionCloseContextMenuAction: false,
  showSessionCommandCopyActions: false,
  showSessionDetailsCopyAction: false,
  /**
   * CDXC:SessionTagFilters 2026-06-13-17:50:
   * First-run sidebar tag filter settings should show the default triage tags,
   * the No tag filter, and the default separators. Users opt out by hiding or
   * disabling individual rows from the collapsed Sidebar Tags settings area.
   */
  sidebarSessionTagListItems: DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS,
  /**
   * CDXC:AutoSleep 2026-05-28-08:06:
   * Background VS Code, Project, and Git panes originally auto-slept after
   * fifteen minutes of idle time by default. Agent terminal auto-sleep starts
   * opt-in because it closes live user-created conversation surfaces.
   *
   * CDXC:AutoSleep 2026-06-15-18:31:
   * Heavy editor, Project, Git/Browser, and browser-session surfaces should
   * retire quickly by default because many awake webviews and code-server
   * processes make sidebar switching laggy. Use ten-minute idle windows for
   * browser and VS Code panes, retain five minutes for Git and Project panes,
   * and enable browser-session Auto Sleep while keeping agent terminals opt-in.
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
  autoSleepBrowserSessionsEnabled: true,
  autoSleepBrowserIdleMinutes: 10,
  autoSleepCodeEditorEnabled: true,
  autoSleepCodeEditorIdleMinutes: 10,
  autoSleepGitEditorEnabled: true,
  autoSleepGitEditorIdleMinutes: 5,
  autoSleepProjectEditorEnabled: true,
  autoSleepProjectEditorIdleMinutes: 5,
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
  keepAwakeWhileWorkingSessions: false,
  /**
   * CDXC:TitlebarKeepAwake 2026-05-28-19:28:
   * Closing a MacBook lid is not covered by the standard caffeinate idle-sleep assertion.
   * Keep lid-close sleep prevention as an explicit opt-in because it changes the system-wide `pmset disablesleep` policy with administrator approval.
   */
  keepAwakePreventLidSleep: false,
  /**
   * CDXC:TitlebarKeepAwake 2026-05-27-07:32:
   * The titlebar keep-awake affordance is optional chrome. Keep the per-control
   * hide preference off by default, but persist a Power setting that can remove
   * the titlebar control completely for users who do not use Mac sleep
   * management from Ghostex.
   *
   * CDXC:ExperimentalFeatures 2026-06-28-07:41:
   * Keep Awake is an experimental macOS feature. Enable Experimental Features
   * must be enabled before the titlebar button or runtime automation is
   * available; this preference only hides the button again inside that enabled
   * state.
   */
  hideKeepAwakeTitlebarControl: false,
  /**
   * CDXC:GlobalActions 2026-08-01:
   * Every built-in tab strip button stays visible until the user hides it, so
   * adding Global Actions never silently removes a control someone relies on.
   */
  hideTabStripNewTerminalButton: false,
  hideTabStripNewBrowserButton: false,
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
   * Floating and menu bar desktop status badges stay independently controlled.
   *
   * CDXC:SessionStatusIndicators 2026-06-15-02:01:
   * Floating session indicators previously started hidden for new installs, while the menu bar session indicator stayed visible unless that separate setting changed.
   *
   * CDXC:SessionStatusIndicators 2026-06-15-14:00:
   * Sidebar presets did not provide the legacy floating indicator value.
   *
   * CDXC:SessionStatusIndicators 2026-06-16-09:20:
   * The legacy floating indicator defaulted on before the surface was removed.
   *
   * CDXC:SessionStatusIndicators 2026-06-27-20:11:
   * The floating badge surface and Settings rows were removed from macOS and
   * GPUI. Keep this legacy key normalized so existing settings JSON remains
   * readable, but no current native or GPUI presentation should consume it.
   */
  hideFloatingSessionStatusIndicators: false,
  hideMenuBarSessionStatusIndicators: SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideMenuBarSessionStatusIndicators,
  petOverlayEnabled: false,
  selectedPetId: DEFAULT_PET_ID,
  /**
   * CDXC:SessionStatusIndicators 2026-05-07-18:20
   * The legacy AppKit floating session indicator size was persisted as a named
   * value rather than raw pixels.
   *
   * CDXC:SessionStatusIndicators 2026-06-27-20:11:
   * Floating size is retained only as a legacy normalized value after removing
   * the floating badge from macOS and GPUI. Do not expose it in Settings or
   * send it through native/GPUI status payloads.
   */
  sessionStatusIndicatorSize: 'medium',
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
  preferredAgentInterface: 'chat',
  /**
   * No per-agent overrides: every agent follows the global Default Agent View
   * until the user picks a different view for that agent in Settings > Agents.
   */
  preferredAgentInterfaceOverrides: {},
  /**
   * CDXC:SidebarV2 2026-07-29:
   * The classic sidebar stays the default for every user. Sidebar V2 must be
   * chosen explicitly, so a settings file without this key keeps V1 behavior.
   */
  sidebarVersion: 'v1',
  /**
   * CDXC:SidebarV2 2026-07-29:
   * Sidebar V2 opens with sessions grouped into collapsible project groups.
   * Users can switch back to the flat, position-stable inbox when preferred.
   */
  sidebarV2Layout: 'byProject',
  /**
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * Three days is the agreed default window, and it matches
   * `DEFAULT_AUTO_SETTLE_AFTER_DAYS` in server so a settings file that has
   * never been written and one that carries the default behave identically.
   */
  sidebarAutoSettleAfterDays: 3,
  /**
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * No overrides: every project follows the automatic origin-remote rule, which
   * is a no-op for anyone with a single machine.
   */
  sidebarProjectGroupingOverrides: {},
  /**
   * CDXC:SidebarV2Worktree 2026-07-29:
   * The plain "+" keeps its historic meaning until the user asks otherwise, so
   * an untouched settings file starts instant local sessions exactly as today.
   */
  newSessionsDefaultEnvMode: 'local',
  /**
   * CDXC:SidebarPlacement 2026-05-06-17:32
   * Sidebar side is a first-class setting so users can choose left or right
   * placement from Settings instead of relying on sidebar placement shortcuts.
   *
   * CDXC:SidebarCollapse 2026-06-12-02:23:
   * Cmd+B is reserved for complete sidebar collapse, so sidebar side placement
   * should remain an explicit setting or user-assigned command.
   */
  sidebarSide: 'left',
  sidebarCollapseAnimationDurationMs: DEFAULT_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
  /**
   * CDXC:SidebarChrome 2026-06-05-04:40:
   * First-run reset target is 275px, but users can change this Settings
   * value for explicit sidebar-handle double-click resets without changing the
   * last-width restore path used at app restart.
   */
  sidebarDefaultWidthPx: DEFAULT_SIDEBAR_DEFAULT_WIDTH_PX,
  projectSessionListCollapsedCount: DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  sidebarProjectGroupStyle: 'branched',
  expandCollapsedProjectsOnJump: true,
  showLessForExpandedProjectJumps: false,
  /**
   * CDXC:SidebarTheme 2026-06-15-02:29:
   * Theme selection is disabled again until the full theme system is ready.
   * Use Dark 2 as the active app theme and present it to users as Dark Gray.
   */
  sidebarTheme: 'dark-2',
  sessionChatTheme: 'dark',
  sessionChatFontFamily: '',
  sessionChatTranscriptWidthPercent: DEFAULT_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
  sessionChatVerboseMode: false,
  /**
   * CDXC:SidebarTitlebarColors 2026-06-15-11:24:
   * Custom sidebar/titlebar colors are scoped to the sidebar and titlebar.
   * The default background matches Dark Gray chrome without changing modal or
   * dropdown color tokens.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-13:22:
   * Foreground is derived from background luminance, so the default foreground
   * remains light for Dark Gray and flips to the dark foreground on light
   * custom backgrounds.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-13:45:
   * The custom background contrast slider defaults near Dark Gray and is
   * restricted to dark applied values to avoid arbitrary bright color blends
   * in sidebar rows.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:01:
   * Clamp the slider to 85-100 per visual review; lighter values made the
   * sidebar feel too gray.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:15:
   * Keep this persisted field named darkness for compatibility while Settings
   * labels the same control Background Contrast.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:28:
   * The tint picker originally defaulted to neutral #808080. The tint
   * algorithm now maps picker colors to very dark chrome backgrounds, so
   * neutral same-channel tints do not change Dark Gray chrome.
   *
   * CDXC:SidebarTitlebarColors 2026-06-16-14:28:
   * The custom chrome default is now 95 contrast with white #FFFFFF tint.
   * Store the computed default background with those controls so Settings,
   * native startup, and protocol snapshots agree.
   *
   * CDXC:SidebarTitlebarColors 2026-07-22:
   * Default app chrome to neutral #808080 at 93 Background Contrast,
   * resolving to #141414.
   *
   * CDXC:SettingsTheming 2026-06-15-21:35:
   * Background Contrast and Background Tint are standard Theming controls.
   * Enable the retained protocol field by default so the removed toggle cannot
   * make those visible controls inert.
   */
  customSidebarTitlebarColorsEnabled: true,
  customSidebarTitlebarForegroundColor: DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_FOREGROUND_COLOR,
  customSidebarTitlebarBackgroundTintColor: DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR,
  customSidebarTitlebarBackgroundDarknessPercent: DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
  customSidebarTitlebarBackgroundColor: getSidebarTitlebarBackgroundForDarkness(
    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR
  ),
  accentColor: DEFAULT_ACCENT_COLOR,
  /**
   * CDXC:GhosttyDefaults 2026-05-22-12:29:
   * New Ghostex terminals should default to the requested GitHub Dark terminal
   * profile: JetBrains Mono 13pt, bar cursor with blink, wght=300, 20% cell
   * height expansion, 15 MB scrollback, no copy-on-select, and one-to-one
   * precision/discrete mouse scrolling.
   */
  terminalCursorStyle: 'bar',
  terminalCursorStyleBlink: true,
  terminalEngine: 'ghostty-native',
  windowsTerminalBackend: 'wsl',
  windowsWslDistribution: '',
  terminalFontFamily: 'JetBrains Mono',
  terminalFontSize: 13,
  terminalFontWeight: 300,
  terminalGhosttyTheme: 'GitHub Dark',
  terminalBackgroundImage: '',
  terminalBackgroundImageOpacity: 1,
  terminalBackgroundImageFit: 'cover',
  terminalLetterSpacing: 0,
  terminalLineHeight: 1.2,
  terminalNarrowerViewEnabled: false,
  terminalViewWidthPercent: DEFAULT_TERMINAL_VIEW_WIDTH_PERCENT,
  terminalWidthApplyToCommandPaneTerminals: false,
  terminalPaneHorizontalPaddingPx: DEFAULT_TERMINAL_PANE_HORIZONTAL_PADDING_PX,
  terminalPaneVerticalPaddingPx: DEFAULT_TERMINAL_PANE_PADDING_PX,
  terminalMouseScrollMultiplierDiscrete: 1,
  terminalMouseScrollMultiplierPrecision: 1,
  terminalScrollToBottomWhenTyping: true,
  terminalScrollbackLimitMb: 15,
  terminalCopyOnSelect: 'false',
  terminalConfirmCloseSurface: 'true',
  terminalClipboardTrimTrailingSpaces: true,
  terminalClipboardPasteProtection: true,
  terminalPastePreviewableImages: true,
  terminalMouseHideWhileTyping: false,
  terminalScrollbar: 'system',
  /**
   * CDXC:TerminalDevServers 2026-06-23-19:22:
   * New installs should discover local dev servers from terminal output and start with no ignored ports.
   *
   * CDXC:WebLinkOpenTarget 2026-08-19:
   * Where a detected URL opens is no longer a Dev Servers choice; it follows webLinkOpenTarget with every other web link.
   */
  terminalDevServerDetectionEnabled: true,
  terminalDevServerIgnoredPortRules: DEFAULT_TERMINAL_DEV_SERVER_IGNORED_PORT_RULES,
  /**
   * CDXC:PortlessSettingsDisabled 2026-07-25:
   * Keep the Portless settings contract available for a later return, but new
   * and legacy settings snapshots must not opt into an app integration that is
   * currently hidden and disabled.
   */
  portlessEnabled: false,
  portlessProtocol: 'https',
  /**
   * CDXC:PromptEditorBackend 2026-05-13-15:58
   * Ctrl+G rich prompt editing originally defaulted to the floating Monaco editor.
   *
   * CDXC:PromptEditorBackend 2026-05-25-11:31:
   * Monaco is the out-of-the-box Ctrl+G prompt editor again. New installs should open the floating Monaco editor for local app terminals.
   *
   * CDXC:PromptEditorBackend 2026-06-30-00:08:
   * Settings must expose only Monaco and the user's machine default editor. Removed gte and custom selections migrate to inherit so Ctrl+G stops injecting a Ghostex-owned editor command when users choose the machine default path.
   */
  promptEditorBackend: 'monaco',
  customPromptEditorCommand: 'code --wait',
  /**
   * CDXC:GtePromptEditing 2026-05-22-09:56
   * The boolean mirrors keep legacy Ctrl+G prompt-editor settings readable while promptEditorBackend remains the source of truth for launch behavior.
   *
   * CDXC:PromptEditorBackend 2026-06-30-00:08:
   * The Settings UI no longer offers gte. New normalized settings keep these legacy mirrors false so removed gte choices cannot keep exporting a gte editor command.
   */
  richPromptEditingWithGte: false,
  useGteForCtrlGPromptEditing: false,
  hotkeys: DEFAULT_ghostex_HOTKEYS,
  workspaceActivePaneBorderColor: '#3b82f6',
  /**
   * CDXC:WorkspaceLayout 2026-06-07-16:53:
   * A near-black workspace background avoids platform compositor handling of
   * literal transparent black while keeping pane chrome visually black.
   */
  workspaceBackgroundColor: '#010101',
  clickToWakeSleepingSessions: true,
  /**
   * CDXC:TitlebarOpenIn 2026-05-11-00:22
   * The titlebar Open In menu is configurable: built-in editor targets can be
   * hidden and user-defined command targets can be appended without changing
   * the default editor catalog.
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
  commandsPanelSide: 'bottom',
};
