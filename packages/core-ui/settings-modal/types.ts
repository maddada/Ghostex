import { type RefObject } from 'react';
import { Command } from '@/packages/components/ui/command';
import { Empty } from '@/packages/components/ui/empty';
import { type SettingsModalTab } from '../settings-modal-tabs';
import { IconSettings } from '@tabler/icons-react';
import { type WebLinkOpenTarget } from '../../shared/ghostex-settings';
import { DEFAULT_SIDEBAR_AGENTS } from '../../shared/sidebar-agents';
import { GHOSTEX_HOTKEY_DEFINITIONS, type ghostexHotkeyActionId } from '../../shared/ghostex-hotkeys';

export function getHotkeySettingsSectionId(
  definition: (typeof GHOSTEX_HOTKEY_DEFINITIONS)[number]
): HotkeySettingsSectionId {
  switch (definition.action.kind) {
    case 'focusedPaneAction':
    case 'renameActiveSession':
    case 'splitFocusedPane':
    case 'terminalToolbarAction':
      return 'paneActions';
    case 'focusAdjacentGroup':
    case 'focusDirection':
      return 'navigation';
    case 'focusSessionSlot':
      return definition.action.slotNumber > 0 ? 'sessionSlots' : 'navigation';
    case 'jumpToProject':
      return 'projects';
    case 'runActionSlot':
      return 'actions';
    default:
      return 'general';
  }
}

export const HOTKEY_SETTINGS_SECTIONS: readonly HotkeySettingsSectionDefinition[] = (
  [
    { id: 'general', title: 'General' },
    { id: 'paneActions', title: 'Pane Actions' },
    { id: 'navigation', title: 'Navigation' },
    { id: 'projects', title: 'Projects' },
    { id: 'sessionSlots', title: 'Session Slots' },
    { id: 'actions', title: 'Actions' },
  ] as const
).map((section) => ({
  ...section,
  /*
   * Settings is a view of the canonical hotkey catalog, not a second catalog.
   * Deriving each section prevents newly registered or unassigned actions from
   * silently disappearing until this modal's former hand-maintained ID lists
   * are updated separately.
   */
  ids: GHOSTEX_HOTKEY_DEFINITIONS.filter((definition) => getHotkeySettingsSectionId(definition) === section.id).map(
    (definition) => definition.id
  ),
}));

export type SettingSearchDefinition = {
  advanced?: boolean;
  key: string;
  options?: ReadonlyArray<{ label: string; value: string }>;
  subtitle?: string;
  title: string;
};

export type SettingsSectionSearchResult = {
  groupTitleMatches?: boolean;
  isSearching: boolean;
  sectionMatches: boolean;
  visibleSettingKeys: Set<string>;
};

export type SettingsSectionNavigationItem<SectionId extends string> = {
  id: SectionId;
  title: string;
};

export type SettingsSectionMeasurementItem<SectionId extends string> = {
  id: SectionId;
  ref: RefObject<HTMLDivElement | null>;
};

export type SettingsSidebarPageSection = {
  active: boolean;
  id: string;
  onSelect: () => void;
  /*
   * CDXC:SettingsNavigation 2026-08-19:
   * General groups several rendered section headers each ("Tools" holds
   * Browser, Editor, and Dev Servers), and those headers had no rail entry at
   * all. They expand as a third level under the active group instead of being
   * promoted to top-level destinations, which would undo the deliberate
   * "fewer sidebar destinations" grouping.
   */
  subsections?: readonly SettingsSidebarPageSubsection[];
  title: string;
};

export type SettingsSidebarPageSubsection = {
  active: boolean;
  id: string;
  onSelect: () => void;
  title: string;
};

export type SettingsSidebarPage = {
  icon: typeof IconSettings;
  id: SettingsModalTab;
  sections?: readonly SettingsSidebarPageSection[];
  title: string;
};

export type HotkeySettingsDefinitionById = ReadonlyMap<
  ghostexHotkeyActionId,
  (typeof GHOSTEX_HOTKEY_DEFINITIONS)[number]
>;

export type HotkeySettingsSectionRefs = Record<HotkeySettingsSectionId, RefObject<HTMLDivElement | null>>;

export type HotkeySettingsSectionSearches = Record<HotkeySettingsSectionId, SettingsSectionSearchResult>;

export type SettingModificationProps = {
  advanced?: boolean;
  isModified?: boolean;
  onResetToDefault?: () => void;
};

export type MainSettingsSectionId =
  | 'agents'
  | 'appearance'
  | 'chat'
  | 'sidebar'
  | 'terminal'
  | 'tools'
  | 'statusIndicators'
  | 'notifications'
  | 'system'
  | 'advanced';

export type MainSettingsScrollTargetId =
  | MainSettingsSectionId
  | 'theming'
  // CDXC:AppIconPicker 2026-06-25-21:50: App Icon is an appearance section that sits next to Theming.
  | 'appIcon'
  | 'sidebarTags'
  | 'sessionCards'
  | 'debugging'
  | 'terminalBehavior'
  | 'terminalScrolling'
  | 'terminalDevServers'
  | 'builtInFeatures'
  | 'browser'
  | 'editor'
  | 'autoSleep'
  | 'power'
  | 'sounds'
  | 'storage'
  | 'beta';

export type MainSettingsSectionRefs = Record<MainSettingsScrollTargetId, RefObject<HTMLDivElement | null>>;

/*
 * CDXC:DebuggingSettings 2026-06-28-18:14:
 * Show debug UI controls is the visibility and routine-logging gate for the
 * support/debugging settings below it. When off, hide diagnostic scenario
 * logging and session context-menu debug utilities instead of leaving disabled
 * rows on screen.
 */
export const DEBUGGING_MODE_DEPENDENT_SETTING_KEYS = [
  'diagnosticLogging',
  'showSessionCommandCopyActions',
  'showSessionDetailsCopyAction',
] as const;
export const DEBUGGING_MODE_DEPENDENT_SETTING_KEY_SET = new Set<string>(DEBUGGING_MODE_DEPENDENT_SETTING_KEYS);

export const MAIN_SETTINGS_SECTION_SETTING_KEYS: Record<MainSettingsSectionId, readonly string[]> = {
  agents: ['agentAcceptAllEnabled'],
  /*
   * CDXC:SettingsNavigation 2026-06-30-01:23:
   * General Settings should expose fewer sidebar destinations. Group related
   * controls into user-facing sections while retaining internal subheadings and
   * legacy scroll targets for direct entries such as Power Settings.
   *
   * CDXC:SettingsNavigation 2026-06-30-01:23:
   * Notifications/Sounds and Status Indicators remain independent sections
   * instead of merging into Appearance because users distinguish audible or
   * system alerts from always-visible status surfaces.
   *
   * CDXC:SettingsNavigation 2026-06-30-10:35:
   * Settings should not expose a standalone Workspace section header or workspace sidebar destination. Active Pane Border belongs with General appearance tuning, Terminal Background and click-to-wake belong with Terminal controls, Command Pane Default Height belongs beside the other default size reset value, and Auto Sleep moves under System.
   */
  appearance: [
    'sidebarTheme',
    'customSidebarTitlebarBackgroundDarknessPercent',
    'customSidebarTitlebarBackgroundTintColor',
    'workspaceActivePaneBorderColor',
    'appIconSourceId',
  ],
  chat: [
    'preferredAgentInterface',
    'sessionChatTheme',
    'sessionChatFontFamily',
    'sessionChatCustomTranscriptWidthEnabled',
    'sessionChatTranscriptWidthPercent',
    'sessionChatVerboseMode',
  ],
  sidebar: [
    'sidebarSettingsPreset',
    /*
     * CDXC:SidebarSettingsPresets 2026-08-26:
     * Preset-owned session-card and project-stat controls stay directly below
     * the preset selector when Advanced settings are visible. They remain in
     * this Sidebar group so users can inspect and tune exactly what a preset
     * changed without moving the rows away from their owning control.
     *
     * CDXC:SidebarProjectStats 2026-06-16-02:14:
     * Project git-stat display controls belong with Sidebar settings because they change sidebar project rows, not editor behavior.
     * Use changed-file wording for the file-count toggle so it does not read like an editor-pane setting.
     */
    'showProjectIcons',
    'hideSessionAgentIconUntilHover',
    'hideBrowserFaviconUntilHover',
    'showCloseButtonOnSessionCards',
    'hideLastActiveTimeOnSessionCards',
    'hideProjectHeaderDiffStats',
    'showProjectEditorDiffFileCount',
    'hideMenuBarSessionStatusIndicators',
    'sidebarSide',
    'sidebarCollapseAnimationDurationMs',
    'sidebarTooltipDelayMs',
    'sidebarDefaultWidthPx',
    'commandsPanelDefaultHeightPx',
    'commandsPanelSide',
    'projectSessionListCollapsedCount',
    'agentManagerZoomPercent',
    'createSessionOnSidebarDoubleClick',
    'enableSessionParking',
    'sleepSessionWhenParking',
    'renameSessionOnDoubleClick',
    'useColoredSessionAgentIcons',
    'showSessionCloseContextMenuAction',
    'sidebarSessionTagListItems',
  ],
  /*
   * CDXC:StatusIndicators 2026-05-20-12:00:
   * Status Indicators groups session presence surfaces that communicate status
   * at a glance.
   *
   * CDXC:StatusIndicators 2026-06-27-20:11:
   * The desktop floating session badge surface was removed from macOS and GPUI.
   *
   * CDXC:SidebarSettingsPresets 2026-06-30-22:22:
   * The menu bar session indicator is preset-owned, so it now renders under
   * Sidebar next to the other preset-controlled rows. This section keeps the
   * floating pet settings without exposing the removed floating badge toggle or
   * size selector.
   */
  statusIndicators: ['petOverlayEnabled', 'selectedPetId'],
  terminal: [
    'ghosttySettingsActions',
    'terminalGhosttyTheme',
    'workspaceBackgroundColor',
    'terminalBackgroundImage',
    'terminalBackgroundImageOpacity',
    'terminalBackgroundImageFit',
    'terminalFontFamily',
    'terminalFontSize',
    'terminalFontWeight',
    'terminalLineHeight',
    'terminalLetterSpacing',
    'terminalViewWidthMode',
    'terminalViewWidthPercent',
    'terminalWidthApplyToCommandPaneTerminals',
    'terminalPaneHorizontalPaddingPx',
    'terminalPaneVerticalPaddingPx',
    'terminalCursorStyle',
    'terminalCursorStyleBlink',
    'clickToWakeSleepingSessions',
    'showSessionIdInTerminalPanes',
    'showNotificationOnTerminalBell',
    'promptEditorBackend',
    'terminalScrollbackLimitMb',
    'terminalCopyOnSelect',
    'terminalConfirmCloseSurface',
    'terminalClipboardTrimTrailingSpaces',
    'terminalClipboardPasteProtection',
    'terminalPastePreviewableImages',
    'terminalMouseHideWhileTyping',
    'terminalScrollbar',
    'terminalMouseScrollMultiplierPrecision',
    'terminalMouseScrollMultiplierDiscrete',
    'terminalScrollToBottomWhenTyping',
  ],
  tools: [
    'webLinkOpenTarget',
    'codeServerLinkVscodeUserConfig',
    'codeServerUseVscodeInsidersUserConfig',
    'showUntrackedProjectDiffWhenNoTrackedChanges',
    /*
     * CDXC:TerminalDevServers 2026-06-23-19:22:
     * Dev-server discovery preferences belong under Terminal settings because they govern terminal-output detection, while remaining separate from Ghostty config-backed terminal emulator controls.
     *
     * CDXC:WebLinkOpenTarget 2026-08-19:
     * Where a detected URL opens is no longer a Dev Servers row; it reads the Browser section's single web-link target.
     */
    'terminalDevServerDetectionEnabled',
    'terminalDevServerIgnoredPortRules',
  ],
  notifications: [
    'completionSound',
    'showMacOSAttentionNotifications',
    'attentionNotificationActions',
    'actionCompletionSound',
  ],
  system: [
    'autoSleepCodeEditorIdleMinutes',
    'autoSleepGitEditorIdleMinutes',
    'autoSleepProjectEditorIdleMinutes',
    'autoSleepBrowserIdleMinutes',
    'autoSleepAgentIdleMinutes',
    'autoSleepRequireAgentResumeCommand',
    'autoSleepFavoriteAgentSessions',
    'hideKeepAwakeTitlebarControl',
    'keepAwakeDefaultDurationMinutes',
    'keepAwakeAllowDisplaySleep',
    'keepAwakePreventLidSleep',
    'keepAwakeActivateOnLaunch',
    'keepAwakeActivateOnExternalDisplay',
    'keepAwakeWhileWorkingSessions',
    'keepAwakeBatteryThresholdPercent',
    'keepAwakeDeactivateOnLowPowerMode',
    'keepAwakeDeactivateOnUserSwitch',
    'ghostexFolderStats',
  ],
  /*
   * CDXC:DebuggingSettings 2026-06-15-21:34:
   * Debugging controls belong in a dedicated bottom Settings section so support-oriented logging and session metadata copy actions are grouped away from everyday Workspace and Session Cards preferences.
   */
  advanced: ['showBetaFeatures', 'debuggingMode', ...DEBUGGING_MODE_DEPENDENT_SETTING_KEYS],
};

export const MAIN_SETTINGS_SCROLL_TARGET_SETTING_KEYS = {
  ...MAIN_SETTINGS_SECTION_SETTING_KEYS,
  theming: [
    'sidebarTheme',
    'customSidebarTitlebarBackgroundDarknessPercent',
    'customSidebarTitlebarBackgroundTintColor',
  ],
  // CDXC:AppIconPicker 2026-06-25-21:50: App Icon owns the persisted Dock icon source id selection.
  appIcon: ['appIconSourceId'],
  sidebarTags: ['sidebarSessionTagListItems'],
  sessionCards: ['useColoredSessionAgentIcons', 'showSessionCloseContextMenuAction'],
  debugging: ['debuggingMode', ...DEBUGGING_MODE_DEPENDENT_SETTING_KEYS],
  terminalBehavior: [
    'terminalScrollbackLimitMb',
    'terminalCopyOnSelect',
    'terminalConfirmCloseSurface',
    'terminalClipboardTrimTrailingSpaces',
    'terminalClipboardPasteProtection',
    'terminalPastePreviewableImages',
    'terminalMouseHideWhileTyping',
    'terminalScrollbar',
  ],
  terminalScrolling: [
    'terminalMouseScrollMultiplierPrecision',
    'terminalMouseScrollMultiplierDiscrete',
    'terminalScrollToBottomWhenTyping',
  ],
  terminalDevServers: ['terminalDevServerDetectionEnabled', 'terminalDevServerIgnoredPortRules'],
  builtInFeatures: ['codeViewTabHidden', 'kanbanViewTabHidden', 'automateViewTabHidden', 'docsViewTabHidden'],
  browser: ['webLinkOpenTarget'],
  editor: [
    'codeServerLinkVscodeUserConfig',
    'codeServerUseVscodeInsidersUserConfig',
    'showUntrackedProjectDiffWhenNoTrackedChanges',
  ],
  autoSleep: [
    'autoSleepCodeEditorIdleMinutes',
    'autoSleepGitEditorIdleMinutes',
    'autoSleepProjectEditorIdleMinutes',
    'autoSleepBrowserIdleMinutes',
    'autoSleepAgentIdleMinutes',
    'autoSleepRequireAgentResumeCommand',
    'autoSleepFavoriteAgentSessions',
  ],
  power: [
    'hideKeepAwakeTitlebarControl',
    'keepAwakeDefaultDurationMinutes',
    'keepAwakeAllowDisplaySleep',
    'keepAwakePreventLidSleep',
    'keepAwakeActivateOnLaunch',
    'keepAwakeActivateOnExternalDisplay',
    'keepAwakeWhileWorkingSessions',
    'keepAwakeBatteryThresholdPercent',
    'keepAwakeDeactivateOnLowPowerMode',
    'keepAwakeDeactivateOnUserSwitch',
  ],
  sounds: [
    'completionSound',
    'showMacOSAttentionNotifications',
    'attentionNotificationActions',
    'actionCompletionSound',
  ],
  storage: ['ghostexFolderStats'],
  beta: ['showBetaFeatures'],
} satisfies Record<MainSettingsScrollTargetId, readonly string[]>;

export type MainSettingsSubsectionId =
  | 'appIcon'
  | 'autoSleep'
  | 'beta'
  | 'browser'
  | 'debugging'
  | 'editor'
  | 'power'
  | 'sessionCards'
  | 'sidebar'
  | 'sidebarTags'
  | 'storage'
  | 'terminal'
  | 'terminalBehavior'
  | 'terminalDevServers'
  | 'terminalScrolling'
  | 'theming';

export type MainSettingsSubsectionNavigationItem = {
  id: MainSettingsSubsectionId;
  title: string;
};

/*
 * CDXC:SettingsNavigation 2026-08-19:
 * The rail rows for each General group, in the order the sections render on the
 * page. A group's own anchor is listed first so its header (Browser under
 * Tools, Terminal under Terminal) is reachable by name rather than only as the
 * side effect of clicking the group. Groups with a single section stay flat and
 * are omitted here.
 */
export const MAIN_SETTINGS_SUBSECTION_NAVIGATION: Partial<
  Record<MainSettingsSectionId, readonly MainSettingsSubsectionNavigationItem[]>
> = {
  advanced: [
    { id: 'beta', title: 'Experimental' },
    { id: 'debugging', title: 'Debugging' },
  ],
  appearance: [
    { id: 'theming', title: 'Theming' },
    { id: 'appIcon', title: 'App Icon' },
  ],
  sidebar: [
    { id: 'sidebar', title: 'Sidebar' },
    { id: 'sessionCards', title: 'Session Cards' },
    { id: 'sidebarTags', title: 'Sidebar Tags' },
  ],
  system: [
    { id: 'autoSleep', title: 'Auto Sleep' },
    { id: 'power', title: 'Power' },
    { id: 'storage', title: 'Storage' },
  ],
  terminal: [
    { id: 'terminal', title: 'Terminal' },
    { id: 'terminalBehavior', title: 'Terminal Behavior' },
    { id: 'terminalScrolling', title: 'Terminal Scrolling' },
  ],
  tools: [
    { id: 'browser', title: 'Browser' },
    { id: 'terminalDevServers', title: 'Dev Servers' },
    { id: 'editor', title: 'Editor' },
  ],
};

export const MAIN_SETTINGS_SUBSECTION_PARENT_IDS: Partial<Record<MainSettingsScrollTargetId, MainSettingsSectionId>> =
  Object.fromEntries(
    (
      Object.entries(MAIN_SETTINGS_SUBSECTION_NAVIGATION) as Array<
        [MainSettingsSectionId, readonly MainSettingsSubsectionNavigationItem[]]
      >
    ).flatMap(([sectionId, subsections]) => subsections.map((subsection) => [subsection.id, sectionId] as const))
  );

/*
 * CDXC:SettingsNavigation 2026-08-19:
 * Scroll tracking now reports the exact section header in view so a nested row
 * can highlight itself. The rail's top-level row still highlights by group, so
 * map the tracked anchor back to the group that owns it.
 */
export function getMainSettingsSectionGroupId(scrollTargetId: MainSettingsScrollTargetId): MainSettingsSectionId {
  return MAIN_SETTINGS_SUBSECTION_PARENT_IDS[scrollTargetId] ?? (scrollTargetId as MainSettingsSectionId);
}

/**
 * CDXC:SidebarSessionRename 2026-06-26-06:27:
 * The double-click rename setting must disclose that enabling it makes single-click session selection respond a bit slower because the card waits for a possible second click before treating the gesture as normal selection.
 *
 * CDXC:SidebarSessionRename 2026-06-28-02:24:
 * The click-delay disclosure should render as a Settings row subtitle below the primary label instead of being embedded in parentheses in the label text, so the control title stays scannable while the tradeoff remains visible.
 */
export const RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_LABEL = 'Double-click session cards to rename';
export const RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_SUBTITLE =
  'Makes clicking on a session respond a bit slower so we can detect the double click';

export type DiagnosticLoggingDurationValue = 'off' | '15m' | '1h' | 'always';

export const DIAGNOSTIC_LOGGING_DURATION_OPTIONS: ReadonlyArray<{
  label: string;
  value: DiagnosticLoggingDurationValue;
}> = [
  { label: 'Off', value: 'off' },
  { label: '15 min', value: '15m' },
  { label: '1 hour', value: '1h' },
  { label: 'Always', value: 'always' },
];

export const DEFAULT_DIAGNOSTIC_LOGGING_ENABLE_DURATION: DiagnosticLoggingDurationValue = '1h';
export const DIAGNOSTIC_LOGGING_GROUPS: readonly ['macOS', 'GPUI', 'gxserver'] = ['macOS', 'GPUI', 'gxserver'];

/*
 * CDXC:SettingsAdvanced 2026-06-16-01:35:
 * The first Settings page should default to everyday controls and hide precision tuning, support/debug toggles, context-menu utilities, and provider-specific terminal options until users enable Show Advanced. Search still exposes matching advanced controls so discoverability is not tied to browsing mode.
 *
 * CDXC:SettingsAdvanced 2026-06-16-01:53:
 * Superseded by CDXC:SettingsNavigation 2026-06-19-08:40.
 *
 * CDXC:SettingsNavigation 2026-06-19-08:40:
 * Show Advanced changes the density of the General Settings page, but the macOS Settings UI should still present it inside the same left sidebar as the section navigation rather than as separate header or footer chrome.
 *
 * CDXC:SettingsAdvanced 2026-06-16-08:12:
 * Browser feedback, Storage, session-card chrome, Workspace tuning, and Terminal Behavior controls are advanced-only browsing rows because the default General page should stay focused on common setup and daily preferences.
 *
 * CDXC:SidebarSettingsPresets 2026-08-26:
 * The detailed presentation toggles changed by sidebar presets are advanced
 * browsing rows. Search still reveals them, and Show Advanced keeps them
 * directly below the preset selector for inspecting a preset's effects.
 *
 * CDXC:SettingsTheming 2026-06-16-08:58:
 * Theming controls should remain visible without Show Advanced. Do not mark Theme, Background Contrast, or Background Tint as advanced rows.
 *
 * CDXC:SettingsAdvanced 2026-06-16-09:20:
 * Empty-sidebar double-click creation remains a low-frequency interaction preference and should hide behind Show Advanced. The menu-bar indicator is preset-owned and stays beside the sidebar preset controls.
 *
 * CDXC:PetControlsVisibility 2026-07-21:
 * Wake Pet and the Pet picker are temporarily hidden from Settings while their
 * implementation and persisted values remain available for a possible return.
 *
 * CDXC:ExperimentalFeatures 2026-06-28-07:41:
 * Enable Experimental Features is the user-facing name for the persisted
 * showBetaFeatures gate. Show Advanced is a persisted browsing-density
 * preference, so keep the experimental gate hidden from ordinary settings
 * browsing until users enable advanced density or search for it.
 *
 * CDXC:SettingsAdvanced 2026-06-28-08:01:
 * Show Advanced persists as a Settings preference so advanced rows stay visible
 * after restart until the user disables the switch.
 *
 * CDXC:SettingsAdvanced 2026-08-26:
 * Sidebar presentation details and double-click card renaming are advanced
 * preferences. Completion sounds, macOS attention notification,
 * action-completion sound, Sidebar Tags, and the sidebar interface-size slider
 * remain visible without Show Advanced.
 *
 */
export const ADVANCED_MAIN_SETTING_KEYS = new Set<string>([
  'showProjectIcons',
  'hideSessionAgentIconUntilHover',
  'hideBrowserFaviconUntilHover',
  'showCloseButtonOnSessionCards',
  'hideLastActiveTimeOnSessionCards',
  'hideProjectHeaderDiffStats',
  'showProjectEditorDiffFileCount',
  'sidebarDefaultWidthPx',
  'projectSessionListCollapsedCount',
  'createSessionOnSidebarDoubleClick',
  'enableSessionParking',
  'sleepSessionWhenParking',
  'renameSessionOnDoubleClick',
  'showSessionCloseContextMenuAction',
  'workspaceActivePaneBorderColor',
  'workspaceBackgroundColor',
  'terminalBackgroundImage',
  'terminalBackgroundImageOpacity',
  'terminalBackgroundImageFit',
  'clickToWakeSleepingSessions',
  'commandsPanelDefaultHeightPx',
  'ghosttySettingsActions',
  'terminalFontWeight',
  'terminalLineHeight',
  'terminalLetterSpacing',
  'terminalViewWidthMode',
  'terminalViewWidthPercent',
  'terminalWidthApplyToCommandPaneTerminals',
  'terminalCursorStyleBlink',
  'showSessionIdInTerminalPanes',
  'promptEditorBackend',
  'terminalScrollbackLimitMb',
  'terminalCopyOnSelect',
  'terminalConfirmCloseSurface',
  'terminalClipboardTrimTrailingSpaces',
  'terminalClipboardPasteProtection',
  'terminalPastePreviewableImages',
  'terminalMouseHideWhileTyping',
  'terminalScrollbar',
  'terminalMouseScrollMultiplierPrecision',
  'terminalMouseScrollMultiplierDiscrete',
  'terminalScrollToBottomWhenTyping',
  'codeServerUseVscodeInsidersUserConfig',
  'codeServerLinkVscodeUserConfig',
  /*
   * CDXC:AppIconPicker 2026-06-28-06:05:
   * Custom Dock icons are advanced appearance personalization. Keep the control searchable, but hide it from normal Settings browsing and place it below Editor so it does not compete with daily sidebar/theme controls.
   */
  'appIconSourceId',
  'showUntrackedProjectDiffWhenNoTrackedChanges',
  'autoSleepCodeEditorIdleMinutes',
  'autoSleepGitEditorIdleMinutes',
  'autoSleepProjectEditorIdleMinutes',
  'autoSleepBrowserIdleMinutes',
  'autoSleepAgentIdleMinutes',
  'autoSleepRequireAgentResumeCommand',
  'autoSleepFavoriteAgentSessions',
  'hideKeepAwakeTitlebarControl',
  'keepAwakeDefaultDurationMinutes',
  'keepAwakeAllowDisplaySleep',
  'keepAwakePreventLidSleep',
  'keepAwakeActivateOnLaunch',
  'keepAwakeActivateOnExternalDisplay',
  'keepAwakeWhileWorkingSessions',
  'keepAwakeBatteryThresholdPercent',
  'keepAwakeDeactivateOnLowPowerMode',
  'keepAwakeDeactivateOnUserSwitch',
  'attentionNotificationActions',
  'ghostexFolderStats',
  'showBetaFeatures',
  'debuggingMode',
  'diagnosticLogging',
  'showSessionCommandCopyActions',
  'showSessionDetailsCopyAction',
]);

export type HotkeySettingsSectionId =
  'general' | 'paneActions' | 'navigation' | 'projects' | 'sessionSlots' | 'actions';

export type HotkeySettingsSectionDefinition = {
  ids: readonly ghostexHotkeyActionId[];
  id: HotkeySettingsSectionId;
  title: string;
};

export const AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS = DEFAULT_SIDEBAR_AGENTS;
