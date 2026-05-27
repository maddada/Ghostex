import {
  DEFAULT_AGENT_MANAGER_ZOOM_PERCENT,
  type SidebarThemeSetting,
  type TerminalEngine,
} from "./session-grid-contract-core";
import {
  clampAgentManagerZoomPercent,
  clampSidebarThemeSetting,
  normalizeTerminalEngine,
} from "./session-grid-contract-session";
import {
  clampCompletionSoundSetting,
  DEFAULT_COMPLETION_SOUND,
  type CompletionSoundSetting,
} from "./completion-sound";
import {
  getGhosttyFontFamilyForPreset,
  getTerminalFontFamilyForPreset,
  normalizeTerminalFontPreset,
} from "./terminal-font-preset";
import {
  DEFAULT_ghostex_HOTKEYS,
  normalizeghostexHotkeySettings,
  type ghostexHotkeySettings,
} from "./ghostex-hotkeys";
import { GHOSTTY_THEME_OPTIONS } from "./ghostty-theme-options";
import {
  DEFAULT_WORKSPACE_OPEN_TARGET_AVAILABILITY,
  normalizeCustomWorkspaceOpenTargets,
  normalizeWorkspaceOpenTargetAvailability,
  normalizeWorkspaceOpenTargetHiddenIds,
  type CustomWorkspaceOpenTarget,
  type WorkspaceOpenTargetAvailability,
} from "./workspace-open-targets";
import { DEFAULT_PET_ID, normalizePetId, type PetId } from "./pets";

export type GhosttyConfirmCloseSurface = "false" | "true" | "always";
export type GhosttyCopyOnSelect = "false" | "true" | "clipboard";
export type GhosttyScrollbar = "system" | "never";
export type TerminalCursorStyle = "bar" | "block" | "underline";
export type BrowserOpenMode = "chrome-canary" | "browser-pane";
export type BrowserFeedbackTool = "react-grab" | "agentation";
export type DefaultEditorCommand =
  | "code"
  | "code-insiders"
  | "zed"
  | "zeditor"
  | "cursor"
  | "windsurf"
  | "codium"
  | "subl"
  | "other";
export type SessionPersistenceProvider = "off" | "tmux" | "zmx" | "zellij";
export type SessionStatusIndicatorSize = "small" | "medium" | "large" | "x-large";
export type SidebarSide = "left" | "right";
export type SidebarSettingsPresetId = "codex" | "minimal" | "detailed";
export type PromptEditorBackend = "inherit" | "monaco" | "gte" | "custom";
export type ZedOverlayTargetApp = "zed" | "zed-preview" | "vscode" | "vscode-insiders";
const MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER = 0.25;
const MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER = 8;
const MIN_GHOSTTY_SCROLLBACK_LIMIT_MB = 1;
const MAX_GHOSTTY_SCROLLBACK_LIMIT_MB = 200;

/**
 * CDXC:Branding 2026-05-12-07:35
 * Public app copy uses Ghostex, and public terminal commands use `ghostex`
 * with `gx` as the short alias. The codebase can keep ghostex in type names,
 * storage/protocol keys, file paths, and implementation identifiers.
 *
 * CDXC:Branding 2026-05-26-15:11
 * New installs should expose `gx` instead of the older `gtx` command, and setup
 * should not claim `gx` when another tool already owns that binary name.
 *
 * CDXC:Branding 2026-05-15-11:54
 * The project rename now applies to source-facing identifiers, docs, scripts,
 * config, release metadata, and native project paths. Preserve each existing
 * casing style while using Ghostex, ghostex, or GHOSTEX consistently.
 */
export type ghostexSettings = {
  actionCompletionSound: CompletionSoundSetting;
  /**
   * CDXC:ChatPromotion 2026-05-26-18:30:
   * When enabled, chat workspaces auto-promote to proper projects when the
   * terminal navigates into a git repository. Disabling keeps chats as
   * standalone workspaces regardless of terminal CWD.
   */
  autoPromoteChatToProject: boolean;
  /**
   * CDXC:SidebarAgents 2026-05-19-10:05:
   * When enabled, built-in and custom agent launches inherit Accept All mode and
   * append each CLI's permission-bypass flag at runtime unless a specific agent
   * overrides the behavior in its own configuration.
   */
  agentAcceptAllEnabled: boolean;
  agentManagerZoomPercent: number;
  browserFeedbackTool: BrowserFeedbackTool;
  browserOpenMode: BrowserOpenMode;
  codeServerLinkVscodeUserConfig: boolean;
  codeServerUseVscodeInsidersUserConfig: boolean;
  customDefaultEditorCommand: string;
  defaultEditorCommand: DefaultEditorCommand;
  hideProjectHeaderDiffStats: boolean;
  showProjectEditorDiffFileCount: boolean;
  completionBellEnabled: boolean;
  completionSound: CompletionSoundSetting;
  createSessionOnSidebarDoubleClick: boolean;
  debuggingMode: boolean;
  renameSessionOnDoubleClick: boolean;
  hideSessionAgentIconUntilHover: boolean;
  showCloseButtonOnSessionCards: boolean;
  showHotkeysOnSessionCards: boolean;
  hideLastActiveTimeOnSessionCards: boolean;
  showMacOSAttentionNotifications: boolean;
  hideFloatingSessionStatusIndicators: boolean;
  hideMenuBarSessionStatusIndicators: boolean;
  petOverlayEnabled: boolean;
  selectedPetId: PetId;
  sessionStatusIndicatorSize: SessionStatusIndicatorSize;
  sessionPersistenceProvider: SessionPersistenceProvider;
  showSessionIdInTerminalPanes: boolean;
  sidebarSide: SidebarSide;
  sidebarTheme: SidebarThemeSetting;
  terminalCursorStyle: TerminalCursorStyle;
  terminalCursorStyleBlink: boolean;
  terminalEngine: TerminalEngine;
  terminalFontFamily: string;
  terminalFontSize: number;
  terminalFontWeight: number;
  terminalGhosttyTheme: string;
  terminalLetterSpacing: number;
  terminalLineHeight: number;
  terminalMouseScrollMultiplierDiscrete: number;
  terminalMouseScrollMultiplierPrecision: number;
  tmuxMode: boolean;
  terminalScrollToBottomWhenTyping: boolean;
  terminalScrollbackLimitMb: number;
  terminalCopyOnSelect: GhosttyCopyOnSelect;
  terminalConfirmCloseSurface: GhosttyConfirmCloseSurface;
  terminalClipboardTrimTrailingSpaces: boolean;
  terminalClipboardPasteProtection: boolean;
  terminalMouseHideWhileTyping: boolean;
  terminalScrollbar: GhosttyScrollbar;
  promptEditorBackend: PromptEditorBackend;
  customPromptEditorCommand: string;
  richPromptEditingWithGte: boolean;
  useGteForCtrlGPromptEditing: boolean;
  hotkeys: ghostexHotkeySettings;
  workspaceActivePaneBorderColor: string;
  workspaceBackgroundColor: string;
  customWorkspaceOpenTargets: CustomWorkspaceOpenTarget[];
  workspaceOpenTargetAvailability: WorkspaceOpenTargetAvailability;
  workspaceOpenTargetHiddenIds: string[];
  workspacePaneGap: number;
  /**
   * CDXC:IDEAttachment 2026-05-06-12:49
   * Keep the persisted Zed-named key for compatibility, but treat it as the
   * default-on setting that lets workspace activation sync to the attached IDE.
   */
  syncOpenProjectWithZed: boolean;
  zedOverlayEnabled: boolean;
  zedOverlayHideTitlebarButton: boolean;
  zedOverlayTargetApp: ZedOverlayTargetApp;
};

export const SIDEBAR_SETTINGS_PRESET_KEYS = [
  "hideSessionAgentIconUntilHover",
  "showCloseButtonOnSessionCards",
  "hideLastActiveTimeOnSessionCards",
  "hideProjectHeaderDiffStats",
  "showProjectEditorDiffFileCount",
  "hideFloatingSessionStatusIndicators",
  "hideMenuBarSessionStatusIndicators",
] as const satisfies ReadonlyArray<keyof ghostexSettings>;

export type SidebarSettingsPresetKey = (typeof SIDEBAR_SETTINGS_PRESET_KEYS)[number];
export type SidebarSettingsPresetSettings = Pick<ghostexSettings, SidebarSettingsPresetKey>;

/**
 * CDXC:SidebarSettingsPresets 2026-05-16-10:11:
 * The Settings top row exposes Codex, Minimal, and Detailed sidebar UI presets as toggle buttons.
 * Preset state is derived from the controlled sidebar settings instead of persisted separately, so manual deviations show Custom without adding another source of truth.
 */
export const SIDEBAR_SETTINGS_PRESET_SETTINGS = {
  codex: {
    hideSessionAgentIconUntilHover: true,
    showCloseButtonOnSessionCards: true,
    hideLastActiveTimeOnSessionCards: false,
    hideProjectHeaderDiffStats: true,
    showProjectEditorDiffFileCount: false,
    hideFloatingSessionStatusIndicators: true,
    hideMenuBarSessionStatusIndicators: true,
  },
  minimal: {
    hideSessionAgentIconUntilHover: true,
    showCloseButtonOnSessionCards: true,
    hideLastActiveTimeOnSessionCards: true,
    hideProjectHeaderDiffStats: true,
    showProjectEditorDiffFileCount: false,
    hideFloatingSessionStatusIndicators: true,
    hideMenuBarSessionStatusIndicators: true,
  },
  detailed: {
    hideSessionAgentIconUntilHover: false,
    showCloseButtonOnSessionCards: false,
    hideLastActiveTimeOnSessionCards: false,
    hideProjectHeaderDiffStats: false,
    showProjectEditorDiffFileCount: false,
    hideFloatingSessionStatusIndicators: false,
    hideMenuBarSessionStatusIndicators: false,
  },
} as const satisfies Record<SidebarSettingsPresetId, SidebarSettingsPresetSettings>;

export const SIDEBAR_SETTINGS_PRESETS: ReadonlyArray<{
  id: SidebarSettingsPresetId;
  label: string;
  settings: SidebarSettingsPresetSettings;
}> = [
  { id: "codex", label: "Codex", settings: SIDEBAR_SETTINGS_PRESET_SETTINGS.codex },
  { id: "minimal", label: "Minimal", settings: SIDEBAR_SETTINGS_PRESET_SETTINGS.minimal },
  { id: "detailed", label: "Detailed", settings: SIDEBAR_SETTINGS_PRESET_SETTINGS.detailed },
];

/**
 * CDXC:IDEAttachment 2026-04-26-22:38
 * Users can attach ghostex to the IDE selected in settings. Keep the existing
 * persisted Zed overlay keys for compatibility while widening the target list
 * to include VS Code and VS Code Insiders.
 *
 * CDXC:IDEAttachment 2026-05-01-13:52
 * The native title-bar Attach/Detach IDE button remains visible by default,
 * but users can hide it from Settings without disabling IDE attachment.
 */
export const DEFAULT_ghostex_SETTINGS: ghostexSettings = {
  actionCompletionSound: "shamisenreverb",
  autoPromoteChatToProject: true,
  agentAcceptAllEnabled: false,
  agentManagerZoomPercent: DEFAULT_AGENT_MANAGER_ZOOM_PERCENT,
  /**
   * CDXC:BrowserFeedbackTools 2026-05-22-09:18:
   * Browser panes can inject either React Grab or Agentation for visual
   * feedback.
   *
   * CDXC:BrowserFeedbackTools 2026-05-22-09:18:
   * Agentation is the default browser feedback tool so browser panes open the
   * structured annotation workflow unless a user explicitly switches back to
   * React Grab in Settings.
   */
  browserFeedbackTool: "agentation",
  /**
   * CDXC:BrowserPanes 2026-05-02-06:35
   * Browser-pane support is opt-in so existing installs keep launching browser
   * actions through the established Chrome Canary placement flow until the user
   * chooses in-app browser panes from Settings.
   */
  browserOpenMode: "chrome-canary",
  /**
   * CDXC:EditorPanes 2026-05-06-15:00
   * Embedded code-server editor panes should reuse the user's local VS Code
   * user settings by default. A separate Insiders toggle switches the linked
   * source directory without disabling the shared project editor runtime.
   */
  codeServerLinkVscodeUserConfig: true,
  codeServerUseVscodeInsidersUserConfig: false,
  /**
   * CDXC:AgentsHub 2026-05-12-09:22
   * Agents Hub file-edit actions should use one Settings-owned editor command.
   * Start with VS Code because its `code <file>` command is the most common
   * cross-project default, while Settings exposes Zed, Cursor, and custom
   * commands for users who prefer a different editor.
   */
  customDefaultEditorCommand: "",
  defaultEditorCommand: "code",
  /**
   * CDXC:ProjectDiffStats 2026-05-16-08:46:
   * Users can hide the project-header +added/-removed git summary completely
   * when they want project names to stay visually quiet. This is independent
   * from the existing changed-file count preference.
   *
   * CDXC:SidebarSettingsPresets 2026-05-16-10:11:
   * Codex is the default sidebar preset, so new settings hide project-header
   * git stats unless the user selects Detailed or changes the setting directly.
   */
  hideProjectHeaderDiffStats: SIDEBAR_SETTINGS_PRESET_SETTINGS.codex.hideProjectHeaderDiffStats,
  /**
   * CDXC:ProjectDiffStats 2026-05-15-14:33:
   * Project-header git stats should hide the changed-file count by default and
   * show only added/removed line counts. Users can opt back into the file
   * number from Settings when they want the full diff summary.
   */
  showProjectEditorDiffFileCount:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.codex.showProjectEditorDiffFileCount,
  completionBellEnabled: false,
  completionSound: DEFAULT_COMPLETION_SOUND,
  createSessionOnSidebarDoubleClick: false,
  debuggingMode: false,
  renameSessionOnDoubleClick: false,
  /**
   * CDXC:SidebarSessions 2026-05-16-08:46:
   * Agent identity remains configurable in Settings through an explicit
   * hover-only mode for quieter session lists.
   *
   * CDXC:SidebarSettingsPresets 2026-05-16-10:11:
   * Codex and Minimal presets hide session agent icons until hover; Detailed is
   * the explicit preset for always-visible session identity chrome.
   */
  hideSessionAgentIconUntilHover:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.codex.hideSessionAgentIconUntilHover,
  /**
   * CDXC:SidebarSessions 2026-05-09-17:00
   * Session-card close controls should be available out of the box. Users can
   * still turn the hover chrome off from Settings when they want quieter cards.
   */
  showCloseButtonOnSessionCards:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.codex.showCloseButtonOnSessionCards,
  showHotkeysOnSessionCards: false,
  /**
   * CDXC:SidebarSessions 2026-05-15-08:57
   * Session-card Last Active timestamps stay visible by default for existing
   * users, but Settings owns an explicit hide toggle for quieter title rows.
   * This setting applies only to session-card timestamps and must not affect
   * project-header git diff stats.
   */
  hideLastActiveTimeOnSessionCards:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.codex.hideLastActiveTimeOnSessionCards,
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
   * Floating and menu bar desktop status badges are hidden by the default
   * Codex preset. Keep separate hide toggles so Detailed can reveal either
   * surface without coupling their visibility.
   */
  hideFloatingSessionStatusIndicators:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.codex.hideFloatingSessionStatusIndicators,
  hideMenuBarSessionStatusIndicators:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.codex.hideMenuBarSessionStatusIndicators,
  petOverlayEnabled: false,
  selectedPetId: DEFAULT_PET_ID,
  /**
   * CDXC:SessionStatusIndicators 2026-05-07-18:20
   * The AppKit floating session indicator defaults to Medium, which is half of
   * the approved X-Large visual size. Persist the named size now so Settings
   * can later tune the same scalable drawing metrics without changing native
   * command shape again.
   */
  sessionStatusIndicatorSize: "medium",
  /**
   * CDXC:SessionPersistence 2026-05-05-07:28
   * Terminal persistence is provider-selected. Off preserves the direct
   * Ghostty launch path; tmux, zmx, and zellij wrap new terminal/agent
   * sessions in a named persistence session so app restart can reattach or
   * recreate+resume.
   *
   * CDXC:SessionPersistence 2026-05-06-03:43
   * zellij uses the same durable session name contract as tmux/zmx for restart
   * attach and missing-session recreate+resume behavior even when hidden from
   * the current Settings dropdown.
   *
   * CDXC:SessionPersistence 2026-05-26-13:41:
   * New installs should start with zmx persistence enabled by default because zmx is the recommended provider for continuing Ghostex-created sessions from other devices.
   */
  sessionPersistenceProvider: "zmx",
  /**
   * CDXC:SessionPersistence 2026-05-23-00:50:
   * The session-id pane overlay preference is enabled by default, but the
   * native label itself must still render only for terminal panes that carry
   * zmx/tmux/zellij persistence metadata.
   */
  showSessionIdInTerminalPanes: true,
  /**
   * CDXC:SidebarPlacement 2026-05-06-17:32
   * Sidebar side is a first-class setting so users can choose left or right
   * placement from Settings instead of relying only on the move-sidebar hotkey.
   */
  sidebarSide: "left",
  /**
   * CDXC:SidebarTheme 2026-05-08-11:14
   * Dark Gray is the only active user-facing sidebar theme while the broader
   * theme picker is hidden. New installs must start on the persisted "plain"
   * value so the resolved chrome is the dark gray palette immediately.
   */
  sidebarTheme: "plain",
  /**
   * CDXC:GhosttyDefaults 2026-05-22-12:29:
   * New Ghostex terminals should default to the requested GitHub Dark terminal
   * profile: JetBrains Mono 13pt, bar cursor with blink, wght=300, 20% cell
   * height expansion, 15 MB scrollback, no copy-on-select, and one-to-one
   * precision/discrete mouse scrolling.
   */
  terminalCursorStyle: "bar",
  terminalCursorStyleBlink: true,
  terminalEngine: "ghostty-native",
  terminalFontFamily: "JetBrains Mono",
  terminalFontSize: 13,
  terminalFontWeight: 300,
  terminalGhosttyTheme: "GitHub Dark",
  terminalLetterSpacing: 0,
  terminalLineHeight: 1.2,
  terminalMouseScrollMultiplierDiscrete: 1,
  terminalMouseScrollMultiplierPrecision: 1,
  /**
   * CDXC:SessionPersistence 2026-05-05-07:28
   * tmuxMode remains as a compatibility mirror for older persisted settings and
   * legacy UI code. New launch behavior reads sessionPersistenceProvider so
   * zmx and zellij can follow the same persistence semantics as tmux.
   */
  tmuxMode: false,
  terminalScrollToBottomWhenTyping: true,
  terminalScrollbackLimitMb: 15,
  terminalCopyOnSelect: "false",
  terminalConfirmCloseSurface: "true",
  terminalClipboardTrimTrailingSpaces: true,
  terminalClipboardPasteProtection: true,
  terminalMouseHideWhileTyping: false,
  terminalScrollbar: "system",
  /**
   * CDXC:PromptEditorBackend 2026-05-13-15:58
   * Ctrl+G rich prompt editing originally defaulted to the floating Monaco editor. Preserve explicit gte choices, but keep new and invalid settings on the current built-in backend.
   *
   * CDXC:PromptEditorBackend 2026-05-22-09:56
   * The terminal prompt editor is named gte for Ghostex Terminal Editor. Settings, launch commands, and install copy must use gte consistently across the app.
   *
   * CDXC:PromptEditorBackend 2026-05-22-10:16
   * Monaco is popup-backed, but gte is terminal-native. A gte backend selection must resolve to the plain `gte` command so Ctrl+G edits inside the terminal that launched the editor.
   *
   * CDXC:PromptEditorBackend 2026-05-25-11:31:
   * Monaco is the out-of-the-box Ctrl+G prompt editor again. New installs should open the floating Monaco editor for local app terminals, while the native runtime resolves Monaco-over-SSH to gte because remote terminals cannot use the local overlay.
   */
  promptEditorBackend: "monaco",
  customPromptEditorCommand: "code --wait",
  /**
   * CDXC:GtePromptEditing 2026-05-22-09:56
   * The boolean mirrors keep the Ctrl+G prompt-editor setting easy to search while promptEditorBackend remains the source of truth for launch behavior.
   *
   * CDXC:GtePromptEditing 2026-05-25-11:31:
   * First-run settings mirror the default Monaco backend so older call sites that still read these booleans do not enable gte unless Settings or legacy persisted keys explicitly request it.
   */
  richPromptEditingWithGte: false,
  useGteForCtrlGPromptEditing: false,
  hotkeys: DEFAULT_ghostex_HOTKEYS,
  workspaceActivePaneBorderColor: "#3b82f6",
  workspaceBackgroundColor: "#0e0e0e",
  /**
   * CDXC:TitlebarOpenIn 2026-05-11-00:22
   * The titlebar Open In menu is configurable: built-in editor targets can be
   * hidden and user-defined command targets can be appended without changing
   * the t3code-derived default editor catalog.
   */
  customWorkspaceOpenTargets: [],
  /**
   * CDXC:TitlebarOpenIn 2026-05-11-02:03
   * First launch starts with only ghostex/Finder until the native sidebar performs
   * its one startup installed-target scan and persists the detected IDE list.
   */
  workspaceOpenTargetAvailability: DEFAULT_WORKSPACE_OPEN_TARGET_AVAILABILITY,
  workspaceOpenTargetHiddenIds: [],
  /**
   * CDXC:WorkspaceLayout 2026-05-11-18:42
   * Native split panes use the Pane Gap setting for every outside edge and
   * internal split divider. Keep the default one pixel wider so the real
   * draggable gap has enough visual weight on all sides.
   */
  workspacePaneGap: 13,
  /**
   * CDXC:IDEAttachment 2026-05-06-12:49
   * New installs should auto-sync the active ghostex project to the attached IDE
   * after workspace activation unless the user disables this setting.
   */
  syncOpenProjectWithZed: true,
  zedOverlayEnabled: false,
  zedOverlayHideTitlebarButton: false,
  zedOverlayTargetApp: "zed-preview",
};

export const SIDEBAR_THEME_SETTING_OPTIONS: ReadonlyArray<{
  label: string;
  value: SidebarThemeSetting;
}> = [
  /**
   * CDXC:SidebarTheme 2026-05-08-11:14
   * Hide Auto and the other theme presets from Settings until theme selection
   * returns. Keep only Dark Gray visible so the UI matches the active default.
   *
   * CDXC:SidebarTheme 2026-04-26-21:32: Keep the persisted value as "plain"
   * for compatibility, but present it as Dark Gray because the option now
   * always selects the dark gray sidebar palette.
   */
  { label: "Dark Gray", value: "plain" },
];

export const TERMINAL_ENGINE_SETTING_OPTIONS: ReadonlyArray<{
  label: string;
  value: TerminalEngine;
}> = [{ label: "Ghostty Native", value: "ghostty-native" }];

export const BROWSER_OPEN_MODE_OPTIONS: ReadonlyArray<{
  label: string;
  value: BrowserOpenMode;
}> = [
  { label: "Chrome Canary", value: "chrome-canary" },
  { label: "Browser Panes", value: "browser-pane" },
];

export const BROWSER_FEEDBACK_TOOL_OPTIONS: ReadonlyArray<{
  label: string;
  value: BrowserFeedbackTool;
}> = [
  { label: "React Grab", value: "react-grab" },
  { label: "Agentation", value: "agentation" },
];

export const DEFAULT_EDITOR_COMMAND_OPTIONS: ReadonlyArray<{
  label: string;
  value: DefaultEditorCommand;
}> = [
  { label: "VS Code (code)", value: "code" },
  { label: "VS Code Insiders (code-insiders)", value: "code-insiders" },
  { label: "Zed (zed)", value: "zed" },
  { label: "Zed alternate (zeditor)", value: "zeditor" },
  { label: "Cursor (cursor)", value: "cursor" },
  { label: "Windsurf (windsurf)", value: "windsurf" },
  { label: "VSCodium (codium)", value: "codium" },
  { label: "Sublime Text (subl)", value: "subl" },
  { label: "Other", value: "other" },
];

export const SESSION_PERSISTENCE_PROVIDER_OPTIONS: ReadonlyArray<{
  label: string;
  value: SessionPersistenceProvider;
}> = [
  /**
   * CDXC:SessionPersistence 2026-05-26-13:41:
   * Settings should recommend zmx and keep tmux/zellij out of the provider dropdown while code still accepts those persisted providers for existing sessions and internal launch paths.
   */
  { label: "Off", value: "off" },
  { label: "zmx (recommended)", value: "zmx" },
];

export const SIDEBAR_SIDE_OPTIONS: ReadonlyArray<{
  label: string;
  value: SidebarSide;
}> = [
  { label: "Left", value: "left" },
  { label: "Right", value: "right" },
];

export const SESSION_STATUS_INDICATOR_SIZE_OPTIONS: ReadonlyArray<{
  label: string;
  value: SessionStatusIndicatorSize;
}> = [
  { label: "X-Large", value: "x-large" },
  { label: "Large", value: "large" },
  { label: "Medium", value: "medium" },
  { label: "Small", value: "small" },
];

export const GHOSTTY_COPY_ON_SELECT_OPTIONS: ReadonlyArray<{
  label: string;
  value: GhosttyCopyOnSelect;
}> = [
  { label: "Off", value: "false" },
  { label: "Selection clipboard", value: "true" },
  { label: "System and selection clipboard", value: "clipboard" },
];

export const GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS: ReadonlyArray<{
  label: string;
  value: GhosttyConfirmCloseSurface;
}> = [
  { label: "Smart confirmation", value: "true" },
  { label: "Always confirm", value: "always" },
  { label: "Do not confirm", value: "false" },
];

export const GHOSTTY_SCROLLBAR_OPTIONS: ReadonlyArray<{
  label: string;
  value: GhosttyScrollbar;
}> = [
  { label: "System", value: "system" },
  { label: "Never", value: "never" },
];

export const PROMPT_EDITOR_BACKEND_OPTIONS: ReadonlyArray<{
  label: string;
  value: PromptEditorBackend;
}> = [
  { label: "Inherit from system", value: "inherit" },
  { label: "Monaco floating editor", value: "monaco" },
  { label: "gte terminal editor", value: "gte" },
  { label: "Custom", value: "custom" },
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
  { label: "Use existing Ghostty config", value: "__ghostex_ghostty_theme_unmanaged__" },
  ...GHOSTTY_THEME_OPTIONS.map((theme) => ({ label: theme, value: theme })),
];

export const ZED_OVERLAY_TARGET_APP_OPTIONS: ReadonlyArray<{
  label: string;
  value: ZedOverlayTargetApp;
}> = [
  { label: "Zed", value: "zed" },
  /**
   * CDXC:IDEAttachment 2026-04-28-00:05
   * Settings must distinguish regular Zed from Zed Preview because the
   * attach target is persisted separately from the shortened title-bar labels.
   */
  { label: "Zed Preview", value: "zed-preview" },
  { label: "VS Code", value: "vscode" },
  { label: "VS Code Insiders", value: "vscode-insiders" },
];

export function getZedOverlayTargetAppLabel(targetApp: ZedOverlayTargetApp): string {
  /**
   * CDXC:WorkspaceActions 2026-05-04-08:22
   * Project context menus must name the IDE selected in Settings before
   * opening a workspace there, so the menu label and native command target
   * are derived from the same persisted target value.
   */
  return (
    ZED_OVERLAY_TARGET_APP_OPTIONS.find((option) => option.value === targetApp)?.label ??
    ZED_OVERLAY_TARGET_APP_OPTIONS[0]!.label
  );
}

export function normalizeghostexSettings(candidate: unknown): ghostexSettings {
  const source = isRecord(candidate) ? candidate : {};
  const promptEditorBackend = normalizePromptEditorBackend(source);
  const sessionPersistenceProvider = normalizeSessionPersistenceProvider(
    readString(
      source,
      "sessionPersistenceProvider",
      readBoolean(source, "tmuxMode", DEFAULT_ghostex_SETTINGS.tmuxMode)
        ? "tmux"
        : DEFAULT_ghostex_SETTINGS.sessionPersistenceProvider,
    ),
  );
  return {
    actionCompletionSound: clampCompletionSoundSetting(
      readString(source, "actionCompletionSound", DEFAULT_ghostex_SETTINGS.actionCompletionSound),
    ),
    autoPromoteChatToProject: readBoolean(
      source,
      "autoPromoteChatToProject",
      DEFAULT_ghostex_SETTINGS.autoPromoteChatToProject,
    ),
    agentAcceptAllEnabled: readBoolean(
      source,
      "agentAcceptAllEnabled",
      DEFAULT_ghostex_SETTINGS.agentAcceptAllEnabled,
    ),
    agentManagerZoomPercent: clampAgentManagerZoomPercent(
      readNumber(source, "agentManagerZoomPercent", DEFAULT_ghostex_SETTINGS.agentManagerZoomPercent),
    ),
    /**
     * CDXC:BrowserFeedbackTools 2026-05-22-09:18:
     * Normalize the browser feedback injector choice so missing or invalid
     * settings use Agentation, while explicit React Grab selections continue
     * to launch the legacy injector.
     */
    browserFeedbackTool: normalizeBrowserFeedbackTool(
      readString(source, "browserFeedbackTool", DEFAULT_ghostex_SETTINGS.browserFeedbackTool),
    ),
    /**
     * CDXC:BrowserPanes 2026-05-02-06:35
     * The setting chooses the browser action target: Chrome Canary keeps the
     * native external browser integration, while Browser Panes creates normal
     * workspace session cards backed by native WKWebView panes.
     */
    browserOpenMode: normalizeBrowserOpenMode(
      readString(source, "browserOpenMode", DEFAULT_ghostex_SETTINGS.browserOpenMode),
    ),
    /**
     * CDXC:EditorPanes 2026-05-06-15:00
     * Normalize the code-server VS Code settings-link toggles on every read so
     * older settings files gain the default local VS Code settings behavior.
     */
    codeServerLinkVscodeUserConfig: readBoolean(
      source,
      "codeServerLinkVscodeUserConfig",
      DEFAULT_ghostex_SETTINGS.codeServerLinkVscodeUserConfig,
    ),
    codeServerUseVscodeInsidersUserConfig: readBoolean(
      source,
      "codeServerUseVscodeInsidersUserConfig",
      DEFAULT_ghostex_SETTINGS.codeServerUseVscodeInsidersUserConfig,
    ),
    defaultEditorCommand: normalizeDefaultEditorCommand(
      readString(source, "defaultEditorCommand", DEFAULT_ghostex_SETTINGS.defaultEditorCommand),
    ),
    customDefaultEditorCommand: normalizeCustomDefaultEditorCommand(
      readString(
        source,
        "customDefaultEditorCommand",
        DEFAULT_ghostex_SETTINGS.customDefaultEditorCommand,
      ),
    ),
    /**
     * CDXC:ProjectDiffStats 2026-05-16-08:46:
     * Missing project-header visibility now follows the Codex preset, which
     * hides git line deltas unless the user selects Detailed or changes this
     * setting directly.
     */
    hideProjectHeaderDiffStats: readBoolean(
      source,
      "hideProjectHeaderDiffStats",
      DEFAULT_ghostex_SETTINGS.hideProjectHeaderDiffStats,
    ),
    /**
     * CDXC:ProjectDiffStats 2026-05-15-14:33:
     * Missing or invalid older settings must keep project-header git stats in
     * the quieter default that hides the changed-file count.
     */
    showProjectEditorDiffFileCount: readBoolean(
      source,
      "showProjectEditorDiffFileCount",
      DEFAULT_ghostex_SETTINGS.showProjectEditorDiffFileCount,
    ),
    completionBellEnabled: readBoolean(
      source,
      "completionBellEnabled",
      DEFAULT_ghostex_SETTINGS.completionBellEnabled,
    ),
    completionSound: clampCompletionSoundSetting(
      readString(source, "completionSound", DEFAULT_ghostex_SETTINGS.completionSound),
    ),
    createSessionOnSidebarDoubleClick: readBoolean(
      source,
      "createSessionOnSidebarDoubleClick",
      DEFAULT_ghostex_SETTINGS.createSessionOnSidebarDoubleClick,
    ),
    debuggingMode: readBoolean(source, "debuggingMode", DEFAULT_ghostex_SETTINGS.debuggingMode),
    renameSessionOnDoubleClick: readBoolean(
      source,
      "renameSessionOnDoubleClick",
      DEFAULT_ghostex_SETTINGS.renameSessionOnDoubleClick,
    ),
    /**
     * CDXC:SidebarSessions 2026-05-16-08:46:
     * Missing session-card icon visibility now follows the Codex preset, which
     * hides agent icons until hover unless the user selects Detailed or changes
     * this setting directly.
     */
    hideSessionAgentIconUntilHover: readBoolean(
      source,
      "hideSessionAgentIconUntilHover",
      DEFAULT_ghostex_SETTINGS.hideSessionAgentIconUntilHover,
    ),
    showCloseButtonOnSessionCards: readBoolean(
      source,
      "showCloseButtonOnSessionCards",
      DEFAULT_ghostex_SETTINGS.showCloseButtonOnSessionCards,
    ),
    showHotkeysOnSessionCards: readBoolean(
      source,
      "showHotkeysOnSessionCards",
      DEFAULT_ghostex_SETTINGS.showHotkeysOnSessionCards,
    ),
    /**
     * CDXC:SidebarSessions 2026-05-15-08:57
     * Older settings files should preserve the current session-card timestamp
     * behavior. Explicit true hides only the Last Active label, not the code
     * project header's separate git additions/deletions summary.
     */
    hideLastActiveTimeOnSessionCards: readBoolean(
      source,
      "hideLastActiveTimeOnSessionCards",
      DEFAULT_ghostex_SETTINGS.hideLastActiveTimeOnSessionCards,
    ),
    /**
     * CDXC:SessionAttentionNotifications 2026-05-10-16:46
     * Older settings files should opt into macOS attention notifications, and
     * explicit false must be preserved for users who disable system banners.
     */
    showMacOSAttentionNotifications: readBoolean(
      source,
      "showMacOSAttentionNotifications",
      DEFAULT_ghostex_SETTINGS.showMacOSAttentionNotifications,
    ),
    /**
     * CDXC:SessionStatusIndicators 2026-05-09-17:30
     * Visibility is persisted as explicit hide flags: floating is hidden by
     * default, while the menu bar remains visible by default. Normalize missing
     * values to those defaults without coupling either surface to indicator size.
     */
    hideFloatingSessionStatusIndicators: readBoolean(
      source,
      "hideFloatingSessionStatusIndicators",
      DEFAULT_ghostex_SETTINGS.hideFloatingSessionStatusIndicators,
    ),
    hideMenuBarSessionStatusIndicators: readBoolean(
      source,
      "hideMenuBarSessionStatusIndicators",
      DEFAULT_ghostex_SETTINGS.hideMenuBarSessionStatusIndicators,
    ),
    petOverlayEnabled: readBoolean(
      source,
      "petOverlayEnabled",
      DEFAULT_ghostex_SETTINGS.petOverlayEnabled,
    ),
    selectedPetId: normalizePetId(
      readString(source, "selectedPetId", DEFAULT_ghostex_SETTINGS.selectedPetId),
    ),
    /**
     * CDXC:SessionStatusIndicators 2026-05-07-18:20
     * Indicator size is a named UX preference, not raw pixels. Normalize to
     * supported sizes so the native AppKit renderer can apply deterministic
     * scale factors while preserving Medium as the first-install default.
     */
    sessionStatusIndicatorSize: normalizeSessionStatusIndicatorSize(
      readString(
        source,
        "sessionStatusIndicatorSize",
        DEFAULT_ghostex_SETTINGS.sessionStatusIndicatorSize,
      ),
    ),
    sessionPersistenceProvider,
    /**
     * CDXC:SessionPersistence 2026-05-23-00:50:
     * Older settings should gain the default-on session-id overlay preference.
     * The native pane still suppresses the actual label unless that terminal is
     * backed by zmx, tmux, or zellij.
     */
    showSessionIdInTerminalPanes: readBoolean(
      source,
      "showSessionIdInTerminalPanes",
      DEFAULT_ghostex_SETTINGS.showSessionIdInTerminalPanes,
    ),
    /**
     * CDXC:SidebarPlacement 2026-05-06-17:32
     * Persist only the supported AppKit chrome sides. Unknown values normalize
     * to the default left placement so the native layout never receives an
     * unsupported sidebar position.
     */
    sidebarSide: normalizeSidebarSide(
      readString(source, "sidebarSide", DEFAULT_ghostex_SETTINGS.sidebarSide),
    ),
    sidebarTheme: clampSidebarThemeSetting(
      readString(source, "sidebarTheme", DEFAULT_ghostex_SETTINGS.sidebarTheme),
    ),
    terminalCursorStyle: normalizeTerminalCursorStyle(
      readString(source, "terminalCursorStyle", DEFAULT_ghostex_SETTINGS.terminalCursorStyle),
    ),
    terminalCursorStyleBlink: readBoolean(
      source,
      "terminalCursorStyleBlink",
      DEFAULT_ghostex_SETTINGS.terminalCursorStyleBlink,
    ),
    terminalEngine: normalizeTerminalEngine(
      readString(source, "terminalEngine", DEFAULT_ghostex_SETTINGS.terminalEngine),
    ),
    /**
     * CDXC:TerminalTypographySettings 2026-04-29-09:32
     * Font family is a raw Ghostty font-family string so users can type any
     * installed font from `ghostty +list-fonts`. Empty means ghostex leaves an
     * existing Ghostty font-family line or Ghostty's platform default in charge.
     * Legacy preset labels are converted to their Ghostty family name.
     */
    terminalFontFamily: normalizeGhosttyFontFamily(
      readString(source, "terminalFontFamily", DEFAULT_ghostex_SETTINGS.terminalFontFamily),
    ),
    terminalFontSize: clampNumber(
      readNumber(source, "terminalFontSize", DEFAULT_ghostex_SETTINGS.terminalFontSize),
      8,
      32,
      DEFAULT_ghostex_SETTINGS.terminalFontSize,
    ),
    terminalFontWeight: clampNumber(
      readNumber(source, "terminalFontWeight", DEFAULT_ghostex_SETTINGS.terminalFontWeight),
      100,
      900,
      DEFAULT_ghostex_SETTINGS.terminalFontWeight,
    ),
    /**
     * CDXC:TerminalThemeSettings 2026-04-29-09:32
     * Ghostty themes are exact strings. Preserve only bundled theme names from
     * the settings list, or an empty unmanaged value that keeps an existing
     * user-authored Ghostty `theme` line outside ghostex control.
     */
    terminalGhosttyTheme: normalizeGhosttyTheme(
      readString(source, "terminalGhosttyTheme", DEFAULT_ghostex_SETTINGS.terminalGhosttyTheme),
    ),
    terminalLetterSpacing: clampNumber(
      readNumber(source, "terminalLetterSpacing", DEFAULT_ghostex_SETTINGS.terminalLetterSpacing),
      -2,
      8,
      DEFAULT_ghostex_SETTINGS.terminalLetterSpacing,
    ),
    terminalLineHeight: clampNumber(
      readNumber(source, "terminalLineHeight", DEFAULT_ghostex_SETTINGS.terminalLineHeight),
      0.8,
      2,
      DEFAULT_ghostex_SETTINGS.terminalLineHeight,
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
        "terminalMouseScrollMultiplierDiscrete",
        DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierDiscrete,
      ),
      MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
      MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
      DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierDiscrete,
    ),
    terminalMouseScrollMultiplierPrecision: clampNumber(
      readNumber(
        source,
        "terminalMouseScrollMultiplierPrecision",
        DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierPrecision,
      ),
      MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
      MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER,
      DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierPrecision,
    ),
    tmuxMode: sessionPersistenceProvider === "tmux",
    terminalScrollToBottomWhenTyping: readBoolean(
      source,
      "terminalScrollToBottomWhenTyping",
      DEFAULT_ghostex_SETTINGS.terminalScrollToBottomWhenTyping,
    ),
    /**
     * CDXC:TerminalBehaviorSettings 2026-04-29-09:32
     * Common Ghostty terminal behavior settings are persisted with the same
     * practical UI ranges and enum values that the settings modal exposes,
     * then written as documented Ghostty config keys by the native host.
     */
    terminalScrollbackLimitMb: clampNumber(
      readNumber(
        source,
        "terminalScrollbackLimitMb",
        DEFAULT_ghostex_SETTINGS.terminalScrollbackLimitMb,
      ),
      MIN_GHOSTTY_SCROLLBACK_LIMIT_MB,
      MAX_GHOSTTY_SCROLLBACK_LIMIT_MB,
      DEFAULT_ghostex_SETTINGS.terminalScrollbackLimitMb,
    ),
    terminalCopyOnSelect: normalizeGhosttyCopyOnSelect(
      readString(source, "terminalCopyOnSelect", DEFAULT_ghostex_SETTINGS.terminalCopyOnSelect),
    ),
    terminalConfirmCloseSurface: normalizeGhosttyConfirmCloseSurface(
      readString(
        source,
        "terminalConfirmCloseSurface",
        DEFAULT_ghostex_SETTINGS.terminalConfirmCloseSurface,
      ),
    ),
    /**
     * CDXC:TerminalBehaviorSettings 2026-04-29-09:32
     * Clipboard cleanup/protection and mouse/scrollbar visibility mirror
     * Ghostty's documented defaults unless the user changes them in ghostex.
     */
    terminalClipboardTrimTrailingSpaces: readBoolean(
      source,
      "terminalClipboardTrimTrailingSpaces",
      DEFAULT_ghostex_SETTINGS.terminalClipboardTrimTrailingSpaces,
    ),
    terminalClipboardPasteProtection: readBoolean(
      source,
      "terminalClipboardPasteProtection",
      DEFAULT_ghostex_SETTINGS.terminalClipboardPasteProtection,
    ),
    terminalMouseHideWhileTyping: readBoolean(
      source,
      "terminalMouseHideWhileTyping",
      DEFAULT_ghostex_SETTINGS.terminalMouseHideWhileTyping,
    ),
    terminalScrollbar: normalizeGhosttyScrollbar(
      readString(source, "terminalScrollbar", DEFAULT_ghostex_SETTINGS.terminalScrollbar),
    ),
    promptEditorBackend,
    customPromptEditorCommand: normalizeCustomPromptEditorCommand(
      readString(
        source,
        "customPromptEditorCommand",
        DEFAULT_ghostex_SETTINGS.customPromptEditorCommand,
      ),
    ),
    /**
     * CDXC:GtePromptEditing 2026-05-10-11:11
     * Keep reading the old opt-in key so older snapshots round-trip cleanly.
     *
     * CDXC:GtePromptEditing 2026-05-23-01:51:
     * Mirror defaults follow the normalized backend so first-run settings and older files without mirror keys still report gte as the active Ctrl+G editor.
     */
    richPromptEditingWithGte: readBoolean(
      source,
      "richPromptEditingWithGte",
      promptEditorBackend === "gte",
    ),
    useGteForCtrlGPromptEditing: readBoolean(
      source,
      "useGteForCtrlGPromptEditing",
      readBoolean(source, "richPromptEditingWithGte", promptEditorBackend === "gte") === true,
    ),
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
        "workspaceActivePaneBorderColor",
        DEFAULT_ghostex_SETTINGS.workspaceActivePaneBorderColor,
      ).trim() || DEFAULT_ghostex_SETTINGS.workspaceActivePaneBorderColor,
    /**
     * CDXC:WorkspaceLayout 2026-04-28-06:08
     * Users can choose the background visible behind terminal panes. Persist a
     * normalized CSS color string so the React workspace and native AppKit
     * workspace render the same color instead of hardcoding dark gray.
     */
    workspaceBackgroundColor:
      readString(source, "workspaceBackgroundColor", DEFAULT_ghostex_SETTINGS.workspaceBackgroundColor)
        .trim() || DEFAULT_ghostex_SETTINGS.workspaceBackgroundColor,
    /**
     * CDXC:TitlebarOpenIn 2026-05-11-00:22
     * Settings owns which titlebar Open In targets are shown. Normalize on read
     * so the React titlebar can trust the persisted custom commands and hidden
     * built-in ids sent through native layout sync.
     */
    customWorkspaceOpenTargets: normalizeCustomWorkspaceOpenTargets(
      source.customWorkspaceOpenTargets,
    ),
    workspaceOpenTargetAvailability: normalizeWorkspaceOpenTargetAvailability(
      source.workspaceOpenTargetAvailability,
    ),
    workspaceOpenTargetHiddenIds: normalizeWorkspaceOpenTargetHiddenIds(
      source.workspaceOpenTargetHiddenIds,
    ),
    workspacePaneGap: clampNumber(
      readNumber(source, "workspacePaneGap", DEFAULT_ghostex_SETTINGS.workspacePaneGap),
      0,
      48,
      DEFAULT_ghostex_SETTINGS.workspacePaneGap,
    ),
    /**
     * CDXC:ZedOverlayWorkspace 2026-04-28-05:18
     * Project-to-Zed syncing is a user-facing behavior setting and defaults
     * on, so switching ghostex workspaces can drive the editor project after the
     * sidebar's debounce instead of doing that work from the Show Zed button.
     */
    syncOpenProjectWithZed: readBoolean(
      source,
      "syncOpenProjectWithZed",
      DEFAULT_ghostex_SETTINGS.syncOpenProjectWithZed,
    ),
    zedOverlayEnabled: readBoolean(
      source,
      "zedOverlayEnabled",
      DEFAULT_ghostex_SETTINGS.zedOverlayEnabled,
    ),
    zedOverlayHideTitlebarButton: readBoolean(
      source,
      "zedOverlayHideTitlebarButton",
      DEFAULT_ghostex_SETTINGS.zedOverlayHideTitlebarButton,
    ),
    zedOverlayTargetApp: normalizeZedOverlayTargetApp(
      readString(source, "zedOverlayTargetApp", DEFAULT_ghostex_SETTINGS.zedOverlayTargetApp),
    ),
  };
}

export function getTerminalFontFamilyForghostexSettings(settings: ghostexSettings): string {
  return settings.terminalFontFamily.trim() || getTerminalFontFamilyForPreset("JetBrains Mono");
}

export function getSidebarSettingsPresetId(
  settings: Pick<ghostexSettings, SidebarSettingsPresetKey>,
): SidebarSettingsPresetId | undefined {
  return SIDEBAR_SETTINGS_PRESETS.find((preset) =>
    SIDEBAR_SETTINGS_PRESET_KEYS.every((key) => Object.is(settings[key], preset.settings[key])),
  )?.id;
}

export function applySidebarSettingsPreset(
  settings: ghostexSettings,
  presetId: SidebarSettingsPresetId,
): ghostexSettings {
  return normalizeghostexSettings({
    ...settings,
    ...SIDEBAR_SETTINGS_PRESET_SETTINGS[presetId],
  });
}

function normalizeTerminalCursorStyle(value: string | undefined): TerminalCursorStyle {
  return value === "block" || value === "underline" ? value : "bar";
}

function normalizeBrowserOpenMode(value: string | undefined): BrowserOpenMode {
  return value === "browser-pane" ? "browser-pane" : DEFAULT_ghostex_SETTINGS.browserOpenMode;
}

function normalizeBrowserFeedbackTool(value: string | undefined): BrowserFeedbackTool {
  return value === "react-grab" ? "react-grab" : DEFAULT_ghostex_SETTINGS.browserFeedbackTool;
}

function normalizeDefaultEditorCommand(value: string | undefined): DefaultEditorCommand {
  return value === "code-insiders" ||
    value === "zed" ||
    value === "zeditor" ||
    value === "cursor" ||
    value === "windsurf" ||
    value === "codium" ||
    value === "subl" ||
    value === "other"
    ? value
    : DEFAULT_ghostex_SETTINGS.defaultEditorCommand;
}

function normalizeCustomDefaultEditorCommand(value: string | undefined): string {
  return (value ?? "").trim().slice(0, 240);
}

function normalizeCustomPromptEditorCommand(value: string | undefined): string {
  return ((value ?? "").trim() || DEFAULT_ghostex_SETTINGS.customPromptEditorCommand).slice(0, 240);
}

export function getDefaultEditorCommandForSettings(settings: ghostexSettings): string {
  const customCommand = settings.customDefaultEditorCommand.trim();
  return settings.defaultEditorCommand === "other"
    ? customCommand || DEFAULT_ghostex_SETTINGS.defaultEditorCommand
    : settings.defaultEditorCommand;
}

function normalizeSidebarSide(value: string | undefined): SidebarSide {
  return value === "right" ? "right" : DEFAULT_ghostex_SETTINGS.sidebarSide;
}

function normalizeSessionStatusIndicatorSize(
  value: string | undefined,
): SessionStatusIndicatorSize {
  return value === "small" || value === "large" || value === "x-large" ? value : "medium";
}

function normalizeSessionPersistenceProvider(
  value: string | undefined,
): SessionPersistenceProvider {
  return value === "tmux" || value === "zmx" || value === "zellij" ? value : "off";
}

function normalizePromptEditorBackend(source: Record<string, unknown>): PromptEditorBackend {
  const backend = readString(source, "promptEditorBackend", "");
  if (backend === "inherit" || backend === "monaco" || backend === "gte" || backend === "custom") {
    return backend;
  }
  if (
    readBoolean(source, "useGteForCtrlGPromptEditing", false) ||
    readBoolean(source, "richPromptEditingWithGte", false)
  ) {
    return "gte";
  }
  return DEFAULT_ghostex_SETTINGS.promptEditorBackend;
}

function normalizeGhosttyTheme(value: string | undefined): string {
  if (!value || value === "__ghostex_ghostty_theme_unmanaged__") {
    return "";
  }
  return (GHOSTTY_THEME_OPTIONS as readonly string[]).includes(value) ? value : "";
}

function normalizeGhosttyFontFamily(value: string | undefined): string {
  const trimmedValue = (value ?? "").trim();
  if (!trimmedValue) {
    return "";
  }
  const legacyPreset = normalizeTerminalFontPreset(trimmedValue);
  if (legacyPreset === trimmedValue) {
    return getGhosttyFontFamilyForPreset(legacyPreset);
  }
  return trimmedValue;
}

function normalizeGhosttyCopyOnSelect(value: string | undefined): GhosttyCopyOnSelect {
  return value === "true" || value === "clipboard" ? value : DEFAULT_ghostex_SETTINGS.terminalCopyOnSelect;
}

function normalizeGhosttyConfirmCloseSurface(
  value: string | undefined,
): GhosttyConfirmCloseSurface {
  return value === "false" || value === "always" ? value : "true";
}

function normalizeGhosttyScrollbar(value: string | undefined): GhosttyScrollbar {
  return value === "never" ? "never" : "system";
}

function normalizeZedOverlayTargetApp(value: string | undefined): ZedOverlayTargetApp {
  return value === "zed" ||
    value === "zed-preview" ||
    value === "vscode" ||
    value === "vscode-insiders"
    ? value
    : DEFAULT_ghostex_SETTINGS.zedOverlayTargetApp;
}

function clampNumber(value: number, min: number, max: number, fallback: number): number {
  if (!Number.isFinite(value)) {
    return fallback;
  }

  return Math.min(max, Math.max(min, value));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function readBoolean(
  source: Record<string, unknown>,
  key: keyof ghostexSettings,
  fallback: boolean,
): boolean {
  const value = source[key];
  return typeof value === "boolean" ? value : fallback;
}

function readNumber(
  source: Record<string, unknown>,
  key: keyof ghostexSettings,
  fallback: number,
): number {
  const value = source[key];
  return typeof value === "number" ? value : fallback;
}

function readString(
  source: Record<string, unknown>,
  key: keyof ghostexSettings,
  fallback: string,
): string {
  const value = source[key];
  return typeof value === "string" ? value : fallback;
}
