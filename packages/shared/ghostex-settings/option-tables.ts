import { type SidebarThemeSetting, type TerminalEngine } from '../session-grid-contract-core';
import { type SessionChatTheme } from '../session-chat';
import { GHOSTTY_THEME_OPTIONS } from '../ghostty-theme-options';
import {
  type AppShotsHotkey,
  type AutoSleepIdleMinutes,
  type BrowserOpenMode,
  type CommandsPanelSide,
  type DefaultEditorCommand,
  type GhosttyConfirmCloseSurface,
  type GhosttyCopyOnSelect,
  type GhosttyScrollbar,
  type KeepAwakeDurationMinutes,
  type PreferredAgentInterface,
  type PromptEditorBackend,
  type SessionPersistenceProvider,
  type SessionStatusIndicatorSize,
  type SidebarProjectGroupStyle,
  type SidebarSide,
  type SidebarV2Layout,
  type SidebarVersion,
  type WebLinkOpenTarget,
  type WindowsTerminalBackend,
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

export const SIDEBAR_THEME_SETTING_OPTIONS: ReadonlyArray<{
  label: string;
  value: SidebarThemeSetting;
}> = [
  /**
   * CDXC:SidebarTheme 2026-06-15-02:29:
   * The Settings theme dropdown is disabled while themes are coming soon.
   * Keep the persisted value concrete as Dark 2, but use the friendly label
   * Dark Gray so the disabled control matches the current app chrome.
   */
  { label: 'Dark Gray', value: 'dark-2' },
];

export const SESSION_CHAT_THEME_OPTIONS: ReadonlyArray<{
  label: string;
  value: SessionChatTheme;
}> = [
  { label: 'Light', value: 'light' },
  { label: 'Dark', value: 'dark' },
];

export const TERMINAL_ENGINE_SETTING_OPTIONS: ReadonlyArray<{
  label: string;
  value: TerminalEngine;
}> = [{ label: 'Ghostty Native', value: 'ghostty-native' }];

export const WINDOWS_TERMINAL_BACKEND_OPTIONS: ReadonlyArray<{
  label: string;
  value: WindowsTerminalBackend;
}> = [{ label: 'WSL2', value: 'wsl' }];

export const BROWSER_OPEN_MODE_OPTIONS: ReadonlyArray<{
  label: string;
  value: BrowserOpenMode;
}> = [{ label: 'Browser Panes', value: 'browser-pane' }];

export const APP_SHOTS_HOTKEY_OPTIONS: ReadonlyArray<{
  label: string;
  value: AppShotsHotkey;
}> = [
  { label: 'Both Command keys', value: 'both-command' },
  { label: 'Both Shift keys', value: 'both-shift' },
  { label: 'Both Option keys', value: 'both-option' },
  { label: 'Double-tap Left Shift', value: 'double-left-shift' },
  { label: 'Double-tap Left Option', value: 'double-left-option' },
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

export const SESSION_PERSISTENCE_PROVIDER_OPTIONS: ReadonlyArray<{
  label: string;
  value: SessionPersistenceProvider;
}> = [
  /**
   * CDXC:SessionPersistence 2026-05-26-13:41:
   * Settings should recommend zmx and keep tmux/zellij out of the provider dropdown while code still accepts those persisted providers for existing sessions and internal launch paths.
   */
  { label: 'Off', value: 'off' },
  { label: 'zmx (recommended)', value: 'zmx' },
];

export const SIDEBAR_SIDE_OPTIONS: ReadonlyArray<{
  label: string;
  value: SidebarSide;
}> = [
  { label: 'Left', value: 'left' },
  { label: 'Right', value: 'right' },
];

export const COMMANDS_PANEL_SIDE_OPTIONS: ReadonlyArray<{
  label: string;
  value: CommandsPanelSide;
}> = [
  { label: 'Bottom', value: 'bottom' },
  { label: 'Right', value: 'right' },
];

export const SIDEBAR_PROJECT_GROUP_STYLE_OPTIONS: ReadonlyArray<{
  label: string;
  value: SidebarProjectGroupStyle;
}> = [
  { label: 'Quiet rail', value: 'quiet' },
  { label: 'Header rail', value: 'header' },
  { label: 'Branched rail', value: 'branched' },
];

/**
 * CDXC:SidebarV2 2026-07-29:
 * Settings and the sidebar Sort & Filter menu present the same two sidebar
 * versions, so both surfaces read their labels from one list.
 */
export const SIDEBAR_VERSION_OPTIONS: ReadonlyArray<{
  label: string;
  value: SidebarVersion;
}> = [
  { label: 'Standard', value: 'v1' },
  { label: 'Inbox (Beta)', value: 'v2' },
];

export const PREFERRED_AGENT_INTERFACE_OPTIONS: ReadonlyArray<{
  label: string;
  value: PreferredAgentInterface;
}> = [
  { label: 'Terminal', value: 'terminal' },
  { label: 'Chat', value: 'chat' },
];

export const SIDEBAR_V2_LAYOUT_OPTIONS: ReadonlyArray<{
  label: string;
  value: SidebarV2Layout;
}> = [
  { label: 'Flat inbox', value: 'flat' },
  { label: 'Group by project', value: 'byProject' },
];

/**
 * CDXC:SidebarV2Lifecycle 2026-07-29:
 * Auto-settle windows offered in Settings. Presets rather than a free number
 * field because the useful values are few and the wrong one is expensive: a
 * mistyped "0.5" would sweep a whole inbox onto the settled shelf overnight.
 * `SIDEBAR_AUTO_SETTLE_OFF_VALUE` is the select's stand-in for `null`.
 */
export const SIDEBAR_AUTO_SETTLE_OFF_VALUE = 'off';

export const SIDEBAR_AUTO_SETTLE_AFTER_DAYS_OPTIONS: ReadonlyArray<{
  label: string;
  value: string;
}> = [
  { label: 'After 1 day', value: '1' },
  { label: 'After 3 days', value: '3' },
  { label: 'After 7 days', value: '7' },
  { label: 'After 14 days', value: '14' },
  { label: 'After 30 days', value: '30' },
  { label: 'Off', value: SIDEBAR_AUTO_SETTLE_OFF_VALUE },
];

/** Select value for the current setting. An unlisted custom window (hand-edited
    settings file) falls back to Off rather than silently showing a preset it is
    not using. */
export function sidebarAutoSettleAfterDaysSelectValue(value: number | null): string {
  if (value === null) {
    return SIDEBAR_AUTO_SETTLE_OFF_VALUE;
  }
  const candidate = String(value);
  return SIDEBAR_AUTO_SETTLE_AFTER_DAYS_OPTIONS.some((option) => option.value === candidate)
    ? candidate
    : SIDEBAR_AUTO_SETTLE_OFF_VALUE;
}

export function parseSidebarAutoSettleAfterDaysSelectValue(value: string): number | null {
  if (value === SIDEBAR_AUTO_SETTLE_OFF_VALUE) {
    return null;
  }
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

export const SESSION_STATUS_INDICATOR_SIZE_OPTIONS: ReadonlyArray<{
  label: string;
  value: SessionStatusIndicatorSize;
}> = [
  { label: 'X-Large', value: 'x-large' },
  { label: 'Large', value: 'large' },
  { label: 'Medium', value: 'medium' },
  { label: 'Small', value: 'small' },
];

export const KEEP_AWAKE_DURATION_OPTIONS: ReadonlyArray<{
  label: string;
  value: KeepAwakeDurationMinutes;
}> = [
  /**
   * CDXC:TitlebarKeepAwake 2026-05-28-19:28:
   * The keep-awake menu should stay intentionally small: indefinite, two hours,
   * five hours, and the runtime Allow Sleep Now action are the complete user-facing duration set.
   *
   * CDXC:TitlebarKeepAwake 2026-06-15-01:25:
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
   * CDXC:PromptEditorBackend 2026-06-30-00:08:
   * Ctrl+G Settings should be a two-choice dropdown: use the bundled Monaco prompt editor or leave $EDITOR/$VISUAL to the user's machine defaults. gte install/use and custom command controls are intentionally absent.
   */
  { label: 'Monaco editor', value: 'monaco' },
  { label: 'Use default from this machine', value: 'inherit' },
];

export const GHOSTTY_THEME_SETTING_OPTIONS: ReadonlyArray<{
  label: string;
  value: string;
}> = [
  /**
   * CDXC:TerminalThemeSettings 2026-04-29-09:32
   * Users may already manage Ghostty themes directly in their Ghostty config.
   * The sentinel value lets ghostex leave any existing `theme` line untouched
   * until the user deliberately chooses a bundled theme from this modal.
   */
  { label: 'Use existing Ghostty config', value: '__ghostex_ghostty_theme_unmanaged__' },
  ...GHOSTTY_THEME_OPTIONS.map((theme) => ({ label: theme, value: theme })),
];
