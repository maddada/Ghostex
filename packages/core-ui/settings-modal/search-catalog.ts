/*
 * CDXC:SettingsModalSplit 2026-08-23:
 * Settings search metadata, the grouped page search, and the main-settings
 * navigation rail are pure derivations of the search query and the settings
 * draft, so they live here instead of inside the SettingsModal component body.
 */
import { COMPLETION_SOUND_OPTIONS } from "../../shared/completion-sound";
import {
  AUTO_SLEEP_IDLE_MINUTE_OPTIONS,
  COMMANDS_PANEL_SIDE_OPTIONS,
  DIAGNOSTIC_LOGGING_SCENARIOS,
  GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS,
  GHOSTTY_COPY_ON_SELECT_OPTIONS,
  GHOSTTY_SCROLLBAR_OPTIONS,
  GHOSTTY_THEME_SETTING_OPTIONS,
  KEEP_AWAKE_DURATION_OPTIONS,
  PREFERRED_AGENT_INTERFACE_OPTIONS,
  PROMPT_EDITOR_BACKEND_OPTIONS,
  SESSION_CHAT_THEME_OPTIONS,
  SESSION_PERSISTENCE_PROVIDER_OPTIONS,
  SIDEBAR_AUTO_SETTLE_AFTER_DAYS_OPTIONS,
  SIDEBAR_PROJECT_GROUP_STYLE_OPTIONS,
  SIDEBAR_SETTINGS_PRESETS,
  SIDEBAR_SIDE_OPTIONS,
  SIDEBAR_VERSION_OPTIONS,
  WEB_LINK_OPEN_TARGET_OPTIONS,
  WINDOWS_TERMINAL_BACKEND_OPTIONS,
  type ghostexSettings,
} from "../../shared/ghostex-settings";
import { PET_CONTROLS_VISIBLE, PET_OPTIONS } from "../../shared/pets";
import {
  DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS,
  getSidebarSessionTagListItemLabel,
} from "../../shared/session-tags";
import { getGroupedSettingsSectionSearch, getSettingsSectionSearch } from "./search";
import {
  MainSettingsSectionId,
  RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_LABEL,
  RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_SUBTITLE,
  SettingsSectionSearchResult,
} from "./types";

export const IS_WINDOWS_HOST =
  typeof navigator !== "undefined" && /Windows/iu.test(navigator.userAgent);

export const PASTE_PREVIEWABLE_IMAGES_DESCRIPTION =
  "Paste clipboard images as previewable Markdown links with Cmd+V or Ctrl+V. Hold Cmd over the linked path to preview it in the terminal, and see the same image preview in the Ctrl+G Rich Prompt Editor.";

export function getSettingsSearchSections(settingsSearchQuery: string, draft: ghostexSettings) {
  /**
   * CDXC:SettingsSearch 2026-05-04-02:30
   * Settings search must be fuzzy and cover section titles, setting subtitles,
   * and selectable option text so users can find controls by the value they
   * want to choose, not only by the visible setting label.
   */
  const settingsSearch = {
    // CDXC:AppIconPicker 2026-06-25-21:50: Make the App Icon section findable by Settings search.
    appIcon: getSettingsSectionSearch(settingsSearchQuery, "App Icon", [
      {
        key: "appIconSourceId",
        subtitle:
          "Choose the macOS Dock and app-switcher icon. The app file icon may also change when macOS allows it.",
        title: "App Icon",
      },
    ]),
    builtInFeatures: getSettingsSectionSearch(
      settingsSearchQuery,
      "Built-in feature switches",
      [
        {
          key: "codeViewTabHidden",
          subtitle: "Show or hide Code in the title bar without disabling its runtime.",
          title: "Code",
        },
        {
          key: "kanbanViewTabHidden",
          subtitle: "Show or hide Kanban in the title bar without disabling its runtime.",
          title: "Kanban",
        },
        {
          key: "automateViewTabHidden",
          subtitle: "Show or hide Automate in the title bar without disabling its runtime.",
          title: "Automate",
        },
        {
          key: "docsViewTabHidden",
          subtitle: "Show or hide Docs in the title bar without disabling its runtime.",
          title: "Docs",
        },
      ],
    ),
    browser: getSettingsSectionSearch(settingsSearchQuery, "Browser", [
      {
        key: "webLinkOpenTarget",
        options: WEB_LINK_OPEN_TARGET_OPTIONS,
        subtitle:
          "Open web links from terminal output (Command-click), session chat, and detected dev servers in the project Browser view or the system default browser.",
        title: "Open links in",
      },
    ]),
    editor: getSettingsSectionSearch(settingsSearchQuery, "Editor", [
      {
        key: "codeServerLinkVscodeUserConfig",
        subtitle: "Use the VS Code settings from the local VS Code install.",
        title: "Use VS Code settings",
      },
      {
        key: "codeServerUseVscodeInsidersUserConfig",
        subtitle: "Use the VS Code Insiders user settings directory.",
        title: "Use VS Code Insiders settings",
      },
      {
        key: "showUntrackedProjectDiffWhenNoTrackedChanges",
        subtitle:
          "When tracked git diff is +0 -0, show untracked line counts in project headers (Starship-style prompts ignore untracked lines).",
        title: "Show untracked lines without tracked changes",
      },
    ]),
    autoSleep: getSettingsSectionSearch(settingsSearchQuery, "Auto Sleep", [
      {
        key: "autoSleepCodeEditorEnabled",
        subtitle: "Sleep inactive VS Code panes after the selected idle period.",
        title: "Sleep inactive VS Code panes",
      },
      {
        key: "autoSleepCodeEditorIdleMinutes",
        options: AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
          label: option.label,
          value: String(option.value),
        })),
        subtitle: "Idle time before inactive VS Code panes sleep.",
        title: "VS Code idle time",
      },
      {
        key: "autoSleepGitEditorEnabled",
        subtitle: "Sleep inactive Git panes after the selected idle period.",
        title: "Sleep inactive Git panes",
      },
      {
        key: "autoSleepGitEditorIdleMinutes",
        options: AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
          label: option.label,
          value: String(option.value),
        })),
        subtitle: "Idle time before inactive Git panes sleep.",
        title: "Git idle time",
      },
      {
        key: "autoSleepProjectEditorEnabled",
        subtitle: "Sleep inactive Project panes after the selected idle period.",
        title: "Sleep inactive Project panes",
      },
      {
        key: "autoSleepProjectEditorIdleMinutes",
        options: AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
          label: option.label,
          value: String(option.value),
        })),
        subtitle: "Idle time before inactive Project panes sleep.",
        title: "Project idle time",
      },
      {
        key: "autoSleepBrowserSessionsEnabled",
        subtitle: "Sleep inactive browser panes after the selected idle period.",
        title: "Sleep inactive browser panes",
      },
      {
        key: "autoSleepBrowserIdleMinutes",
        options: AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
          label: option.label,
          value: String(option.value),
        })),
        subtitle: "Idle time before inactive browser panes sleep.",
        title: "Browser idle time",
      },
      {
        key: "autoSleepAgentSessionsEnabled",
        subtitle: "Sleep idle agent terminal sessions automatically.",
        title: "Sleep idle agent sessions",
      },
      {
        key: "autoSleepAgentIdleMinutes",
        options: AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
          label: option.label,
          value: String(option.value),
        })),
        subtitle: "Idle time before eligible agent terminals sleep.",
        title: "Agent idle time",
      },
      {
        key: "autoSleepRequireAgentResumeCommand",
        subtitle: "Only auto-sleep agent sessions Ghostex can wake with a resume command.",
        title: "Require resume command",
      },
      {
        key: "autoSleepFavoriteAgentSessions",
        subtitle: "Allow favorite agent sessions to auto-sleep.",
        title: "Include favorite agents",
      },
    ]),
    power: getSettingsSectionSearch(settingsSearchQuery, "Power", [
      {
        key: "hideKeepAwakeTitlebarControl",
        subtitle: "Hide the keep-awake control from the title bar.",
        title: "Hide title-bar keep-awake control",
      },
      {
        key: "keepAwakeDefaultDurationMinutes",
        options: KEEP_AWAKE_DURATION_OPTIONS.map((option) => ({
          label: option.label,
          value: String(option.value),
        })),
        subtitle: "Choose the duration used by the title-bar keep-awake button.",
        title: "Default keep-awake duration",
      },
      {
        key: "keepAwakeAllowDisplaySleep",
        subtitle: "Keep the Mac awake but allow the display to turn off.",
        title: "Allow display sleep",
      },
      {
        key: "keepAwakePreventLidSleep",
        subtitle:
          "Optional. When Keep Awake is on, Ghostex can install a small privileged helper once so closing the lid stays awake only for that active keep-awake session.",
        title: "Prevent lid-close sleep",
      },
      {
        key: "keepAwakeActivateOnLaunch",
        subtitle: "Start preventing sleep when Ghostex launches.",
        title: "Activate on launch",
      },
      {
        key: "keepAwakeActivateOnExternalDisplay",
        subtitle: "Start preventing sleep when an external display is connected.",
        title: "Activate on external display",
      },
      {
        key: "keepAwakeWhileWorkingSessions",
        subtitle: "Keep the Mac awake while sessions are working and for 20 minutes after.",
        title: "Keep awake for working sessions",
      },
      {
        key: "keepAwakeDeactivateBelowBatteryThreshold",
        subtitle: "Stop preventing sleep when battery capacity drops below the threshold.",
        title: "Deactivate below battery threshold",
      },
      {
        key: "keepAwakeBatteryThresholdPercent",
        subtitle: "Battery percentage used by the threshold rule.",
        title: "Battery threshold",
      },
      {
        key: "keepAwakeDeactivateOnLowPowerMode",
        subtitle: "Stop preventing sleep when macOS Low Power Mode is enabled.",
        title: "Deactivate in Low Power Mode",
      },
      {
        key: "keepAwakeDeactivateOnUserSwitch",
        subtitle: "Stop preventing sleep when this user session is no longer active.",
        title: "Deactivate on user switch",
      },
    ]),
    sessionCards: getSettingsSectionSearch(settingsSearchQuery, "Session Cards", [
      {
        key: "useColoredSessionAgentIcons",
        subtitle: "Render session and selected-agent logos with colored brand artwork instead of monochrome masks.",
        title: "Use colored agent icons",
      },
      /*
       * CDXC:SidebarSessions 2026-05-15-19:46:
       * Settings must not expose the card-hotkey visibility row; session-card shortcut visibility is no longer configurable from the modal.
       */
      {
        key: "showSessionCloseContextMenuAction",
        subtitle: "Show the Close item in session context menus.",
        title: "Show Close option in context menu",
      },
    ]),
    statusIndicators: getSettingsSectionSearch(
      settingsSearchQuery,
      "Status Indicators",
      PET_CONTROLS_VISIBLE
        ? [
            /*
             * CDXC:StatusIndicators 2026-05-20-12:00:
             * Status Indicators groups session presence surfaces that communicate
             * status at a glance.
             *
             * CDXC:StatusIndicators 2026-06-27-20:11:
             * The removed floating session badge and its size selector must not
             * appear in macOS or GPUI Settings.
             *
             * CDXC:SidebarSettingsPresets 2026-06-30-22:22:
             * The menu bar session indicator now lives under Sidebar because sidebar
             * presets mutate it.
             */
            {
              key: "petOverlayEnabled",
              subtitle: "Show the draggable animated pet in the native sidebar.",
              title: "Wake Pet",
            },
            {
              key: "selectedPetId",
              options: PET_OPTIONS.map((option) => ({
                label: option.displayName,
                value: option.id,
              })),
              subtitle: "Choose the pet sprite.",
              title: "Pet",
            },
          ]
        : [],
    ),
    sidebar: getSettingsSectionSearch(settingsSearchQuery, "Sidebar", [
      /*
       * CDXC:SidebarV2 2026-07-29:
       * Sidebar version must be findable by searching for the new Inbox
       * sidebar, its Classic alternative, or the Group by Project sub-mode.
       */
      {
        key: "sidebarVersion",
        options: SIDEBAR_VERSION_OPTIONS,
        subtitle:
          "Choose the classic sidebar or the new Inbox sidebar, a flat list of sessions across all projects.",
        title: "Sidebar version",
      },
      {
        key: "sidebarV2Layout",
        subtitle: "Group Inbox sidebar sessions into collapsible project groups.",
        title: "Group by project",
      },
      /*
       * CDXC:SidebarV2Lifecycle 2026-07-29:
       * Auto-settle must be findable by searching for "settle", "snooze", or
       * "inactive", because the visible symptom is a session leaving the inbox.
       */
      {
        key: "sidebarAutoSettleAfterDays",
        options: SIDEBAR_AUTO_SETTLE_AFTER_DAYS_OPTIONS,
        subtitle:
          "Move inactive Inbox sidebar sessions to the Settled shelf after this many days. Working and blocked sessions never settle.",
        title: "Auto-settle inactive sessions",
      },
      {
        key: "sidebarSettingsPreset",
        options: [
          ...SIDEBAR_SETTINGS_PRESETS.map((preset) => ({
            label: preset.label,
            value: preset.id,
          })),
          { label: "Custom", value: "custom" },
        ],
        subtitle: "Apply a sidebar UI preset or show Custom when controlled settings diverge.",
        title: "Preset",
      },
      {
        key: "sidebarProjectGroupStyle",
        options: SIDEBAR_PROJECT_GROUP_STYLE_OPTIONS,
        subtitle: "Choose how project groups are marked in the sidebar.",
        title: "Project group style",
      },
      {
        key: "showProjectIcons",
        subtitle: "Show project artwork or a folder or worktree icon beside project names.",
        title: "Show project icons",
      },
      /*
       * CDXC:SidebarSettingsPresets 2026-06-30-22:22:
       * Search metadata follows the visible row order: preset-controlled rows
       * sit immediately after Preset, before independent sidebar sizing and
       * placement controls.
       */
      {
        key: "hideSessionAgentIconUntilHover",
        subtitle: "Hide session agent icons until a session row is hovered.",
        title: "Hide agent icon until hover",
      },
      {
        key: "hideBrowserFaviconUntilHover",
        subtitle: "Hide browser page favicons until a session row is hovered.",
        title: "Hide browser favicon until hover",
      },
      {
        key: "showCloseButtonOnSessionCards",
        subtitle: "Reveal the close control when hovering a card.",
        title: "Show close button on hover",
      },
      {
        key: "hideLastActiveTimeOnSessionCards",
        subtitle: "Hide Last Active timestamps from session-card title rows.",
        title: "Hide last active time",
      },
      {
        key: "hideProjectHeaderDiffStats",
        subtitle: "Hide +added/-removed line counts in sidebar project rows.",
        title: "Hide project git stats",
      },
      {
        key: "showProjectEditorDiffFileCount",
        subtitle: "Show changed-file counts in sidebar project row git stats.",
        title: "Show changed-file count",
      },
      {
        key: "hideMenuBarSessionStatusIndicators",
        subtitle: "Show the menu bar session status badges.",
        title: "Show Menu Bar Session Indicators",
      },
      {
        key: "sidebarSide",
        options: SIDEBAR_SIDE_OPTIONS,
        subtitle: "Choose which side of the screen holds the sidebar.",
        title: "Side",
      },
      {
        key: "sidebarCollapseAnimationDurationMs",
        subtitle: "Set how quickly sidebar sections, groups, and projects expand or collapse. Set to 0 for no animation.",
        title: "Collapse animation speed",
      },
      {
        key: "sidebarDefaultWidthPx",
        subtitle: "Width restored when double-clicking the sidebar resize handle.",
        title: "Default Width",
      },
      {
        key: "commandsPanelDefaultHeightPx",
        subtitle: "Height used when opening the command pane and when double-clicking its top resize rail.",
        title: "Command Pane Default Height",
      },
      {
        key: "commandsPanelSide",
        options: COMMANDS_PANEL_SIDE_OPTIONS,
        subtitle: "Dock the command pane below the workspace or to its right.",
        title: "Command Pane Side",
      },
      {
        key: "projectSessionListCollapsedCount",
        subtitle: "Number of project sessions kept visible after Show less.",
        title: "Show Less Count",
      },
      {
        key: "agentManagerZoomPercent",
        subtitle: "Scale the sidebar interface.",
        title: "Sidebar Interface Size",
      },
      {
        key: "createSessionOnSidebarDoubleClick",
        subtitle: "Create a session from empty sidebar space.",
        title: "Double-click empty sidebar space to create a session",
      },
      {
        key: "renameSessionOnDoubleClick",
        subtitle: RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_SUBTITLE,
        title: RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_LABEL,
      },
    ]),
    theming: getSettingsSectionSearch(settingsSearchQuery, "Theming", [
      {
        key: "sidebarTheme",
        subtitle: "Light theme coming soon.",
        title: "Theme",
      },
      {
        key: "customSidebarTitlebarBackgroundDarknessPercent",
        subtitle: "Contrast level for the sidebar and titlebar background.",
        title: "Background Contrast",
      },
      {
        key: "customSidebarTitlebarBackgroundTintColor",
        subtitle: "Subtle tint color for the sidebar and titlebar background.",
        title: "Background Tint",
      },
      {
        key: "workspaceActivePaneBorderColor",
        subtitle: "CSS color for the focused pane border.",
        title: "Active Pane Border",
      },
    ]),
    chat: getSettingsSectionSearch(settingsSearchQuery, "Chat", [
      {
        key: "preferredAgentInterface",
        options: PREFERRED_AGENT_INTERFACE_OPTIONS,
        subtitle:
          "Automatically switch to chat as soon as Ghostex detects that an agent session supports it.",
        title: "Default view for compatible agents",
      },
      {
        key: "sessionChatTheme",
        options: SESSION_CHAT_THEME_OPTIONS,
        subtitle: "Choose the palette used by chat messages, thinking, tools, edits, and Markdown.",
        title: "Chat appearance",
      },
      {
        key: "sessionChatFontFamily",
        subtitle: "Use any installed font in chat messages and the prompt composer.",
        title: "Chat font family",
      },
      {
        key: "sessionChatTranscriptWidthPercent",
        subtitle: "Set the width of the message transcript without changing the prompt composer.",
        title: "Chat message width",
      },
      {
        key: "sessionChatVerboseMode",
        subtitle:
          "Expand thinking blocks to show their tool calls by default. Each chat can override it from its composer.",
        title: "Verbose mode",
      },
    ]),
    sidebarTags: getSettingsSectionSearch(settingsSearchQuery, "Sidebar Tags", [
      {
        key: "sidebarSessionTagListItems",
        options: [
          ...DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS.map((item) => ({
            label: getSidebarSessionTagListItemLabel(item),
            value: item.id,
          })),
          { label: "Hide tag", value: "hide" },
          { label: "Disable tag", value: "disable" },
          { label: "Reorder tags", value: "reorder" },
        ],
        subtitle:
          "Reorder, hide, or disable sidebar tag filters and their separators.",
        title: "Tag Filter List",
      },
    ]),
    sounds: getSettingsSectionSearch(settingsSearchQuery, "Sounds", [
      {
        key: "completionBellEnabled",
        subtitle: "Play a completion sound when work finishes.",
        title: "Enable completion bell",
      },
      {
        key: "completionSound",
        options: COMPLETION_SOUND_OPTIONS,
        subtitle: "Sound for terminal completions.",
        title: "Completion Sound",
      },
      {
        key: "showMacOSAttentionNotifications",
        subtitle: "Show a macOS banner when a session needs attention.",
        title: "macOS Attention Notifications",
      },
      {
        key: "attentionNotificationActions",
        subtitle: "Test the current completion alert settings or open macOS Notification Settings.",
        title: "Agent Completion Alert Test",
      },
      {
        key: "actionCompletionSound",
        options: COMPLETION_SOUND_OPTIONS,
        subtitle: "Sound for action completions.",
        title: "Action Completion Sound",
      },
    ]),
    storage: getSettingsSectionSearch(settingsSearchQuery, "Storage", [
      {
        key: "ghostexFolderStats",
        options: [
          { label: "Open Ghostex folder", value: "openGhostexFolder" },
          { label: "Folder sizes", value: "folderSizes" },
          { label: "Disk usage", value: "diskUsage" },
        ],
        subtitle: "Show Ghostex data-folder sizes and open the resolved storage folder.",
        title: "Ghostex folder",
      },
    ]),
    terminal: getSettingsSectionSearch(settingsSearchQuery, "Terminal", [
      ...(IS_WINDOWS_HOST
        ? [
            {
              key: "windowsTerminalBackend",
              options: WINDOWS_TERMINAL_BACKEND_OPTIONS,
              subtitle: "Windows terminals currently run through WSL2.",
              title: "Windows terminal backend",
            },
            {
              key: "windowsWslDistribution",
              subtitle:
                "Optional exact distro name from `wsl.exe --list --verbose`; blank uses automatic WSL2 discovery.",
              title: "WSL distribution",
            },
          ]
        : []),
      {
        key: "ghosttySettingsActions",
        options: [
          { label: "Apply recommended", value: "applyRecommendedGhosttySettings" },
          { label: "Open Ghostty config", value: "openGhosttyConfigFile" },
          { label: "Open Ghostty docs", value: "openGhosttySettingsDocs" },
          { label: "Reset Ghostty defaults", value: "resetGhosttySettingsToDefault" },
        ],
        subtitle:
          "Recommended Ghostty settings, Ghostty config file, Ghostty docs, and Ghostty defaults.",
        title: "Ghostty settings actions",
      },
      {
        key: "terminalGhosttyTheme",
        options: GHOSTTY_THEME_SETTING_OPTIONS,
        subtitle: "Choose a bundled Ghostty theme or leave the config unmanaged.",
        title: "Theme",
      },
      {
        key: "workspaceBackgroundColor",
        subtitle: "Color shown behind terminal panes.",
        title: "Terminal Background",
      },
      {
        key: "terminalBackgroundImage",
        subtitle: "Absolute path to an image drawn behind terminal panes.",
        title: "Background Image",
      },
      {
        key: "terminalBackgroundImageOpacity",
        subtitle: "Blend the background image toward the terminal background color.",
        title: "Background Image Opacity",
      },
      {
        key: "terminalBackgroundImageFit",
        options: [
          { label: "Cover", value: "cover" },
          { label: "Contain", value: "contain" },
          { label: "Stretch", value: "stretch" },
          { label: "Natural size", value: "natural" },
        ],
        subtitle: "How the background image is scaled inside each pane.",
        title: "Background Image Fit",
      },
      {
        key: "terminalFontFamily",
        subtitle: "Type a Ghostty font-family name.",
        title: "Font Family",
      },
      {
        key: "terminalFontSize",
        subtitle: "Set terminal text size.",
        title: "Font Size",
      },
      {
        key: "terminalFontWeight",
        subtitle: "Set terminal text weight.",
        title: "Font Weight",
      },
      {
        key: "terminalLineHeight",
        subtitle: "Adjust terminal row height.",
        title: "Line Height",
      },
      {
        key: "terminalLetterSpacing",
        subtitle: "Adjust spacing between glyphs.",
        title: "Letter Spacing",
      },
      {
        key: "terminalPaneHorizontalPaddingPx",
        subtitle: "Add left and right inner padding inside every terminal pane.",
        title: "Horizontal Padding",
      },
      {
        key: "terminalPaneVerticalPaddingPx",
        subtitle: "Add top and bottom inner padding inside every terminal pane.",
        title: "Vertical Padding",
      },
      {
        key: "terminalCursorStyle",
        options: [
          { label: "Line", value: "bar" },
          { label: "Block", value: "block" },
          { label: "Underline", value: "underline" },
        ],
        subtitle: "Choose the cursor shape.",
        title: "Cursor Style",
      },
      {
        key: "terminalCursorStyleBlink",
        subtitle: "Blink the terminal cursor.",
        title: "Cursor blink",
      },
      {
        key: "sessionPersistenceProvider",
        options: SESSION_PERSISTENCE_PROVIDER_OPTIONS,
        subtitle:
          "Choose whether new terminal and agent sessions should use zmx persistence.",
        title: "Session Persistence",
      },
      {
        key: "clickToWakeSleepingSessions",
        subtitle: "Select sleeping pane tabs without waking them until the empty pane is clicked.",
        title: "Click to Wake Sleeping Panes",
      },
      ...(draft.sessionPersistenceProvider === "off"
        ? []
        : [
            {
              key: "showSessionIdInTerminalPanes",
              subtitle: "Show the provider session id in the top-right corner of terminal panes.",
              title: "Show session id in terminal panes",
            },
          ]),
      {
        key: "showNotificationOnTerminalBell",
        subtitle: "Treat terminal bell events as session attention.",
        title: "Show notification on terminal bell",
      },
      {
        key: "promptEditorBackend",
        options: PROMPT_EDITOR_BACKEND_OPTIONS,
        subtitle:
          "Choose which editor Ctrl+G uses when a terminal prompt asks for $EDITOR.",
        title: "Ctrl+G prompt editor",
      },
    ]),
    terminalBehavior: getSettingsSectionSearch(settingsSearchQuery, "Terminal Behavior", [
      {
        key: "terminalScrollbackLimitMb",
        subtitle: "Set scrollback memory per terminal surface.",
        title: "Scrollback limit",
      },
      {
        key: "terminalCopyOnSelect",
        options: GHOSTTY_COPY_ON_SELECT_OPTIONS,
        subtitle: "Copy selected terminal text automatically.",
        title: "Copy on select",
      },
      {
        key: "terminalConfirmCloseSurface",
        options: GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS,
        subtitle: "Confirm before closing terminal surfaces.",
        title: "Confirm close",
      },
      {
        key: "terminalClipboardTrimTrailingSpaces",
        subtitle: "Trim trailing whitespace when copying terminal text.",
        title: "Trim trailing spaces on copy",
      },
      {
        key: "terminalClipboardPasteProtection",
        subtitle: "Ask before pasting text Ghostty considers unsafe.",
        title: "Paste protection",
      },
      {
        key: "terminalPastePreviewableImages",
        subtitle: PASTE_PREVIEWABLE_IMAGES_DESCRIPTION,
        title: "Paste previewable images",
      },
      {
        key: "terminalMouseHideWhileTyping",
        subtitle: "Hide the pointer while typing in the terminal.",
        title: "Hide mouse while typing",
      },
      {
        key: "terminalScrollbar",
        options: GHOSTTY_SCROLLBAR_OPTIONS,
        subtitle: "Control whether Ghostty shows its native scrollback scrollbar.",
        title: "Scrollbar",
      },
    ]),
    terminalScrolling: getSettingsSectionSearch(settingsSearchQuery, "Terminal Scrolling", [
      {
        key: "terminalMouseScrollMultiplierPrecision",
        subtitle: "Trackpads and high-resolution scroll wheels. Ghostty default is 1.",
        title: "Precision scroll multiplier",
      },
      {
        key: "terminalMouseScrollMultiplierDiscrete",
        subtitle: "Traditional notched mouse wheels. Ghostty default is 3.",
        title: "Discrete scroll multiplier",
      },
      {
        key: "terminalScrollToBottomWhenTyping",
        subtitle: "Keep the prompt visible while typing.",
        title: "Scroll to bottom when typing",
      },
    ]),
    terminalDevServers: getSettingsSectionSearch(settingsSearchQuery, "Dev Servers", [
      {
        key: "terminalDevServerDetectionEnabled",
        subtitle: "Detect localhost dev server URLs from terminal output.",
        title: "Detect running servers in terminals",
      },
      {
        key: "terminalDevServerIgnoredPortRules",
        options: [
          { label: "9229", value: "9229" },
          { label: "24678-24680", value: "24678-24680" },
        ],
        subtitle: "Hide detected servers on specific ports or inclusive port ranges.",
        title: "Ignored ports",
      },
    ]),
    beta: getSettingsSectionSearch(settingsSearchQuery, "Experimental", [
      /*
       * CDXC:ExperimentalFeatures 2026-06-28-07:41:
       * Settings search should find the advanced experimental gate by label and
       * by the concrete surfaces it enables so the required inventory stays
       * discoverable without tying Agents Hub to this gate.
       */
      {
        key: "showBetaFeatures",
        subtitle:
          "Show experimental surfaces: OS Integration settings, Browser color scheme, and Keep Awake.",
        title: "Enable Experimental Features",
      },
    ]),
    debugging: getSettingsSectionSearch(settingsSearchQuery, "Debugging", [
      /*
       * CDXC:DiagnosticsSettings 2026-06-06-07:09:
       * Show debug UI controls is the global gate for routine diagnostic disk
       * logging as well as debug-only UI. Scenario controls narrow which
       * routine log area writes while the global gate is on; important
       * warnings, errors, and crashes remain available independently.
       *
       * CDXC:DebuggingSettings 2026-06-15-21:34:
       * The Debugging section owns support and diagnostic toggles at the bottom of Settings, including command copy actions and Copy details, so users can find debug-only context-menu features together.
       *
       * CDXC:DiagnosticsSettings 2026-06-27-22:07:
       * Disk logging needs exact scenario controls. Search should match both
       * the scenario labels and their support-bundle file names so a user can
       * enable only the requested repro log without browsing every Debugging row.
       */
      {
        key: "debuggingMode",
        subtitle: "Show debug-only UI controls and allow enabled routine diagnostic logs.",
        title: "Show debug UI controls",
      },
      {
        key: "diagnosticLogging",
        options: DIAGNOSTIC_LOGGING_SCENARIOS.flatMap((scenario) => [
          { label: scenario.label, value: scenario.id },
          ...scenario.logFiles.map((logFile) => ({ label: logFile, value: logFile })),
        ]),
        subtitle: "Choose routine repro log areas while Show debug UI controls is on. Important warnings, errors, and crashes remain captured when it is off.",
        title: "Diagnostic disk logging scenarios",
      },
      {
        key: "showSessionCommandCopyActions",
        subtitle: "Show Copy resume and Copy attach command in session context menus.",
        title: "Show command copy actions",
      },
      {
        key: "showSessionDetailsCopyAction",
        subtitle: "Show Copy details in session context menus.",
        title: "Show Copy details option",
      },
    ]),
  };

  return settingsSearch;
}

export type SettingsSearchSections = ReturnType<typeof getSettingsSearchSections>;

export function getMainSettingsGroupSearch(
  settingsSearchQuery: string,
  settingsSearch: SettingsSearchSections,
) {
  const mainSettingsGroupSearch = {
    appearance: getGroupedSettingsSectionSearch(settingsSearchQuery, "Appearance", [
      settingsSearch.theming,
      settingsSearch.appIcon,
    ]),
    chat: settingsSearch.chat,
    sidebar: getGroupedSettingsSectionSearch(settingsSearchQuery, "Sidebar", [
      settingsSearch.sidebar,
      settingsSearch.sessionCards,
      settingsSearch.sidebarTags,
    ]),
    terminal: getGroupedSettingsSectionSearch(settingsSearchQuery, "Terminal", [
      settingsSearch.terminal,
      settingsSearch.terminalBehavior,
      settingsSearch.terminalScrolling,
    ]),
    tools: getGroupedSettingsSectionSearch(settingsSearchQuery, "Tools", [
      settingsSearch.browser,
      settingsSearch.editor,
      settingsSearch.terminalDevServers,
    ]),
    statusIndicators: settingsSearch.statusIndicators,
    notifications: getGroupedSettingsSectionSearch(settingsSearchQuery, "Notifications", [
      settingsSearch.sounds,
    ]),
    system: getGroupedSettingsSectionSearch(settingsSearchQuery, "System", [
      settingsSearch.autoSleep,
      settingsSearch.power,
      settingsSearch.storage,
    ]),
    advanced: getGroupedSettingsSectionSearch(settingsSearchQuery, "Advanced", [
      settingsSearch.beta,
      settingsSearch.debugging,
    ]),
  };

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
    { id: "sidebar", searchResult: mainSettingsGroupSearch.sidebar, title: "Sidebar" },
    {
      id: "appearance",
      searchResult: mainSettingsGroupSearch.appearance,
      title: "Appearance",
    },
    { id: "chat", searchResult: mainSettingsGroupSearch.chat, title: "Chat" },
    ...(PET_CONTROLS_VISIBLE
      ? [
          {
            id: "statusIndicators" as const,
            searchResult: mainSettingsGroupSearch.statusIndicators,
            title: "Status Indicators",
          },
        ]
      : []),
    {
      id: "tools",
      searchResult: mainSettingsGroupSearch.tools,
      title: "Tools",
    },
    /*
     * CDXC:SettingsNavigation 2026-06-12-04:13:
     * Ghostty terminal controls belong on the main Settings page so one search query can find app settings and terminal settings together.
     */
    { id: "terminal", searchResult: mainSettingsGroupSearch.terminal, title: "Terminal" },
    {
      id: "system",
      searchResult: mainSettingsGroupSearch.system,
      title: "System",
    },
    {
      id: "notifications",
      searchResult: mainSettingsGroupSearch.notifications,
      title: "Notifications",
    },
    { id: "advanced", searchResult: mainSettingsGroupSearch.advanced, title: "Advanced" },
  ];

  return mainSettingsSectionNavigation;
}

export type MainSettingsSectionNavigation = ReturnType<typeof getMainSettingsSectionNavigation>;
