import { formatSidebarHotkeyLabel } from '../hotkey-label';
import { type SidebarThemeSetting } from '../session-grid-contract-core';
import { type SessionChatThemeSetting } from '../session-chat';
import { GHOSTTY_THEME_OPTIONS } from '../ghostty-theme-options';
import type { DarkThemePreset, LightThemePreset } from './titlebar-color';
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
  type PreferredAgentInterface,
  type PromptEditorBackend,
  type SidebarSpaceSwitchBehavior,
  type SidebarVisibilityMemory,
  type WebLinkOpenTarget,
  type PanelAnimationSpeed,
  type WindowGlassMode,
  type WindowGlassSource,
  type WindowGlassImagePlacement,
} from './types';

export const WEB_LINK_OPEN_TARGET_OPTIONS: ReadonlyArray<{
  label: string;
  value: WebLinkOpenTarget;
}> = [
  { label: 'Internal Browser', value: 'internal-browser' },
  { label: 'System Default Browser', value: 'system-default-browser' },
];
export const DEFAULT_WEB_LINK_OPEN_TARGET: WebLinkOpenTarget = 'internal-browser';
export const WEB_LINK_OPEN_TARGET_SET = new Set(WEB_LINK_OPEN_TARGET_OPTIONS.map((option) => option.value));

export const CHAT_FILE_OPEN_VIEW_OPTIONS: ReadonlyArray<{
  label: string;
  value: ChatFileOpenView;
}> = [
  { label: 'Docs', value: 'docs' },
  { label: 'Code', value: 'code' },
];
export const DEFAULT_CHAT_FILE_OPEN_VIEW: ChatFileOpenView = 'docs';
export const CHAT_FILE_OPEN_VIEW_SET = new Set(CHAT_FILE_OPEN_VIEW_OPTIONS.map((option) => option.value));

export const SIDEBAR_THEME_SETTING_OPTIONS: ReadonlyArray<{
  label: string;
  value: SidebarThemeSetting;
}> = [
  { label: 'Dark', value: 'dark-2' },
  { label: 'Light', value: 'plain-light' },
  { label: 'System', value: 'system' },
];

/**
 * CDXC:Theming 2026-09-22 DECISION:
 * User: two Theme dropdowns, one for the light theme and one for the dark theme, list preset themes plus
 * Custom; Custom reveals that appearance's background contrast and tint controls.
 */
export const DARK_THEME_PRESET_OPTIONS: ReadonlyArray<{
  label: string;
  value: DarkThemePreset;
}> = [
  { label: 'Graphite', value: 'gray' },
  { label: 'Black', value: 'black' },
  { label: 'Slate', value: 'slate' },
  { label: 'Midnight', value: 'midnight' },
  { label: 'Blue', value: 'blue' },
  { label: 'Indigo', value: 'indigo' },
  { label: 'Teal', value: 'teal' },
  { label: 'Green', value: 'green' },
  { label: 'Forest', value: 'forest' },
  { label: 'Olive', value: 'olive' },
  { label: 'Amber', value: 'amber' },
  { label: 'Orange', value: 'orange' },
  { label: 'Red', value: 'red' },
  { label: 'Rose', value: 'rose' },
  { label: 'Pink', value: 'pink' },
  { label: 'Purple', value: 'purple' },
  { label: 'Custom', value: 'custom' },
];

export const LIGHT_THEME_PRESET_OPTIONS: ReadonlyArray<{
  label: string;
  value: LightThemePreset;
}> = [
  { label: 'Graphite', value: 'gray' },
  { label: 'White', value: 'white' },
  { label: 'Slate', value: 'slate' },
  { label: 'Midnight', value: 'midnight' },
  { label: 'Blue', value: 'blue' },
  { label: 'Indigo', value: 'indigo' },
  { label: 'Teal', value: 'teal' },
  { label: 'Green', value: 'green' },
  { label: 'Forest', value: 'forest' },
  { label: 'Olive', value: 'olive' },
  { label: 'Amber', value: 'amber' },
  { label: 'Orange', value: 'orange' },
  { label: 'Red', value: 'red' },
  { label: 'Rose', value: 'rose' },
  { label: 'Pink', value: 'pink' },
  { label: 'Purple', value: 'purple' },
  { label: 'Custom', value: 'custom' },
];

export const SESSION_CHAT_THEME_OPTIONS: ReadonlyArray<{
  label: string;
  value: SessionChatThemeSetting;
}> = [
  { label: 'Follow app', value: 'app' },
  { label: 'System', value: 'system' },
  { label: 'Light', value: 'light' },
  { label: 'Dark', value: 'dark' },
];

export const APP_SHOTS_HOTKEY_OPTIONS: ReadonlyArray<{
  label: string;
  value: AppShotsHotkey;
}> = [
  { label: `Both ${formatSidebarHotkeyLabel('cmd')} keys`, value: 'both-command' },
  { label: `Both ${formatSidebarHotkeyLabel('shift')} keys`, value: 'both-shift' },
  { label: `Both ${formatSidebarHotkeyLabel('alt')} keys`, value: 'both-option' },
  { label: `Double-tap Left ${formatSidebarHotkeyLabel('shift')}`, value: 'double-left-shift' },
  { label: `Double-tap Left ${formatSidebarHotkeyLabel('alt')}`, value: 'double-left-option' },
];

export const DEFAULT_EDITOR_COMMAND_OPTIONS: ReadonlyArray<{
  label: string;
  value: DefaultEditorCommand;
}> = [
  { label: 'VS Code (code)', value: 'code' },
  { label: 'VS Code Insiders (code-insiders)', value: 'code-insiders' },
  { label: 'Zed (zed)', value: 'zed' },
  { label: 'Zed alternate (zeditor)', value: 'zeditor' },
  { label: 'Cursor (cursor)', value: 'cursor' },
  { label: 'Windsurf (windsurf)', value: 'windsurf' },
  { label: 'VSCodium (codium)', value: 'codium' },
  { label: 'Sublime Text (subl)', value: 'subl' },
  { label: 'Other', value: 'other' },
];

export const COMMANDS_PANEL_AUTO_MINIMIZE_DELAY_OPTIONS: ReadonlyArray<{
  label: string;
  value: number;
}> = [
  { label: '15 seconds', value: 15 },
  { label: '30 seconds', value: 30 },
  { label: '1 minute', value: 60 },
  { label: '2 minutes', value: 120 },
  { label: '5 minutes', value: 300 },
];

export const COMMANDS_PANEL_SIDE_OPTIONS: ReadonlyArray<{
  label: string;
  value: CommandsPanelSide;
}> = [
  { label: 'Bottom', value: 'bottom' },
  { label: 'Right', value: 'right' },
];

export const WINDOW_GLASS_OPTIONS: ReadonlyArray<{
  label: string;
  value: WindowGlassMode;
}> = [
  { label: 'Glass in dark mode', value: 'auto' },
  { label: 'Always glass', value: 'frosted' },
  { label: 'Always opaque', value: 'opaque' },
];

export const WINDOW_GLASS_SOURCE_OPTIONS: ReadonlyArray<{
  label: string;
  value: WindowGlassSource;
}> = [
  { label: 'Desktop and windows', value: 'desktopAndWindows' },
  { label: 'Wallpaper only', value: 'wallpaper' },
  { label: 'Custom image', value: 'customImage' },
  { label: 'Video', value: 'video' },
];

export const WINDOW_GLASS_IMAGE_PLACEMENT_OPTIONS: ReadonlyArray<{
  label: string;
  value: WindowGlassImagePlacement;
}> = [
  { label: 'Moves with the window', value: 'static' },
  { label: 'Stays with the desktop', value: 'desktop' },
];

export const PANEL_ANIMATION_SPEED_OPTIONS: ReadonlyArray<{
  label: string;
  value: PanelAnimationSpeed;
}> = [
  { label: 'Off', value: 'off' },
  { label: 'Slow', value: 'slow' },
  { label: 'Normal', value: 'normal' },
  { label: 'Fast', value: 'fast' },
];

/**
 * CDXC:Spaces 2026-08-28:
 * The Spaces feature switch is a boolean rendered as a combined button, so its
 * two segments are named once here for both the settings row and settings
 * search.
 */
export const SIDEBAR_SPACES_ENABLED_OPTIONS: ReadonlyArray<{
  label: string;
  value: 'off' | 'on';
}> = [
  { label: 'Off', value: 'off' },
  { label: 'On', value: 'on' },
];

/**
 * CDXC:Spaces 2026-09-11 DECISION:
 * User: the default, "Restore the Space's projects", is the first entry so the dropdown opens on it.
 */
export const SIDEBAR_SPACE_SWITCH_BEHAVIOR_OPTIONS: ReadonlyArray<{
  label: string;
  value: SidebarSpaceSwitchBehavior;
}> = [
  { label: "Restore the Space's projects", value: 'restore' },
  { label: "Don't switch projects", value: 'keep' },
];

export const SIDEBAR_VISIBILITY_MEMORY_OPTIONS: ReadonlyArray<{
  label: string;
  value: SidebarVisibilityMemory;
}> = [
  { label: 'Same in every view', value: 'shared' },
  { label: 'Remembered per view', value: 'perView' },
];

export const PREFERRED_AGENT_INTERFACE_OPTIONS: ReadonlyArray<{
  label: string;
  value: PreferredAgentInterface;
}> = [
  { label: 'Terminal', value: 'terminal' },
  { label: 'Chat', value: 'chat' },
];

/**
 * Select value used by the per-agent Default Agent View control for "no
 * override". It is never persisted: inherit is stored as an absent key in
 * `preferredAgentInterfaceOverrides`.
 */
export const PREFERRED_AGENT_INTERFACE_INHERIT_VALUE = 'inherit';

export function getPreferredAgentInterfaceOverrideOptions(
  globalPreferredAgentInterface: PreferredAgentInterface
): ReadonlyArray<{ label: string; value: string }> {
  const inheritedLabel =
    PREFERRED_AGENT_INTERFACE_OPTIONS.find((option) => option.value === globalPreferredAgentInterface)?.label ??
    globalPreferredAgentInterface;
  return [
    { label: `Inherit (${inheritedLabel})`, value: PREFERRED_AGENT_INTERFACE_INHERIT_VALUE },
    ...PREFERRED_AGENT_INTERFACE_OPTIONS,
  ];
}

/**
 * The one place that answers "which view should this agent's new sessions open
 * in": the agent's own override when it has one, otherwise the global Default
 * Agent View. Both the session-create path and the desktop auto-switch path
 * resolve through this so a per-agent choice cannot mean two different things.
 */
export function resolveEffectivePreferredAgentInterface(
  settings: {
    preferredAgentInterface: PreferredAgentInterface;
    preferredAgentInterfaceOverrides: Readonly<Record<string, PreferredAgentInterface>>;
  },
  agentId: string | null | undefined
): PreferredAgentInterface {
  const normalizedAgentId = agentId?.trim();
  if (normalizedAgentId) {
    const override = settings.preferredAgentInterfaceOverrides[normalizedAgentId];
    if (override === 'chat' || override === 'terminal') {
      return override;
    }
  }
  return settings.preferredAgentInterface;
}

export const KEEP_AWAKE_DURATION_OPTIONS: ReadonlyArray<{
  label: string;
  value: KeepAwakeDurationMinutes;
}> = [
  /**
   * CDXC:KeepAwake 2026-05-28-19:28:
   * The keep-awake menu should stay intentionally small: indefinite, two hours,
   * five hours, and the runtime Allow Sleep Now action are the complete user-facing duration set.
   *
   * CDXC:KeepAwake 2026-06-15-01:25:
   * Dropdown settings must never expose an empty selected value. The indefinite keep-awake duration uses explicit friendly copy so Settings and the title-bar menu both render a readable option label.
   */
  { label: 'Until turned off', value: 0 },
  { label: '2 hours', value: 120 },
  { label: '5 hours', value: 300 },
];

export const AUTO_SLEEP_IDLE_MINUTE_OPTIONS: ReadonlyArray<{
  label: string;
  value: AutoSleepIdleMinutes;
}> = [
  { label: 'Off', value: 0 },
  { label: '5 minutes', value: 5 },
  { label: '10 minutes', value: 10 },
  { label: '15 minutes', value: 15 },
  { label: '30 minutes', value: 30 },
  { label: '1 hour', value: 60 },
  { label: '2 hours', value: 120 },
  { label: '5 hours', value: 300 },
];

export const GHOSTTY_COPY_ON_SELECT_OPTIONS: ReadonlyArray<{
  label: string;
  value: GhosttyCopyOnSelect;
}> = [
  { label: 'Off', value: 'false' },
  { label: 'Selection clipboard', value: 'true' },
  { label: 'System and selection clipboard', value: 'clipboard' },
];

export const GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS: ReadonlyArray<{
  label: string;
  value: GhosttyConfirmCloseSurface;
}> = [
  { label: 'Smart confirmation', value: 'true' },
  { label: 'Always confirm', value: 'always' },
  { label: 'Do not confirm', value: 'false' },
];

export const GHOSTTY_SCROLLBAR_OPTIONS: ReadonlyArray<{
  label: string;
  value: GhosttyScrollbar;
}> = [
  { label: 'System', value: 'system' },
  { label: 'Never', value: 'never' },
];

export const PROMPT_EDITOR_BACKEND_OPTIONS: ReadonlyArray<{
  label: string;
  value: PromptEditorBackend;
}> = [
  /**
   * CDXC:PromptEditor 2026-06-30-00:08:
   * Ctrl+G Settings should be a two-choice dropdown: use the bundled prompt editor or leave $EDITOR/$VISUAL to the user's machine defaults. gte install/use and custom command controls are intentionally absent.
   */
  { label: 'Ghostex editor', value: 'monaco' },
  { label: 'Use default from this machine', value: 'inherit' },
];

export const GHOSTTY_THEME_SETTING_OPTIONS: ReadonlyArray<{
  label: string;
  value: string;
}> = [
  /**
   * CDXC:Theming 2026-04-29-09:32
   * Users may already manage Ghostty themes directly in their Ghostty config.
   * The sentinel value lets ghostex leave any existing `theme` line untouched
   * until the user deliberately chooses a bundled theme from this modal.
   */
  { label: 'Use existing Ghostty config', value: '__ghostex_ghostty_theme_unmanaged__' },
  // CDXC:Theming 2026-09-14 DECISION: User: display "GitHub Light" while retaining Ghostty's internal "GitHub Light Default" name.
  ...GHOSTTY_THEME_OPTIONS.map((theme) => ({
    label: theme === 'GitHub Light Default' ? 'GitHub Light' : theme,
    value: theme,
  })),
];

/** Keep a configured custom theme visible even when it is not in Ghostty's bundled list. */
export function getGhosttyThemeSettingOptions(selectedTheme: string) {
  if (!selectedTheme || GHOSTTY_THEME_SETTING_OPTIONS.some((option) => option.value === selectedTheme)) {
    return GHOSTTY_THEME_SETTING_OPTIONS;
  }
  return [{ label: selectedTheme, value: selectedTheme }, ...GHOSTTY_THEME_SETTING_OPTIONS];
}
