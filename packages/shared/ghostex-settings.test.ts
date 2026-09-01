import { describe, expect, test } from 'vitest';
import {
  AUTO_SLEEP_IDLE_MINUTE_OPTIONS,
  APP_SHOTS_HOTKEY_OPTIONS,
  COMMANDS_PANEL_SIDE_OPTIONS,
  DEFAULT_ghostex_SETTINGS,
  DEFAULT_EDITOR_COMMAND_OPTIONS,
  DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  DEFAULT_SIDEBAR_DEFAULT_WIDTH_PX,
  DEFAULT_TERMINAL_PANE_HORIZONTAL_PADDING_PX,
  DEFAULT_TERMINAL_PANE_PADDING_PX,
  GHOSTTY_THEME_SETTING_OPTIONS,
  KEEP_AWAKE_DURATION_OPTIONS,
  MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  MAX_SIDEBAR_DEFAULT_WIDTH_PX,
  MAX_TERMINAL_PANE_PADDING_PX,
  MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  MIN_SIDEBAR_DEFAULT_WIDTH_PX,
  MIN_TERMINAL_PANE_PADDING_PX,
  applySidebarSettingsPreset,
  getDefaultEditorCommandForSettings,
  getSidebarTitlebarBackgroundForDarkness,
  getSessionTitleGenerationCommandPreview,
  getSidebarTitlebarGradientColors,
  getSidebarSettingsPresetId,
  normalizeTerminalDevServerIgnoredPortRuleInput,
  normalizeTerminalDevServerIgnoredPortRules,
  normalizeghostexSettings,
  PROMPT_EDITOR_BACKEND_OPTIONS,
  SIDEBAR_SETTINGS_PRESET_SETTINGS,
  SIDEBAR_SETTINGS_PRESETS,
  SIDEBAR_PROJECT_GROUP_STYLE_OPTIONS,
  SIDEBAR_SIDE_OPTIONS,
  SIDEBAR_THEME_SETTING_OPTIONS,
  WEB_LINK_OPEN_TARGET_OPTIONS,
} from './ghostex-settings';
import { DEFAULT_PET_ID } from './pets';
import {
  DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS,
  getEnabledVisibleSidebarSessionTags,
  getEnabledVisibleSidebarSessionTagFilters,
  getEnabledVisibleSidebarSessionTagSections,
} from './session-tags';

describe('normalizeghostexSettings', () => {
  test('normalizes extension titlebar visibility', () => {
    expect(DEFAULT_ghostex_SETTINGS.extensionsTitlebarButtonHidden).toBe(false);
    expect(normalizeghostexSettings({})).toMatchObject({
      extensionsTitlebarButtonHidden: false,
    });
    expect(
      normalizeghostexSettings({
        extensionsTitlebarButtonHidden: true,
      })
    ).toMatchObject({ extensionsTitlebarButtonHidden: true });
    expect(
      normalizeghostexSettings({
        extensionsTitlebarButtonHidden: 'hidden',
      })
    ).toMatchObject({ extensionsTitlebarButtonHidden: false });
  });

  test('normalizes terminal dev-server discovery settings', () => {
    /*
     * CDXC:TerminalDevServers 2026-06-23-19:22:
     * Terminal dev-server preferences should persist as app settings with detection enabled by default and ignored ports stored as canonical port or range strings.
     *
     * CDXC:WebLinkOpenTarget 2026-08-19:
     * Where a detected URL opens is no longer a dev-server setting; it lives in webLinkOpenTarget with every other web link.
     */
    expect(DEFAULT_ghostex_SETTINGS.terminalDevServerDetectionEnabled).toBe(true);
    expect(normalizeghostexSettings({})).toMatchObject({
      terminalDevServerDetectionEnabled: true,
      terminalDevServerIgnoredPortRules: [],
    });
    expect(
      normalizeghostexSettings({
        terminalDevServerDetectionEnabled: false,
        terminalDevServerIgnoredPortRules: ['3000-3005', '3004-3008', 'abc', '9229', '9230'],
      })
    ).toMatchObject({
      terminalDevServerDetectionEnabled: false,
      terminalDevServerIgnoredPortRules: ['3000-3008', '9229-9230'],
    });
    expect(normalizeTerminalDevServerIgnoredPortRuleInput(' 24678 - 24680 ')).toBe('24678-24680');
    expect(normalizeTerminalDevServerIgnoredPortRuleInput('0')).toBeUndefined();
    expect(normalizeTerminalDevServerIgnoredPortRuleInput('70000')).toBeUndefined();
    expect(normalizeTerminalDevServerIgnoredPortRuleInput('5000-4000')).toBeUndefined();
    expect(normalizeTerminalDevServerIgnoredPortRules(['3000', '3001', '3002-3003'])).toEqual(['3000-3003']);
  });

  test('merges the legacy link-destination settings into one web-link target', () => {
    /*
     * CDXC:WebLinkOpenTarget 2026-08-19:
     * The Browser toggle and the Dev Servers dropdown answered the same question with opposite defaults. They merge into one target, so the toggle has to win migration: it is the switch users actually flipped, while nearly every install carries a dev-server default it never chose.
     */
    expect(DEFAULT_ghostex_SETTINGS.webLinkOpenTarget).toBe('internal-browser');
    expect(WEB_LINK_OPEN_TARGET_OPTIONS).toEqual([
      { label: 'Internal Browser', value: 'internal-browser' },
      { label: 'System Default Browser', value: 'system-default-browser' },
    ]);
    expect(normalizeghostexSettings({})).toMatchObject({
      webLinkOpenTarget: 'internal-browser',
    });
    expect(normalizeghostexSettings({ webLinkOpenTarget: 'system-default-browser' })).toMatchObject({
      webLinkOpenTarget: 'system-default-browser',
    });
    expect(
      normalizeghostexSettings({
        openTerminalLinksInApp: false,
        terminalDevServerOpenTarget: 'internal-browser',
      })
    ).toMatchObject({ webLinkOpenTarget: 'system-default-browser' });
    expect(
      normalizeghostexSettings({
        openTerminalLinksInApp: true,
        terminalDevServerOpenTarget: 'system-default-browser',
      })
    ).toMatchObject({ webLinkOpenTarget: 'internal-browser' });
    expect(normalizeghostexSettings({ terminalDevServerOpenTarget: 'system-default-browser' })).toMatchObject({
      webLinkOpenTarget: 'system-default-browser',
    });
    expect(normalizeghostexSettings({ terminalDevServerDefaultBrowserId: 'edge' })).toMatchObject({
      webLinkOpenTarget: 'system-default-browser',
    });
  });

  test('normalizes global Portless settings', () => {
    /*
     * CDXC:PortlessSettingsDisabled 2026-07-25:
     * Portless settings remain global, but missing or invalid values default
     * off while the app integration is hidden. Explicit stored booleans remain
     * readable so the implementation can return later without a schema reset.
     */
    expect(DEFAULT_ghostex_SETTINGS.portlessEnabled).toBe(false);
    expect(DEFAULT_ghostex_SETTINGS.portlessProtocol).toBe('https');
    expect(normalizeghostexSettings({})).toMatchObject({
      portlessEnabled: false,
      portlessProtocol: 'https',
    });

    const legacySettings = { ...DEFAULT_ghostex_SETTINGS } as Record<string, unknown>;
    delete legacySettings.portlessEnabled;
    delete legacySettings.portlessProtocol;
    expect(normalizeghostexSettings(legacySettings)).toMatchObject({
      portlessEnabled: false,
      portlessProtocol: 'https',
    });

    expect(
      normalizeghostexSettings({
        portlessEnabled: 'false',
        portlessProtocol: 'HTTPS',
      })
    ).toMatchObject({
      portlessEnabled: false,
      portlessProtocol: 'https',
    });
    expect(
      normalizeghostexSettings({
        portlessEnabled: true,
        portlessProtocol: 'https',
      })
    ).toMatchObject({
      portlessEnabled: true,
      portlessProtocol: 'https',
    });
    expect(
      normalizeghostexSettings({
        portlessEnabled: false,
        portlessProtocol: 'http',
      })
    ).toMatchObject({
      portlessEnabled: false,
      portlessProtocol: 'http',
    });

    const normalizedProjectLikeSettings = normalizeghostexSettings({
      portlessEnabledByProject: { projectId: false },
      projectPortlessEnabled: false,
    });
    expect(normalizedProjectLikeSettings).toMatchObject({
      portlessEnabled: false,
      portlessProtocol: 'https',
    });
    expect(
      Object.keys(normalizedProjectLikeSettings).filter(
        (key) => key.toLowerCase().includes('portless') && key.toLowerCase().includes('project')
      )
    ).toEqual([]);
  });

  test('normalizes App Shots settings', () => {
    expect(DEFAULT_ghostex_SETTINGS.appShotsEnabled).toBe(false);
    expect(DEFAULT_ghostex_SETTINGS.appShotsHotkey).toBe('both-command');
    expect(DEFAULT_ghostex_SETTINGS.appShotsMetadataEnabled).toBe(false);
    expect(normalizeghostexSettings({})).toMatchObject({
      appShotsEnabled: false,
      appShotsHotkey: 'both-command',
      appShotsMetadataEnabled: false,
    });
    expect(
      normalizeghostexSettings({
        appShotsEnabled: true,
        appShotsHotkey: 'both-option',
        appShotsMetadataEnabled: true,
      })
    ).toMatchObject({
      appShotsEnabled: true,
      appShotsHotkey: 'both-option',
      appShotsMetadataEnabled: true,
    });
    expect(normalizeghostexSettings({ appShotsHotkey: 'cmd+r' })).toMatchObject({
      appShotsHotkey: 'both-command',
    });
    expect(APP_SHOTS_HOTKEY_OPTIONS.map((option) => option.value)).toEqual([
      'both-command',
      'both-shift',
      'both-option',
      'double-left-shift',
      'double-left-option',
    ]);
  });

  test('defaults experimental features off and normalizes the persisted gate', () => {
    /*
     * CDXC:ExperimentalFeatures 2026-06-28-07:41:
     * Enable Experimental Features should be disabled for new installs and
     * missing settings, with only an explicit boolean true exposing
     * experimental surfaces.
     */
    expect(DEFAULT_ghostex_SETTINGS.showBetaFeatures).toBe(false);
    expect(normalizeghostexSettings({})).toMatchObject({
      showBetaFeatures: false,
    });
    expect(normalizeghostexSettings({ showBetaFeatures: true })).toMatchObject({
      showBetaFeatures: true,
    });
    expect(normalizeghostexSettings({ showBetaFeatures: 'true' })).toMatchObject({
      showBetaFeatures: false,
    });
  });

  test('persists Show Advanced settings density', () => {
    /*
     * CDXC:SettingsAdvanced 2026-06-28-08:01:
     * Show Advanced is a durable Settings preference so advanced rows remain
     * visible after restarting the app until the switch is turned off.
     */
    expect(DEFAULT_ghostex_SETTINGS.showAdvancedSettings).toBe(false);
    expect(normalizeghostexSettings({})).toMatchObject({
      showAdvancedSettings: false,
    });
    expect(normalizeghostexSettings({ showAdvancedSettings: true })).toMatchObject({
      showAdvancedSettings: true,
    });
    expect(normalizeghostexSettings({ showAdvancedSettings: 'true' })).toMatchObject({
      showAdvancedSettings: false,
    });
  });

  test('normalizes the persisted Settings modal location', () => {
    /*
     * CDXC:SettingsNavigation 2026-06-29-17:54:
     * macOS Settings restart restore should persist only safe tab ids and
     * bounded scroll offsets so app relaunch can return to the last closed
     * Settings spot without accepting malformed storage.
     */
    expect(DEFAULT_ghostex_SETTINGS.settingsModalNavigation).toEqual({
      activeTab: 'settings',
      scrollTopByTab: {},
      version: 1,
    });
    expect(
      normalizeghostexSettings({
        settingsModalNavigation: {
          activeTab: 'hotkeys',
          scrollTopByTab: {
            hotkeys: 340.5,
            settings: 120,
          },
          version: 1,
        },
      }).settingsModalNavigation
    ).toEqual({
      activeTab: 'hotkeys',
      scrollTopByTab: {
        settings: 120,
        hotkeys: 340.5,
      },
      version: 1,
    });
    expect(
      normalizeghostexSettings({
        settingsModalNavigation: {
          activeTab: 'missing',
          scrollTopByTab: {
            hotkeys: -20,
            integrations: 2_000_000,
            settings: '500',
          },
        },
      }).settingsModalNavigation
    ).toEqual({
      activeTab: 'settings',
      scrollTopByTab: {
        integrations: 1_000_000,
      },
      version: 1,
    });
  });

  test('normalizes the default prompt agent setting', () => {
    /**
     * CDXC:PromptAgents 2026-05-28-07:15:
     * Automated prompt launchers share one Settings-selected agent id. Missing
     * values default to Codex, while custom agent ids stay valid because the
     * runtime agent registry resolves whether the selected id is configured.
     */
    expect(DEFAULT_ghostex_SETTINGS.defaultPromptAgentId).toBe('codex');
    expect(normalizeghostexSettings({})).toMatchObject({
      defaultPromptAgentId: 'codex',
    });
    expect(normalizeghostexSettings({ defaultPromptAgentId: ' claude ' })).toMatchObject({
      defaultPromptAgentId: 'claude',
    });
    expect(normalizeghostexSettings({ defaultPromptAgentId: '' })).toMatchObject({
      defaultPromptAgentId: 'codex',
    });
  });

  test('normalizes the session title generation agent settings', () => {
    /*
    CDXC:GxserverSessionTitle 2026-06-04-08:24:
    Settings exposes a separate first-prompt title generator choice so users can switch Codex, Cursor, Claude, Grok Build, or a custom command without changing the broader default prompt agent used by Git, board, or worktree prompts.
    */
    expect(DEFAULT_ghostex_SETTINGS.sessionTitleGenerationAgent).toBe('codex');
    expect(normalizeghostexSettings({})).toMatchObject({
      customSessionTitleGenerationCommand: '',
      sessionTitleGenerationAgent: 'codex',
    });
    expect(
      normalizeghostexSettings({
        customSessionTitleGenerationCommand: '  title-wrapper --json  ',
        sessionTitleGenerationAgent: 'custom',
      })
    ).toMatchObject({
      customSessionTitleGenerationCommand: 'title-wrapper --json',
      sessionTitleGenerationAgent: 'custom',
    });
    expect(normalizeghostexSettings({ sessionTitleGenerationAgent: 'grok' })).toMatchObject({
      sessionTitleGenerationAgent: 'grok',
    });
    expect(normalizeghostexSettings({ sessionTitleGenerationAgent: 'unknown' })).toMatchObject({
      sessionTitleGenerationAgent: 'codex',
    });
  });

  test('normalizes the app icon source id', () => {
    /*
     * CDXC:AppIconPicker 2026-06-25-21:50:
     * The app icon source id is a trimmed filename, or "" for the default
     * bundled icon. Missing or non-string values fall back to the default.
     */
    expect(DEFAULT_ghostex_SETTINGS.appIconSourceId).toBe('');
    expect(normalizeghostexSettings({})).toMatchObject({
      appIconSourceId: '',
    });
    expect(normalizeghostexSettings({ appIconSourceId: '  panda.icns  ' })).toMatchObject({
      appIconSourceId: 'panda.icns',
    });
    expect(normalizeghostexSettings({ appIconSourceId: '../panda.png' })).toMatchObject({
      appIconSourceId: '',
    });
    expect(normalizeghostexSettings({ appIconSourceId: 'icons\\panda.png' })).toMatchObject({
      appIconSourceId: '',
    });
    expect(normalizeghostexSettings({ appIconSourceId: 'a'.repeat(256) })).toMatchObject({
      appIconSourceId: '',
    });
    expect(normalizeghostexSettings({ appIconSourceId: 42 as unknown as string })).toMatchObject({
      appIconSourceId: '',
    });
  });

  test('previews session title generation commands', () => {
    /*
    CDXC:GxserverSessionTitle 2026-06-04-22:44:
    The Settings and first-time modal title-agent dropdowns must show the exact command template Ghostex sends, including model ids from each installed CLI's local model catalog.
    */
    expect(getSessionTitleGenerationCommandPreview('codex')).toBe(
      "codex --yolo exec --ephemeral --skip-git-repo-check -m gpt-5.6-luna -c 'model_reasoning_effort=\"low\"' <<'PROMPT'\n<title generation prompt>\nPROMPT"
    );
    expect(getSessionTitleGenerationCommandPreview('cursor')).toBe(
      "cursor-agent --print --yolo --trust --model cursor-grok-4.5-low --output-format text '<title generation prompt>'"
    );
    expect(getSessionTitleGenerationCommandPreview('claude')).toBe(
      "claude --dangerously-skip-permissions -p --model haiku --effort low <<'PROMPT'\n<title generation prompt>\nPROMPT"
    );
    expect(getSessionTitleGenerationCommandPreview('grok')).toBe(
      "grok --model grok-4.5 --reasoning-effort low --output-format plain --no-alt-screen --no-plan --no-subagents --disable-web-search --max-turns 1 --single '<title generation prompt>'"
    );
    expect(getSessionTitleGenerationCommandPreview('custom', { command: 'title-wrapper' })).toBe(
      "title-wrapper <<'PROMPT'\n<title generation prompt>\nPROMPT"
    );
  });

  test('normalizes the sidebar handle reset default width', () => {
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

  test('normalizes the project session Show less count', () => {
    /*
    CDXC:ProjectSessionLists 2026-06-13-01:06:
    Settings owns how many project sessions remain visible after Show less. Use ten as the current default while continuing to clamp explicit user counts.
    */
    expect(DEFAULT_ghostex_SETTINGS.projectSessionListCollapsedCount).toBe(
      DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT
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

  test('normalizes project jump expansion settings', () => {
    /*
    CDXC:ProjectHotkeys 2026-06-15-11:12:
    Project jumps should reveal collapsed Projects rows by default, while the
    narrower Show less side effect remains opt-in and hidden behind that setting
    in the Hotkeys tab.
    */
    expect(normalizeghostexSettings({})).toMatchObject({
      expandCollapsedProjectsOnJump: true,
      showLessForExpandedProjectJumps: false,
    });
    expect(
      normalizeghostexSettings({
        expandCollapsedProjectsOnJump: false,
        showLessForExpandedProjectJumps: true,
      })
    ).toMatchObject({
      expandCollapsedProjectsOnJump: false,
      showLessForExpandedProjectJumps: true,
    });
  });

  test('normalizes sidebar tag filter list presentation', () => {
    /*
    CDXC:SessionTagFilters 2026-06-13-17:50:
    The sidebar tag filter list is configurable presentation chrome. Defaults
    keep every supported tag row recoverable, while persisted custom order,
    hidden-state, and disabled-state normalize without changing tag values.

    CDXC:SessionTagFilters 2026-06-15-18:32:
    First-run defaults reduce visible sidebar tag filters by hiding High
    Priority, Low Priority, Todo, Bug, and Feature while keeping Testing,
    Research, and Design visible.

    CDXC:SessionTagFilters 2026-06-15-22:10:
    Default-hidden tags are fully off: both disabled and hidden. The Settings
    management list still carries those rows so users can turn them back on.
    */
    expect(DEFAULT_ghostex_SETTINGS.sidebarSessionTagListItems).toEqual(DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS);
    expect(normalizeghostexSettings({}).sidebarSessionTagListItems).toEqual(DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS);
    expect(
      DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS.filter(
        (item) => item.type === 'tag' && !item.enabled && !item.visible
      ).map((item) => item.id)
    ).toEqual(['high-priority', 'low-priority', 'todo', 'bug', 'feature']);
    expect(getEnabledVisibleSidebarSessionTags(DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS)).toEqual([
      'favorite',
      'in-progress',
      'testing',
      'blocked',
      'on-hold',
      'done',
      'research',
      'design',
    ]);
    expect(getEnabledVisibleSidebarSessionTagFilters(DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS)).toEqual([
      'favorite',
      'in-progress',
      'testing',
      'blocked',
      'on-hold',
      'done',
      'research',
      'design',
      'untagged',
    ]);
    expect(
      getEnabledVisibleSidebarSessionTagSections(DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS).map((section) => ({
        label: section.label,
        tags: section.options.map((option) => option.value),
      }))
    ).toEqual([
      { label: 'Priority', tags: ['favorite'] },
      { label: 'Progress', tags: ['in-progress', 'testing', 'blocked', 'on-hold', 'done'] },
      { label: 'Type', tags: ['research', 'design'] },
    ]);
    const normalizedCustomTags = normalizeghostexSettings({
      sidebarSessionTagListItems: [
        { enabled: false, id: 'separator-progress-type', type: 'separator', visible: true },
        { enabled: false, id: 'testing', tag: 'testing', type: 'tag', visible: false },
        { enabled: true, id: 'unknown', type: 'tag', visible: true },
        { enabled: true, id: 'testing', tag: 'testing', type: 'tag', visible: true },
      ],
    }).sidebarSessionTagListItems;
    expect(normalizedCustomTags.slice(0, 3)).toEqual([
      { enabled: false, id: 'separator-progress-type', type: 'separator', visible: true },
      { enabled: false, id: 'testing', tag: 'testing', type: 'tag', visible: false },
      DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS[0],
    ]);
    expect(getEnabledVisibleSidebarSessionTagFilters(normalizedCustomTags)).not.toContain('testing');
    expect(normalizedCustomTags.at(-2)).toEqual({
      enabled: true,
      id: 'separator-type-untagged',
      type: 'separator',
      visible: true,
    });
    expect(normalizedCustomTags.at(-1)).toEqual({
      enabled: true,
      id: 'untagged',
      type: 'untagged',
      visible: true,
    });
  });

  test('keeps untracked project diff lines off unless explicitly enabled', () => {
    expect(DEFAULT_ghostex_SETTINGS.showUntrackedProjectDiffWhenNoTrackedChanges).toBe(false);
    expect(normalizeghostexSettings({})).toMatchObject({
      showUntrackedProjectDiffWhenNoTrackedChanges: false,
    });
    expect(normalizeghostexSettings({ showUntrackedProjectDiffWhenNoTrackedChanges: true })).toMatchObject({
      showUntrackedProjectDiffWhenNoTrackedChanges: true,
    });
  });

  test('hides project-header git file counts by default', () => {
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

  test('defaults sidebar UI settings to the Recommended preset', () => {
    /**
     * CDXC:SidebarSettingsPresets 2026-06-13-01:06:
     * Superseded by CDXC:SidebarSettingsPresets 2026-06-30-22:29.
     *
     * CDXC:SidebarSettingsPresets 2026-06-13-15:42:
     * Recommended also hides session-card Last Active timestamps so the default
     * sidebar stays compact without switching to the Minimal preset.
     *
     * CDXC:SessionStatusIndicators 2026-06-15-14:00:
     * Sidebar presets intentionally omitted the legacy macOS floating badge
     * toggle. This preset test should cover preset-owned sidebar chrome and
     * menu bar indicator state without coupling the retired desktop surface.
     *
     * CDXC:SessionStatusIndicators 2026-06-27-20:11:
     * The floating badge is no longer exposed, but presets must still omit the
     * legacy compatibility key so old settings JSON never changes preset state.
     *
     * CDXC:SidebarSettingsPresets 2026-06-23-08:20:
     * All preset buttons should leave session-card close buttons enabled on
     * hover so switching sidebar density or detail mode does not hide the
     * primary per-session close affordance.
     *
     * CDXC:SidebarSettingsPresets 2026-06-30-22:29:
     * Recommended is the default sidebar preset and matches the user's current
     * preset-controlled sidebar configuration, including visible session agent
     * icons.
     */
    expect(DEFAULT_ghostex_SETTINGS).toMatchObject(SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended);
    expect(normalizeghostexSettings({})).toMatchObject(SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended);
    expect(getSidebarSettingsPresetId(DEFAULT_ghostex_SETTINGS)).toBe('recommended');
    expect(SIDEBAR_SETTINGS_PRESETS.map((preset) => preset.id)).toEqual([
      'recommended',
      'codex',
      'minimal',
      'detailed',
    ]);
    expect(SIDEBAR_SETTINGS_PRESET_SETTINGS.codex.hideBrowserFaviconUntilHover).toBe(false);
    expect(SIDEBAR_SETTINGS_PRESET_SETTINGS.minimal.hideBrowserFaviconUntilHover).toBe(true);
    expect(SIDEBAR_SETTINGS_PRESET_SETTINGS.detailed.hideBrowserFaviconUntilHover).toBe(false);
    expect(SIDEBAR_SETTINGS_PRESET_SETTINGS.minimal.showProjectIcons).toBe(false);
    expect(SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.showProjectIcons).toBe(true);
    expect(SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideLastActiveTimeOnSessionCards).toBe(true);
    expect(SIDEBAR_SETTINGS_PRESETS.every((preset) => preset.settings.showCloseButtonOnSessionCards)).toBe(true);
    expect(SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideMenuBarSessionStatusIndicators).toBe(false);
    expect(
      normalizeghostexSettings({
        hideProjectHeaderDiffStats: false,
        hideBrowserFaviconUntilHover: true,
        hideSessionAgentIconUntilHover: false,
        showProjectIcons: false,
        useColoredSessionAgentIcons: true,
      })
    ).toMatchObject({
      hideProjectHeaderDiffStats: false,
      hideBrowserFaviconUntilHover: true,
      hideSessionAgentIconUntilHover: false,
      showProjectIcons: false,
      useColoredSessionAgentIcons: true,
    });
  });

  test('detects sidebar presets and custom deviations', () => {
    /** Preset selection is derived from the controlled setting values. */
    expect(getSidebarSettingsPresetId(applySidebarSettingsPreset(DEFAULT_ghostex_SETTINGS, 'codex'))).toBe('codex');
    expect(getSidebarSettingsPresetId(applySidebarSettingsPreset(DEFAULT_ghostex_SETTINGS, 'minimal'))).toBe('minimal');
    expect(getSidebarSettingsPresetId(applySidebarSettingsPreset(DEFAULT_ghostex_SETTINGS, 'detailed'))).toBe(
      'detailed'
    );
    expect(getSidebarSettingsPresetId(applySidebarSettingsPreset(DEFAULT_ghostex_SETTINGS, 'recommended'))).toBe(
      'recommended'
    );
    expect(SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideSessionAgentIconUntilHover).toBe(false);
    expect(SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideProjectHeaderDiffStats).toBe(false);
    for (const preset of SIDEBAR_SETTINGS_PRESETS) {
      const appliedPreset = applySidebarSettingsPreset(DEFAULT_ghostex_SETTINGS, preset.id);
      expect(appliedPreset.showCloseButtonOnSessionCards).toBe(true);
    }
    expect(
      getSidebarSettingsPresetId({
        ...DEFAULT_ghostex_SETTINGS,
        showProjectEditorDiffFileCount: true,
      })
    ).toBeUndefined();
    /*
     * CDXC:SidebarSessionAgentIcons 2026-06-29-23:58:
     * Colored agent icons are an independent Session Cards preference, not a
     * sidebar density preset. Toggling color mode must not make the current
     * preset become Custom.
     */
    expect(DEFAULT_ghostex_SETTINGS.useColoredSessionAgentIcons).toBe(true);
    expect(normalizeghostexSettings({ useColoredSessionAgentIcons: true })).toMatchObject({
      useColoredSessionAgentIcons: true,
    });
    expect(
      getSidebarSettingsPresetId({
        ...DEFAULT_ghostex_SETTINGS,
        useColoredSessionAgentIcons: true,
      })
    ).toBe('recommended');
  });

  test('hides session-card last active timestamps by default unless explicitly shown', () => {
    /**
     * CDXC:SidebarSessions 2026-06-13-15:42
     * Recommended hides Last Active timestamps on session cards by default.
     * Users can show that timestamp without affecting the project header's
     * independent git additions/deletions stats.
     */
    expect(DEFAULT_ghostex_SETTINGS.hideLastActiveTimeOnSessionCards).toBe(true);
    expect(normalizeghostexSettings({})).toMatchObject({
      hideLastActiveTimeOnSessionCards: true,
    });
    expect(normalizeghostexSettings({ hideLastActiveTimeOnSessionCards: false })).toMatchObject({
      hideLastActiveTimeOnSessionCards: false,
    });
  });

  test('hides session command-copy context actions unless explicitly enabled', () => {
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

  test('hides the session close context-menu option unless explicitly enabled', () => {
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

  test('hides the session details copy context-menu option unless explicitly enabled', () => {
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

  test('keeps title-bar keep-awake settings English and bounded', () => {
    expect(DEFAULT_ghostex_SETTINGS.keepAwakeDefaultDurationMinutes).toBe(0);
    expect(DEFAULT_ghostex_SETTINGS.hideKeepAwakeTitlebarControl).toBe(false);
    expect(DEFAULT_ghostex_SETTINGS.keepAwakePreventLidSleep).toBe(false);
    expect(DEFAULT_ghostex_SETTINGS.keepAwakeWhileWorkingSessions).toBe(false);
    expect(KEEP_AWAKE_DURATION_OPTIONS).toEqual([
      { label: 'Until turned off', value: 0 },
      { label: '2 hours', value: 120 },
      { label: '5 hours', value: 300 },
    ]);
    expect(KEEP_AWAKE_DURATION_OPTIONS.every((option) => option.label.trim().length > 0)).toBe(true);
    expect(
      normalizeghostexSettings({
        keepAwakeAllowDisplaySleep: true,
        keepAwakeBatteryThresholdPercent: 4,
        keepAwakeDefaultDurationMinutes: 120,
        keepAwakePreventLidSleep: true,
        keepAwakeWhileWorkingSessions: true,
      })
    ).toMatchObject({
      hideKeepAwakeTitlebarControl: false,
      keepAwakeAllowDisplaySleep: true,
      keepAwakeBatteryThresholdPercent: 10,
      keepAwakeDefaultDurationMinutes: 120,
      keepAwakePreventLidSleep: true,
      keepAwakeWhileWorkingSessions: true,
    });
    expect(normalizeghostexSettings({ hideKeepAwakeTitlebarControl: true })).toMatchObject({
      hideKeepAwakeTitlebarControl: true,
    });
    expect(normalizeghostexSettings({ keepAwakeDefaultDurationMinutes: 999 })).toMatchObject({
      keepAwakeDefaultDurationMinutes: 0,
    });
  });

  test('pins removed macOS pane gap setting to zero', () => {
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

  test('normalizes auto sleep settings separately for editors, Git, and agents', () => {
    /**
     * CDXC:AutoSleep 2026-05-28-08:06:
     * Settings must normalize editor/Git sleep defaults while making agent
     * auto-sleep opt-in and bounded to visible idle-duration choices.
     *
     * CDXC:AutoSleep 2026-06-15-18:31:
     * Performance defaults should retire heavy editor, Project, Git/Browser,
     * and browser-session surfaces after five idle minutes, with browser-session
     * Auto Sleep enabled by default and agent terminal Auto Sleep still opt-in.
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
      { label: 'Off', value: 0 },
      { label: '5 minutes', value: 5 },
      { label: '10 minutes', value: 10 },
      { label: '15 minutes', value: 15 },
      { label: '30 minutes', value: 30 },
      { label: '1 hour', value: 60 },
      { label: '2 hours', value: 120 },
      { label: '5 hours', value: 300 },
    ]);
    expect(normalizeghostexSettings({})).toMatchObject({
      autoSleepAgentIdleMinutes: 0,
      autoSleepBrowserIdleMinutes: 10,
      autoSleepCodeEditorIdleMinutes: 10,
      autoSleepFavoriteAgentSessions: false,
      autoSleepGitEditorIdleMinutes: 5,
      autoSleepProjectEditorIdleMinutes: 5,
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
      })
    ).toMatchObject({
      autoSleepAgentIdleMinutes: 15,
      autoSleepBrowserIdleMinutes: 120,
      autoSleepCodeEditorIdleMinutes: 10,
      autoSleepGitEditorIdleMinutes: 0,
      autoSleepProjectEditorIdleMinutes: 5,
    });
  });

  test('supports built-in and custom default editor commands', () => {
    /**
     * CDXC:AgentsHub 2026-05-12-09:22
     * Agents Hub edit actions should have one normalized editor command
     * setting, with common editor CLIs available without custom text.
     */
    expect(DEFAULT_ghostex_SETTINGS.defaultEditorCommand).toBe('code');
    expect(normalizeghostexSettings({})).toMatchObject({
      customDefaultEditorCommand: '',
      defaultEditorCommand: 'code',
    });
    expect(normalizeghostexSettings({ defaultEditorCommand: 'code-insiders' })).toMatchObject({
      defaultEditorCommand: 'code-insiders',
    });
    expect(normalizeghostexSettings({ defaultEditorCommand: 'zed' })).toMatchObject({
      defaultEditorCommand: 'zed',
    });
    expect(normalizeghostexSettings({ defaultEditorCommand: 'invalid' })).toMatchObject({
      defaultEditorCommand: 'code',
    });
    const customSettings = normalizeghostexSettings({
      customDefaultEditorCommand: '  my-editor --reuse-window  ',
      defaultEditorCommand: 'other',
    });
    expect(customSettings).toMatchObject({
      customDefaultEditorCommand: 'my-editor --reuse-window',
      defaultEditorCommand: 'other',
    });
    expect(getDefaultEditorCommandForSettings(customSettings)).toBe('my-editor --reuse-window');
    expect(
      getDefaultEditorCommandForSettings(
        normalizeghostexSettings({ customDefaultEditorCommand: '', defaultEditorCommand: 'other' })
      )
    ).toBe('code');
    expect(DEFAULT_EDITOR_COMMAND_OPTIONS).toContainEqual({
      label: 'VS Code Insiders (code-insiders)',
      value: 'code-insiders',
    });
    expect(DEFAULT_EDITOR_COMMAND_OPTIONS).toContainEqual({
      label: 'Other',
      value: 'other',
    });
  });

  test('defaults bundled code-server panes to Ghostex-owned settings', () => {
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
      })
    ).toMatchObject({
      codeServerLinkVscodeUserConfig: true,
      codeServerUseVscodeInsidersUserConfig: true,
    });
  });

  test('keeps sidebar side as a selectable left or right setting', () => {
    /**
     * CDXC:SidebarPlacement 2026-05-06-17:32
     * Sidebar placement is persisted with the rest of Settings so users can
     * choose right-side chrome from the top Sidebar setting or an explicit
     * move-sidebar command, while invalid
     * values still normalize to the left-side default AppKit layout.
     */
    expect(DEFAULT_ghostex_SETTINGS.sidebarSide).toBe('left');
    expect(normalizeghostexSettings({})).toMatchObject({
      sidebarSide: 'left',
    });
    expect(normalizeghostexSettings({ sidebarSide: 'right' })).toMatchObject({
      sidebarSide: 'right',
    });
    expect(normalizeghostexSettings({ sidebarSide: 'bottom' })).toMatchObject({
      sidebarSide: 'left',
    });
    expect(SIDEBAR_SIDE_OPTIONS).toEqual([
      { label: 'Left', value: 'left' },
      { label: 'Right', value: 'right' },
    ]);
  });

  test('keeps command pane side as a selectable bottom or right setting', () => {
    expect(DEFAULT_ghostex_SETTINGS.commandsPanelSide).toBe('bottom');
    expect(normalizeghostexSettings({})).toMatchObject({
      commandsPanelSide: 'bottom',
    });
    expect(normalizeghostexSettings({ commandsPanelSide: 'right' })).toMatchObject({
      commandsPanelSide: 'right',
    });
    expect(normalizeghostexSettings({ commandsPanelSide: 'left' })).toMatchObject({
      commandsPanelSide: 'bottom',
    });
    expect(COMMANDS_PANEL_SIDE_OPTIONS).toEqual([
      { label: 'Bottom', value: 'bottom' },
      { label: 'Right', value: 'right' },
    ]);
  });

  test('normalizes the selectable project group rail style', () => {
    expect(DEFAULT_ghostex_SETTINGS.sidebarProjectGroupStyle).toBe('branched');
    expect(normalizeghostexSettings({})).toMatchObject({
      sidebarProjectGroupStyle: 'branched',
    });
    expect(normalizeghostexSettings({ sidebarProjectGroupStyle: 'quiet' })).toMatchObject({
      sidebarProjectGroupStyle: 'quiet',
    });
    expect(normalizeghostexSettings({ sidebarProjectGroupStyle: 'header' })).toMatchObject({
      sidebarProjectGroupStyle: 'header',
    });
    expect(normalizeghostexSettings({ sidebarProjectGroupStyle: 'branched' })).toMatchObject({
      sidebarProjectGroupStyle: 'branched',
    });
    expect(normalizeghostexSettings({ sidebarProjectGroupStyle: 'boxed' })).toMatchObject({
      sidebarProjectGroupStyle: 'branched',
    });
    expect(SIDEBAR_PROJECT_GROUP_STYLE_OPTIONS).toEqual([
      { label: 'Quiet rail', value: 'quiet' },
      { label: 'Header rail', value: 'header' },
      { label: 'Branched rail', value: 'branched' },
    ]);
  });

  test('defaults sidebar theme to Dark Gray and keeps the theme option disabled', () => {
    /**
     * CDXC:SidebarTheme 2026-06-15-02:29:
     * Theme selection is disabled while themes are coming soon. New installs,
     * legacy Auto, old plain, and temporarily exposed theme values all resolve
     * to Dark 2, whose disabled Settings label is Dark Gray.
     */
    expect(DEFAULT_ghostex_SETTINGS.sidebarTheme).toBe('dark-2');
    expect(normalizeghostexSettings({})).toMatchObject({
      sidebarTheme: 'dark-2',
    });
    expect(normalizeghostexSettings({ sidebarTheme: 'auto' })).toMatchObject({
      sidebarTheme: 'dark-2',
    });
    expect(normalizeghostexSettings({ sidebarTheme: 'plain' })).toMatchObject({
      sidebarTheme: 'dark-2',
    });
    expect(normalizeghostexSettings({ sidebarTheme: 'dark-1' })).toMatchObject({
      sidebarTheme: 'dark-2',
    });
    expect(normalizeghostexSettings({ sidebarTheme: 'plain-light' })).toMatchObject({
      sidebarTheme: 'dark-2',
    });
    expect(SIDEBAR_THEME_SETTING_OPTIONS).toEqual([{ label: 'Dark Gray', value: 'dark-2' }]);
  });

  test('derives custom sidebar and titlebar background from the theming contrast slider', () => {
    /**
     * CDXC:SidebarTitlebarColors 2026-06-15-11:24:
     * Custom chrome colors default to Dark Gray-compatible values and persist
     * only as six-digit hex strings.
     *
     * CDXC:SidebarTitlebarColors 2026-06-15-13:22:
     * Settings no longer expose a foreground picker. Normalize legacy saved
     * foreground values away and derive foreground from the custom background's
     * luminance so light custom chrome stays readable.
     *
     * CDXC:SidebarTitlebarColors 2026-06-15-13:45:
     * The background is no longer a freeform color picker. Settings exposes a
     * contrast slider and stores a computed dark hex color for native
     * protocol compatibility.
     *
     * CDXC:SidebarTitlebarColors 2026-06-15-15:01:
     * The contrast slider is now limited to 85-100 so lower saved values clamp
     * to the lightest allowed dark gray instead of a mid-gray sidebar.
     *
     * CDXC:SidebarTitlebarColors 2026-06-15-15:15:
     * The persisted key still says darkness for compatibility, but Settings
     * presents this control as background contrast.
     *
     * CDXC:SidebarTitlebarColors 2026-06-15-15:28:
     * Background tint is chosen with a web picker and then folded into the
     * computed background hex as a calibrated dark tint. Neutral #808080 must keep
     * existing Dark Gray output unchanged.
     *
     * CDXC:SidebarTitlebarColors 2026-06-16-14:28:
     * The custom chrome default is 95 contrast with white #FFFFFF tint. Missing
     * settings must use that explicit slider default, while valid legacy saved
     * background colors still seed the slider during migration.
     *
     * CDXC:SidebarTitlebarColors 2026-07-22:
     * New app defaults used neutral #808080 at 93 Background Contrast,
     * resolving to #141414.
     *
     * CDXC:SidebarTitlebarColors 2026-08-30:
     * New app defaults use ice #88d7ff at 98 Background Contrast, resolving
     * to #040607.
     *
     * CDXC:SidebarTitlebarColors 2026-06-19-14:20:
     * Preset tint previews stay brighter than the applied chrome. The default
     * applied backgrounds should be very dark, including #0d0005 for red and
     * #0c0e11 for blue, while white and black remain neutral instead of
     * receiving a blue cast.
     */
    expect(DEFAULT_ghostex_SETTINGS.customSidebarTitlebarForegroundColor).toBe('#d8d8d8');
    expect(DEFAULT_ghostex_SETTINGS.customSidebarTitlebarBackgroundTintColor).toBe('#88d7ff');
    expect(DEFAULT_ghostex_SETTINGS.customSidebarTitlebarBackgroundDarknessPercent).toBe(98);
    expect(DEFAULT_ghostex_SETTINGS.customSidebarTitlebarBackgroundColor).toBe('#040607');
    expect(DEFAULT_ghostex_SETTINGS.accentColor).toBe('#86d3f8');
    expect(getSidebarTitlebarBackgroundForDarkness(95, '#884444')).toBe('#0d0005');
    expect(getSidebarTitlebarBackgroundForDarkness(95, '#336699')).toBe('#0c0e11');
    expect(getSidebarTitlebarBackgroundForDarkness(95, '#000000')).toBe('#000000');
    expect(normalizeghostexSettings({})).toMatchObject({
      customSidebarTitlebarForegroundColor: '#d8d8d8',
      customSidebarTitlebarBackgroundTintColor: '#88d7ff',
      customSidebarTitlebarBackgroundDarknessPercent: 98,
      customSidebarTitlebarBackgroundColor: '#040607',
      accentColor: '#86d3f8',
    });
    expect(
      normalizeghostexSettings({
        customSidebarTitlebarBackgroundColor: '#080c0e',
      })
    ).toMatchObject({
      customSidebarTitlebarBackgroundTintColor: '#88d7ff',
      customSidebarTitlebarBackgroundDarknessPercent: 96,
      customSidebarTitlebarBackgroundColor: '#080c0e',
    });
    expect(
      normalizeghostexSettings({
        customSidebarTitlebarForegroundColor: '#ABCDEF',
        customSidebarTitlebarBackgroundTintColor: '#336699',
        customSidebarTitlebarBackgroundDarknessPercent: 85,
        customSidebarTitlebarBackgroundColor: '#123456',
      })
    ).toMatchObject({
      customSidebarTitlebarForegroundColor: '#d8d8d8',
      customSidebarTitlebarBackgroundTintColor: '#336699',
      customSidebarTitlebarBackgroundDarknessPercent: 85,
      customSidebarTitlebarBackgroundColor: '#242a33',
    });
    expect(
      normalizeghostexSettings({
        customSidebarTitlebarForegroundColor: '#ABCDEF',
        customSidebarTitlebarBackgroundTintColor: 'not-a-color',
        customSidebarTitlebarBackgroundDarknessPercent: 20,
      })
    ).toMatchObject({
      customSidebarTitlebarForegroundColor: '#d8d8d8',
      customSidebarTitlebarBackgroundTintColor: '#88d7ff',
      customSidebarTitlebarBackgroundDarknessPercent: 85,
      customSidebarTitlebarBackgroundColor: '#1e2d36',
    });
    expect(
      normalizeghostexSettings({
        customSidebarTitlebarForegroundColor: 'red',
        customSidebarTitlebarBackgroundColor: '#fff',
      })
    ).toMatchObject({
      customSidebarTitlebarForegroundColor: '#d8d8d8',
      customSidebarTitlebarBackgroundDarknessPercent: 98,
      customSidebarTitlebarBackgroundColor: '#040607',
    });
  });

  test('derives fixed-strength sidebar and titlebar gradient stops from custom chrome color', () => {
    /*
     * CDXC:SidebarTitlebarColors 2026-06-19-12:33:
     * Custom sidebar chrome should use a deterministic gradient with the same
     * endpoint distance for neutral and tinted backgrounds. The titlebar starts
     * from the sidebar top stop and moves to the sidebar bottom stop.
     *
     * CDXC:SidebarTitlebarColors 2026-06-19-14:20:
     * White, black, and gray custom chrome must stay neutral. The old cool
     * fallback direction should not add blue to same-channel backgrounds.
     *
     * CDXC:SidebarTitlebarColors 2026-08-30:
     * Invalid gradient input falls back to the ice #88d7ff/98 default (#040607).
     */
    expect(getSidebarTitlebarGradientColors('#0e0e0e')).toEqual({
      sidebarTop: '#0e0e0e',
      sidebarBottom: '#0e0e0e',
      titlebarLeft: '#0e0e0e',
      titlebarRight: '#0e0e0e',
    });
    expect(getSidebarTitlebarGradientColors('#000000')).toEqual({
      sidebarTop: '#000000',
      sidebarBottom: '#000000',
      titlebarLeft: '#000000',
      titlebarRight: '#000000',
    });
    expect(getSidebarTitlebarGradientColors('#0c0e11')).toEqual({
      sidebarTop: '#0a0e13',
      sidebarBottom: '#030d1b',
      titlebarLeft: '#0a0e13',
      titlebarRight: '#030d1b',
    });
    expect(getSidebarTitlebarGradientColors('invalid')).toEqual({
      sidebarTop: '#020609',
      sidebarBottom: '#00080f',
      titlebarLeft: '#020609',
      titlebarRight: '#00080f',
    });
  });

  test('keeps the pet overlay opt-in and normalizes selected pets', () => {
    expect(DEFAULT_ghostex_SETTINGS.petOverlayEnabled).toBe(false);
    expect(DEFAULT_ghostex_SETTINGS.selectedPetId).toBe(DEFAULT_PET_ID);
    expect(normalizeghostexSettings({})).toMatchObject({
      petOverlayEnabled: false,
      selectedPetId: 'boo',
    });
    expect(normalizeghostexSettings({ petOverlayEnabled: true, selectedPetId: 'dewey' })).toMatchObject({
      petOverlayEnabled: true,
      selectedPetId: 'dewey',
    });
    expect(normalizeghostexSettings({ selectedPetId: 'not-a-pet' })).toMatchObject({
      selectedPetId: 'boo',
    });
  });

  test('enables macOS attention notifications by default', () => {
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

  test('keeps terminal bell attention notifications opt-in', () => {
    /*
     * CDXC:TerminalBellAttention 2026-07-01-01:13:
     * A terminal BEL can be normal shell feedback, so missing settings must not
     * turn zsh completion misses into Ghostex attention notifications. Persisted
     * true remains available for users who explicitly want bell-driven attention.
     */
    expect(DEFAULT_ghostex_SETTINGS.showNotificationOnTerminalBell).toBe(false);
    expect(normalizeghostexSettings({})).toMatchObject({
      showNotificationOnTerminalBell: false,
    });
    expect(normalizeghostexSettings({ showNotificationOnTerminalBell: true })).toMatchObject({
      showNotificationOnTerminalBell: true,
    });
  });

  test('keeps the workspace background color setting', () => {
    expect(DEFAULT_ghostex_SETTINGS.workspaceBackgroundColor).toBe('#010101');
    expect(normalizeghostexSettings({ workspaceBackgroundColor: '#202020' })).toMatchObject({
      workspaceBackgroundColor: '#202020',
    });
    expect(normalizeghostexSettings({ workspaceBackgroundColor: '   ' })).toMatchObject({
      workspaceBackgroundColor: DEFAULT_ghostex_SETTINGS.workspaceBackgroundColor,
    });
  });

  test('keeps Ghostty mouse scroll multipliers in the settings slider range', () => {
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
      })
    ).toMatchObject({
      terminalMouseScrollMultiplierDiscrete: 4,
      terminalMouseScrollMultiplierPrecision: 0.75,
    });
    expect(
      normalizeghostexSettings({
        terminalMouseScrollMultiplierDiscrete: 10001,
        terminalMouseScrollMultiplierPrecision: 0,
      })
    ).toMatchObject({
      terminalMouseScrollMultiplierDiscrete: 8,
      terminalMouseScrollMultiplierPrecision: 0.25,
    });
  });

  test('keeps common Ghostty terminal behavior settings', () => {
    /**
     * CDXC:TerminalBehaviorSettings 2026-04-29-09:32
     * The settings modal owns common Ghostty behavior controls and writes the
     * documented enum/range values into the shared Ghostty config.
     */
    expect(normalizeghostexSettings({})).toMatchObject({
      terminalClipboardPasteProtection: true,
      terminalClipboardTrimTrailingSpaces: true,
      terminalPastePreviewableImages: true,
      terminalCopyOnSelect: 'false',
      terminalCursorStyleBlink: true,
      terminalMouseHideWhileTyping: false,
      terminalScrollbackLimitMb: 15,
      terminalScrollbar: 'system',
    });
    expect(
      normalizeghostexSettings({
        terminalClipboardPasteProtection: false,
        terminalClipboardTrimTrailingSpaces: false,
        terminalPastePreviewableImages: false,
        terminalConfirmCloseSurface: 'always',
        terminalCopyOnSelect: 'clipboard',
        terminalCursorStyleBlink: false,
        terminalMouseHideWhileTyping: true,
        terminalScrollbackLimitMb: 25,
        terminalScrollbar: 'never',
      })
    ).toMatchObject({
      terminalClipboardPasteProtection: false,
      terminalClipboardTrimTrailingSpaces: false,
      terminalPastePreviewableImages: false,
      terminalConfirmCloseSurface: 'always',
      terminalCopyOnSelect: 'clipboard',
      terminalCursorStyleBlink: false,
      terminalMouseHideWhileTyping: true,
      terminalScrollbackLimitMb: 25,
      terminalScrollbar: 'never',
    });
    expect(
      normalizeghostexSettings({
        terminalConfirmCloseSurface: 'ask-me',
        terminalCopyOnSelect: 'system',
        terminalScrollbackLimitMb: 1000,
        terminalScrollbar: 'always',
      })
    ).toMatchObject({
      terminalCopyOnSelect: 'false',
      terminalScrollbackLimitMb: 200,
      terminalScrollbar: 'system',
    });
  });

  test('defaults Ctrl+G prompt editing to Monaco and only exposes machine default as the alternative', () => {
    /**
     * CDXC:PromptEditorBackend 2026-05-11-14:38
     * Monaco is the default floating editor backend.
     *
     * CDXC:PromptEditorBackend 2026-05-25-11:31:
     * Monaco is the built-in default again. New settings normalize to Monaco unless a backend is explicitly selected.
     *
     * CDXC:PromptEditorBackend 2026-06-30-00:08:
     * Settings must offer only Monaco and the user's machine default editor. Removed gte/custom persisted choices and legacy gte booleans normalize to inherit so the app stops advertising or installing gte from Ctrl+G Settings.
     */
    expect(DEFAULT_ghostex_SETTINGS.promptEditorBackend).toBe('monaco');
    expect(normalizeghostexSettings({})).toMatchObject({
      promptEditorBackend: 'monaco',
    });
    expect(normalizeghostexSettings({ richPromptEditingWithGte: false })).toMatchObject({
      promptEditorBackend: 'monaco',
    });
    expect(normalizeghostexSettings({ promptEditorBackend: 'monaco' })).toMatchObject({
      promptEditorBackend: 'monaco',
    });
    expect(normalizeghostexSettings({ richPromptEditingWithGte: true })).toMatchObject({
      promptEditorBackend: 'inherit',
    });
    expect(normalizeghostexSettings({ useGteForCtrlGPromptEditing: true })).toMatchObject({
      promptEditorBackend: 'inherit',
    });
    expect(normalizeghostexSettings({ promptEditorBackend: 'gte' })).toMatchObject({
      promptEditorBackend: 'inherit',
    });
    expect(normalizeghostexSettings({ promptEditorBackend: 'inherit' })).toMatchObject({
      promptEditorBackend: 'inherit',
    });
    expect(
      normalizeghostexSettings({
        customPromptEditorCommand: '  vim -f  ',
        promptEditorBackend: 'custom',
      })
    ).toMatchObject({
      promptEditorBackend: 'inherit',
    });
    expect(
      normalizeghostexSettings({
        customPromptEditorCommand: '',
        promptEditorBackend: 'custom',
      })
    ).toMatchObject({
      promptEditorBackend: 'inherit',
    });
    expect(normalizeghostexSettings({ promptEditorBackend: 'invalid' })).toMatchObject({
      promptEditorBackend: 'monaco',
    });
    expect(PROMPT_EDITOR_BACKEND_OPTIONS).toEqual([
      { label: 'Monaco editor', value: 'monaco' },
      { label: 'Use default from this machine', value: 'inherit' },
    ]);
  });

  test('keeps Ghostty typography settings in documented practical ranges', () => {
    /**
     * CDXC:TerminalTypographySettings 2026-04-29-09:32
     * CDXC:GhosttyDefaults 2026-05-22-12:29:
     * Typography settings default to the requested Ghostex terminal profile:
     * JetBrains Mono, 13pt, wght=300, no cell-width adjustment, and a 20%
     * cell-height expansion.
     *
     * CDXC:TerminalPanePadding 2026-06-25-21:27:
     * New terminal pane padding settings default to zero, persist as separate
     * horizontal and vertical integer pixels, and clamp to the Settings slider
     * range so native layout receives bounded content insets.
     */
    expect(normalizeghostexSettings({})).toMatchObject({
      terminalFontFamily: 'JetBrains Mono',
      terminalFontSize: 13,
      terminalFontWeight: 300,
      terminalLetterSpacing: 0,
      terminalLineHeight: 1.2,
      terminalPaneHorizontalPaddingPx: DEFAULT_TERMINAL_PANE_HORIZONTAL_PADDING_PX,
      terminalPaneVerticalPaddingPx: DEFAULT_TERMINAL_PANE_PADDING_PX,
    });
    expect(
      normalizeghostexSettings({
        terminalFontFamily: 'Hack',
        terminalFontSize: 13.5,
        terminalFontWeight: 650,
        terminalLetterSpacing: 0.6,
        terminalLineHeight: 1.3,
        terminalPaneHorizontalPaddingPx: 18,
        terminalPaneVerticalPaddingPx: 9,
      })
    ).toMatchObject({
      terminalFontFamily: 'Hack',
      terminalFontSize: 13.5,
      terminalFontWeight: 650,
      terminalLetterSpacing: 0.6,
      terminalLineHeight: 1.3,
      terminalPaneHorizontalPaddingPx: 18,
      terminalPaneVerticalPaddingPx: 9,
    });
    expect(
      normalizeghostexSettings({
        terminalFontFamily: 'Cross Platform Mono',
        terminalFontSize: 512,
        terminalFontWeight: 10,
        terminalLetterSpacing: 99,
        terminalLineHeight: -1,
        terminalPaneHorizontalPaddingPx: 999,
        terminalPaneVerticalPaddingPx: -12,
      })
    ).toMatchObject({
      terminalFontFamily: 'Consolas',
      terminalFontSize: 32,
      terminalFontWeight: 100,
      terminalLetterSpacing: 8,
      terminalLineHeight: 0.8,
      terminalPaneHorizontalPaddingPx: MAX_TERMINAL_PANE_PADDING_PX,
      terminalPaneVerticalPaddingPx: MIN_TERMINAL_PANE_PADDING_PX,
    });
  });

  test('keeps bundled Ghostty theme settings', () => {
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
      label: 'Use existing Ghostty config',
      value: '__ghostex_ghostty_theme_unmanaged__',
    });
    expect(GHOSTTY_THEME_SETTING_OPTIONS).toContainEqual({
      label: 'GitHub Dark',
      value: 'GitHub Dark',
    });
    expect(normalizeghostexSettings({})).toMatchObject({
      terminalGhosttyTheme: 'GitHub Dark',
    });
    expect(
      normalizeghostexSettings({
        terminalGhosttyTheme: 'GitHub Dark Default',
      })
    ).toMatchObject({
      terminalGhosttyTheme: 'GitHub Dark Default',
    });
    expect(normalizeghostexSettings({ terminalGhosttyTheme: 'Not A Bundled Theme' })).toMatchObject({
      terminalGhosttyTheme: '',
    });
  });

  test('normalizes SSH-only remote machine settings for sidebar sections', () => {
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
            id: 'remote-main',
            name: ' Main machine ',
            sshHost: ' 100.77.81.4 ',
            sshIdentityFile: ' ~/.ssh/id_ed25519 ',
            sshPassword: 'never-store-this',
            sshPasswordSaved: true,
            sshPort: 2222,
            sshUser: ' madda ',
            disabled: true,
          },
          { id: 'remote-main', name: 'Second', sshHost: 'example.local', sshPort: 100000 },
          { id: 'remote-blank-name', name: '', sshHost: 'example.local' },
          { id: 'remote-blank-host', name: 'Blank host', sshHost: '' },
        ],
      }).remoteMachines
    ).toEqual([
      {
        id: 'remote-main',
        name: 'Main machine',
        sshHost: '100.77.81.4',
        sshIdentityFile: '~/.ssh/id_ed25519',
        sshPasswordSaved: true,
        sshPort: 2222,
        sshUser: 'madda',
        disabled: true,
      },
      {
        id: 'remote-2',
        name: 'Second',
        sshHost: 'example.local',
      },
    ]);
  });
});
