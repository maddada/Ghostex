import { formatSidebarHotkeyLabel } from '@/packages/core-ui/hotkey-label';
/*
 * CDXC:RepoStructure 2026-08-23:
 * Settings search metadata, the grouped page search, and the main-settings
 * navigation rail are pure derivations of the search query and the settings
 * draft, so they live here instead of inside the SettingsModal component body.
 */
import { COMPLETION_SOUND_OPTIONS } from '../../shared/completion-sound';
import {
  AUTO_SLEEP_IDLE_MINUTE_OPTIONS,
  CHAT_FILE_OPEN_VIEW_OPTIONS,
  COMMANDS_PANEL_SIDE_OPTIONS,
  WINDOW_GLASS_OPTIONS,
  WINDOW_GLASS_SOURCE_OPTIONS,
  WINDOW_GLASS_LIVE_STYLE_OPTIONS,
  WINDOW_GLASS_IMAGE_PLACEMENT_OPTIONS,
  PANEL_ANIMATION_SPEED_OPTIONS,
  COMMANDS_PANEL_AUTO_MINIMIZE_DELAY_OPTIONS,
  GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS,
  GHOSTTY_COPY_ON_SELECT_OPTIONS,
  GHOSTTY_SCROLLBAR_OPTIONS,
  GHOSTTY_THEME_SETTING_OPTIONS,
  KEEP_AWAKE_DURATION_OPTIONS,
  PREFERRED_AGENT_INTERFACE_OPTIONS,
  PROMPT_EDITOR_BACKEND_OPTIONS,
  SESSION_CHAT_THEME_OPTIONS,
  DARK_THEME_PRESET_OPTIONS,
  LIGHT_THEME_PRESET_OPTIONS,
  SIDEBAR_SETTINGS_PRESETS,
  SIDEBAR_THEME_SETTING_OPTIONS,
  SIDEBAR_SPACES_ENABLED_OPTIONS,
  SIDEBAR_SPACE_SWITCH_BEHAVIOR_OPTIONS,
  SIDEBAR_VISIBILITY_MEMORY_OPTIONS,
  TERMINAL_VIEW_WIDTH_MODE_OPTIONS,
  WEB_LINK_OPEN_TARGET_OPTIONS,
  type ghostexSettings,
} from '../../shared/ghostex-settings';
import { PET_CONTROLS_VISIBLE, PET_OPTIONS } from '../../shared/pets';
import { DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS, getSidebarSessionTagListItemLabel } from '../../shared/session-tags';
import { getGroupedSettingsSectionSearch, getSettingsSectionSearch } from './search';
import {
  MainSettingsSectionId,
  RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_LABEL,
  RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_SUBTITLE,
  SettingSearchDefinition,
  SettingsSectionSearchResult,
} from './types';

export const IS_WINDOWS_HOST = typeof navigator !== 'undefined' && /Windows/iu.test(navigator.userAgent);

export const PASTE_PREVIEWABLE_IMAGES_DESCRIPTION = `Paste clipboard images as previewable Markdown links with ${formatSidebarHotkeyLabel('cmd+v')}. Hold ${formatSidebarHotkeyLabel('cmd')} over the linked path to preview it in the terminal, and see the same image preview in the ${formatSidebarHotkeyLabel('ctrl+g')} Rich Prompt Editor.`;

export type SettingsSearchSectionDefinition = {
  settings: readonly SettingSearchDefinition[];
  title: string;
};

/**
 * CDXC:Settings 2026-09-09 WHY:
 * The General page's search rows are exported as plain data so the Ghostex Help
 * skill generator (`tooling/ghostex-help/generate.ts`) can turn the same titles,
 * subtitles, and option lists into the agent-facing settings catalog instead of
 * maintaining a second hand-written list that drifts from Settings.
 * SEE-ALSO: skills/ghostex-help/references/settings-catalog.json, server/src/ghostex_cli/settings.rs.
 */
/**
 * CDXC:Theming 2026-09-23 DECISION:
 * User: "fully hide the custom app icon feature". The App Icon picker, its search row, the Help catalog row and the
 * `ghostex settings` entry are all off while this is false; the picker code and `appIconSourceId` stay, so an icon
 * that was already chosen keeps working.
 */
export const APP_ICON_CONTROLS_VISIBLE = false;

export function getSettingsSearchSectionDefinitions() {
  const settingsSearchSections = {
    // CDXC:Icons 2026-06-25-21:50: Make the App Icon section findable by Settings search.
    appIcon: {
      title: 'App Icon',
      settings: APP_ICON_CONTROLS_VISIBLE
        ? [
            {
              key: 'appIconSourceId',
              subtitle:
                'Choose the application and app-switcher icon. The app file icon may also change when the operating system allows it.',
              title: 'App Icon',
            },
          ]
        : [],
    },
    /*
     * CDXC:Extensions 2026-08-30:
     * The built-in view switches are owned by Settings → Extensions and are
     * searchable from that page's own definitions. General no longer claims
     * them, so a query like "kanban" lands on Extensions instead of matching a
     * General page that has no such section to scroll to.
     */
    fileOpening: {
      title: 'File opening',
      settings: [
        {
          key: 'markdownFileOpenView',
          options: CHAT_FILE_OPEN_VIEW_OPTIONS,
          subtitle: 'Choose whether Markdown links from agent chat open in Docs or Code.',
          title: 'Markdown files',
        },
        {
          key: 'htmlFileOpenView',
          options: CHAT_FILE_OPEN_VIEW_OPTIONS,
          subtitle: 'Choose whether HTML links from agent chat open in Docs or Code.',
          title: 'HTML files',
        },
      ],
    },
    browser: {
      title: 'Browser',
      settings: [
        {
          key: 'webLinkOpenTarget',
          options: WEB_LINK_OPEN_TARGET_OPTIONS,
          subtitle:
            'Open web links from terminal output (Command-click), session chat, and detected dev servers in the project Browser view or the system default browser.',
          title: 'Open links in',
        },
      ],
    },
    editor: {
      title: 'Editor',
      settings: [
        {
          key: 'codeServerLinkVscodeUserConfig',
          subtitle: 'Use the VS Code settings from the local VS Code install.',
          title: 'Use VS Code settings',
        },
        {
          key: 'codeServerUseVscodeInsidersUserConfig',
          subtitle: 'Use the VS Code Insiders user settings directory.',
          title: 'Use VS Code Insiders settings',
        },
        {
          key: 'showUntrackedProjectDiffWhenNoTrackedChanges',
          subtitle:
            'When tracked git diff is +0 -0, show untracked line counts in project headers (Starship-style prompts ignore untracked lines).',
          title: 'Show untracked lines without tracked changes',
        },
      ],
    },
    autoSleep: {
      title: 'Auto Sleep',
      settings: [
        {
          key: 'autoSleepCodeEditorIdleMinutes',
          options: AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
            label: option.label,
            value: String(option.value),
          })),
          subtitle: 'Choose when inactive VS Code panes sleep, or turn Auto Sleep off.',
          title: 'VS Code Auto Sleep',
        },
        {
          key: 'autoSleepGitEditorIdleMinutes',
          options: AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
            label: option.label,
            value: String(option.value),
          })),
          subtitle: 'Choose when inactive Git panes sleep, or turn Auto Sleep off.',
          title: 'Git Auto Sleep',
        },
        {
          key: 'autoSleepProjectEditorIdleMinutes',
          options: AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
            label: option.label,
            value: String(option.value),
          })),
          subtitle: 'Choose when inactive Project panes sleep, or turn Auto Sleep off.',
          title: 'Project Auto Sleep',
        },
        {
          key: 'autoSleepBrowserIdleMinutes',
          options: AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
            label: option.label,
            value: String(option.value),
          })),
          subtitle: 'Choose when inactive browser panes sleep, or turn Auto Sleep off.',
          title: 'Browser Auto Sleep',
        },
        {
          key: 'autoSleepAgentIdleMinutes',
          options: AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
            label: option.label,
            value: String(option.value),
          })),
          subtitle: 'Choose when eligible agent terminals sleep, or turn Auto Sleep off.',
          title: 'Agent Auto Sleep',
        },
        {
          key: 'autoSleepRequireAgentResumeCommand',
          subtitle: 'Only auto-sleep agent sessions Ghostex can wake with a resume command.',
          title: 'Require resume command',
        },
        {
          key: 'autoSleepFavoriteAgentSessions',
          subtitle: 'Allow favorite agent sessions to auto-sleep.',
          title: 'Include favorite agents',
        },
      ],
    },
    power: {
      title: 'Power',
      settings: [
        {
          key: 'hideKeepAwakeTitlebarControl',
          subtitle: 'Hide the Keep Awake entry from the sidebar menu.',
          title: 'Hide Keep Awake',
        },
        {
          key: 'keepAwakeDefaultDurationMinutes',
          options: KEEP_AWAKE_DURATION_OPTIONS.map((option) => ({
            label: option.label,
            value: String(option.value),
          })),
          subtitle: 'Choose the duration Keep Awake uses by default.',
          title: 'Default keep-awake duration',
        },
        {
          key: 'keepAwakeAllowDisplaySleep',
          subtitle: 'Keep the computer awake but allow the display to turn off.',
          title: 'Allow display sleep',
        },
        {
          key: 'keepAwakePreventLidSleep',
          subtitle:
            'Optional. When Keep Awake is on, Ghostex can install a small privileged helper once so closing the lid stays awake only for that active keep-awake session.',
          title: 'Prevent lid-close sleep',
        },
        {
          key: 'keepAwakeActivateOnLaunch',
          subtitle: 'Start preventing sleep when Ghostex launches.',
          title: 'Activate on launch',
        },
        {
          key: 'keepAwakeActivateOnExternalDisplay',
          subtitle: 'Start preventing sleep when an external display is connected.',
          title: 'Activate on external display',
        },
        {
          key: 'keepAwakeWhileWorkingSessions',
          subtitle: 'Keep the computer awake while sessions are working and for 20 minutes after.',
          title: 'Keep awake for working sessions',
        },
        {
          key: 'keepAwakeBatteryThresholdPercent',
          options: [
            { label: 'Off', value: '0' },
            ...Array.from({ length: 17 }, (_, index) => {
              const percent = 10 + index * 5;
              return { label: `${percent}%`, value: String(percent) };
            }),
          ],
          subtitle: 'Stop preventing sleep below this battery level, or turn the rule off.',
          title: 'Battery threshold',
        },
        {
          key: 'keepAwakeDeactivateOnLowPowerMode',
          subtitle: 'Stop preventing sleep when Low Power Mode is enabled.',
          title: 'Deactivate in Low Power Mode',
        },
        {
          key: 'keepAwakeDeactivateOnUserSwitch',
          subtitle: 'Stop preventing sleep when this user session is no longer active.',
          title: 'Deactivate on user switch',
        },
      ],
    },
    sessionCards: {
      title: 'Session Cards',
      settings: [
        /*
         * CDXC:Sessions 2026-05-15-19:46:
         * Settings must not expose the card-hotkey visibility row; session-card shortcut visibility is no longer configurable from the modal.
         */
        {
          key: 'sessionCardHoverButtons',
          subtitle:
            'Buttons a session card shows when you hover it. Click an icon to turn it on or off; drag icons to reorder them. Buttons to the right of the chevron always show, buttons to its left hide until the chevron is clicked. By default the strip is Tag, Park, Sleep, chevron, Close.',
          title: 'Session hover buttons (click to toggle, drag to reorder)',
        },
        {
          key: 'showSessionCardHoverButtonsInContextMenu',
          subtitle:
            'Keep the enabled hover buttons at the top of the session right-click menu too, in their right-to-left order on the card. Close is never listed while it is on the card. Turn off to leave every button out of the menu once it is on the card.',
          title: 'Hover buttons also in context menu',
        },
      ],
    },
    statusIndicators: {
      title: 'Status Indicators',
      settings: PET_CONTROLS_VISIBLE
        ? [
            /*
             * CDXC:SessionStatus 2026-05-20-12:00:
             * Status Indicators groups session presence surfaces that communicate
             * status at a glance.
             *
             * CDXC:SessionStatus 2026-06-27-20:11:
             * The removed floating session badge and its size selector must not
             * appear in macOS or GPUI Settings.
             *
             * CDXC:Settings 2026-06-30-22:22:
             * The menu bar session indicator now lives under Sidebar because sidebar
             * presets mutate it.
             */
            {
              key: 'petOverlayEnabled',
              subtitle: 'Show the draggable animated pet in the native sidebar.',
              title: 'Wake Pet',
            },
            {
              key: 'selectedPetId',
              options: PET_OPTIONS.map((option) => ({
                label: option.displayName,
                value: option.id,
              })),
              subtitle: 'Choose the pet sprite.',
              title: 'Pet',
            },
          ]
        : [],
    },
    sidebar: {
      title: 'Sidebar',
      settings: [
        {
          key: 'sidebarSettingsPreset',
          options: [
            ...SIDEBAR_SETTINGS_PRESETS.map((preset) => ({
              label: preset.label,
              value: preset.id,
            })),
            { label: 'Custom', value: 'custom' },
          ],
          subtitle: 'Apply a sidebar UI preset or show Custom when controlled settings diverge.',
          title: 'Preset',
        },
        {
          key: 'sidebarSpacesEnabled',
          options: SIDEBAR_SPACES_ENABLED_OPTIONS,
          subtitle: "Show a row of Space filter buttons in each server's sidebar section.",
          title: 'Spaces',
        },
        {
          key: 'sidebarSpaceSwitchBehavior',
          options: SIDEBAR_SPACE_SWITCH_BEHAVIOR_OPTIONS,
          subtitle:
            'Reopen the session you last had open in a Space when you switch to it, in the view its project was in. Requires Spaces.',
          title: 'When switching to a Space',
        },
        {
          key: 'sidebarSpaceFollowActiveSession',
          subtitle:
            'Switch the selected Space to the one that owns a session you open from outside it, such as through Back/Forward or Search by Prompt. Requires Spaces.',
          title: "Follow the active session's Space",
        },
        {
          key: 'projectSwitchKeepAliveMinutes',
          subtitle:
            'After you switch to another project or Space, keep the terminals, chats, and view that were open in the previous project running for this many minutes so switching back is instant. 0 releases them right away.',
          title: 'Keep the previous project live for',
        },
        {
          key: 'sidebarVisibilityMemory',
          options: SIDEBAR_VISIBILITY_MEMORY_OPTIONS,
          subtitle:
            'Keep one sidebar state everywhere, or remember it separately for Agents and for the wide views (Browser, Code, Docs, Kanban, Automate).',
          title: 'Sidebar visibility memory',
        },
        {
          key: 'showProjectIcons',
          subtitle: 'Show project artwork or a square with the project’s first letter beside project names.',
          title: 'Show project icons',
        },
        /*
         * CDXC:Settings 2026-06-30-22:22:
         * Search metadata follows the visible row order: preset-controlled rows
         * sit immediately after Preset, before independent sidebar sizing and
         * placement controls.
         */
        {
          key: 'hideSessionAgentIconUntilHover',
          subtitle: 'Hide session agent icons until a session row is hovered.',
          title: 'Hide agent icon until hover',
        },
        {
          key: 'hideBrowserFaviconUntilHover',
          subtitle: 'Hide browser page favicons until a session row is hovered.',
          title: 'Hide browser favicon until hover',
        },
        {
          key: 'hideLastActiveTimeOnSessionCards',
          subtitle: 'Hide Last Active timestamps from session-card title rows.',
          title: 'Hide last active time',
        },
        {
          key: 'hideProjectHeaderDiffStats',
          subtitle: 'Hide +added/-removed line counts in sidebar project rows.',
          title: 'Hide project git stats',
        },
        {
          key: 'showProjectEditorDiffFileCount',
          subtitle: 'Show changed-file counts in sidebar project row git stats.',
          title: 'Show changed-file count',
        },
        {
          key: 'hideMenuBarSessionStatusIndicators',
          subtitle: 'Show the menu bar session status badges.',
          title: 'Show Menu Bar Session Indicators',
        },
        {
          key: 'sidebarCollapseAnimationDurationMs',
          subtitle:
            'Set how quickly sidebar sections, groups, and projects expand or collapse, and how quickly the floating sidebar and Agents Panel slide in from the window edge. Set to 0 for no animation.',
          title: 'Collapse animation speed',
        },
        {
          key: 'panelAnimationSpeed',
          options: PANEL_ANIMATION_SPEED_OPTIONS,
          subtitle:
            'Set how fast the sidebar, the side panel, the Agents Panel and the bottom or right panel slide open and closed: Off, Slow, Normal or Fast. Reduce Motion in your computer settings always turns it off.',
          title: 'Panel animations',
        },
        {
          key: 'closeSidePanelWithLastTab',
          subtitle: 'Close the side panel when you close its last tab, instead of showing the Open a view picker.',
          title: 'Close side panel with its last tab',
        },
        {
          key: 'sidebarTooltipDelayMs',
          subtitle: 'Set how long sidebar hover labels wait before appearing. Set to 0 to show them immediately.',
          title: 'Tooltip Delay',
        },
        {
          key: 'sidebarDefaultWidthPx',
          subtitle: 'Width restored when double-clicking the sidebar resize handle.',
          title: 'Default Width',
        },
        {
          key: 'commandsPanelDefaultHeightPx',
          subtitle: 'Height used when opening the command pane and when double-clicking its top resize rail.',
          title: 'Command Pane Default Height',
        },
        {
          key: 'commandsPanelSide',
          options: COMMANDS_PANEL_SIDE_OPTIONS,
          subtitle: 'Dock the command pane below the workspace or to its right.',
          title: 'Command Pane Side',
        },
        {
          key: 'commandsPanelAutoMinimize',
          title: 'Auto-minimize Commands pane',
          subtitle:
            'Minimize the Commands pane after you stop using it and move focus elsewhere. Commands keep running.',
        },
        {
          key: 'commandsPanelAutoMinimizeDelaySeconds',
          title: 'Minimize after',
          subtitle: 'How long the Commands pane stays open after focus and the pointer leave it.',
          options: COMMANDS_PANEL_AUTO_MINIMIZE_DELAY_OPTIONS.map((option) => ({
            label: option.label,
            value: String(option.value),
          })),
        },
        {
          key: 'projectSessionListCollapsedCount',
          subtitle:
            'Rows a project shows in Compact mode before its "Show all" row. Rows in collapsed sections do not count.',
          title: 'Compact Session Rows',
        },
        {
          key: 'agentManagerZoomPercent',
          subtitle: 'Scale the sidebar interface.',
          title: 'Sidebar Interface Size',
        },
        {
          key: 'createSessionOnSidebarDoubleClick',
          subtitle: 'Create a session from empty sidebar space.',
          title: 'Double-click empty sidebar space to create a session',
        },
        {
          key: 'enableSessionParking',
          subtitle: 'Move deferred sessions into a collapsible Parked section at the bottom of the sidebar.',
          title: 'Enable session parking',
        },
        {
          key: 'sleepSessionWhenParking',
          subtitle: 'Sleep a session automatically when it is moved into the Parked section.',
          title: 'Sleep session when parking',
        },
        {
          key: 'showTagMenuWhenParking',
          subtitle: 'Open the Tag as menu when a session is parked or snoozed so it can be tagged right away.',
          title: 'Park & Snooze with tags',
        },
        {
          key: 'unparkAfterSendingMessage',
          subtitle: 'Move a parked session out of the Parked section when you send it a message.',
          title: 'Unpark after sending a message',
        },
        {
          key: 'renameSessionOnDoubleClick',
          subtitle: RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_SUBTITLE,
          title: RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_LABEL,
        },
      ],
    },
    theming: {
      title: 'Theme',
      settings: [
        {
          key: 'sidebarTheme',
          options: SIDEBAR_THEME_SETTING_OPTIONS,
          subtitle: 'Follow the system appearance by default, or choose Light or Dark.',
          title: 'Appearance',
        },
        {
          key: 'darkThemePreset',
          options: DARK_THEME_PRESET_OPTIONS,
          subtitle:
            'The colour of the sidebar and window in dark mode: one of sixteen colour squares, or a custom colour under More colour options.',
          title: 'Dark mode colour',
        },
        {
          key: 'customSidebarTitlebarBackgroundDarknessPercent',
          subtitle: 'How deep the custom dark mode colour is, while Custom colour in dark mode is on.',
          title: 'Custom dark mode depth',
        },
        {
          key: 'customSidebarTitlebarBackgroundTintColor',
          subtitle: 'The hue of the custom dark mode colour, while Custom colour in dark mode is on.',
          title: 'Custom dark mode tint',
        },
        {
          key: 'lightThemePreset',
          options: LIGHT_THEME_PRESET_OPTIONS,
          subtitle:
            'The colour of the sidebar and window in light mode: one of sixteen colour squares, or a custom colour under More colour options.',
          title: 'Light mode colour',
        },
        {
          key: 'themeSidebarContrast',
          subtitle:
            'How much of the theme colour shows in the sidebar, from Subtle (deeper, nearly neutral) to Vivid (lighter, more colourful). Colourfulness sets the sidebar and work area together.',
          title: 'Sidebar colourfulness',
        },
        {
          key: 'themeWorkAreaContrast',
          subtitle:
            'How much of the theme colour shows in the work area (chat, terminals and views), from Subtle to Vivid. Colourfulness sets the sidebar and work area together.',
          title: 'Work area colourfulness',
        },
        {
          key: 'customSidebarTitlebarLightBackgroundLightnessPercent',
          subtitle: 'How deep the custom light mode colour is, while Custom colour in light mode is on.',
          title: 'Custom light mode depth',
        },
        {
          key: 'customSidebarTitlebarLightBackgroundTintColor',
          subtitle: 'The hue of the custom light mode colour, while Custom colour in light mode is on.',
          title: 'Custom light mode tint',
        },
        {
          key: 'sessionChatTheme',
          options: SESSION_CHAT_THEME_OPTIONS,
          subtitle: 'Follow the app theme by default, or override chat with Light, Dark, or System.',
          title: 'Chat theme',
        },
        {
          key: 'terminalColorScheme',
          options: SESSION_CHAT_THEME_OPTIONS,
          subtitle: 'Follow the app theme by default, or override terminals with Light, Dark, or System.',
          title: 'Terminal theme',
        },
        {
          key: 'terminalGhosttyLightTheme',
          options: GHOSTTY_THEME_SETTING_OPTIONS.filter(
            (option) => option.value !== '__ghostex_ghostty_theme_unmanaged__'
          ),
          subtitle: 'Uses your configured Ghostty light theme, or GitHub Light when no theme is configured.',
          title: 'Terminal palette in light mode',
        },
        {
          key: 'terminalGhosttyTheme',
          options: GHOSTTY_THEME_SETTING_OPTIONS,
          subtitle: 'Uses your configured Ghostty dark theme, or GitHub Dark when no theme is configured.',
          title: 'Terminal palette in dark mode',
        },
        {
          key: 'windowGlass',
          options: WINDOW_GLASS_OPTIONS,
          subtitle: 'Let your desktop show through the window. Use transparency picks Dark only, Always or Never.',
          title: 'Enable transparency',
        },
        {
          key: 'windowGlassSource',
          options: WINDOW_GLASS_SOURCE_OPTIONS,
          subtitle:
            'Desktop and windows, your wallpaper, a picture you choose, a video, or a Live animated background in your theme colours.',
          title: 'What shows behind the glass',
        },
        {
          key: 'windowGlassImagePlacement',
          options: WINDOW_GLASS_IMAGE_PLACEMENT_OPTIONS,
          subtitle:
            'Where the wallpaper, custom picture or video sits behind the glass. Stays with the desktop can trail the window while you drag it.',
          title: 'Picture position',
        },
        {
          key: 'windowGlassImageDark',
          subtitle: 'The picture the glass blurs in dark mode when Picture shows behind the glass.',
          title: 'Picture for dark mode',
        },
        {
          key: 'windowGlassImageLight',
          subtitle: 'The picture the glass blurs in light mode when Picture shows behind the glass.',
          title: 'Picture for light mode',
        },
        {
          key: 'windowGlassVideoDark',
          subtitle:
            'The video the glass plays in dark mode when Video shows behind the glass: an aerial wallpaper your computer has downloaded, or a video file you choose.',
          title: 'Video for dark mode',
        },
        {
          key: 'windowGlassVideoLight',
          subtitle:
            'The video the glass plays in light mode when Video shows behind the glass: an aerial wallpaper your computer has downloaded, or a video file you choose.',
          title: 'Video for light mode',
        },
        {
          key: 'windowGlassVideoOnlyOnPower',
          subtitle:
            'Pause the glass video or Live background while your computer runs on battery. It always pauses when Ghostex is in the background.',
          title: 'Play only when plugged in',
        },
        {
          key: 'windowGlassLiveStyleDark',
          options: WINDOW_GLASS_LIVE_STYLE_OPTIONS,
          subtitle: 'The animated background the glass shows in dark mode when Live shows behind the glass.',
          title: 'Live background for dark mode',
        },
        {
          key: 'windowGlassLiveStyleLight',
          options: WINDOW_GLASS_LIVE_STYLE_OPTIONS,
          subtitle: 'The animated background the glass shows in light mode when Live shows behind the glass.',
          title: 'Live background for light mode',
        },
        {
          key: 'windowGlassLiveSpeed',
          subtitle: 'How fast the Live background moves, from a quarter of its pace to twice as fast.',
          title: 'Live background speed',
        },
        {
          key: 'windowGlassLiveBrightness',
          subtitle: 'How bright the Live background glows behind the glass. Lower keeps it a subtle glow.',
          title: 'Live background brightness',
        },
        {
          key: 'windowGlassSidebarOpacityDark',
          subtitle:
            'How much of the desktop the sidebar hides in dark mode. Lower shows more of your desktop through it.',
          title: 'Sidebar tint in dark mode',
        },
        {
          key: 'windowGlassWorkAreaTintDark',
          subtitle:
            'How much of the desktop the work area hides in dark mode, set on its own so either area can be the darker one. Lower shows more of your desktop through it.',
          title: 'Work area tint in dark mode',
        },
        {
          key: 'windowGlassSidebarOpacityLight',
          subtitle:
            'How much of the desktop the sidebar hides in light mode. Lower shows more of your desktop through it.',
          title: 'Sidebar tint in light mode',
        },
        {
          key: 'windowGlassWorkAreaTintLight',
          subtitle:
            'How much of the desktop the work area hides in light mode, set on its own so either area can be the darker one. Lower shows more of your desktop through it.',
          title: 'Work area tint in light mode',
        },
        {
          key: 'showActivePaneOutline',
          subtitle: 'Show an outline around the currently focused pane.',
          title: 'Show active pane outline',
        },
        {
          key: 'workspaceActivePaneBorderColor',
          subtitle: 'Color of the outline around the currently focused pane.',
          title: 'Active pane outline colour',
        },
      ],
    },
    chat: {
      title: 'Chat',
      settings: [
        {
          key: 'preferredAgentInterface',
          options: PREFERRED_AGENT_INTERFACE_OPTIONS,
          subtitle: 'Automatically switch to chat as soon as Ghostex detects that an agent session supports it.',
          title: 'Default view for compatible agents',
        },
        {
          key: 'sessionChatFontFamily',
          subtitle: 'Use any installed font in chat messages and the prompt composer.',
          title: 'Chat font family',
        },
        {
          key: 'sessionChatZoomPercent',
          subtitle:
            'Scale the desktop chat interface, including messages and the prompt composer, from 70% to 200% in 5% steps. Default: 100%.',
          title: 'Default chat zoom (%)',
        },
        {
          key: 'sessionChatCustomTranscriptWidthEnabled',
          subtitle: 'Let the transcript use a different width from the prompt composer.',
          title: 'Custom transcript width',
        },
        {
          key: 'sessionChatTranscriptWidthPercent',
          subtitle: 'Set the centered transcript width without changing the prompt composer.',
          title: 'Transcript width',
        },
        {
          key: 'sessionChatFileEditPreviews',
          subtitle:
            'Show the first seven code lines in each file edit. Turn off to show only the path and change counts.',
          title: 'Show file edit previews',
        },
        {
          key: 'sessionChatKeepComposerExpanded',
          subtitle:
            'Keep the desktop chat box at full size while you scroll the transcript instead of shrinking it as you scroll up and growing it back at the end.',
          title: 'Keep chat box expanded while scrolling',
        },
        {
          key: 'sessionChatSimpleMode',
          subtitle:
            'Simplify all chats: hide tool command previews and group file edits behind an expandable file count.',
          title: 'Simple mode',
        },
        {
          key: 'sessionChatVerboseMode',
          subtitle:
            'Expand thinking blocks to show their tool calls by default. Each chat can override it from its composer.',
          title: 'Verbose mode',
        },
      ],
    },
    sidebarTags: {
      title: 'Sidebar Tags',
      settings: [
        {
          key: 'sidebarSessionTagListItems',
          options: [
            ...DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS.map((item) => ({
              label: getSidebarSessionTagListItemLabel(item),
              value: item.id,
            })),
            { label: 'Hide tag', value: 'hide' },
            { label: 'Disable tag', value: 'disable' },
            { label: 'Reorder tags', value: 'reorder' },
          ],
          subtitle:
            'Add your own tags, then reorder, hide, disable, or delete tags and their separators for the sidebar and the Tag as menu.',
          title: 'Tag Filter List',
        },
      ],
    },
    sounds: {
      title: 'Sounds',
      settings: [
        {
          key: 'completionSound',
          options: [{ label: 'Off', value: 'off' }, ...COMPLETION_SOUND_OPTIONS],
          subtitle: 'Sound for terminal completions, or Off.',
          title: 'Completion Sound',
        },
        {
          key: 'showMacOSAttentionNotifications',
          subtitle: 'Show a system notification when a session needs attention.',
          title: 'Attention Notifications',
        },
        {
          key: 'attentionNotificationActions',
          subtitle: 'Test the current completion alert settings or open Notification Settings.',
          title: 'Agent Completion Alert Test',
        },
        {
          key: 'actionCompletionSound',
          options: COMPLETION_SOUND_OPTIONS,
          subtitle: 'Sound for action completions.',
          title: 'Action Completion Sound',
        },
        {
          key: 'copySound',
          subtitle: 'Play a short sound when copying to the clipboard, including text from the chat composer.',
          title: 'Copy Sound',
        },
      ],
    },
    terminal: {
      title: 'Terminal',
      settings: [
        ...(IS_WINDOWS_HOST
          ? [
              {
                key: 'windowsTerminalBackend',
                title: 'Windows Environment',
                subtitle:
                  'PowerShell (default) runs native Windows projects and agents. WSL runs Linux projects. Changing environments prompts you to restart Ghostex.',
                options: [
                  { label: 'PowerShell (Native)', value: 'powershell' },
                  { label: 'WSL (Linux)', value: 'wsl' },
                ],
              },
              {
                key: 'windowsWslDistribution',
                subtitle:
                  'Optional exact distro name from `wsl.exe --list --verbose`; blank uses automatic WSL2 discovery.',
                title: 'WSL distribution',
              },
            ]
          : []),
        {
          key: 'ghosttySettingsActions',
          options: [
            { label: 'Apply recommended', value: 'applyRecommendedGhosttySettings' },
            { label: 'Open Ghostty config', value: 'openGhosttyConfigFile' },
            { label: 'Open Ghostty docs', value: 'openGhosttySettingsDocs' },
            { label: 'Reset Ghostty defaults', value: 'resetGhosttySettingsToDefault' },
          ],
          subtitle: 'Recommended Ghostty settings, Ghostty config file, Ghostty docs, and Ghostty defaults.',
          title: 'Ghostty settings actions',
        },
        {
          key: 'workspaceBackgroundColor',
          subtitle: 'Only changes the terminal panes. Leave on Follow theme to match your theme.',
          title: 'Terminal background',
        },
        {
          key: 'terminalBackgroundImage',
          subtitle: 'Absolute path to an image drawn behind terminal panes.',
          title: 'Background Image',
        },
        {
          key: 'terminalBackgroundImageOpacity',
          subtitle: 'Blend the background image toward the terminal background color.',
          title: 'Background Image Opacity',
        },
        {
          key: 'terminalBackgroundImageFit',
          options: [
            { label: 'Cover', value: 'cover' },
            { label: 'Contain', value: 'contain' },
            { label: 'Stretch', value: 'stretch' },
            { label: 'Natural size', value: 'natural' },
          ],
          subtitle: 'How the background image is scaled inside each pane.',
          title: 'Background Image Fit',
        },
        {
          key: 'terminalFontFamily',
          subtitle: 'Type a Ghostty font-family name.',
          title: 'Font Family',
        },
        {
          key: 'terminalFontSize',
          subtitle: 'Set terminal text size.',
          title: 'Font Size',
        },
        {
          key: 'terminalFontWeight',
          subtitle: 'Set terminal text weight.',
          title: 'Font Weight',
        },
        {
          key: 'terminalLineHeight',
          subtitle: 'Adjust terminal row height.',
          title: 'Line Height',
        },
        {
          key: 'terminalLetterSpacing',
          subtitle: 'Adjust spacing between glyphs.',
          title: 'Letter Spacing',
        },
        {
          key: 'terminalViewWidthMode',
          options: TERMINAL_VIEW_WIDTH_MODE_OPTIONS,
          subtitle: 'Use the full pane, match the chat transcript, or set an independent terminal width.',
          title: 'Terminal width mode',
        },
        {
          key: 'terminalViewWidthPercent',
          subtitle: 'Set the centered terminal body width as a percentage.',
          title: 'Terminal Width (%)',
        },
        {
          key: 'terminalWidthApplyToCommandPaneTerminals',
          subtitle: 'Apply the narrower terminal width to command pane terminals too.',
          title: 'Apply Width to Command Pane Terminals',
        },
        {
          key: 'terminalPaneHorizontalPaddingPx',
          subtitle: 'Add left and right inner padding inside the terminal content area.',
          title: 'Horizontal Padding',
        },
        {
          key: 'terminalPaneVerticalPaddingPx',
          subtitle: 'Add top and bottom inner padding inside the terminal content area.',
          title: 'Vertical Padding',
        },
        {
          key: 'terminalCursorStyle',
          options: [
            { label: 'Line', value: 'bar' },
            { label: 'Block', value: 'block' },
            { label: 'Underline', value: 'underline' },
          ],
          subtitle: 'Choose the cursor shape.',
          title: 'Cursor Style',
        },
        {
          key: 'terminalCursorStyleBlink',
          subtitle: 'Blink the terminal cursor.',
          title: 'Cursor blink',
        },
        {
          key: 'clickToWakeSleepingSessions',
          subtitle: 'Select sleeping pane tabs without waking them until the empty pane is clicked.',
          title: 'Click to Wake Sleeping Panes',
        },
        {
          key: 'showQuickModelPickerInTerminal',
          subtitle: `Show a model button in the terminal bar and open the model picker with its shortcut (${formatSidebarHotkeyLabel('alt+p')} by default) in agent terminal sessions. Turn off to use terminal bindings.`,
          title: 'Model picker in terminal view',
        },
        {
          key: 'showSessionIdInTerminalPanes',
          subtitle: 'Show the provider session id in the top-right corner of terminal panes.',
          title: 'Show session id in terminal panes',
        },
        {
          key: 'showNotificationOnTerminalBell',
          subtitle: 'Treat terminal bell events as session attention.',
          title: 'Show notification on terminal bell',
        },
        {
          key: 'promptEditorBackend',
          options: PROMPT_EDITOR_BACKEND_OPTIONS,
          subtitle: `Choose which editor ${formatSidebarHotkeyLabel('ctrl+g')} uses when a terminal prompt asks for $EDITOR.`,
          title: `${formatSidebarHotkeyLabel('ctrl+g')} prompt editor`,
        },
      ],
    },
    terminalBehavior: {
      title: 'Terminal Behavior',
      settings: [
        {
          key: 'terminalScrollbackLimitMb',
          subtitle: 'Set scrollback memory per terminal surface.',
          title: 'Scrollback limit',
        },
        {
          key: 'terminalCopyOnSelect',
          options: GHOSTTY_COPY_ON_SELECT_OPTIONS,
          subtitle: 'Copy selected terminal text automatically.',
          title: 'Copy on select',
        },
        {
          key: 'terminalConfirmCloseSurface',
          options: GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS,
          subtitle: 'Confirm before closing terminal surfaces.',
          title: 'Confirm close',
        },
        {
          key: 'terminalClipboardTrimTrailingSpaces',
          subtitle: 'Trim trailing whitespace when copying terminal text.',
          title: 'Trim trailing spaces on copy',
        },
        {
          key: 'terminalClipboardPasteProtection',
          subtitle: 'Ask before pasting text Ghostty considers unsafe.',
          title: 'Paste protection',
        },
        {
          key: 'terminalPastePreviewableImages',
          subtitle: PASTE_PREVIEWABLE_IMAGES_DESCRIPTION,
          title: 'Paste previewable images',
        },
        {
          key: 'terminalMouseHideWhileTyping',
          subtitle: 'Hide the pointer while typing in the terminal.',
          title: 'Hide mouse while typing',
        },
        {
          key: 'terminalScrollbar',
          options: GHOSTTY_SCROLLBAR_OPTIONS,
          subtitle: 'Control whether Ghostty shows its native scrollback scrollbar.',
          title: 'Scrollbar',
        },
      ],
    },
    terminalScrolling: {
      title: 'Terminal Scrolling',
      settings: [
        {
          key: 'terminalMouseScrollMultiplierPrecision',
          subtitle: 'Trackpads and high-resolution scroll wheels. Ghostty default is 1.',
          title: 'Precision scroll multiplier',
        },
        {
          key: 'terminalMouseScrollMultiplierDiscrete',
          subtitle: 'Traditional notched mouse wheels. Ghostty default is 3.',
          title: 'Discrete scroll multiplier',
        },
        {
          key: 'terminalScrollToBottomWhenTyping',
          subtitle: 'Keep the prompt visible while typing.',
          title: 'Scroll to bottom when typing',
        },
      ],
    },
    terminalDevServers: {
      title: 'Dev Servers',
      settings: [
        {
          key: 'terminalDevServerDetectionEnabled',
          subtitle: 'Detect localhost dev server URLs from terminal output.',
          title: 'Detect running servers in terminals',
        },
        {
          key: 'terminalDevServerIgnoredPortRules',
          options: [
            { label: '9229', value: '9229' },
            { label: '24678-24680', value: '24678-24680' },
          ],
          subtitle: 'Hide detected servers on specific ports or inclusive port ranges.',
          title: 'Ignored ports',
        },
      ],
    },
    beta: {
      title: 'Experimental',
      settings: [
        /*
         * CDXC:Settings 2026-06-28-07:41:
         * Settings search should find the advanced experimental gate by label and
         * by the concrete surfaces it enables so the required inventory stays
         * discoverable without tying Agents Hub to this gate.
         */
        {
          key: 'showBetaFeatures',
          subtitle: 'Show experimental surfaces: OS Integration settings, Browser color scheme, and Keep Awake.',
          title: 'Enable Experimental Features',
        },
      ],
    },
  } satisfies Record<string, SettingsSearchSectionDefinition>;

  return settingsSearchSections;
}

export type SettingsSearchSectionId = keyof ReturnType<typeof getSettingsSearchSectionDefinitions>;

export function getSettingsSearchSections(settingsSearchQuery: string, _draft: ghostexSettings) {
  /**
   * CDXC:Settings 2026-05-04-02:30
   * Settings search must be fuzzy and cover section titles, setting subtitles,
   * and selectable option text so users can find controls by the value they
   * want to choose, not only by the visible setting label.
   */
  const definitions = getSettingsSearchSectionDefinitions();
  const settingsSearch = Object.fromEntries(
    Object.entries(definitions).map(([sectionId, definition]) => [
      sectionId,
      getSettingsSectionSearch(settingsSearchQuery, definition.title, definition.settings),
    ])
  ) as Record<SettingsSearchSectionId, SettingsSectionSearchResult>;

  return settingsSearch;
}

export type SettingsSearchSections = ReturnType<typeof getSettingsSearchSections>;

/**
 * Which General-page search sections each navigation-rail group collects, in
 * rendered order. Single-section groups keep using that section's own search
 * result (no separate group-title match), exactly as before this table existed.
 */
export type MainSettingsGroupId = Exclude<MainSettingsSectionId, 'agents'>;

export const MAIN_SETTINGS_GROUP_SECTIONS: Record<
  MainSettingsGroupId,
  { sections: readonly SettingsSearchSectionId[]; title: string }
> = {
  appearance: { sections: ['theming', 'appIcon'], title: 'Theme' },
  chat: { sections: ['chat'], title: 'Chat' },
  sidebar: { sections: ['sidebar', 'sessionCards', 'sidebarTags'], title: 'Sidebar' },
  terminal: { sections: ['terminal', 'terminalBehavior', 'terminalScrolling'], title: 'Terminal' },
  tools: { sections: ['browser', 'terminalDevServers', 'editor'], title: 'Tools' },
  statusIndicators: { sections: ['statusIndicators'], title: 'Status Indicators' },
  notifications: { sections: ['sounds'], title: 'Notifications' },
  system: { sections: ['autoSleep', 'power'], title: 'System' },
  advanced: { sections: ['beta'], title: 'Advanced' },
};

export function getMainSettingsGroupSearch(settingsSearchQuery: string, settingsSearch: SettingsSearchSections) {
  const mainSettingsGroupSearch = Object.fromEntries(
    Object.entries(MAIN_SETTINGS_GROUP_SECTIONS).map(([groupId, group]) => [
      groupId,
      group.sections.length === 1
        ? settingsSearch[group.sections[0]]
        : getGroupedSettingsSectionSearch(
            settingsSearchQuery,
            group.title,
            group.sections.map((sectionId) => settingsSearch[sectionId])
          ),
    ])
  ) as Record<MainSettingsGroupId, SettingsSectionSearchResult>;

  return mainSettingsGroupSearch;
}

export type MainSettingsGroupSearch = ReturnType<typeof getMainSettingsGroupSearch>;

export function getMainSettingsSectionNavigation(mainSettingsGroupSearch: MainSettingsGroupSearch) {
  const mainSettingsSectionNavigation: Array<{
    id: MainSettingsSectionId;
    searchResult: SettingsSectionSearchResult;
    title: string;
  }> = [
    /*
     * Keep these destinations in the same order as their first rendered
     * section anchors below. The grouped pages intentionally collect related
     * subsections, but clicking down this rail should always move down the
     * Settings page instead of jumping above an earlier-looking destination.
     */
    /*
     * CDXC:Theming 2026-09-23 DECISION:
     * User: "make theme into it's own page in settings below General". The Theme and App Icon sections render on the Theme page (settings-modal/tabs/theme.tsx), so General's rail starts at Sidebar; their search rows and the `appearance` group stay in this catalog so one query still finds them.
     */
    { id: 'sidebar', searchResult: mainSettingsGroupSearch.sidebar, title: 'Sidebar' },
    { id: 'chat', searchResult: mainSettingsGroupSearch.chat, title: 'Chat' },
    ...(PET_CONTROLS_VISIBLE
      ? [
          {
            id: 'statusIndicators' as const,
            searchResult: mainSettingsGroupSearch.statusIndicators,
            title: 'Status Indicators',
          },
        ]
      : []),
    {
      id: 'tools',
      searchResult: mainSettingsGroupSearch.tools,
      title: 'Tools',
    },
    /*
     * CDXC:Settings 2026-06-12-04:13:
     * Ghostty terminal controls belong on the main Settings page so one search query can find app settings and terminal settings together.
     */
    { id: 'terminal', searchResult: mainSettingsGroupSearch.terminal, title: 'Terminal' },
    {
      id: 'system',
      searchResult: mainSettingsGroupSearch.system,
      title: 'System',
    },
    {
      id: 'notifications',
      searchResult: mainSettingsGroupSearch.notifications,
      title: 'Notifications',
    },
    { id: 'advanced', searchResult: mainSettingsGroupSearch.advanced, title: 'Advanced' },
  ];

  return mainSettingsSectionNavigation;
}

export type MainSettingsSectionNavigation = ReturnType<typeof getMainSettingsSectionNavigation>;
