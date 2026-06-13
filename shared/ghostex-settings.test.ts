import { describe, expect, test } from "vitest";
import {
  AUTO_SLEEP_IDLE_MINUTE_OPTIONS,
  APP_SHOTS_HOTKEY_OPTIONS,
  BROWSER_FEEDBACK_TOOL_OPTIONS,
  BROWSER_OPEN_MODE_OPTIONS,
  DEFAULT_ghostex_SETTINGS,
  DEFAULT_EDITOR_COMMAND_OPTIONS,
  DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  DEFAULT_SIDEBAR_DEFAULT_WIDTH_PX,
  GHOSTTY_THEME_SETTING_OPTIONS,
  KEEP_AWAKE_DURATION_OPTIONS,
  MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  MAX_SIDEBAR_DEFAULT_WIDTH_PX,
  MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  MIN_SIDEBAR_DEFAULT_WIDTH_PX,
  applySidebarSettingsPreset,
  getDefaultEditorCommandForSettings,
  getSessionTitleGenerationCommandPreview,
  getSidebarSettingsPresetId,
  normalizeghostexSettings,
  PROMPT_EDITOR_BACKEND_OPTIONS,
  SESSION_PERSISTENCE_PROVIDER_OPTIONS,
  SESSION_STATUS_INDICATOR_SIZE_OPTIONS,
  SIDEBAR_SETTINGS_PRESET_SETTINGS,
  SIDEBAR_SETTINGS_PRESETS,
  SIDEBAR_SIDE_OPTIONS,
  SIDEBAR_THEME_SETTING_OPTIONS,
} from "./ghostex-settings";
import { DEFAULT_PET_ID } from "./pets";

describe("normalizeghostexSettings", () => {
  test("normalizes browser actions to browser panes", () => {
    /**
     * CDXC:BrowserPanes 2026-05-27-07:24
     * Browser actions no longer support Chrome Canary attachment. Legacy stored
     * values must normalize to browser panes so removed settings cannot restore
     * the old external browser route.
     */
    expect(DEFAULT_ghostex_SETTINGS.browserOpenMode).toBe("browser-pane");
    expect(normalizeghostexSettings({})).toMatchObject({
      browserOpenMode: "browser-pane",
    });
    expect(normalizeghostexSettings({ browserOpenMode: "browser-pane" })).toMatchObject({
      browserOpenMode: "browser-pane",
    });
    expect(normalizeghostexSettings({ browserOpenMode: "chrome-canary" })).toMatchObject({
      browserOpenMode: "browser-pane",
    });
    expect(normalizeghostexSettings({ browserOpenMode: "Safari" })).toMatchObject({
      browserOpenMode: "browser-pane",
    });
    expect(BROWSER_OPEN_MODE_OPTIONS).toEqual([{
      label: "Browser Panes",
      value: "browser-pane",
    }]);
  });

  test("defaults browser feedback tools to Agentation and allows React Grab", () => {
    /**
     * CDXC:BrowserFeedbackTools 2026-05-22-09:18:
     * The browser-pane feedback action defaults to Agentation for selector and
     * annotation output from the CEF page, while Settings can switch the same
     * action back to React Grab.
     */
    expect(DEFAULT_ghostex_SETTINGS.browserFeedbackTool).toBe("agentation");
    expect(normalizeghostexSettings({})).toMatchObject({
      browserFeedbackTool: "agentation",
    });
    expect(normalizeghostexSettings({ browserFeedbackTool: "react-grab" })).toMatchObject({
      browserFeedbackTool: "react-grab",
    });
    expect(normalizeghostexSettings({ browserFeedbackTool: "unknown" })).toMatchObject({
      browserFeedbackTool: "agentation",
    });
    expect(BROWSER_FEEDBACK_TOOL_OPTIONS).toEqual([
      { label: "React Grab", value: "react-grab" },
      { label: "Agentation", value: "agentation" },
    ]);
  });

  test("normalizes App Shots settings", () => {
    expect(DEFAULT_ghostex_SETTINGS.appShotsEnabled).toBe(true);
    expect(DEFAULT_ghostex_SETTINGS.appShotsHotkey).toBe("both-command");
    expect(normalizeghostexSettings({})).toMatchObject({
      appShotsEnabled: true,
      appShotsHotkey: "both-command",
    });
    expect(
      normalizeghostexSettings({
        appShotsEnabled: true,
        appShotsHotkey: "double-left-shift",
      }),
    ).toMatchObject({
      appShotsEnabled: true,
      appShotsHotkey: "double-left-shift",
    });
    expect(normalizeghostexSettings({ appShotsHotkey: "cmd+r" })).toMatchObject({
      appShotsHotkey: "both-command",
    });
    expect(APP_SHOTS_HOTKEY_OPTIONS.map((option) => option.value)).toEqual([
      "both-command",
      "double-left-shift",
      "double-left-option",
    ]);
  });

  test("normalizes the default prompt agent setting", () => {
    /**
     * CDXC:PromptAgents 2026-05-28-07:15:
     * Automated prompt launchers share one Settings-selected agent id. Missing
     * values default to Codex, while custom agent ids stay valid because the
     * runtime agent registry resolves whether the selected id is configured.
     */
    expect(DEFAULT_ghostex_SETTINGS.defaultPromptAgentId).toBe("codex");
    expect(normalizeghostexSettings({})).toMatchObject({
      defaultPromptAgentId: "codex",
    });
    expect(normalizeghostexSettings({ defaultPromptAgentId: " claude " })).toMatchObject({
      defaultPromptAgentId: "claude",
    });
    expect(normalizeghostexSettings({ defaultPromptAgentId: "" })).toMatchObject({
      defaultPromptAgentId: "codex",
    });
  });

  test("normalizes the session title generation agent settings", () => {
    /*
    CDXC:GxserverSessionTitle 2026-06-04-08:24:
    Settings exposes a separate first-prompt title generator choice so users can switch Codex, Cursor, Claude, Grok Build, or a custom command without changing the broader default prompt agent used by Git, board, or worktree prompts.
    */
    expect(DEFAULT_ghostex_SETTINGS.sessionTitleGenerationAgent).toBe("codex");
    expect(normalizeghostexSettings({})).toMatchObject({
      customSessionTitleGenerationCommand: "",
      sessionTitleGenerationAgent: "codex",
    });
    expect(normalizeghostexSettings({
      customSessionTitleGenerationCommand: "  title-wrapper --json  ",
      sessionTitleGenerationAgent: "custom",
    })).toMatchObject({
      customSessionTitleGenerationCommand: "title-wrapper --json",
      sessionTitleGenerationAgent: "custom",
    });
    expect(normalizeghostexSettings({ sessionTitleGenerationAgent: "grok" })).toMatchObject({
      sessionTitleGenerationAgent: "grok",
    });
    expect(normalizeghostexSettings({ sessionTitleGenerationAgent: "unknown" })).toMatchObject({
      sessionTitleGenerationAgent: "codex",
    });
  });

  test("previews session title generation commands", () => {
    /*
    CDXC:GxserverSessionTitle 2026-06-04-22:44:
    The Settings and first-time modal title-agent dropdowns must show the exact command template Ghostex sends, including Grok Build's Composer 2.5 model id from the local `grok models` contract.
    */
    expect(getSessionTitleGenerationCommandPreview("grok")).toBe(
      "grok -p --model grok-composer-2.5-fast --output-format plain --no-alt-screen --no-plan --no-subagents --disable-web-search --max-turns 1 '<title generation prompt>'",
    );
    expect(getSessionTitleGenerationCommandPreview("custom", { command: "title-wrapper" })).toBe(
      "title-wrapper <<'PROMPT'\n<title generation prompt>\nPROMPT",
    );
  });

  test("normalizes the sidebar handle reset default width", () => {
    /*
    CDXC:SidebarChrome 2026-06-05-04:40:
    Settings owns the sidebar handle double-click reset width, while app restart continues restoring the separately persisted last sidebar width.
    */
    expect(DEFAULT_ghostex_SETTINGS.sidebarDefaultWidthPx).toBe(DEFAULT_SIDEBAR_DEFAULT_WIDTH_PX);
    expect(normalizeghostexSettings({})).toMatchObject({
      sidebarDefaultWidthPx: DEFAULT_SIDEBAR_DEFAULT_WIDTH_PX,
    });
    expect(normalizeghostexSettings({ sidebarDefaultWidthPx: 312.6 })).toMatchObject({
      sidebarDefaultWidthPx: 313,
    });
    expect(normalizeghostexSettings({ sidebarDefaultWidthPx: 10 })).toMatchObject({
      sidebarDefaultWidthPx: MIN_SIDEBAR_DEFAULT_WIDTH_PX,
    });
    expect(normalizeghostexSettings({ sidebarDefaultWidthPx: 900 })).toMatchObject({
      sidebarDefaultWidthPx: MAX_SIDEBAR_DEFAULT_WIDTH_PX,
    });
  });

  test("normalizes the project session Show less count", () => {
    /*
    CDXC:ProjectSessionLists 2026-06-13-01:06:
    Settings owns how many project sessions remain visible after Show less. Use ten as the current default while continuing to clamp explicit user counts.
    */
    expect(DEFAULT_ghostex_SETTINGS.projectSessionListCollapsedCount).toBe(
      DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
    );
    expect(DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT).toBe(10);
    expect(normalizeghostexSettings({})).toMatchObject({
      projectSessionListCollapsedCount: DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
    });
    expect(normalizeghostexSettings({ projectSessionListCollapsedCount: 6 })).toMatchObject({
      projectSessionListCollapsedCount: 6,
    });
    expect(normalizeghostexSettings({ projectSessionListCollapsedCount: 0 })).toMatchObject({
      projectSessionListCollapsedCount: MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
    });
    expect(normalizeghostexSettings({ projectSessionListCollapsedCount: 999 })).toMatchObject({
      projectSessionListCollapsedCount: MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
    });
  });

  test("keeps untracked project diff lines off unless explicitly enabled", () => {
    expect(DEFAULT_ghostex_SETTINGS.showUntrackedProjectDiffWhenNoTrackedChanges).toBe(false);
    expect(normalizeghostexSettings({})).toMatchObject({
      showUntrackedProjectDiffWhenNoTrackedChanges: false,
    });
    expect(
      normalizeghostexSettings({ showUntrackedProjectDiffWhenNoTrackedChanges: true }),
    ).toMatchObject({
      showUntrackedProjectDiffWhenNoTrackedChanges: true,
    });
  });

  test("hides project-header git file counts by default", () => {
    /**
     * CDXC:ProjectDiffStats 2026-05-15-14:33:
     * When project-header git stats are visible, they should omit the
     * changed-file number by default. The file count stays off in every
     * sidebar preset and is only enabled by an explicit setting change.
     */
    expect(DEFAULT_ghostex_SETTINGS.showProjectEditorDiffFileCount).toBe(false);
    expect(normalizeghostexSettings({})).toMatchObject({
      showProjectEditorDiffFileCount: false,
    });
    expect(normalizeghostexSettings({ showProjectEditorDiffFileCount: true })).toMatchObject({
      showProjectEditorDiffFileCount: true,
    });
  });

  test("defaults sidebar UI settings to the Recommended preset", () => {
    /**
     * CDXC:SidebarSettingsPresets 2026-06-13-01:06:
     * Recommended is the default sidebar preset for normalized settings and the
     * leftmost Settings preset button. It keeps agent icons hover-only while
     * showing detailed sidebar status chrome.
     */
    expect(DEFAULT_ghostex_SETTINGS).toMatchObject(SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended);
    expect(normalizeghostexSettings({})).toMatchObject(
      SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended,
    );
    expect(getSidebarSettingsPresetId(DEFAULT_ghostex_SETTINGS)).toBe("recommended");
    expect(SIDEBAR_SETTINGS_PRESETS.map((preset) => preset.id)).toEqual([
      "recommended",
      "codex",
      "minimal",
      "detailed",
    ]);
    expect(SIDEBAR_SETTINGS_PRESET_SETTINGS.codex.hideBrowserFaviconUntilHover).toBe(false);
    expect(SIDEBAR_SETTINGS_PRESET_SETTINGS.minimal.hideBrowserFaviconUntilHover).toBe(true);
    expect(SIDEBAR_SETTINGS_PRESET_SETTINGS.detailed.hideBrowserFaviconUntilHover).toBe(false);
    expect(
      normalizeghostexSettings({
        hideProjectHeaderDiffStats: false,
        hideBrowserFaviconUntilHover: true,
        hideSessionAgentIconUntilHover: false,
      }),
    ).toMatchObject({
      hideProjectHeaderDiffStats: false,
      hideBrowserFaviconUntilHover: true,
      hideSessionAgentIconUntilHover: false,
    });
  });

  test("detects sidebar presets and custom deviations", () => {
    /**
     * CDXC:SidebarSettingsPresets 2026-05-16-10:11:
     * Preset selection is derived from the controlled setting values. Any
     * controlled value that differs from all presets is Custom rather than a
     * persisted fourth preset state.
     */
    expect(
      getSidebarSettingsPresetId(applySidebarSettingsPreset(DEFAULT_ghostex_SETTINGS, "codex")),
    ).toBe("codex");
    expect(
      getSidebarSettingsPresetId(applySidebarSettingsPreset(DEFAULT_ghostex_SETTINGS, "minimal")),
    ).toBe("minimal");
    expect(
      getSidebarSettingsPresetId(applySidebarSettingsPreset(DEFAULT_ghostex_SETTINGS, "detailed")),
    ).toBe("detailed");
    expect(
      getSidebarSettingsPresetId(
        applySidebarSettingsPreset(DEFAULT_ghostex_SETTINGS, "recommended"),
      ),
    ).toBe("recommended");
    expect(SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideSessionAgentIconUntilHover).toBe(
      true,
    );
    expect(SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideProjectHeaderDiffStats).toBe(false);
    expect(
      getSidebarSettingsPresetId({
        ...DEFAULT_ghostex_SETTINGS,
        showProjectEditorDiffFileCount: true,
      }),
    ).toBeUndefined();
  });

  test("keeps session-card last active timestamps visible unless explicitly hidden", () => {
    /**
     * CDXC:SidebarSessions 2026-05-15-08:57
     * Last Active timestamps on session cards stay visible by default. Users
     * can hide that timestamp without affecting the project header's
     * independent git additions/deletions stats.
     */
    expect(DEFAULT_ghostex_SETTINGS.hideLastActiveTimeOnSessionCards).toBe(false);
    expect(normalizeghostexSettings({})).toMatchObject({
      hideLastActiveTimeOnSessionCards: false,
    });
    expect(normalizeghostexSettings({ hideLastActiveTimeOnSessionCards: true })).toMatchObject({
      hideLastActiveTimeOnSessionCards: true,
    });
  });

  test("hides session command-copy context actions unless explicitly enabled", () => {
    /**
     * CDXC:SidebarContextMenu 2026-06-09-23:17:
     * Copy resume and Copy attach command are advanced context-menu utilities.
     * Missing settings must keep both hidden by default, while an explicit
     * Settings opt-in should persist and reveal both actions.
     */
    expect(DEFAULT_ghostex_SETTINGS.showSessionCommandCopyActions).toBe(false);
    expect(normalizeghostexSettings({})).toMatchObject({
      showSessionCommandCopyActions: false,
    });
    expect(normalizeghostexSettings({ showSessionCommandCopyActions: true })).toMatchObject({
      showSessionCommandCopyActions: true,
    });
  });

  test("hides the session close context-menu option unless explicitly enabled", () => {
    /**
     * CDXC:SidebarContextMenu 2026-06-10-13:58:
     * The single-session Close context-menu item should be absent by default.
     * Users can opt into it separately from the hover close button.
     */
    expect(DEFAULT_ghostex_SETTINGS.showSessionCloseContextMenuAction).toBe(false);
    expect(normalizeghostexSettings({})).toMatchObject({
      showSessionCloseContextMenuAction: false,
    });
    expect(normalizeghostexSettings({ showSessionCloseContextMenuAction: true })).toMatchObject({
      showSessionCloseContextMenuAction: true,
    });
  });

  test("hides the session details copy context-menu option unless explicitly enabled", () => {
    /**
     * CDXC:SidebarContextMenu 2026-06-11-23:08:
     * Copy details writes session metadata to the clipboard. Missing settings
     * must keep the action hidden by default while an explicit opt-in persists.
     */
    expect(DEFAULT_ghostex_SETTINGS.showSessionDetailsCopyAction).toBe(false);
    expect(normalizeghostexSettings({})).toMatchObject({
      showSessionDetailsCopyAction: false,
    });
    expect(normalizeghostexSettings({ showSessionDetailsCopyAction: true })).toMatchObject({
      showSessionDetailsCopyAction: true,
    });
  });

  test("keeps title-bar keep-awake settings English and bounded", () => {
    expect(DEFAULT_ghostex_SETTINGS.keepAwakeDefaultDurationMinutes).toBe(0);
    expect(DEFAULT_ghostex_SETTINGS.hideKeepAwakeTitlebarControl).toBe(false);
    expect(DEFAULT_ghostex_SETTINGS.keepAwakePreventLidSleep).toBe(false);
    expect(KEEP_AWAKE_DURATION_OPTIONS).toEqual([
      { label: "", value: 0 },
      { label: "2 hours", value: 120 },
      { label: "5 hours", value: 300 },
    ]);
    expect(
      normalizeghostexSettings({
        keepAwakeAllowDisplaySleep: true,
        keepAwakeBatteryThresholdPercent: 4,
        keepAwakeDefaultDurationMinutes: 120,
        keepAwakePreventLidSleep: true,
      }),
    ).toMatchObject({
      hideKeepAwakeTitlebarControl: false,
      keepAwakeAllowDisplaySleep: true,
      keepAwakeBatteryThresholdPercent: 10,
      keepAwakeDefaultDurationMinutes: 120,
      keepAwakePreventLidSleep: true,
    });
    expect(normalizeghostexSettings({ hideKeepAwakeTitlebarControl: true })).toMatchObject({
      hideKeepAwakeTitlebarControl: true,
    });
    expect(normalizeghostexSettings({ keepAwakeDefaultDurationMinutes: 999 })).toMatchObject({
      keepAwakeDefaultDurationMinutes: 0,
    });
  });

  test("pins removed macOS pane gap setting to zero", () => {
    /**
     * CDXC:WorkspaceLayout 2026-05-30-07:24:
     * Pane Gap is no longer a macOS app setting. Persisted legacy values should
     * normalize to zero so existing installations lose pane spacing immediately.
     */
    expect(DEFAULT_ghostex_SETTINGS.workspacePaneGap).toBe(0);
    expect(DEFAULT_ghostex_SETTINGS.commandsPanelDefaultHeightPx).toBe(125);
    expect(normalizeghostexSettings({ commandsPanelDefaultHeightPx: 9999 })).toMatchObject({
      commandsPanelDefaultHeightPx: 600,
    });
    expect(normalizeghostexSettings({ commandsPanelDefaultHeightPx: 12 })).toMatchObject({
      commandsPanelDefaultHeightPx: 40,
    });
    expect(normalizeghostexSettings({ workspacePaneGap: 24 })).toMatchObject({
      workspacePaneGap: 0,
    });
    expect(normalizeghostexSettings({})).toMatchObject({
      clickToWakeSleepingSessions: true,
    });
    expect(normalizeghostexSettings({ clickToWakeSleepingSessions: false })).toMatchObject({
      clickToWakeSleepingSessions: false,
    });
  });

  test("normalizes auto sleep settings separately for editors, Git, and agents", () => {
    /**
     * CDXC:AutoSleep 2026-05-28-08:06:
     * Settings must preserve the existing editor/Git sleep defaults while
     * making agent auto-sleep opt-in and bounded to visible idle-duration choices.
     *
     * CDXC:AutoSleep 2026-06-07-00:53:
     * Agent auto-sleep defaults to fifteen idle minutes once enabled, matching
     * editor auto-sleep while keeping the opt-in gate.
     *
     * CDXC:AutoSleep 2026-06-07-00:56:
     * Focused agent sessions are always excluded from auto-sleep, so the old
     * focused-agent override is no longer normalized as a setting.
     */
    expect(AUTO_SLEEP_IDLE_MINUTE_OPTIONS).toEqual([
      { label: "5 minutes", value: 5 },
      { label: "10 minutes", value: 10 },
      { label: "15 minutes", value: 15 },
      { label: "30 minutes", value: 30 },
      { label: "1 hour", value: 60 },
      { label: "2 hours", value: 120 },
      { label: "5 hours", value: 300 },
    ]);
    expect(normalizeghostexSettings({})).toMatchObject({
      autoSleepAgentIdleMinutes: 15,
      autoSleepAgentSessionsEnabled: false,
      autoSleepBrowserIdleMinutes: 30,
      autoSleepBrowserSessionsEnabled: false,
      autoSleepCodeEditorEnabled: true,
      autoSleepCodeEditorIdleMinutes: 15,
      autoSleepFavoriteAgentSessions: false,
      autoSleepGitEditorEnabled: true,
      autoSleepGitEditorIdleMinutes: 15,
      autoSleepProjectEditorEnabled: true,
      autoSleepProjectEditorIdleMinutes: 15,
      autoSleepRequireAgentResumeCommand: true,
    });
    expect(
      normalizeghostexSettings({
        autoSleepAgentIdleMinutes: 999,
        autoSleepAgentSessionsEnabled: true,
        autoSleepBrowserIdleMinutes: 120,
        autoSleepBrowserSessionsEnabled: true,
        autoSleepCodeEditorIdleMinutes: 999,
        autoSleepGitEditorEnabled: false,
        autoSleepGitEditorIdleMinutes: 30,
        autoSleepProjectEditorIdleMinutes: 999,
      }),
    ).toMatchObject({
      autoSleepAgentIdleMinutes: 15,
      autoSleepAgentSessionsEnabled: true,
      autoSleepBrowserIdleMinutes: 120,
      autoSleepBrowserSessionsEnabled: true,
      autoSleepCodeEditorIdleMinutes: 15,
      autoSleepGitEditorEnabled: false,
      autoSleepGitEditorIdleMinutes: 30,
      autoSleepProjectEditorIdleMinutes: 15,
    });
  });

  test("supports built-in and custom default editor commands", () => {
    /**
     * CDXC:AgentsHub 2026-05-12-09:22
     * Agents Hub edit actions should have one normalized editor command
     * setting, with common editor CLIs available without custom text.
     */
    expect(DEFAULT_ghostex_SETTINGS.defaultEditorCommand).toBe("code");
    expect(normalizeghostexSettings({})).toMatchObject({
      customDefaultEditorCommand: "",
      defaultEditorCommand: "code",
    });
    expect(normalizeghostexSettings({ defaultEditorCommand: "code-insiders" })).toMatchObject({
      defaultEditorCommand: "code-insiders",
    });
    expect(normalizeghostexSettings({ defaultEditorCommand: "zed" })).toMatchObject({
      defaultEditorCommand: "zed",
    });
    expect(normalizeghostexSettings({ defaultEditorCommand: "invalid" })).toMatchObject({
      defaultEditorCommand: "code",
    });
    const customSettings = normalizeghostexSettings({
      customDefaultEditorCommand: "  my-editor --reuse-window  ",
      defaultEditorCommand: "other",
    });
    expect(customSettings).toMatchObject({
      customDefaultEditorCommand: "my-editor --reuse-window",
      defaultEditorCommand: "other",
    });
    expect(getDefaultEditorCommandForSettings(customSettings)).toBe("my-editor --reuse-window");
    expect(
      getDefaultEditorCommandForSettings(
        normalizeghostexSettings({ customDefaultEditorCommand: "", defaultEditorCommand: "other" }),
      ),
    ).toBe("code");
    expect(DEFAULT_EDITOR_COMMAND_OPTIONS).toContainEqual({
      label: "VS Code Insiders (code-insiders)",
      value: "code-insiders",
    });
    expect(DEFAULT_EDITOR_COMMAND_OPTIONS).toContainEqual({
      label: "Other",
      value: "other",
    });
  });

  test("defaults bundled code-server panes to Ghostex-owned settings", () => {
    /**
     * CDXC:EditorPanes 2026-06-08-20:12:
     * The bundled macOS code-server runtime should start with Ghostex-owned
     * editor settings so new installs use Dark 2026 unless users explicitly
     * opt into local VS Code settings.
     */
    expect(DEFAULT_ghostex_SETTINGS.codeServerLinkVscodeUserConfig).toBe(false);
    expect(DEFAULT_ghostex_SETTINGS.codeServerUseVscodeInsidersUserConfig).toBe(false);
    expect(normalizeghostexSettings({})).toMatchObject({
      codeServerLinkVscodeUserConfig: false,
      codeServerUseVscodeInsidersUserConfig: false,
    });
    expect(
      normalizeghostexSettings({
        codeServerLinkVscodeUserConfig: true,
        codeServerUseVscodeInsidersUserConfig: true,
      }),
    ).toMatchObject({
      codeServerLinkVscodeUserConfig: true,
      codeServerUseVscodeInsidersUserConfig: true,
    });
  });

  test("keeps sidebar side as a selectable left or right setting", () => {
    /**
     * CDXC:SidebarPlacement 2026-05-06-17:32
     * Sidebar placement is persisted with the rest of Settings so users can
     * choose right-side chrome from the top Sidebar setting or an explicit
     * move-sidebar command, while invalid
     * values still normalize to the left-side default AppKit layout.
     */
    expect(DEFAULT_ghostex_SETTINGS.sidebarSide).toBe("left");
    expect(normalizeghostexSettings({})).toMatchObject({
      sidebarSide: "left",
    });
    expect(normalizeghostexSettings({ sidebarSide: "right" })).toMatchObject({
      sidebarSide: "right",
    });
    expect(normalizeghostexSettings({ sidebarSide: "bottom" })).toMatchObject({
      sidebarSide: "left",
    });
    expect(SIDEBAR_SIDE_OPTIONS).toEqual([
      { label: "Left", value: "left" },
      { label: "Right", value: "right" },
    ]);
  });

  test("defaults sidebar theme to Dark Gray and removes Auto from visible theme options", () => {
    /**
     * CDXC:SidebarTheme 2026-05-08-11:14
     * Auto is retired from user-facing sidebar themes while the full picker is
     * hidden. Defaults and legacy Auto values normalize to Dark Gray so the
     * sidebar starts in the requested palette without a transient auto theme.
     */
    expect(DEFAULT_ghostex_SETTINGS.sidebarTheme).toBe("plain");
    expect(normalizeghostexSettings({})).toMatchObject({
      sidebarTheme: "plain",
    });
    expect(normalizeghostexSettings({ sidebarTheme: "auto" })).toMatchObject({
      sidebarTheme: "plain",
    });
    expect(SIDEBAR_THEME_SETTING_OPTIONS).toEqual([{ label: "Dark Gray", value: "plain" }]);
  });

  test("defaults session status indicators to Medium and keeps four selectable sizes", () => {
    /**
     * CDXC:SessionStatusIndicators 2026-05-07-18:20
     * Medium is the default because it is 50% of the current approved X-Large
     * indicator size. Settings must expose all named scale points so users can
     * return to the larger visual or choose smaller indicators later.
     * CDXC:SessionStatusIndicators 2026-06-13-01:06:
     * Floating and menu bar status badges are visible in the default Recommended
     * preset while remaining independent from the selected indicator size.
     */
    expect(DEFAULT_ghostex_SETTINGS.hideFloatingSessionStatusIndicators).toBe(false);
    expect(DEFAULT_ghostex_SETTINGS.hideMenuBarSessionStatusIndicators).toBe(false);
    expect(DEFAULT_ghostex_SETTINGS.sessionStatusIndicatorSize).toBe("medium");
    expect(normalizeghostexSettings({})).toMatchObject({
      hideFloatingSessionStatusIndicators: false,
      hideMenuBarSessionStatusIndicators: false,
      sessionStatusIndicatorSize: "medium",
    });
    expect(
      normalizeghostexSettings({
        hideFloatingSessionStatusIndicators: false,
        hideMenuBarSessionStatusIndicators: true,
      }),
    ).toMatchObject({
      hideFloatingSessionStatusIndicators: false,
      hideMenuBarSessionStatusIndicators: true,
    });
    expect(normalizeghostexSettings({ sessionStatusIndicatorSize: "x-large" })).toMatchObject({
      sessionStatusIndicatorSize: "x-large",
    });
    expect(normalizeghostexSettings({ sessionStatusIndicatorSize: "giant" })).toMatchObject({
      sessionStatusIndicatorSize: "medium",
    });
    expect(SESSION_STATUS_INDICATOR_SIZE_OPTIONS).toEqual([
      { label: "X-Large", value: "x-large" },
      { label: "Large", value: "large" },
      { label: "Medium", value: "medium" },
      { label: "Small", value: "small" },
    ]);
  });

  test("keeps the pet overlay opt-in and normalizes selected pets", () => {
    expect(DEFAULT_ghostex_SETTINGS.petOverlayEnabled).toBe(false);
    expect(DEFAULT_ghostex_SETTINGS.selectedPetId).toBe(DEFAULT_PET_ID);
    expect(normalizeghostexSettings({})).toMatchObject({
      petOverlayEnabled: false,
      selectedPetId: "boo",
    });
    expect(
      normalizeghostexSettings({ petOverlayEnabled: true, selectedPetId: "dewey" }),
    ).toMatchObject({
      petOverlayEnabled: true,
      selectedPetId: "dewey",
    });
    expect(normalizeghostexSettings({ selectedPetId: "not-a-pet" })).toMatchObject({
      selectedPetId: "boo",
    });
  });

  test("enables macOS attention notifications by default", () => {
    /**
     * CDXC:SessionAttentionNotifications 2026-05-10-16:46
     * Attention banners are a first-install behavior so finished background
     * sessions can surface themselves. Persisted false remains authoritative
     * because users need a Settings switch to disable system notifications.
     */
    expect(DEFAULT_ghostex_SETTINGS.showMacOSAttentionNotifications).toBe(true);
    expect(normalizeghostexSettings({})).toMatchObject({
      showMacOSAttentionNotifications: true,
    });
    expect(normalizeghostexSettings({ showMacOSAttentionNotifications: false })).toMatchObject({
      showMacOSAttentionNotifications: false,
    });
  });

  test("keeps the workspace background color setting", () => {
    expect(DEFAULT_ghostex_SETTINGS.workspaceBackgroundColor).toBe("#000000");
    expect(normalizeghostexSettings({ workspaceBackgroundColor: "#202020" })).toMatchObject({
      workspaceBackgroundColor: "#202020",
    });
    expect(normalizeghostexSettings({ workspaceBackgroundColor: "   " })).toMatchObject({
      workspaceBackgroundColor: DEFAULT_ghostex_SETTINGS.workspaceBackgroundColor,
    });
  });

  test("keeps Ghostty mouse scroll multipliers in the settings slider range", () => {
    /**
     * CDXC:TerminalScrollSettings 2026-04-29-08:56
     * The settings modal exposes Ghostty's precision and discrete scroll
     * multipliers as 0.25-step sliders, so normalization preserves valid
     * tuning values and clamps saved values to the same practical range before
     * writing the shared Ghostty config.
     */
    expect(DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierPrecision).toBe(1);
    expect(DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierDiscrete).toBe(1);
    expect(
      normalizeghostexSettings({
        terminalMouseScrollMultiplierDiscrete: 4,
        terminalMouseScrollMultiplierPrecision: 0.75,
      }),
    ).toMatchObject({
      terminalMouseScrollMultiplierDiscrete: 4,
      terminalMouseScrollMultiplierPrecision: 0.75,
    });
    expect(
      normalizeghostexSettings({
        terminalMouseScrollMultiplierDiscrete: 10001,
        terminalMouseScrollMultiplierPrecision: 0,
      }),
    ).toMatchObject({
      terminalMouseScrollMultiplierDiscrete: 8,
      terminalMouseScrollMultiplierPrecision: 0.25,
    });
  });

  test("defaults session persistence to recommended zmx provider", () => {
    /**
     * CDXC:SessionPersistence 2026-05-05-07:28
     * Legacy tmuxMode=true settings should migrate to the tmux provider, and
     * zmx/zellij must persist as provider choices with the same restart-safe
     * attach/recreate contract.
     *
     * CDXC:SessionPersistence 2026-05-23-00:50:
     * The top-right provider/session overlay preference is normalized with
     * settings defaults, but non-persistent terminal panes still have no
     * provider session label to render.
     *
     * CDXC:SessionPersistence 2026-05-26-13:41:
     * First-run settings should enable zmx by default, label it as recommended
     * in Settings, and hide tmux/zellij from the dropdown while preserving
     * their normalization support for existing settings and sessions.
     *
     * CDXC:SessionPersistence 2026-06-06-05:47:
     * Provider session ids in terminal panes are disabled by default and remain
     * available only when the user explicitly enables the pane overlay setting.
     */
    expect(DEFAULT_ghostex_SETTINGS.sessionPersistenceProvider).toBe("zmx");
    expect(DEFAULT_ghostex_SETTINGS.showSessionIdInTerminalPanes).toBe(false);
    expect(DEFAULT_ghostex_SETTINGS.tmuxMode).toBe(false);
    expect(normalizeghostexSettings({})).toMatchObject({
      sessionPersistenceProvider: "zmx",
      showSessionIdInTerminalPanes: false,
      tmuxMode: false,
    });
    expect(normalizeghostexSettings({ tmuxMode: true })).toMatchObject({
      sessionPersistenceProvider: "tmux",
      tmuxMode: true,
    });
    expect(normalizeghostexSettings({ sessionPersistenceProvider: "zmx" })).toMatchObject({
      sessionPersistenceProvider: "zmx",
      tmuxMode: false,
    });
    expect(normalizeghostexSettings({ sessionPersistenceProvider: "zellij" })).toMatchObject({
      sessionPersistenceProvider: "zellij",
      tmuxMode: false,
    });
    expect(
      normalizeghostexSettings({
        sessionPersistenceProvider: "zmx",
        showSessionIdInTerminalPanes: true,
      }),
    ).toMatchObject({
      sessionPersistenceProvider: "zmx",
      showSessionIdInTerminalPanes: true,
    });
    expect(
      normalizeghostexSettings({
        sessionPersistenceProvider: "zmx",
        showSessionIdInTerminalPanes: false,
      }),
    ).toMatchObject({
      sessionPersistenceProvider: "zmx",
      showSessionIdInTerminalPanes: false,
    });
    expect(SESSION_PERSISTENCE_PROVIDER_OPTIONS).toEqual([
      {
        label: "Off",
        value: "off",
      },
      {
        label: "zmx (recommended)",
        value: "zmx",
      },
    ]);
    expect(SESSION_PERSISTENCE_PROVIDER_OPTIONS).not.toContainEqual({
      label: "tmux",
      value: "tmux",
    });
    expect(SESSION_PERSISTENCE_PROVIDER_OPTIONS).not.toContainEqual({
      label: "zellij",
      value: "zellij",
    });
    expect(normalizeghostexSettings({ sessionPersistenceProvider: "wat" })).toMatchObject({
      sessionPersistenceProvider: "off",
      tmuxMode: false,
    });
  });

  test("keeps common Ghostty terminal behavior settings", () => {
    /**
     * CDXC:TerminalBehaviorSettings 2026-04-29-09:32
     * The settings modal owns common Ghostty behavior controls and writes the
     * documented enum/range values into the shared Ghostty config.
     */
    expect(normalizeghostexSettings({})).toMatchObject({
      terminalClipboardPasteProtection: true,
      terminalClipboardTrimTrailingSpaces: true,
      terminalPastePreviewableImages: true,
      terminalConfirmCloseSurface: "true",
      terminalCopyOnSelect: "false",
      terminalCursorStyleBlink: true,
      terminalMouseHideWhileTyping: false,
      terminalScrollbackLimitMb: 15,
      terminalScrollbar: "system",
    });
    expect(
      normalizeghostexSettings({
        terminalClipboardPasteProtection: false,
        terminalClipboardTrimTrailingSpaces: false,
        terminalPastePreviewableImages: false,
        terminalConfirmCloseSurface: "always",
        terminalCopyOnSelect: "clipboard",
        terminalCursorStyleBlink: false,
        terminalMouseHideWhileTyping: true,
        terminalScrollbackLimitMb: 25,
        terminalScrollbar: "never",
      }),
    ).toMatchObject({
      terminalClipboardPasteProtection: false,
      terminalClipboardTrimTrailingSpaces: false,
      terminalPastePreviewableImages: false,
      terminalConfirmCloseSurface: "always",
      terminalCopyOnSelect: "clipboard",
      terminalCursorStyleBlink: false,
      terminalMouseHideWhileTyping: true,
      terminalScrollbackLimitMb: 25,
      terminalScrollbar: "never",
    });
    expect(
      normalizeghostexSettings({
        terminalConfirmCloseSurface: "ask-me",
        terminalCopyOnSelect: "system",
        terminalScrollbackLimitMb: 1000,
        terminalScrollbar: "always",
      }),
    ).toMatchObject({
      terminalConfirmCloseSurface: "true",
      terminalCopyOnSelect: "false",
      terminalScrollbackLimitMb: 200,
      terminalScrollbar: "system",
    });
  });

  test("defaults Ctrl+G prompt editing to Monaco and supports explicit backend choices", () => {
    /**
     * CDXC:PromptEditorBackend 2026-05-11-14:38
     * Monaco is the default floating editor backend. Explicit gte opt-in keys
     * normalize to gte so selected Ctrl+G prompt-editor behavior is stable.
     *
     * CDXC:PromptEditorBackend 2026-05-22-09:56
     * The terminal prompt editor is named gte for Ghostex Terminal Editor. Tests should pin gte as the persisted backend value and visible Settings option.
     *
     * CDXC:PromptEditorBackend 2026-05-25-11:31:
     * Monaco is the built-in default again. New settings normalize to Monaco
     * unless a backend is explicitly selected, while native SSH runtime handling
     * can still resolve configured Monaco to gte for remote terminals.
     */
    expect(DEFAULT_ghostex_SETTINGS.promptEditorBackend).toBe("monaco");
    expect(normalizeghostexSettings({})).toMatchObject({
      customPromptEditorCommand: "code --wait",
      promptEditorBackend: "monaco",
      richPromptEditingWithGte: false,
      useGteForCtrlGPromptEditing: false,
    });
    expect(normalizeghostexSettings({ richPromptEditingWithGte: false })).toMatchObject({
      promptEditorBackend: "monaco",
      richPromptEditingWithGte: false,
      useGteForCtrlGPromptEditing: false,
    });
    expect(normalizeghostexSettings({ promptEditorBackend: "monaco" })).toMatchObject({
      promptEditorBackend: "monaco",
      richPromptEditingWithGte: false,
      useGteForCtrlGPromptEditing: false,
    });
    expect(normalizeghostexSettings({ richPromptEditingWithGte: true })).toMatchObject({
      promptEditorBackend: "gte",
      richPromptEditingWithGte: true,
      useGteForCtrlGPromptEditing: true,
    });
    expect(normalizeghostexSettings({ useGteForCtrlGPromptEditing: true })).toMatchObject({
      promptEditorBackend: "gte",
    });
    expect(normalizeghostexSettings({ promptEditorBackend: "gte" })).toMatchObject({
      promptEditorBackend: "gte",
    });
    expect(normalizeghostexSettings({ promptEditorBackend: "inherit" })).toMatchObject({
      promptEditorBackend: "inherit",
    });
    expect(
      normalizeghostexSettings({
        customPromptEditorCommand: "  vim -f  ",
        promptEditorBackend: "custom",
      }),
    ).toMatchObject({
      customPromptEditorCommand: "vim -f",
      promptEditorBackend: "custom",
    });
    expect(
      normalizeghostexSettings({
        customPromptEditorCommand: "",
        promptEditorBackend: "custom",
      }),
    ).toMatchObject({
      customPromptEditorCommand: "code --wait",
      promptEditorBackend: "custom",
    });
    expect(normalizeghostexSettings({ promptEditorBackend: "invalid" })).toMatchObject({
      promptEditorBackend: "monaco",
    });
    expect(PROMPT_EDITOR_BACKEND_OPTIONS).toEqual([
      { label: "Inherit from system", value: "inherit" },
      { label: "Monaco floating editor", value: "monaco" },
      { label: "gte terminal editor", value: "gte" },
      { label: "Custom", value: "custom" },
    ]);
  });

  test("keeps Ghostty typography settings in documented practical ranges", () => {
    /**
     * CDXC:TerminalTypographySettings 2026-04-29-09:32
     * CDXC:GhosttyDefaults 2026-05-22-12:29:
     * Typography settings default to the requested Ghostex terminal profile:
     * JetBrains Mono, 13pt, wght=300, no cell-width adjustment, and a 20%
     * cell-height expansion.
     */
    expect(normalizeghostexSettings({})).toMatchObject({
      terminalFontFamily: "JetBrains Mono",
      terminalFontSize: 13,
      terminalFontWeight: 300,
      terminalLetterSpacing: 0,
      terminalLineHeight: 1.2,
    });
    expect(
      normalizeghostexSettings({
        terminalFontFamily: "Hack",
        terminalFontSize: 13.5,
        terminalFontWeight: 650,
        terminalLetterSpacing: 0.6,
        terminalLineHeight: 1.3,
      }),
    ).toMatchObject({
      terminalFontFamily: "Hack",
      terminalFontSize: 13.5,
      terminalFontWeight: 650,
      terminalLetterSpacing: 0.6,
      terminalLineHeight: 1.3,
    });
    expect(
      normalizeghostexSettings({
        terminalFontFamily: "Cross Platform Mono",
        terminalFontSize: 512,
        terminalFontWeight: 10,
        terminalLetterSpacing: 99,
        terminalLineHeight: -1,
      }),
    ).toMatchObject({
      terminalFontFamily: "Consolas",
      terminalFontSize: 32,
      terminalFontWeight: 100,
      terminalLetterSpacing: 8,
      terminalLineHeight: 0.8,
    });
  });

  test("keeps bundled Ghostty theme settings", () => {
    /**
     * CDXC:TerminalThemeSettings 2026-04-29-09:32
     * Ghostty theme names are exact strings from the bundled theme list. The
     * empty value means ghostex should leave the user's Ghostty theme unmanaged.
     *
     * CDXC:GhosttyDefaults 2026-05-22-12:29:
     * New installs default to GitHub Dark rather than leaving the theme
     * unmanaged.
     */
    expect(GHOSTTY_THEME_SETTING_OPTIONS).toContainEqual({
      label: "Use existing Ghostty config",
      value: "__ghostex_ghostty_theme_unmanaged__",
    });
    expect(GHOSTTY_THEME_SETTING_OPTIONS).toContainEqual({
      label: "GitHub Dark",
      value: "GitHub Dark",
    });
    expect(normalizeghostexSettings({})).toMatchObject({
      terminalGhosttyTheme: "GitHub Dark",
    });
    expect(
      normalizeghostexSettings({
        terminalGhosttyTheme: "GitHub Dark Default",
      }),
    ).toMatchObject({
      terminalGhosttyTheme: "GitHub Dark Default",
    });
    expect(normalizeghostexSettings({ terminalGhosttyTheme: "Not A Bundled Theme" })).toMatchObject({
      terminalGhosttyTheme: "",
    });
  });

  test("normalizes SSH-only remote machine settings for sidebar sections", () => {
    /**
     * CDXC:RemoteMachines 2026-06-02-23:47:
     * Remote machine settings require a display name and SSH host because the
     * sidebar renders each saved machine as its own named section and v1 remote
     * connection support is SSH-only.
     *
     * CDXC:RemoteMachines 2026-06-09-18:23:
     * SSH passwords are Keychain credentials, not settings data. Normalization
     * preserves only the saved-password marker and drops any raw password value.
     */
    expect(
      normalizeghostexSettings({
        remoteMachines: [
          {
            id: "remote-main",
            name: " Main machine ",
            sshHost: " 100.77.81.4 ",
            sshIdentityFile: " ~/.ssh/id_ed25519 ",
            sshPassword: "never-store-this",
            sshPasswordSaved: true,
            sshPort: 2222,
            sshUser: " madda ",
          },
          { id: "remote-main", name: "Second", sshHost: "example.local", sshPort: 100000 },
          { id: "remote-blank-name", name: "", sshHost: "example.local" },
          { id: "remote-blank-host", name: "Blank host", sshHost: "" },
        ],
      }).remoteMachines,
    ).toEqual([
      {
        id: "remote-main",
        name: "Main machine",
        sshHost: "100.77.81.4",
        sshIdentityFile: "~/.ssh/id_ed25519",
        sshPasswordSaved: true,
        sshPort: 2222,
        sshUser: "madda",
      },
      {
        id: "remote-2",
        name: "Second",
        sshHost: "example.local",
      },
    ]);
  });

});
