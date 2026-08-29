import { clampAgentManagerZoomPercent, clampSidebarThemeSetting } from '../session-grid-contract-session';
import { normalizeSessionChatTheme } from '../session-chat';
import { clampCompletionSoundPreference, clampCompletionSoundSetting } from '../completion-sound';
import {
  getGhosttyFontFamilyForPreset,
  getTerminalFontFamilyForPreset,
  normalizeTerminalFontPreset,
} from '../terminal-font-preset';
import { normalizeghostexHotkeySettings } from '../ghostex-hotkeys';
import { GHOSTTY_THEME_OPTIONS } from '../ghostty-theme-options';
import {
  normalizeCustomWorkspaceOpenTargets,
  normalizeWorkspaceOpenTargetAvailability,
  normalizeWorkspaceOpenTargetHiddenIds,
} from '../workspace-open-targets';
import { normalizePetId } from '../pets';
import { normalizeSidebarSessionTagListItems } from '../session-tags';
import { DEFAULT_ghostex_SETTINGS } from './defaults';
import { normalizeDiagnosticLoggingSettings } from './diagnostic-logging';
import {
  AUTO_SLEEP_IDLE_MINUTE_OPTIONS,
  CHAT_FILE_OPEN_VIEW_SET,
  DEFAULT_CHAT_FILE_OPEN_VIEW,
  DEFAULT_WEB_LINK_OPEN_TARGET,
  KEEP_AWAKE_DURATION_OPTIONS,
  WEB_LINK_OPEN_TARGET_SET,
} from './option-tables';
import { SIDEBAR_SETTINGS_PRESET_SETTINGS } from './presets';
import { clampNumber, isRecord, readBoolean, readLooseString, readNumber, readString } from './primitives';
import { normalizeRemoteMachineSettings } from './remote-machines';
import {
  normalizeCustomSessionTitleGenerationCommand,
  normalizeSessionTitleGenerationAgent,
} from './session-title-generation';
import { normalizeSettingsModalNavigationState } from './settings-modal-navigation';
import { normalizeTerminalDevServerIgnoredPortRules } from './terminal-dev-servers';
import {
  clampSidebarTitlebarBackgroundDarknessPercent,
  getSidebarTitlebarBackgroundDarknessForColor,
  getSidebarTitlebarBackgroundForDarkness,
  getSidebarTitlebarForegroundForBackground,
  normalizeSidebarTitlebarHexColor,
} from './titlebar-color';
import {
  type AppShotsHotkey,
  type AutoSleepIdleMinutes,
  type ChatFileOpenView,
  type CommandsPanelSide,
  type DefaultEditorCommand,
  type GhosttyConfirmCloseSurface,
  type GhosttyCopyOnSelect,
  type GhosttyScrollbar,
  type KeepAwakeDurationMinutes,
  type PortlessProtocol,
  type PreferredAgentInterface,
  type PromptEditorBackend,
  type SidebarProjectGroupStyle,
  type SidebarSettingsPresetId,
  type SidebarSide,
  type TerminalBackgroundImageFit,
  type TerminalCursorStyle,
  type TerminalViewWidthMode,
  type WebLinkOpenTarget,
  clampCommandsPanelDefaultHeightPx,
  clampProjectSessionListCollapsedCount,
  clampSessionChatTranscriptWidthPercent,
  clampTerminalViewWidthPercent,
  clampSidebarCollapseAnimationDurationMs,
  clampSidebarDefaultWidthPx,
  clampSidebarTooltipDelayMs,
  clampTerminalPanePaddingPx,
  type ghostexSettings,
} from './types';

const MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER = 0.25;
const MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER = 8;
const MIN_GHOSTTY_SCROLLBACK_LIMIT_MB = 1;
const MAX_GHOSTTY_SCROLLBACK_LIMIT_MB = 200;

function normalizeTitlebarProjectSelectionMap(candidate: unknown): Record<string, string> {
  if (!isRecord(candidate)) {
    return {};
  }
  const normalized: Record<string, string> = {};
  for (const [rawProjectId, rawSelection] of Object.entries(candidate).slice(0, 256)) {
    const projectId = rawProjectId.trim();
    const selection = typeof rawSelection === 'string' ? rawSelection.trim() : '';
    if (projectId.length === 0 || projectId.length > 512 || selection.length === 0 || selection.length > 512) {
      continue;
    }
    normalized[projectId] = selection;
  }
  return normalized;
}

function normalizeTerminalViewWidthMode(source: Record<string, unknown>): TerminalViewWidthMode {
  const candidate = source.terminalViewWidthMode;
  if (candidate === 'full' || candidate === 'match-chat' || candidate === 'custom') {
    return candidate;
  }
  // The removed boolean had only two states. Preserve its explicit narrow
  // state as Custom; its old default false migrates to the new Match Chat
  // default instead of pinning existing installs to Full.
  return source.terminalNarrowerViewEnabled === true ? 'custom' : DEFAULT_ghostex_SETTINGS.terminalViewWidthMode;
}

export function normalizeghostexSettings(candidate: unknown): ghostexSettings {
  const source = isRecord(candidate) ? candidate : {};
  const promptEditorBackend = normalizePromptEditorBackend(source);
  const webLinkOpenTarget = normalizeWebLinkOpenTarget(source);
  const markdownFileOpenView = normalizeChatFileOpenView(source.markdownFileOpenView);
  const htmlFileOpenView = normalizeChatFileOpenView(source.htmlFileOpenView);
  const rawLegacyCustomSidebarTitlebarBackgroundColor = source.customSidebarTitlebarBackgroundColor;
  const hasValidLegacyCustomSidebarTitlebarBackgroundColor =
    typeof rawLegacyCustomSidebarTitlebarBackgroundColor === 'string' &&
    /^#[0-9a-f]{6}$/u.test(rawLegacyCustomSidebarTitlebarBackgroundColor.trim().toLowerCase());
  const legacyCustomSidebarTitlebarBackgroundColor = normalizeSidebarTitlebarHexColor(
    readString(
      source,
      'customSidebarTitlebarBackgroundColor',
      DEFAULT_ghostex_SETTINGS.customSidebarTitlebarBackgroundColor
    ),
    DEFAULT_ghostex_SETTINGS.customSidebarTitlebarBackgroundColor
  );
  /**
   * CDXC:SidebarTitlebarColors 2026-06-16-14:28:
   * Missing Settings should use the explicit 95 contrast default instead of
   * reverse-mapping the default background hex, because that reverse mapping
   * cannot exactly invert the slider's channel curve. Only valid legacy saved
   * background colors should continue to seed the slider during migration.
   */
  const customSidebarTitlebarBackgroundDarknessFallback = hasValidLegacyCustomSidebarTitlebarBackgroundColor
    ? getSidebarTitlebarBackgroundDarknessForColor(legacyCustomSidebarTitlebarBackgroundColor)
    : DEFAULT_ghostex_SETTINGS.customSidebarTitlebarBackgroundDarknessPercent;
  const customSidebarTitlebarBackgroundDarknessPercent = clampSidebarTitlebarBackgroundDarknessPercent(
    readNumber(
      source,
      'customSidebarTitlebarBackgroundDarknessPercent',
      customSidebarTitlebarBackgroundDarknessFallback
    )
  );
  const customSidebarTitlebarBackgroundTintColor = normalizeSidebarTitlebarHexColor(
    readString(
      source,
      'customSidebarTitlebarBackgroundTintColor',
      DEFAULT_ghostex_SETTINGS.customSidebarTitlebarBackgroundTintColor
    ),
    DEFAULT_ghostex_SETTINGS.customSidebarTitlebarBackgroundTintColor
  );
  const customSidebarTitlebarBackgroundColor = getSidebarTitlebarBackgroundForDarkness(
    customSidebarTitlebarBackgroundDarknessPercent,
    customSidebarTitlebarBackgroundTintColor
  );
  /**
   * CDXC:AccentColor 2026-08-24:
   * The accent color is a plain hex color like the sidebar/titlebar colors, so
   * it uses the same six-digit hex normalization and falls back to the default
   * sky tone when the stored value is not a usable color.
   */
  const accentColor = normalizeSidebarTitlebarHexColor(
    readString(source, 'accentColor', DEFAULT_ghostex_SETTINGS.accentColor),
    DEFAULT_ghostex_SETTINGS.accentColor
  );
  return {
    actionCompletionSound: clampCompletionSoundSetting(
      readString(source, 'actionCompletionSound', DEFAULT_ghostex_SETTINGS.actionCompletionSound)
    ),
    gpuiTitlebarActionCommandByProject: normalizeTitlebarProjectSelectionMap(source.gpuiTitlebarActionCommandByProject),
    gpuiTitlebarOpenTargetByProject: normalizeTitlebarProjectSelectionMap(source.gpuiTitlebarOpenTargetByProject),
    appShotsEnabled: readBoolean(source, 'appShotsEnabled', DEFAULT_ghostex_SETTINGS.appShotsEnabled),
    appShotsHotkey: normalizeAppShotsHotkey(
      readString(source, 'appShotsHotkey', DEFAULT_ghostex_SETTINGS.appShotsHotkey)
    ),
    appShotsMetadataEnabled: readBoolean(
      source,
      'appShotsMetadataEnabled',
      DEFAULT_ghostex_SETTINGS.appShotsMetadataEnabled
    ),
    agentAcceptAllEnabled: readBoolean(source, 'agentAcceptAllEnabled', DEFAULT_ghostex_SETTINGS.agentAcceptAllEnabled),
    agentManagerZoomPercent: clampAgentManagerZoomPercent(
      readNumber(source, 'agentManagerZoomPercent', DEFAULT_ghostex_SETTINGS.agentManagerZoomPercent)
    ),
    /**
     * CDXC:PromptAgents 2026-05-28-07:15:
     * Keep the selected default prompt agent as a plain agent id so built-in,
     * reordered, hidden-restored, and custom agents can all be selected without
     * coupling settings normalization to the runtime agent registry.
     */
    defaultPromptAgentId: normalizeDefaultPromptAgentId(
      readString(source, 'defaultPromptAgentId', DEFAULT_ghostex_SETTINGS.defaultPromptAgentId)
    ),
    sessionTitleGenerationAgent: normalizeSessionTitleGenerationAgent(
      readString(source, 'sessionTitleGenerationAgent', DEFAULT_ghostex_SETTINGS.sessionTitleGenerationAgent)
    ),
    customSessionTitleGenerationCommand: normalizeCustomSessionTitleGenerationCommand(
      readString(
        source,
        'customSessionTitleGenerationCommand',
        DEFAULT_ghostex_SETTINGS.customSessionTitleGenerationCommand
      )
    ),
    webLinkOpenTarget,
    markdownFileOpenView,
    htmlFileOpenView,
    /**
     * CDXC:SettingsAdvanced 2026-06-28-08:01:
     * Persist the Show Advanced density switch with other Settings so advanced
     * rows stay visible after a restart until the user disables the switch.
     */
    showAdvancedSettings: readBoolean(source, 'showAdvancedSettings', DEFAULT_ghostex_SETTINGS.showAdvancedSettings),
    /**
     * CDXC:SettingsNavigation 2026-06-29-17:54:
     * Restart restore reads Settings location from the shared settings file, so
     * normalize active tab and scroll offsets at the storage boundary before
     * React uses them to choose the initial Settings surface.
     */
    settingsModalNavigation: normalizeSettingsModalNavigationState(source.settingsModalNavigation),
    /**
     * CDXC:BetaFeatures 2026-06-16-13:08:
     * Normalize the beta gate as a strict boolean so stale or malformed settings
     * cannot expose beta-only OS Integration or other experimental surfaces.
     */
    showBetaFeatures: readBoolean(source, 'showBetaFeatures', DEFAULT_ghostex_SETTINGS.showBetaFeatures),
    codeViewTabHidden: readBoolean(source, 'codeViewTabHidden', DEFAULT_ghostex_SETTINGS.codeViewTabHidden),
    browserViewTabHidden: readBoolean(source, 'browserViewTabHidden', DEFAULT_ghostex_SETTINGS.browserViewTabHidden),
    kanbanViewTabHidden: readBoolean(source, 'kanbanViewTabHidden', DEFAULT_ghostex_SETTINGS.kanbanViewTabHidden),
    automateViewTabHidden: readBoolean(source, 'automateViewTabHidden', DEFAULT_ghostex_SETTINGS.automateViewTabHidden),
    docsViewTabHidden: readBoolean(source, 'docsViewTabHidden', DEFAULT_ghostex_SETTINGS.docsViewTabHidden),
    tipsAndTricksTitlebarButtonHidden: readBoolean(
      source,
      'tipsAndTricksTitlebarButtonHidden',
      DEFAULT_ghostex_SETTINGS.tipsAndTricksTitlebarButtonHidden
    ),
    resourcesTitlebarButtonHidden: readBoolean(
      source,
      'resourcesTitlebarButtonHidden',
      DEFAULT_ghostex_SETTINGS.resourcesTitlebarButtonHidden
    ),
    extensionsTitlebarButtonHidden: readBoolean(
      source,
      'extensionsTitlebarButtonHidden',
      DEFAULT_ghostex_SETTINGS.extensionsTitlebarButtonHidden
    ),
    gitActionsTitlebarButtonHidden: readBoolean(
      source,
      'gitActionsTitlebarButtonHidden',
      DEFAULT_ghostex_SETTINGS.gitActionsTitlebarButtonHidden
    ),
    quickActionsTitlebarButtonHidden: readBoolean(
      source,
      'quickActionsTitlebarButtonHidden',
      DEFAULT_ghostex_SETTINGS.quickActionsTitlebarButtonHidden
    ),
    openInTitlebarButtonHidden: readBoolean(
      source,
      'openInTitlebarButtonHidden',
      DEFAULT_ghostex_SETTINGS.openInTitlebarButtonHidden
    ),
    /**
     * CDXC:EditorPanes 2026-06-08-20:12:
     * Normalize the code-server VS Code settings-link toggles on every read so
     * missing values use the bundled editor defaults while explicit local VS
     * Code settings choices remain persisted.
     */
    codeServerLinkVscodeUserConfig: readBoolean(
      source,
      'codeServerLinkVscodeUserConfig',
      DEFAULT_ghostex_SETTINGS.codeServerLinkVscodeUserConfig
    ),
    codeServerUseVscodeInsidersUserConfig: readBoolean(
      source,
      'codeServerUseVscodeInsidersUserConfig',
      DEFAULT_ghostex_SETTINGS.codeServerUseVscodeInsidersUserConfig
    ),
    defaultEditorCommand: normalizeDefaultEditorCommand(
      readString(source, 'defaultEditorCommand', DEFAULT_ghostex_SETTINGS.defaultEditorCommand)
    ),
    customDefaultEditorCommand: normalizeCustomDefaultEditorCommand(
      readString(source, 'customDefaultEditorCommand', DEFAULT_ghostex_SETTINGS.customDefaultEditorCommand)
    ),
    // CDXC:AppIconPicker 2026-06-25-21:50: Coerce stored app icon source id to a trimmed string, defaulting to the bundled icon.
    appIconSourceId: normalizeAppIconSourceId(
      readString(source, 'appIconSourceId', DEFAULT_ghostex_SETTINGS.appIconSourceId)
    ),
    /**
     * CDXC:ProjectDiffStats 2026-05-16-08:46:
     * Missing project-header visibility now follows the Codex preset, which
     * hides git line deltas unless the user selects Detailed or changes this
     * setting directly.
     */
    hideProjectHeaderDiffStats: readBoolean(
      source,
      'hideProjectHeaderDiffStats',
      DEFAULT_ghostex_SETTINGS.hideProjectHeaderDiffStats
    ),
    manageAdditionalDocsFolders: normalizeManageAdditionalDocsFolders(
      readString(source, 'manageAdditionalDocsFolders', DEFAULT_ghostex_SETTINGS.manageAdditionalDocsFolders)
    ),
    // CDXC:GlobalProjectDefaults 2026-08-02: Global Defaults for the Projects page fields.
    globalWorktreeCommand: normalizeGlobalWorktreeCommand(
      readString(source, 'globalWorktreeCommand', DEFAULT_ghostex_SETTINGS.globalWorktreeCommand)
    ),
    globalBeadsDisplayKey: normalizeGlobalBeadsDisplayKey(
      readString(source, 'globalBeadsDisplayKey', DEFAULT_ghostex_SETTINGS.globalBeadsDisplayKey)
    ),
    globalBeadsDirectory: normalizeGlobalBeadsDirectory(
      readString(source, 'globalBeadsDirectory', DEFAULT_ghostex_SETTINGS.globalBeadsDirectory)
    ),
    globalDocsDirectory: normalizeGlobalDocsDirectory(
      readString(source, 'globalDocsDirectory', DEFAULT_ghostex_SETTINGS.globalDocsDirectory)
    ),
    /**
     * CDXC:ProjectDiffStats 2026-05-15-14:33:
     * Missing or invalid older settings must keep project-header git stats in
     * the quieter default that hides the changed-file count.
     */
    showProjectEditorDiffFileCount: readBoolean(
      source,
      'showProjectEditorDiffFileCount',
      DEFAULT_ghostex_SETTINGS.showProjectEditorDiffFileCount
    ),
    showUntrackedProjectDiffWhenNoTrackedChanges: readBoolean(
      source,
      'showUntrackedProjectDiffWhenNoTrackedChanges',
      DEFAULT_ghostex_SETTINGS.showUntrackedProjectDiffWhenNoTrackedChanges
    ),
    completionSound: normalizeCompletionSoundPreference(source),
    showNotificationOnTerminalBell: readBoolean(
      source,
      'showNotificationOnTerminalBell',
      DEFAULT_ghostex_SETTINGS.showNotificationOnTerminalBell
    ),
    createSessionOnSidebarDoubleClick: readBoolean(
      source,
      'createSessionOnSidebarDoubleClick',
      DEFAULT_ghostex_SETTINGS.createSessionOnSidebarDoubleClick
    ),
    enableSessionParking: readBoolean(source, 'enableSessionParking', DEFAULT_ghostex_SETTINGS.enableSessionParking),
    sleepSessionWhenParking: readBoolean(
      source,
      'sleepSessionWhenParking',
      DEFAULT_ghostex_SETTINGS.sleepSessionWhenParking
    ),
    analyticsEnabled: readBoolean(source, 'analyticsEnabled', DEFAULT_ghostex_SETTINGS.analyticsEnabled),
    debuggingMode: readBoolean(source, 'debuggingMode', DEFAULT_ghostex_SETTINGS.debuggingMode),
    diagnosticLogging: normalizeDiagnosticLoggingSettings(source.diagnosticLogging),
    renameSessionOnDoubleClick: readBoolean(
      source,
      'renameSessionOnDoubleClick',
      DEFAULT_ghostex_SETTINGS.renameSessionOnDoubleClick
    ),
    showProjectIcons: readBoolean(source, 'showProjectIcons', DEFAULT_ghostex_SETTINGS.showProjectIcons),
    /**
     * CDXC:SidebarSessions 2026-05-16-08:46:
     * Missing session-card icon visibility now follows the Codex preset, which
     * hides agent icons until hover unless the user selects Detailed or changes
     * this setting directly.
     */
    hideSessionAgentIconUntilHover: readBoolean(
      source,
      'hideSessionAgentIconUntilHover',
      DEFAULT_ghostex_SETTINGS.hideSessionAgentIconUntilHover
    ),
    useColoredSessionAgentIcons: readBoolean(
      source,
      'useColoredSessionAgentIcons',
      DEFAULT_ghostex_SETTINGS.useColoredSessionAgentIcons
    ),
    /**
     * CDXC:BrowserPanes 2026-05-28-07:38:
     * Missing browser-favicon visibility should follow the sidebar preset
     * independently from the older agent-icon hover-only setting so browser
     * page identity does not disappear just because agent logos are quiet.
     */
    hideBrowserFaviconUntilHover: readBoolean(
      source,
      'hideBrowserFaviconUntilHover',
      DEFAULT_ghostex_SETTINGS.hideBrowserFaviconUntilHover
    ),
    showCloseButtonOnSessionCards: readBoolean(
      source,
      'showCloseButtonOnSessionCards',
      DEFAULT_ghostex_SETTINGS.showCloseButtonOnSessionCards
    ),
    /**
     * CDXC:SidebarSessions 2026-05-15-08:57
     * Older settings files should preserve the current session-card timestamp
     * behavior. Explicit true hides only the Last Active label, not the code
     * project header's separate git additions/deletions summary.
     */
    hideLastActiveTimeOnSessionCards: readBoolean(
      source,
      'hideLastActiveTimeOnSessionCards',
      DEFAULT_ghostex_SETTINGS.hideLastActiveTimeOnSessionCards
    ),
    showSessionCloseContextMenuAction: readBoolean(
      source,
      'showSessionCloseContextMenuAction',
      DEFAULT_ghostex_SETTINGS.showSessionCloseContextMenuAction
    ),
    showSessionCommandCopyActions: readBoolean(
      source,
      'showSessionCommandCopyActions',
      DEFAULT_ghostex_SETTINGS.showSessionCommandCopyActions
    ),
    showSessionDetailsCopyAction: readBoolean(
      source,
      'showSessionDetailsCopyAction',
      DEFAULT_ghostex_SETTINGS.showSessionDetailsCopyAction
    ),
    sidebarSessionTagListItems: normalizeSidebarSessionTagListItems(source.sidebarSessionTagListItems),
    /**
     * CDXC:AutoSleep 2026-05-28-08:06:
     * Normalize Auto Sleep policy independently from keep-awake so Mac power
     * assertions and Ghostex session retirement can be configured separately.
     */
    autoSleepAgentIdleMinutes: normalizeAutoSleepIdleMinutes(
      source,
      'autoSleepAgentIdleMinutes',
      'autoSleepAgentSessionsEnabled',
      DEFAULT_ghostex_SETTINGS.autoSleepAgentIdleMinutes,
      15
    ),
    autoSleepBrowserIdleMinutes: normalizeAutoSleepIdleMinutes(
      source,
      'autoSleepBrowserIdleMinutes',
      'autoSleepBrowserSessionsEnabled',
      DEFAULT_ghostex_SETTINGS.autoSleepBrowserIdleMinutes,
      10
    ),
    autoSleepCodeEditorIdleMinutes: normalizeAutoSleepIdleMinutes(
      source,
      'autoSleepCodeEditorIdleMinutes',
      'autoSleepCodeEditorEnabled',
      DEFAULT_ghostex_SETTINGS.autoSleepCodeEditorIdleMinutes,
      10
    ),
    autoSleepGitEditorIdleMinutes: normalizeAutoSleepIdleMinutes(
      source,
      'autoSleepGitEditorIdleMinutes',
      'autoSleepGitEditorEnabled',
      DEFAULT_ghostex_SETTINGS.autoSleepGitEditorIdleMinutes,
      5
    ),
    autoSleepProjectEditorIdleMinutes: normalizeAutoSleepIdleMinutes(
      source,
      'autoSleepProjectEditorIdleMinutes',
      'autoSleepProjectEditorEnabled',
      DEFAULT_ghostex_SETTINGS.autoSleepProjectEditorIdleMinutes,
      5
    ),
    autoSleepRequireAgentResumeCommand: readBoolean(
      source,
      'autoSleepRequireAgentResumeCommand',
      DEFAULT_ghostex_SETTINGS.autoSleepRequireAgentResumeCommand
    ),
    autoSleepFavoriteAgentSessions: readBoolean(
      source,
      'autoSleepFavoriteAgentSessions',
      DEFAULT_ghostex_SETTINGS.autoSleepFavoriteAgentSessions
    ),
    keepAwakeActivateOnExternalDisplay: readBoolean(
      source,
      'keepAwakeActivateOnExternalDisplay',
      DEFAULT_ghostex_SETTINGS.keepAwakeActivateOnExternalDisplay
    ),
    keepAwakeActivateOnLaunch: readBoolean(
      source,
      'keepAwakeActivateOnLaunch',
      DEFAULT_ghostex_SETTINGS.keepAwakeActivateOnLaunch
    ),
    keepAwakeAllowDisplaySleep: readBoolean(
      source,
      'keepAwakeAllowDisplaySleep',
      DEFAULT_ghostex_SETTINGS.keepAwakeAllowDisplaySleep
    ),
    keepAwakeBatteryThresholdPercent: normalizeKeepAwakeBatteryThresholdPercent(source),
    keepAwakeDeactivateOnLowPowerMode: readBoolean(
      source,
      'keepAwakeDeactivateOnLowPowerMode',
      DEFAULT_ghostex_SETTINGS.keepAwakeDeactivateOnLowPowerMode
    ),
    keepAwakeDeactivateOnUserSwitch: readBoolean(
      source,
      'keepAwakeDeactivateOnUserSwitch',
      DEFAULT_ghostex_SETTINGS.keepAwakeDeactivateOnUserSwitch
    ),
    keepAwakeDefaultDurationMinutes: normalizeKeepAwakeDurationMinutes(
      readNumber(source, 'keepAwakeDefaultDurationMinutes', DEFAULT_ghostex_SETTINGS.keepAwakeDefaultDurationMinutes)
    ),
    keepAwakeWhileWorkingSessions: readBoolean(
      source,
      'keepAwakeWhileWorkingSessions',
      DEFAULT_ghostex_SETTINGS.keepAwakeWhileWorkingSessions
    ),
    keepAwakePreventLidSleep: readBoolean(
      source,
      'keepAwakePreventLidSleep',
      DEFAULT_ghostex_SETTINGS.keepAwakePreventLidSleep
    ),
    /**
     * CDXC:TitlebarKeepAwake 2026-05-27-07:32:
     * Normalize the hide preference independently from the caffeinate rules so
     * hiding titlebar chrome does not rewrite existing power automation settings.
     *
     * CDXC:TitlebarKeepAwake 2026-06-19-13:13:
     * Keep the persisted hide preference independent from the beta gate because
     * the titlebar bridge computes effective visibility from both settings.
     */
    hideKeepAwakeTitlebarControl: readBoolean(
      source,
      'hideKeepAwakeTitlebarControl',
      DEFAULT_ghostex_SETTINGS.hideKeepAwakeTitlebarControl
    ),
    hideTabStripNewTerminalButton: readBoolean(
      source,
      'hideTabStripNewTerminalButton',
      DEFAULT_ghostex_SETTINGS.hideTabStripNewTerminalButton
    ),
    hideTabStripNewBrowserButton: readBoolean(
      source,
      'hideTabStripNewBrowserButton',
      DEFAULT_ghostex_SETTINGS.hideTabStripNewBrowserButton
    ),
    /**
     * CDXC:SessionAttentionNotifications 2026-05-10-16:46
     * Older settings files should opt into macOS attention notifications, and
     * explicit false must be preserved for users who disable system banners.
     */
    showMacOSAttentionNotifications: readBoolean(
      source,
      'showMacOSAttentionNotifications',
      DEFAULT_ghostex_SETTINGS.showMacOSAttentionNotifications
    ),
    hideMenuBarSessionStatusIndicators: readBoolean(
      source,
      'hideMenuBarSessionStatusIndicators',
      DEFAULT_ghostex_SETTINGS.hideMenuBarSessionStatusIndicators
    ),
    petOverlayEnabled: readBoolean(source, 'petOverlayEnabled', DEFAULT_ghostex_SETTINGS.petOverlayEnabled),
    selectedPetId: normalizePetId(readString(source, 'selectedPetId', DEFAULT_ghostex_SETTINGS.selectedPetId)),
    /**
     * CDXC:SessionPersistence 2026-05-23-00:50:
     * Older settings should normalize the session-id overlay preference from
     * the canonical default while preserving explicit user choices.
     * The native pane still suppresses the actual label unless that terminal is
     * backed by zmx, tmux, or zellij.
     */
    showSessionIdInTerminalPanes: readBoolean(
      source,
      'showSessionIdInTerminalPanes',
      DEFAULT_ghostex_SETTINGS.showSessionIdInTerminalPanes
    ),
    preferredAgentInterface: normalizePreferredAgentInterface(
      readString(source, 'preferredAgentInterface', DEFAULT_ghostex_SETTINGS.preferredAgentInterface)
    ),
    preferredAgentInterfaceOverrides: normalizePreferredAgentInterfaceOverrides(
      source['preferredAgentInterfaceOverrides']
    ),
    /**
     * CDXC:SidebarPlacement 2026-05-06-17:32
     * Persist only the supported AppKit chrome sides. Unknown values normalize
     * to the default left placement so the native layout never receives an
     * unsupported sidebar position.
     */
    sidebarSide: normalizeSidebarSide(readString(source, 'sidebarSide', DEFAULT_ghostex_SETTINGS.sidebarSide)),
    sidebarCollapseAnimationDurationMs: clampSidebarCollapseAnimationDurationMs(
      readNumber(
        source,
        'sidebarCollapseAnimationDurationMs',
        DEFAULT_ghostex_SETTINGS.sidebarCollapseAnimationDurationMs
      )
    ),
    sidebarTooltipDelayMs: clampSidebarTooltipDelayMs(
      readNumber(source, 'sidebarTooltipDelayMs', DEFAULT_ghostex_SETTINGS.sidebarTooltipDelayMs)
    ),
    sidebarDefaultWidthPx: clampSidebarDefaultWidthPx(
      readNumber(source, 'sidebarDefaultWidthPx', DEFAULT_ghostex_SETTINGS.sidebarDefaultWidthPx)
    ),
    /**
     * CDXC:ProjectSessionLists 2026-06-13-01:06:
     * Missing settings should use the current ten-session Show less behavior, while explicit numeric values tune how many project sessions remain visible before the header toggle offers Show more.
     */
    projectSessionListCollapsedCount: clampProjectSessionListCollapsedCount(
      readNumber(source, 'projectSessionListCollapsedCount', DEFAULT_ghostex_SETTINGS.projectSessionListCollapsedCount)
    ),
    sidebarProjectGroupStyle: normalizeSidebarProjectGroupStyle(
      readString(source, 'sidebarProjectGroupStyle', DEFAULT_ghostex_SETTINGS.sidebarProjectGroupStyle)
    ),
    sidebarSpacesEnabled: readBoolean(source, 'sidebarSpacesEnabled', DEFAULT_ghostex_SETTINGS.sidebarSpacesEnabled),
    expandCollapsedProjectsOnJump: readBoolean(
      source,
      'expandCollapsedProjectsOnJump',
      DEFAULT_ghostex_SETTINGS.expandCollapsedProjectsOnJump
    ),
    showLessForExpandedProjectJumps: readBoolean(
      source,
      'showLessForExpandedProjectJumps',
      DEFAULT_ghostex_SETTINGS.showLessForExpandedProjectJumps
    ),
    sidebarTheme: clampSidebarThemeSetting(readString(source, 'sidebarTheme', DEFAULT_ghostex_SETTINGS.sidebarTheme)),
    sessionChatTheme: normalizeSessionChatTheme(source.sessionChatTheme),
    sessionChatFontFamily: readString(
      source,
      'sessionChatFontFamily',
      DEFAULT_ghostex_SETTINGS.sessionChatFontFamily
    ).trim(),
    sessionChatCustomTranscriptWidthEnabled: readBoolean(
      source,
      'sessionChatCustomTranscriptWidthEnabled',
      DEFAULT_ghostex_SETTINGS.sessionChatCustomTranscriptWidthEnabled
    ),
    sessionChatTranscriptWidthPercent: clampSessionChatTranscriptWidthPercent(
      readNumber(
        source,
        'sessionChatTranscriptWidthPercent',
        DEFAULT_ghostex_SETTINGS.sessionChatTranscriptWidthPercent
      )
    ),
    sessionChatVerboseMode: readBoolean(
      source,
      'sessionChatVerboseMode',
      DEFAULT_ghostex_SETTINGS.sessionChatVerboseMode
    ),
    customSidebarTitlebarForegroundColor: getSidebarTitlebarForegroundForBackground(
      customSidebarTitlebarBackgroundColor
    ),
    customSidebarTitlebarBackgroundTintColor,
    customSidebarTitlebarBackgroundDarknessPercent,
    customSidebarTitlebarBackgroundColor,
    accentColor,
    terminalCursorStyle: normalizeTerminalCursorStyle(
      readString(source, 'terminalCursorStyle', DEFAULT_ghostex_SETTINGS.terminalCursorStyle)
    ),
    terminalCursorStyleBlink: readBoolean(
      source,
      'terminalCursorStyleBlink',
      DEFAULT_ghostex_SETTINGS.terminalCursorStyleBlink
    ),
    windowsWslDistribution: normalizeWindowsWslDistribution(
      readString(source, 'windowsWslDistribution', DEFAULT_ghostex_SETTINGS.windowsWslDistribution)
    ),
    /**
     * CDXC:TerminalTypographySettings 2026-04-29-09:32
     * Font family is a raw Ghostty font-family string so users can type any
     * installed font from `ghostty +list-fonts`. Empty means ghostex leaves an
     * existing Ghostty font-family line or Ghostty's platform default in charge.
     * Legacy preset labels are converted to their Ghostty family name.
     */
    terminalFontFamily: normalizeGhosttyFontFamily(
      readString(source, 'terminalFontFamily', DEFAULT_ghostex_SETTINGS.terminalFontFamily)
    ),
    terminalFontSize: clampNumber(
      readNumber(source, 'terminalFontSize', DEFAULT_ghostex_SETTINGS.terminalFontSize),
      8,
      32,
      DEFAULT_ghostex_SETTINGS.terminalFontSize
    ),
    terminalFontWeight: clampNumber(
      readNumber(source, 'terminalFontWeight', DEFAULT_ghostex_SETTINGS.terminalFontWeight),
      100,
      900,
      DEFAULT_ghostex_SETTINGS.terminalFontWeight
    ),
    /**
     * CDXC:TerminalThemeSettings 2026-04-29-09:32
     * Ghostty themes are exact strings. Preserve only bundled theme names from
     * the settings list, or an empty unmanaged value that keeps an existing
     * user-authored Ghostty `theme` line outside ghostex control.
     */
    terminalGhosttyTheme: normalizeGhosttyTheme(
      readString(source, 'terminalGhosttyTheme', DEFAULT_ghostex_SETTINGS.terminalGhosttyTheme)
    ),
    terminalBackgroundImage: readString(
      source,
      'terminalBackgroundImage',
      DEFAULT_ghostex_SETTINGS.terminalBackgroundImage
    ).trim(),
    terminalBackgroundImageOpacity: clampNumber(
      readNumber(source, 'terminalBackgroundImageOpacity', DEFAULT_ghostex_SETTINGS.terminalBackgroundImageOpacity),
      0,
      1,
      DEFAULT_ghostex_SETTINGS.terminalBackgroundImageOpacity
    ),
    terminalBackgroundImageFit: normalizeTerminalBackgroundImageFit(
      readString(source, 'terminalBackgroundImageFit', DEFAULT_ghostex_SETTINGS.terminalBackgroundImageFit)
    ),
    terminalLetterSpacing: clampNumber(
      readNumber(source, 'terminalLetterSpacing', DEFAULT_ghostex_SETTINGS.terminalLetterSpacing),
      -2,
      8,
      DEFAULT_ghostex_SETTINGS.terminalLetterSpacing
    ),
    terminalLineHeight: clampNumber(
      readNumber(source, 'terminalLineHeight', DEFAULT_ghostex_SETTINGS.terminalLineHeight),
      0.8,
      2,
      DEFAULT_ghostex_SETTINGS.terminalLineHeight
    ),
    terminalViewWidthMode: normalizeTerminalViewWidthMode(source),
    terminalViewWidthPercent: clampTerminalViewWidthPercent(
      readNumber(source, 'terminalViewWidthPercent', DEFAULT_ghostex_SETTINGS.terminalViewWidthPercent)
    ),
    terminalWidthApplyToCommandPaneTerminals: readBoolean(
      source,
      'terminalWidthApplyToCommandPaneTerminals',
      DEFAULT_ghostex_SETTINGS.terminalWidthApplyToCommandPaneTerminals
    ),
    /**
     * CDXC:TerminalPanePadding 2026-06-25-21:27:
     * Missing settings use the same 16px horizontal inset as Chat. Explicit
     * values are integer pixels clamped to the Settings slider range so native
     * layout receives bounded inner padding without adding spacing between
     * adjacent panes.
     */
    terminalPaneHorizontalPaddingPx: clampTerminalPanePaddingPx(
      readNumber(source, 'terminalPaneHorizontalPaddingPx', DEFAULT_ghostex_SETTINGS.terminalPaneHorizontalPaddingPx)
    ),
    terminalPaneVerticalPaddingPx: clampTerminalPanePaddingPx(
      readNumber(source, 'terminalPaneVerticalPaddingPx', DEFAULT_ghostex_SETTINGS.terminalPaneVerticalPaddingPx)
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
        'terminalMouseScrollMultiplierDiscrete',
        DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierDiscrete
      ),
      MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
      MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
      DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierDiscrete
    ),
    terminalMouseScrollMultiplierPrecision: clampNumber(
      readNumber(
        source,
        'terminalMouseScrollMultiplierPrecision',
        DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierPrecision
      ),
      MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
      MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
      DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierPrecision
    ),
    terminalScrollToBottomWhenTyping: readBoolean(
      source,
      'terminalScrollToBottomWhenTyping',
      DEFAULT_ghostex_SETTINGS.terminalScrollToBottomWhenTyping
    ),
    /**
     * CDXC:TerminalBehaviorSettings 2026-04-29-09:32
     * Common Ghostty terminal behavior settings are persisted with the same
     * practical UI ranges and enum values that the settings modal exposes,
     * then written as documented Ghostty config keys by the native host.
     */
    terminalScrollbackLimitMb: clampNumber(
      readNumber(source, 'terminalScrollbackLimitMb', DEFAULT_ghostex_SETTINGS.terminalScrollbackLimitMb),
      MIN_GHOSTTY_SCROLLBACK_LIMIT_MB,
      MAX_GHOSTTY_SCROLLBACK_LIMIT_MB,
      DEFAULT_ghostex_SETTINGS.terminalScrollbackLimitMb
    ),
    terminalCopyOnSelect: normalizeGhosttyCopyOnSelect(
      readString(source, 'terminalCopyOnSelect', DEFAULT_ghostex_SETTINGS.terminalCopyOnSelect)
    ),
    terminalConfirmCloseSurface: normalizeGhosttyConfirmCloseSurface(
      readString(source, 'terminalConfirmCloseSurface', DEFAULT_ghostex_SETTINGS.terminalConfirmCloseSurface)
    ),
    /**
     * CDXC:TerminalBehaviorSettings 2026-04-29-09:32
     * Clipboard cleanup/protection and mouse/scrollbar visibility mirror
     * Ghostty's documented defaults unless the user changes them in ghostex.
     */
    terminalClipboardTrimTrailingSpaces: readBoolean(
      source,
      'terminalClipboardTrimTrailingSpaces',
      DEFAULT_ghostex_SETTINGS.terminalClipboardTrimTrailingSpaces
    ),
    terminalClipboardPasteProtection: readBoolean(
      source,
      'terminalClipboardPasteProtection',
      DEFAULT_ghostex_SETTINGS.terminalClipboardPasteProtection
    ),
    terminalPastePreviewableImages: readBoolean(
      source,
      'terminalPastePreviewableImages',
      DEFAULT_ghostex_SETTINGS.terminalPastePreviewableImages
    ),
    terminalMouseHideWhileTyping: readBoolean(
      source,
      'terminalMouseHideWhileTyping',
      DEFAULT_ghostex_SETTINGS.terminalMouseHideWhileTyping
    ),
    terminalScrollbar: normalizeGhosttyScrollbar(
      readString(source, 'terminalScrollbar', DEFAULT_ghostex_SETTINGS.terminalScrollbar)
    ),
    /**
     * CDXC:TerminalDevServers 2026-06-23-19:22:
     * Dev-server settings normalize in the app layer because they are not Ghostty keys. Canonicalize ignored port rules to sorted, merged strings.
     *
     * CDXC:WebLinkOpenTarget 2026-08-19:
     * The launch choice moved to webLinkOpenTarget, which absorbs both the legacy dev-server target and its older per-browser default.
     */
    terminalDevServerDetectionEnabled: readBoolean(
      source,
      'terminalDevServerDetectionEnabled',
      DEFAULT_ghostex_SETTINGS.terminalDevServerDetectionEnabled
    ),
    terminalDevServerIgnoredPortRules: normalizeTerminalDevServerIgnoredPortRules(
      source.terminalDevServerIgnoredPortRules
    ),
    /**
     * CDXC:PortlessSettings 2026-06-22-22:35:
     * Portless normalization accepts only explicit booleans and lowercase http/https. Missing, legacy, string-boolean, and invalid values fall back to enabled HTTPS without preserving project-scoped Portless keys.
     */
    portlessEnabled: readBoolean(source, 'portlessEnabled', DEFAULT_ghostex_SETTINGS.portlessEnabled),
    portlessProtocol: normalizePortlessProtocol(
      readString(source, 'portlessProtocol', DEFAULT_ghostex_SETTINGS.portlessProtocol)
    ),
    promptEditorBackend,
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
        'workspaceActivePaneBorderColor',
        DEFAULT_ghostex_SETTINGS.workspaceActivePaneBorderColor
      ).trim() || DEFAULT_ghostex_SETTINGS.workspaceActivePaneBorderColor,
    /**
     * CDXC:WorkspaceLayout 2026-04-28-06:08
     * Users can choose the background visible behind terminal panes. Persist a
     * normalized CSS color string so the React workspace and native AppKit
     * workspace render the same color instead of hardcoding dark gray.
     */
    workspaceBackgroundColor:
      readString(source, 'workspaceBackgroundColor', DEFAULT_ghostex_SETTINGS.workspaceBackgroundColor).trim() ||
      DEFAULT_ghostex_SETTINGS.workspaceBackgroundColor,
    clickToWakeSleepingSessions: readBoolean(
      source,
      'clickToWakeSleepingSessions',
      DEFAULT_ghostex_SETTINGS.clickToWakeSleepingSessions
    ),
    /**
     * CDXC:TitlebarOpenIn 2026-05-11-00:22
     * Settings owns which titlebar Open In targets are shown. Normalize on read
     * so the React titlebar can trust the persisted custom commands and hidden
     * built-in ids sent through native layout sync.
     */
    customWorkspaceOpenTargets: normalizeCustomWorkspaceOpenTargets(source.customWorkspaceOpenTargets),
    workspaceOpenTargetAvailability: normalizeWorkspaceOpenTargetAvailability(source.workspaceOpenTargetAvailability),
    workspaceOpenTargetHiddenIds: normalizeWorkspaceOpenTargetHiddenIds(source.workspaceOpenTargetHiddenIds),
    workspacePaneGap: 0,
    remoteMachines: normalizeRemoteMachineSettings(source.remoteMachines),
    commandsPanelDefaultHeightPx: clampCommandsPanelDefaultHeightPx(
      readNumber(source, 'commandsPanelDefaultHeightPx', DEFAULT_ghostex_SETTINGS.commandsPanelDefaultHeightPx)
    ),
    commandsPanelSide: normalizeCommandsPanelSide(
      readString(source, 'commandsPanelSide', DEFAULT_ghostex_SETTINGS.commandsPanelSide)
    ),
  };
}

export function getTerminalFontFamilyForghostexSettings(settings: ghostexSettings): string {
  return settings.terminalFontFamily.trim() || getTerminalFontFamilyForPreset('JetBrains Mono');
}

export function applySidebarSettingsPreset(
  settings: ghostexSettings,
  presetId: SidebarSettingsPresetId
): ghostexSettings {
  return normalizeghostexSettings({
    ...settings,
    ...SIDEBAR_SETTINGS_PRESET_SETTINGS[presetId],
  });
}

export function normalizeManageAdditionalDocsFolders(value: string | undefined): string {
  /*
   * CDXC:DocsSidebar 2026-06-30-19:47:
   * The Projects setting is typed as comma-separated text because folder names may contain spaces. Settings normalizes on every keystroke, so preserve the user's draft text here and let native trim comma boundaries and reject unsafe path shapes when scanning.
   */
  return (value ?? '').replace(/\0/gu, '').replace(/\r?\n/gu, ', ').slice(0, 1_000);
}

/*
 * CDXC:GlobalProjectDefaults 2026-08-02:
 * Each Global Default normalizes exactly like the per-project field it backs, so
 * a value that is valid globally is also valid when a project stores it. The
 * worktree cap matches the 16384-byte limit gxserver enforces on the project
 * field, and the ticket key matches the three-character A-Z0-9 shape the
 * Projects page already forces while typing.
 */
export function normalizeGlobalWorktreeCommand(value: string | undefined): string {
  return (value ?? '').replace(/\0/gu, '').slice(0, 16_384);
}

export function normalizeGlobalBeadsDisplayKey(value: string | undefined): string {
  return (value ?? '')
    .toUpperCase()
    .replace(/[^A-Z0-9]/gu, '')
    .slice(0, 3);
}

export function normalizeGlobalBeadsDirectory(value: string | undefined): string {
  return (value ?? '').replace(/\0/gu, '').trim().slice(0, 1_000);
}

/*
 * CDXC:DocsRootDirectory 2026-08-09:
 * The Docs directory normalizes exactly like the Beads directory it sits next
 * to: one absolute folder path, trimmed, with no embedded NULs.
 */
export function normalizeGlobalDocsDirectory(value: string | undefined): string {
  return (value ?? '').replace(/\0/gu, '').trim().slice(0, 1_000);
}

function normalizeTerminalCursorStyle(value: string | undefined): TerminalCursorStyle {
  return value === 'block' || value === 'underline' ? value : 'bar';
}

function normalizeWindowsWslDistribution(value: string | undefined): string {
  return (value ?? '').replace(/\0/gu, '').replace(/\r?\n/gu, '').trim().slice(0, 128);
}

function normalizeAppShotsHotkey(value: string | undefined): AppShotsHotkey {
  /*
   * CDXC:AppShots 2026-06-29-01:29:
   * App Shots hotkeys must support both physical Shift keys and both physical Option keys in addition to both Command and left-key double-taps, because modifier-only capture should be usable without overloading one hand.
   */
  return value === 'both-shift' ||
    value === 'both-option' ||
    value === 'double-left-shift' ||
    value === 'double-left-option'
    ? value
    : DEFAULT_ghostex_SETTINGS.appShotsHotkey;
}

function normalizeDefaultEditorCommand(value: string | undefined): DefaultEditorCommand {
  return value === 'code-insiders' ||
    value === 'zed' ||
    value === 'zeditor' ||
    value === 'cursor' ||
    value === 'windsurf' ||
    value === 'codium' ||
    value === 'subl' ||
    value === 'other'
    ? value
    : DEFAULT_ghostex_SETTINGS.defaultEditorCommand;
}

function normalizeCustomDefaultEditorCommand(value: string | undefined): string {
  return (value ?? '').trim().slice(0, 240);
}

/*
 * CDXC:AppIconPicker 2026-06-26-23:42:
 * Empty string means default icon; otherwise the persisted id must remain a
 * filename-only value that round-trips exactly after native confirms it. Reject
 * invalid/path-like ids instead of slicing or otherwise rewriting them.
 */
function normalizeAppIconSourceId(value: string | undefined): string {
  const normalized = (value ?? '').trim();
  if (normalized.length === 0) {
    return '';
  }
  if (normalized.length > 255) {
    return '';
  }
  if (normalized === '.' || normalized === '..') {
    return '';
  }
  if (normalized.includes('/') || normalized.includes('\\') || normalized.includes('\0')) {
    return '';
  }
  return normalized;
}

function normalizeDefaultPromptAgentId(value: string | undefined): string {
  return ((value ?? '').trim() || DEFAULT_ghostex_SETTINGS.defaultPromptAgentId).slice(0, 120);
}

/*
 * CDXC:WebLinkOpenTarget 2026-08-19:
 * Two settings answered "where does this web link open" and could disagree:
 * the Browser toggle openTerminalLinksInApp (default on) and the Dev Servers
 * dropdown terminalDevServerOpenTarget (default system browser). They merge
 * into one target, so migration has to pick a winner for existing installs.
 * The toggle wins: it is the switch users actually flipped and the one the
 * in-app toast points at, while nearly every install carries the dev-server
 * default it never chose. Reading that field first would silently move
 * everyone off the embedded browser.
 */
function normalizeWebLinkOpenTarget(source: Record<string, unknown>): WebLinkOpenTarget {
  const value = readLooseString(source.webLinkOpenTarget);
  if (WEB_LINK_OPEN_TARGET_SET.has(value as WebLinkOpenTarget)) {
    return value as WebLinkOpenTarget;
  }

  const legacyOpenLinksInApp = source.openTerminalLinksInApp;
  if (typeof legacyOpenLinksInApp === 'boolean') {
    return legacyOpenLinksInApp ? 'internal-browser' : 'system-default-browser';
  }

  const legacyDevServerTarget = readLooseString(source.terminalDevServerOpenTarget);
  if (WEB_LINK_OPEN_TARGET_SET.has(legacyDevServerTarget as WebLinkOpenTarget)) {
    return legacyDevServerTarget as WebLinkOpenTarget;
  }

  /*
   * readLooseString returns "" for a missing key, not undefined, so this has to
   * test for content. The predecessor compared against undefined and therefore
   * matched every install; it went unnoticed only because it returned the value
   * that was already the default.
   */
  if (readLooseString(source.terminalDevServerDefaultBrowserId).length > 0) {
    return 'system-default-browser';
  }

  return DEFAULT_WEB_LINK_OPEN_TARGET;
}

function normalizeChatFileOpenView(value: unknown): ChatFileOpenView {
  const normalized = readLooseString(value);
  return CHAT_FILE_OPEN_VIEW_SET.has(normalized as ChatFileOpenView)
    ? (normalized as ChatFileOpenView)
    : DEFAULT_CHAT_FILE_OPEN_VIEW;
}

export function getDefaultEditorCommandForSettings(settings: ghostexSettings): string {
  const customCommand = settings.customDefaultEditorCommand.trim();
  return settings.defaultEditorCommand === 'other'
    ? customCommand || DEFAULT_ghostex_SETTINGS.defaultEditorCommand
    : settings.defaultEditorCommand;
}

function normalizeSidebarSide(value: string | undefined): SidebarSide {
  return value === 'right' ? 'right' : DEFAULT_ghostex_SETTINGS.sidebarSide;
}

function normalizeCommandsPanelSide(value: string | undefined): CommandsPanelSide {
  return value === 'right' ? 'right' : DEFAULT_ghostex_SETTINGS.commandsPanelSide;
}

function normalizeSidebarProjectGroupStyle(value: string | undefined): SidebarProjectGroupStyle {
  return value === 'quiet' || value === 'header' || value === 'branched'
    ? value
    : DEFAULT_ghostex_SETTINGS.sidebarProjectGroupStyle;
}

/* Both spellings are real user choices: normalizing "terminal" to the default
   would silently re-enable the automatic Chat handoff for a user who explicitly
   turned it off. Only unknown or missing values fall back to the default. */
function normalizePreferredAgentInterface(value: string | undefined): PreferredAgentInterface {
  return value === 'chat' || value === 'terminal' ? value : DEFAULT_ghostex_SETTINGS.preferredAgentInterface;
}

/* Per-agent overrides are user-visible state that a user can also hand-edit, so
   normalization drops anything it does not recognize instead of substituting a
   default. An unrecognized value must not become "chat": that would force an
   agent into a view the user never chose. Keys are kept verbatim because they
   are agent ids, including ids of custom agents this build has never seen. */
function normalizePreferredAgentInterfaceOverrides(value: unknown): Readonly<Record<string, PreferredAgentInterface>> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    return DEFAULT_ghostex_SETTINGS.preferredAgentInterfaceOverrides;
  }
  const normalized: Record<string, PreferredAgentInterface> = {};
  for (const [agentId, preferredInterface] of Object.entries(value as Record<string, unknown>)) {
    if (agentId.trim().length === 0) {
      continue;
    }
    if (preferredInterface === 'chat' || preferredInterface === 'terminal') {
      normalized[agentId] = preferredInterface;
    }
  }
  return normalized;
}

function normalizeKeepAwakeDurationMinutes(value: number): KeepAwakeDurationMinutes {
  return KEEP_AWAKE_DURATION_OPTIONS.some((option) => option.value === value)
    ? (value as KeepAwakeDurationMinutes)
    : DEFAULT_ghostex_SETTINGS.keepAwakeDefaultDurationMinutes;
}

function normalizeAutoSleepIdleMinutes(
  source: Record<string, unknown>,
  idleMinutesKey: string,
  legacyEnabledKey: string,
  fallback: AutoSleepIdleMinutes,
  legacyEnabledFallback: AutoSleepIdleMinutes
): AutoSleepIdleMinutes {
  const legacyEnabled = source[legacyEnabledKey];
  if (legacyEnabled === false) {
    return 0;
  }
  const effectiveFallback = legacyEnabled === true ? legacyEnabledFallback : fallback;
  const storedValue = source[idleMinutesKey];
  const value = typeof storedValue === 'number' && Number.isFinite(storedValue) ? storedValue : effectiveFallback;
  return AUTO_SLEEP_IDLE_MINUTE_OPTIONS.some((option) => option.value === value)
    ? (value as AutoSleepIdleMinutes)
    : effectiveFallback;
}

function normalizeKeepAwakeBatteryThresholdPercent(source: Record<string, unknown>): number {
  if (source.keepAwakeDeactivateBelowBatteryThreshold === false) {
    return 0;
  }
  const fallback = source.keepAwakeDeactivateBelowBatteryThreshold === true ? 20 : 0;
  const value = readNumber(source, 'keepAwakeBatteryThresholdPercent', fallback);
  return value === 0 ? 0 : clampNumber(value, 10, 90, fallback || 20);
}

function normalizeCompletionSoundPreference(source: Record<string, unknown>) {
  if (source.completionBellEnabled === false) {
    return 'off' as const;
  }
  return clampCompletionSoundPreference(
    readString(source, 'completionSound', DEFAULT_ghostex_SETTINGS.completionSound)
  );
}

function normalizePromptEditorBackend(source: Record<string, unknown>): PromptEditorBackend {
  const backend = readString(source, 'promptEditorBackend', '');
  if (backend === 'inherit' || backend === 'monaco') {
    return backend;
  }
  if (backend === 'gte' || backend === 'custom') {
    return 'inherit';
  }
  if (source.useGteForCtrlGPromptEditing === true || source.richPromptEditingWithGte === true) {
    return 'inherit';
  }
  return DEFAULT_ghostex_SETTINGS.promptEditorBackend;
}

function normalizeGhosttyTheme(value: string | undefined): string {
  if (!value || value === '__ghostex_ghostty_theme_unmanaged__') {
    return '';
  }
  return (GHOSTTY_THEME_OPTIONS as readonly string[]).includes(value) ? value : '';
}

function normalizeGhosttyFontFamily(value: string | undefined): string {
  const trimmedValue = (value ?? '').trim();
  if (!trimmedValue) {
    return '';
  }
  const legacyPreset = normalizeTerminalFontPreset(trimmedValue);
  if (legacyPreset === trimmedValue) {
    return getGhosttyFontFamilyForPreset(legacyPreset);
  }
  return trimmedValue;
}

function normalizeGhosttyCopyOnSelect(value: string | undefined): GhosttyCopyOnSelect {
  return value === 'true' || value === 'clipboard' ? value : DEFAULT_ghostex_SETTINGS.terminalCopyOnSelect;
}

function normalizeGhosttyConfirmCloseSurface(value: string | undefined): GhosttyConfirmCloseSurface {
  return value === 'false' || value === 'true' || value === 'always'
    ? value
    : DEFAULT_ghostex_SETTINGS.terminalConfirmCloseSurface;
}

function normalizeGhosttyScrollbar(value: string | undefined): GhosttyScrollbar {
  return value === 'never' ? 'never' : 'system';
}

function normalizeTerminalBackgroundImageFit(value: string | undefined): TerminalBackgroundImageFit {
  return value === 'contain' || value === 'stretch' || value === 'natural'
    ? value
    : DEFAULT_ghostex_SETTINGS.terminalBackgroundImageFit;
}

function normalizePortlessProtocol(value: string | undefined): PortlessProtocol {
  return value === 'http' || value === 'https' ? value : DEFAULT_ghostex_SETTINGS.portlessProtocol;
}
