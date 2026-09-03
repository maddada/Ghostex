import { type SidebarThemeSetting } from '../session-grid-contract-core';
import { type SessionChatTheme } from '../session-chat';
import { GHOSTTY_THEME_OPTIONS } from '../ghostty-theme-options';
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
  type SidebarProjectGroupStyle,
  type SidebarSide,
  type WebLinkOpenTarget,
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
  /**
   * CDXC:Theming 2026-06-15-02:29:
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
   * CDXC:Theming 2026-04-29-09:32
   * Users may already manage Ghostty themes directly in their Ghostty config.
   * The sentinel value lets ghostex leave any existing `theme` line untouched
   * until the user deliberately chooses a bundled theme from this modal.
   */
  { label: 'Use existing Ghostty config', value: '__ghostex_ghostty_theme_unmanaged__' },
  ...GHOSTTY_THEME_OPTIONS.map((theme) => ({ label: theme, value: theme })),
];
