import {
  DEFAULT_AGENT_MANAGER_ZOOM_PERCENT,
  type SidebarThemeSetting,
  type TerminalEngine,
} from "./session-grid-contract-core";
import {
  clampAgentManagerZoomPercent,
  clampSidebarThemeSetting,
  DEFAULT_COMMANDS_PANEL_HEIGHT_PX,
  normalizeTerminalEngine,
} from "./session-grid-contract-session";
import {
  normalizeSessionChatTheme,
  type SessionChatTheme,
} from "./session-chat";
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
import {
  DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS,
  normalizeSidebarSessionTagListItems,
  type SidebarSessionTagListItem,
} from "./session-tags";

export type GhosttyConfirmCloseSurface = "false" | "true" | "always";
export type GhosttyCopyOnSelect = "false" | "true" | "clipboard";
export type GhosttyScrollbar = "system" | "never";
export type TerminalCursorStyle = "bar" | "block" | "underline";
export type TerminalBackgroundImageFit = "cover" | "contain" | "stretch" | "natural";
export type WindowsTerminalBackend = "wsl";
export type BrowserOpenMode = "browser-pane";
export type BrowserFeedbackTool = "agentation";
export type PortlessProtocol = "https" | "http";
export type TerminalDevServerOpenTarget = "internal-browser" | "system-default-browser";
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
export type CommandsPanelSide = "bottom" | "right";
export type SidebarProjectGroupStyle = "quiet" | "header" | "branched";
export const MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS = 0;
export const MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS = 1000;
export const SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS = 100;
export const DEFAULT_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS = 0;
export const MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT = 50;
export const MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT = 100;
export const SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP = 5;
export const DEFAULT_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT = 75;

export function clampSessionChatTranscriptWidthPercent(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT;
  }
  const clamped = Math.min(
    MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
    Math.max(MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT, value),
  );
  return (
    Math.round(clamped / SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP) *
    SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP
  );
}

export function clampSidebarCollapseAnimationDurationMs(value: number): number {
  const clamped = Math.min(
    MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
    Math.max(MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS, value),
  );
  return (
    Math.round(clamped / SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS) *
    SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS
  );
}
/**
 * CDXC:SidebarV2 2026-07-29:
 * Sidebar V2 ("Inbox") is an opt-in presentation layer beside the classic
 * sidebar. The stored version selector rides the normal settings file, so
 * hosts that only persist unknown keys need no change to support it.
 */
export type SidebarVersion = "v1" | "v2";
/** The surface shown first when a newly launched agent supports Session Chat. */
export type PreferredAgentInterface = "terminal" | "chat";
/**
 * CDXC:SidebarV2 2026-07-29:
 * Sidebar V2 renders one flat session inbox by default and can switch to
 * collapsible per-project groups. Keep the sub-mode as its own key so the
 * layout choice survives switching back and forth between V1 and V2.
 */
export type SidebarV2Layout = "flat" | "byProject";
/**
 * CDXC:SidebarV2Worktree 2026-07-29:
 * What the plain "+" does in Sidebar V2: start a session in the project itself
 * ("local", the unchanged instant path) or open the worktree popover
 * pre-filled ("worktree").
 */
export type SidebarNewSessionEnvMode = "local" | "worktree";
/**
 * CDXC:SidebarV2LogicalProjects 2026-07-29:
 * How aggressively one checkout merges with other checkouts of the same
 * repository in Sidebar V2. Mirrors
 * `SidebarV2ProjectGroupingMode` in `shared/sidebar-v2-logical-project.ts`
 * one-for-one; the two spellings must stay identical because this settings
 * value is fed straight into that module.
 */
export type SidebarProjectGroupingMode = "repository" | "repositoryPath" | "separate";
export type SidebarSettingsPresetId = "codex" | "minimal" | "detailed" | "recommended";
export type PromptEditorBackend = "inherit" | "monaco";
export type SessionTitleGenerationAgent = "codex" | "cursor" | "claude" | "grok" | "custom";
export const SETTINGS_MODAL_NAVIGATION_TABS = [
  "settings",
  "integrations",
  "plugins",
  "osIntegration",
  "remote",
  "projects",
  "agents",
  "actions",
  "openTargets",
  "hotkeys",
  "about",
] as const;
export type SettingsModalNavigationTab = (typeof SETTINGS_MODAL_NAVIGATION_TABS)[number];
export type SettingsModalNavigationState = {
  activeTab: SettingsModalNavigationTab;
  scrollTopByTab: Partial<Record<SettingsModalNavigationTab, number>>;
  version: 1;
};
export type AppShotsHotkey =
  | "both-command"
  | "both-shift"
  | "both-option"
  | "double-left-shift"
  | "double-left-option";
export type KeepAwakeDurationMinutes = 0 | 120 | 300;
export type AutoSleepIdleMinutes = 5 | 10 | 15 | 30 | 60 | 120 | 300;
export type RemoteMachineSettings = {
  id: string;
  name: string;
  sshHost: string;
  sshIdentityFile?: string;
  sshPasswordSaved?: boolean;
  sshPort?: number;
  sshUser?: string;
  wslDistribution?: string;
};
export type DiagnosticLoggingScenarioId =
  (typeof DIAGNOSTIC_LOGGING_SCENARIOS)[number]["id"];
export type DiagnosticLoggingScenarioState = {
  enabled: boolean;
  expiresAt?: string;
};
export type DiagnosticLoggingSettings = {
  scenarios: Partial<Record<DiagnosticLoggingScenarioId, DiagnosticLoggingScenarioState>>;
  version: 1;
};
export type DiagnosticLoggingScenarioGroup = "macOS" | "GPUI" | "gxserver";
export type DiagnosticLoggingScenarioDefinition = {
  description: string;
  group: DiagnosticLoggingScenarioGroup;
  id: string;
  label: string;
  logFiles: readonly string[];
};
export const DIAGNOSTIC_LOGGING_SCENARIOS = [
  {
    description: "AppKit focus, first responder, key/input routing, and terminal focus repair breadcrumbs.",
    group: "macOS",
    id: "native.terminal.focus",
    label: "Terminal focus and input routing",
    logFiles: ["native-terminal-focus-debug.log", "gpui-terminal-focus-debug.log"],
  },
  {
    description: "Terminal pane resize, Ghostty surface grid changes, zmx resize refreshes, and reflow timing breadcrumbs.",
    group: "macOS",
    id: "native.terminal.resize",
    label: "Terminal resize and reflow",
    logFiles: ["native-terminal-focus-debug.log"],
  },
  {
    description: "Sidebar hydration, gxserver presentation, React refresh, and sidebar lifecycle breadcrumbs.",
    group: "macOS",
    id: "native.sidebar.refresh",
    label: "Sidebar refresh and hydration",
    logFiles: ["sidebar-refresh-debug.log", "gpui-sidebar-refresh-debug.log"],
  },
  {
    description: "Sidebar disclosure-state localStorage, hydrate timing, and collapse-state repro breadcrumbs.",
    group: "macOS",
    id: "native.sidebar.collapse",
    label: "Sidebar collapse state",
    logFiles: ["sidebar-collapse-state-debug.log"],
  },
  {
    description: "Pane-tab buttons, resize rails, sidebar divider, and related AppKit geometry diagnostics.",
    group: "macOS",
    id: "native.pane.tabs",
    label: "Pane tabs, resize rails, and sidebar divider",
    logFiles: ["native-pane-tabs-debug.log"],
  },
  {
    description: "Pane reorder repro breadcrumbs for tab/split ownership investigations.",
    group: "macOS",
    id: "native.pane.reorder",
    label: "Pane reorder repros",
    logFiles: ["native-pane-reorder-repro.log"],
  },
  {
    description: "Browser/editor layering, hit-testing, active pane, and visible-surface ordering diagnostics.",
    group: "macOS",
    id: "native.layout.layering",
    label: "Layout, layering, and hit testing",
    logFiles: ["native-layout-layering-debug.log"],
  },
  {
    description: "Titlebar mode switching, route handoff, project-surface wake, and AppKit settle timings.",
    group: "macOS",
    id: "native.mode.switcher",
    label: "Mode switcher and titlebar routing",
    logFiles: ["native-mode-switcher-debug.log"],
  },
  {
    description: "Sidebar and titlebar WebKit lifecycle, titlebar event-loop stalls, and Resources sampler timing.",
    group: "macOS",
    id: "native.chrome.responsiveness",
    label: "Sidebar and titlebar responsiveness",
    logFiles: ["native-chrome-responsiveness-debug.log", "sidebar-refresh-debug.log"],
  },
  {
    description: "Session-title synchronization, first-prompt rename, and title-generation diagnostics.",
    group: "macOS",
    id: "native.session.title",
    label: "Session titles and auto-rename",
    logFiles: ["session-title-sync-debug.log"],
  },
  {
    description: "Agent detection, semantic activity, completion sound, and attention-notification diagnostics.",
    group: "macOS",
    id: "native.agent.detection",
    label: "Agent detection and activity",
    logFiles: ["agent-detection-debug.log"],
  },
  {
    description: "Workspace restore, startup layout cache, provider-state refresh, and previous-session diagnostics.",
    group: "macOS",
    id: "native.workspace.restore",
    label: "Workspace restore and startup",
    logFiles: ["workspace-restore-debug.log"],
  },
  {
    description: "Workspace dock/rail status indicator diagnostics and titlebar resource projection breadcrumbs.",
    group: "macOS",
    id: "native.workspace.dock",
    label: "Workspace dock indicator",
    logFiles: ["workspace-dock-indicator-debug.log"],
  },
  {
    description: "Native host lifecycle, activation, window close, and termination breadcrumbs.",
    group: "macOS",
    id: "native.host.lifecycle",
    label: "Native host lifecycle",
    logFiles: ["native-host-lifecycle.log", "gpui-host-lifecycle.log"],
  },
  {
    description: "Menu bar session-status item visibility, click delivery, dropdown ordering, and dismissal diagnostics.",
    group: "macOS",
    id: "native.menuBar.status",
    label: "Menu bar session status dropdown",
    logFiles: ["native-menu-bar-status-debug.log"],
  },
  {
    description: "Project board create/start, title generation, Beads, and worktree setup breadcrumbs.",
    group: "macOS",
    id: "native.project.board",
    label: "Project board actions",
    logFiles: ["project-board-debug.log", "gpui-project-board-debug.log"],
  },
  {
    description: "Ghostty config startup and managed terminal configuration diagnostics.",
    group: "macOS",
    id: "native.ghostty.config",
    label: "Ghostty config startup",
    logFiles: ["native-ghostty-config.log"],
  },
  {
    description:
      "Command-clicked terminal link routing: Ghostty open-url classification, host event delivery, and sidebar Browser-view routing.",
    group: "macOS",
    id: "native.terminal.links",
    label: "Terminal link opening",
    logFiles: ["terminal-link-open-debug.log"],
  },
  {
    description: "Prompt editor window, Monaco/GTE initialization, prewarm, and native child-window diagnostics.",
    group: "macOS",
    id: "native.prompt.editor",
    label: "Prompt editor",
    logFiles: ["native-prompt-editor-debug.log"],
  },
  {
    description: "Remote gxserver install approval, SSH setup phase, package selection, upload, token read, and tunnel diagnostics.",
    group: "macOS",
    id: "native.remote.gxserver.install",
    label: "Remote gxserver install",
    logFiles: ["remote-gxserver-install-debug.log", "gpui-remote-gxserver-install-debug.log"],
  },
  {
    description: "Native child-window modal lifecycle, Settings host readiness, and app-modal diagnostics.",
    group: "macOS",
    id: "native.app.modal",
    label: "App modals and Settings windows",
    logFiles: ["app-modal-debug.log", "app-modal-errors.log"],
  },
  {
    description: "GPUI sidebar focus ownership and rapid session-bounce diagnostics.",
    group: "GPUI",
    id: "gpui.sidebar.focus",
    label: "GPUI sidebar focus and bouncing",
    logFiles: ["gpui-sidebar-focus-debug.jsonl"],
  },
  {
    description: "Shared-sidebar CEF renderer readiness, responsiveness transitions, and termination diagnostics.",
    group: "GPUI",
    id: "gpui.sidebar.renderer",
    label: "GPUI sidebar renderer lifecycle",
    logFiles: ["gpui-sidebar-renderer-debug.jsonl"],
  },
  {
    description: "GPUI app-modal host lifecycle, Settings hydration, renderer checkpoints, and modal errors.",
    group: "GPUI",
    id: "gpui.app.modal",
    label: "GPUI app modals and Settings",
    logFiles: ["gpui-app-modal-debug.jsonl"],
  },
  {
    description: "gxserver process startup, shutdown, and daemon lifecycle breadcrumbs.",
    group: "gxserver",
    id: "gxserver.lifecycle",
    label: "Daemon lifecycle",
    logFiles: ["gxserver.jsonl"],
  },
  {
    description: "gxserver API request timing and status breadcrumbs.",
    group: "gxserver",
    id: "gxserver.requests",
    label: "API requests",
    logFiles: ["gxserver.jsonl"],
  },
  {
    description: "gxserver typed-operation routing and result breadcrumbs.",
    group: "gxserver",
    id: "gxserver.typedOperations",
    label: "Typed operations",
    logFiles: ["gxserver.jsonl"],
  },
  {
    description: "gxserver repository clone lifecycle breadcrumbs.",
    group: "gxserver",
    id: "gxserver.repositoryClone",
    label: "Repository cloning",
    logFiles: ["gxserver.jsonl"],
  },
  {
    description: "gxserver Portless state and background synchronization breadcrumbs.",
    group: "gxserver",
    id: "gxserver.portless",
    label: "Portless",
    logFiles: ["gxserver.jsonl"],
  },
] as const satisfies readonly DiagnosticLoggingScenarioDefinition[];

// Routine diagnostic disk logging is opt-in. Agents enable only the scenario
// needed for a repro and restore it when collection is complete.
const DEFAULT_DIAGNOSTIC_LOGGING_SCENARIOS: DiagnosticLoggingSettings["scenarios"] = {};
const DIAGNOSTIC_LOGGING_SCENARIO_IDS = new Set<string>(
  DIAGNOSTIC_LOGGING_SCENARIOS.map((scenario) => scenario.id),
);
const MIN_GHOSTTY_MOUSE_SCROLL_MULTIPLIER = 0.25;
const MAX_GHOSTTY_MOUSE_SCROLL_MULTIPLIER = 8;
const MIN_GHOSTTY_SCROLLBACK_LIMIT_MB = 1;
const MAX_GHOSTTY_SCROLLBACK_LIMIT_MB = 200;
export const DEFAULT_TERMINAL_PANE_PADDING_PX = 0;
export const MIN_TERMINAL_PANE_PADDING_PX = 0;
export const MAX_TERMINAL_PANE_PADDING_PX = 64;
export const MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX = 40;
export const MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX = 600;
export const DEFAULT_SIDEBAR_DEFAULT_WIDTH_PX = 235;
export const MIN_SIDEBAR_DEFAULT_WIDTH_PX = 150;
export const MAX_SIDEBAR_DEFAULT_WIDTH_PX = 520;
export const DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT = 10;
export const MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT = 1;
export const MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT = 50;
export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_FOREGROUND_COLOR = "#d8d8d8";
export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_DARK_FOREGROUND_COLOR = "#262626";
export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_COLOR = "#1c1c1c";
export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR = "#808080";
export const DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT = 90;
/*
 * CDXC:SidebarTitlebarColors 2026-06-28-08:01:
 * The tint scale keeps the original 95 reference so existing saved contrast
 * values do not darken or brighten when the app default changes.
 *
 * CDXC:SidebarTitlebarColors 2026-07-22:
 * New installs use the neutral #808080 tint at 90 Background Contrast,
 * resolving to #1c1c1c while preserving the existing calibrated scale.
 */
const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_SCALE_REFERENCE_DARKNESS_PERCENT = 95;
export const MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT = 85;
export const MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT = 100;
const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS: ReadonlyMap<string, string> = new Map([
  ["#000000", "#000000"],
  ["#ffffff", "#0e0e0e"],
  ["#808080", "#0e0e0e"],
  ["#88d7ff", "#0a0f12"],
  ["#4f6672", "#0c0e10"],
  ["#884444", "#0d0005"],
  ["#8a5330", "#100502"],
  ["#8a6a2f", "#110a02"],
  ["#657a3f", "#0c1005"],
  ["#3f7a5f", "#031006"],
  ["#2f7d66", "#03100c"],
  ["#287c7f", "#031011"],
  ["#336699", "#0c0e11"],
  ["#4f5f96", "#080912"],
  ["#6c4f8f", "#0a0611"],
  ["#854f7a", "#100611"],
  ["#8a4f5f", "#100409"],
]);
export const TERMINAL_DEV_SERVER_OPEN_TARGET_OPTIONS: ReadonlyArray<{
  label: string;
  value: TerminalDevServerOpenTarget;
}> = [
  { label: "System Default Browser", value: "system-default-browser" },
  { label: "Internal Browser", value: "internal-browser" },
];
const DEFAULT_TERMINAL_DEV_SERVER_OPEN_TARGET: TerminalDevServerOpenTarget =
  "system-default-browser";
const DEFAULT_TERMINAL_DEV_SERVER_IGNORED_PORT_RULES: readonly string[] = [];
const TERMINAL_DEV_SERVER_OPEN_TARGET_SET = new Set(
  TERMINAL_DEV_SERVER_OPEN_TARGET_OPTIONS.map((option) => option.value),
);
const SETTINGS_MODAL_NAVIGATION_TAB_SET = new Set<string>(SETTINGS_MODAL_NAVIGATION_TABS);
const MAX_SETTINGS_MODAL_SCROLL_TOP = 1_000_000;
export const DEFAULT_SETTINGS_MODAL_NAVIGATION_STATE: SettingsModalNavigationState = {
  activeTab: "settings",
  scrollTopByTab: {},
  version: 1,
};
export const SESSION_TITLE_GENERATION_AGENT_OPTIONS: ReadonlyArray<{
  label: string;
  value: SessionTitleGenerationAgent;
}> = [
  { label: "Codex", value: "codex" },
  { label: "Cursor CLI", value: "cursor" },
  { label: "Claude", value: "claude" },
  { label: "Grok Build", value: "grok" },
  { label: "Custom", value: "custom" },
];
export const SESSION_TITLE_GENERATION_PROMPT_PLACEHOLDER = "<title generation prompt>";

export function clampCommandsPanelDefaultHeightPx(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_COMMANDS_PANEL_HEIGHT_PX;
  }
  return Math.min(
    MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX,
    Math.max(MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX, Math.round(value)),
  );
}

export function clampSidebarDefaultWidthPx(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_SIDEBAR_DEFAULT_WIDTH_PX;
  }
  return Math.min(
    MAX_SIDEBAR_DEFAULT_WIDTH_PX,
    Math.max(MIN_SIDEBAR_DEFAULT_WIDTH_PX, Math.round(value)),
  );
}

export function clampProjectSessionListCollapsedCount(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT;
  }
  return Math.min(
    MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
    Math.max(MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT, Math.round(value)),
  );
}

export function clampTerminalPanePaddingPx(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_TERMINAL_PANE_PADDING_PX;
  }
  return Math.min(
    MAX_TERMINAL_PANE_PADDING_PX,
    Math.max(MIN_TERMINAL_PANE_PADDING_PX, Math.round(value)),
  );
}

function normalizeSidebarTitlebarHexColor(value: string, fallback: string): string {
  const normalized = value.trim().toLowerCase();
  return /^#[0-9a-f]{6}$/u.test(normalized) ? normalized : fallback;
}

function clampColorChannel(value: number): number {
  return Math.min(255, Math.max(0, Math.round(value)));
}

type SidebarTitlebarRgbColor = {
  blue: number;
  green: number;
  red: number;
};

function parseSidebarTitlebarHexColor(color: string): SidebarTitlebarRgbColor {
  const normalized = normalizeSidebarTitlebarHexColor(
    color,
    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_COLOR,
  );
  return {
    red: Number.parseInt(normalized.slice(1, 3), 16),
    green: Number.parseInt(normalized.slice(3, 5), 16),
    blue: Number.parseInt(normalized.slice(5, 7), 16),
  };
}

function formatSidebarTitlebarHexColor(color: SidebarTitlebarRgbColor): string {
  return `#${[color.red, color.green, color.blue]
    .map((channel) => clampColorChannel(channel).toString(16).padStart(2, "0"))
    .join("")}`;
}

function scaleSidebarTitlebarVector(color: SidebarTitlebarRgbColor, amount: number): SidebarTitlebarRgbColor {
  return {
    red: color.red * amount,
    green: color.green * amount,
    blue: color.blue * amount,
  };
}

function addSidebarTitlebarColors(
  base: SidebarTitlebarRgbColor,
  offset: SidebarTitlebarRgbColor,
): SidebarTitlebarRgbColor {
  return {
    red: base.red + offset.red,
    green: base.green + offset.green,
    blue: base.blue + offset.blue,
  };
}

function normalizedSidebarTitlebarTintDirection(background: SidebarTitlebarRgbColor): SidebarTitlebarRgbColor {
  const average = (background.red + background.green + background.blue) / 3;
  const direction = {
    red: background.red - average,
    green: background.green - average,
    blue: background.blue - average,
  };
  const magnitude = Math.max(
    Math.abs(direction.red),
    Math.abs(direction.green),
    Math.abs(direction.blue),
  );
  if (magnitude < 0.5) {
    return {
      red: 0,
      green: 0,
      blue: 0,
    };
  }
  return scaleSidebarTitlebarVector(direction, 1 / magnitude);
}

export function clampSidebarTitlebarBackgroundDarknessPercent(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT;
  }
  return Math.min(
    MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
    Math.max(MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT, Math.round(value)),
  );
}

function getSidebarTitlebarBackgroundDarknessForColor(backgroundColor: string): number {
  const background = normalizeSidebarTitlebarHexColor(
    backgroundColor,
    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_COLOR,
  );
  const red = Number.parseInt(background.slice(1, 3), 16);
  const green = Number.parseInt(background.slice(3, 5), 16);
  const blue = Number.parseInt(background.slice(5, 7), 16);
  const luminance = (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255;
  return clampSidebarTitlebarBackgroundDarknessPercent((1 - luminance) * 100);
}

function isNeutralSidebarTitlebarColor(color: SidebarTitlebarRgbColor): boolean {
  return Math.max(color.red, color.green, color.blue) - Math.min(color.red, color.green, color.blue) < 1;
}

function getSidebarTitlebarDefaultDarkTintBackground(tint: string): SidebarTitlebarRgbColor {
  const calibratedTintBackground = CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS.get(tint);
  if (calibratedTintBackground) {
    return parseSidebarTitlebarHexColor(calibratedTintBackground);
  }

  const color = parseSidebarTitlebarHexColor(tint);
  if (isNeutralSidebarTitlebarColor(color)) {
    return parseSidebarTitlebarHexColor(DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_COLOR);
  }

  const direction = normalizedSidebarTitlebarTintDirection(color);
  const base = parseSidebarTitlebarHexColor(DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_COLOR);
  return addSidebarTitlebarColors(base, scaleSidebarTitlebarVector(direction, 4));
}

function scaleSidebarTitlebarDefaultDarkTintBackground(
  background: SidebarTitlebarRgbColor,
  darknessPercent: number,
): SidebarTitlebarRgbColor {
  if (darknessPercent === MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT) {
    return { red: 0, green: 0, blue: 0 };
  }
  const defaultRange =
    MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT -
    CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_SCALE_REFERENCE_DARKNESS_PERCENT;
  const scale =
    (MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT - darknessPercent) / defaultRange;
  return {
    red: background.red * scale,
    green: background.green * scale,
    blue: background.blue * scale,
  };
}

export function getSidebarTitlebarBackgroundForDarkness(
  darknessPercent: number,
  tintColor = DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR,
): string {
  /**
   * CDXC:SidebarTitlebarColors 2026-06-15-13:45:
   * Replace the freeform custom background color picker with a contrast slider.
   * The slider controls how strongly the calibrated dark tint background is
   * applied so custom chrome can vary in contrast without turning into
   * arbitrary bright sidebar colors.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:01:
   * Limit the contrast slider to 85-100 so custom chrome stays in the dark
   * gray range instead of drifting into mid-gray sidebar backgrounds.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:15:
   * Keep the internal darkness percentage name for compatibility while the
   * visible Settings control is labeled Background Contrast.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:28:
   * Add a web-only tint picker without returning to arbitrary background
   * colors. Map tint choices to dark applied backgrounds so tint changes are
   * subtle and neutral #808080 preserves the original gray.
   *
   * CDXC:SidebarTitlebarColors 2026-06-16-14:28:
   * Default custom chrome should now use 95 contrast with white tint. White
   * remains neutral in the calibrated tint table because all same-channel
   * tints should keep the sidebar/titlebar background gray.
   *
   * CDXC:SidebarTitlebarColors 2026-06-19-14:20:
   * Tint swatches stay visually legible in Settings, but applied custom chrome
   * should default to calibrated very-dark backgrounds such as #0d0005 for red
   * and #0c0e11 for blue. Scale those dark targets with the Contrast slider,
   * and keep same-channel tints such as white, black, and gray neutral instead
   * of adding a blue cast.
   */
  const darkness = clampSidebarTitlebarBackgroundDarknessPercent(darknessPercent);
  const tint = normalizeSidebarTitlebarHexColor(
    tintColor,
    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR,
  );
  const defaultDarkTintBackground = getSidebarTitlebarDefaultDarkTintBackground(tint);
  const background = scaleSidebarTitlebarDefaultDarkTintBackground(
    defaultDarkTintBackground,
    darkness,
  );
  const channels = [background.red, background.green, background.blue].map(clampColorChannel);
  return `#${channels.map((channel) => channel.toString(16).padStart(2, "0")).join("")}`;
}

/**
 * CDXC:SidebarTitlebarColors 2026-06-15-13:22:
 * The foreground is no longer user-selectable. Ignore any legacy saved
 * foreground value and recompute it from the validated background color, using
 * the standard light foreground for dark backgrounds and standard dark
 * foreground for light backgrounds.
 */
export function getSidebarTitlebarForegroundForBackground(backgroundColor: string): string {
  const background = normalizeSidebarTitlebarHexColor(
    backgroundColor,
    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_COLOR,
  );
  const red = Number.parseInt(background.slice(1, 3), 16);
  const green = Number.parseInt(background.slice(3, 5), 16);
  const blue = Number.parseInt(background.slice(5, 7), 16);
  const luminance = (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255;
  return luminance > 0.54
    ? DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_DARK_FOREGROUND_COLOR
    : DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_FOREGROUND_COLOR;
}

export type SidebarTitlebarGradientColors = {
  sidebarBottom: string;
  sidebarTop: string;
  titlebarLeft: string;
  titlebarRight: string;
};

export function getSidebarTitlebarGradientColors(backgroundColor: string): SidebarTitlebarGradientColors {
  /*
   * CDXC:SidebarTitlebarColors 2026-06-19-12:33:
   * Custom sidebar chrome should render as a fixed-strength gradient instead of
   * a flat color. Derive the hue direction from the resolved tint-adjusted
   * background, normalize it so every tint uses the same gradient degree, and
   * keep neutral white/black/gray tints on a neutral gray gradient.
   *
   * CDXC:SidebarTitlebarColors 2026-06-19-13:26:
   * The titlebar should share the sidebar's gradient stops: left side matches
   * the sidebar top stop and right side matches the sidebar bottom stop so the
   * chrome fades darker across the titlebar instead of brighter.
   *
   * CDXC:SidebarTitlebarColors 2026-06-19-14:20:
   * Same-channel tint outputs must not receive the older blue fallback
   * direction. White and black selections should leave the dark sidebar area
   * neutral instead of shifting it toward blue.
   */
  const base = parseSidebarTitlebarHexColor(backgroundColor);
  const tintDirection = normalizedSidebarTitlebarTintDirection(base);
  const sidebarTop = addSidebarTitlebarColors(base, scaleSidebarTitlebarVector(tintDirection, 2));
  const sidebarBottom = addSidebarTitlebarColors(base, scaleSidebarTitlebarVector(tintDirection, 10));
  return {
    sidebarTop: formatSidebarTitlebarHexColor(sidebarTop),
    sidebarBottom: formatSidebarTitlebarHexColor(sidebarBottom),
    titlebarLeft: formatSidebarTitlebarHexColor(sidebarTop),
    titlebarRight: formatSidebarTitlebarHexColor(sidebarBottom),
  };
}

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
  /** GPUI titlebar choices, keyed by canonical main-project id. */
  gpuiTitlebarActionCommandByProject: Record<string, string>;
  gpuiTitlebarOpenTargetByProject: Record<string, string>;
  appShotsEnabled: boolean;
  appShotsHotkey: AppShotsHotkey;
  appShotsMetadataEnabled: boolean;
  /**
   * CDXC:GxserverAgentSettings 2026-06-02-22:23:
   * This field is the sidebar render cache for gxserver-owned global Accept All
   * settings. Settings UI can display and edit it, but gxserver persists the
   * canonical value and applies each agent's runtime permission-bypass mode.
   *
   * CDXC:GxserverAgentSettings 2026-06-09-14:22:
   * OpenCode Accept All is runtime config rather than a CLI flag, so settings
   * copy and storage must describe the policy without promising flag insertion.
   */
  agentAcceptAllEnabled: boolean;
  agentManagerZoomPercent: number;
  /**
   * CDXC:PromptAgents 2026-05-28-07:15:
   * Automated prompt flows such as Git helper prompts, project board Start Work,
   * and worktree first prompts need one user-selected default agent instead of
   * hardcoding Codex in each launcher.
   *
   * CDXC:GxserverAgentSettings 2026-06-19-08:58:
   * gxserver now owns the canonical Default Prompt Agent alongside global
   * Accept All. Keep this field as the sidebar's synchronous render cache so
   * Settings can draw immediately from startup snapshots and gxserver update
   * responses without localStorage becoming a competing source of truth.
   */
  defaultPromptAgentId: string;
  /**
   * CDXC:GxserverSessionTitle 2026-06-04-08:24:
   * First-prompt session-title generation is gxserver-owned, but Settings owns
   * which headless agent command should produce those titles. Keep this scoped
   * away from Default Prompt Agent so changing title generation does not alter
   * Git prompts, worktree starts, or project-board prompts.
   *
   * CDXC:GxserverSessionTitle 2026-06-04-22:44:
   * The selector includes Grok Build and its Composer 2.5 command preview, so
   * users can see the exact headless CLI command Ghostex will send before
   * automatic first-prompt session naming runs.
   */
  sessionTitleGenerationAgent: SessionTitleGenerationAgent;
  customSessionTitleGenerationCommand: string;
  browserFeedbackTool: BrowserFeedbackTool;
  browserOpenMode: BrowserOpenMode;
  /**
   * CDXC:TerminalLinkInAppBrowser 2026-07-02-13:05:
   * Command-clicked http/https terminal links open as tabs in the project
   * Browser view by default. Turning this off restores handing web links to
   * the system default browser. File paths and non-web schemes always keep
   * the external NSWorkspace route regardless of this setting.
   */
  openTerminalLinksInApp: boolean;
  /**
   * CDXC:SettingsAdvanced 2026-06-28-08:01:
   * Show Advanced is a persisted Settings browsing preference. When users enable
   * advanced rows, keep that density enabled across app restarts until they
   * explicitly turn it off again.
   */
  showAdvancedSettings: boolean;
  /**
   * CDXC:SettingsNavigation 2026-06-29-17:54:
   * Closing Settings should persist the user's current Settings page and
   * scroll offsets so relaunching the macOS app can reopen Settings at the
   * exact spot they left, while explicit deep links can still override it.
   *
   * CDXC:SettingsNavigation 2026-06-30-04:47:
   * Persist page navigation as the user moves through Settings because the
   * native AppKit close button can tear down the child window before React's
   * dialog-close callback runs.
   */
  settingsModalNavigation: SettingsModalNavigationState;
  /**
   * CDXC:ExperimentalFeatures 2026-06-28-07:41:
   * Enable Experimental Features is the user-facing name for this persisted
   * showBetaFeatures key. Experimental surfaces stay hidden by default, while
   * Agents Hub remains outside this gate and visible in the sidebar.
   *
   * CDXC:Automations 2026-07-01-03:24:
   * Automations Overview and project Automate are experimental macOS surfaces.
   * Keep their real page content behind this gate; disabled users should see
   * only the coming-soon overlay for those pages.
   */
  showBetaFeatures: boolean;
  /**
   * Built-in project workarea switches control titlebar presentation only;
   * they do not stop runtimes, unmount surfaces, or disable hotkeys.
   */
  codeViewTabHidden: boolean;
  browserViewTabHidden: boolean;
  kanbanViewTabHidden: boolean;
  automateViewTabHidden: boolean;
  docsViewTabHidden: boolean;
  /**
   * Quick-access switches affect only the matching right-side titlebar button.
   * The menus and commands remain available through their other entry points.
   */
  tipsAndTricksTitlebarButtonHidden: boolean;
  resourcesTitlebarButtonHidden: boolean;
  gitActionsTitlebarButtonHidden: boolean;
  quickActionsTitlebarButtonHidden: boolean;
  openInTitlebarButtonHidden: boolean;
  codeServerLinkVscodeUserConfig: boolean;
  codeServerUseVscodeInsidersUserConfig: boolean;
  customDefaultEditorCommand: string;
  /**
   * CDXC:AppIconPicker 2026-06-25-21:50: Persisted id of the selected Dock /
   * app-switcher icon. Empty string means the default bundled app icon. The
   * value is a filename living in the native icons folder; native confirms the
   * selection via an appIconState ok event before the sidebar persists it.
   */
  appIconSourceId: string;
  defaultEditorCommand: DefaultEditorCommand;
  hideProjectHeaderDiffStats: boolean;
  /**
   * CDXC:DocsSidebar 2026-06-30-19:47:
   * The Docs sidebar scans ./docs, ./artifacts, and ./ai recursively plus root artifacts by default. Users can add comma-separated project-relative folder roots from global Projects settings. Trim spaces around each folder name while preserving spaces inside names such as "my documents".
   *
   * CDXC:DocsRootAdditive 2026-08-09:
   * These folders are project-root-relative, always. A configured Docs directory is mounted as an ADDITIONAL top-level folder that always shows its whole tree, so it is never narrowed by this list (round 2 briefly made it a narrowing control for that root; additive mounting replaced that).
   */
  manageAdditionalDocsFolders: string;
  /**
   * CDXC:GlobalProjectDefaults 2026-08-02:
   * Global Defaults for the three per-project fields on the Projects settings
   * page. A project keeps overriding the default whenever its own value is
   * non-empty; an empty project value now falls back here before falling back
   * to the previous built-in behavior. Empty globals therefore preserve the
   * exact pre-existing resolution for every project.
   */
  globalWorktreeCommand: string;
  globalBeadsDisplayKey: string;
  globalBeadsDirectory: string;
  /**
   * CDXC:DocsRootDirectory 2026-08-09:
   * Absolute folder Docs shows IN ADDITION to the project's own docs when a
   * project sets no Docs directory of its own.
   *
   * CDXC:DocsRootAdditive 2026-08-09:
   * It never replaces the project's docs — README.md, CLAUDE.md, docs/, and the
   * configured Docs folders all keep listing, and this folder is added beside
   * them as one top-level node named after itself. Empty adds nothing.
   *
   * A project's own `docsDirectory` takes this one's place in the cascade; it is
   * likewise an addition to that project's docs, never a replacement for them.
   */
  globalDocsDirectory: string;
  showProjectEditorDiffFileCount: boolean;
  showUntrackedProjectDiffWhenNoTrackedChanges: boolean;
  completionBellEnabled: boolean;
  completionSound: CompletionSoundSetting;
  showNotificationOnTerminalBell: boolean;
  createSessionOnSidebarDoubleClick: boolean;
  debuggingMode: boolean;
  /**
   * CDXC:DiagnosticsSettings 2026-06-27-22:07:
   * Debugging Mode no longer acts as the broad disk-logging switch. Routine
   * persistent diagnostics are controlled by explicit scenario ids so users can
   * enable one repro area, such as GPUI app modals or macOS terminal focus,
   * without turning on every noisy log writer.
   */
  diagnosticLogging: DiagnosticLoggingSettings;
  renameSessionOnDoubleClick: boolean;
  /** Show project artwork or the folder/worktree fallback beside project names. */
  showProjectIcons: boolean;
  hideSessionAgentIconUntilHover: boolean;
  /**
   * CDXC:SidebarSessionAgentIcons 2026-06-29-23:58:
   * Session-card agent logos are monochrome by default for compatibility, but
   * Settings needs an independent toggle for colored brand artwork. Favorite
   * state must not recolor the agent logo to gold.
   */
  useColoredSessionAgentIcons: boolean;
  hideBrowserFaviconUntilHover: boolean;
  showCloseButtonOnSessionCards: boolean;
  hideLastActiveTimeOnSessionCards: boolean;
  /**
   * CDXC:SidebarContextMenu 2026-06-10-13:58:
   * The destructive single-session Close context-menu item is advanced chrome.
   * Hide it by default and expose it through an explicit Session Cards setting
   * so context menus stay focused unless users opt into close-from-menu actions.
   */
  showSessionCloseContextMenuAction: boolean;
  /**
   * CDXC:SidebarContextMenu 2026-06-09-23:17:
   * Session context menus should hide Copy resume and Copy attach command by default because they expose raw shell-command utilities. Settings owns a single opt-in that reveals both actions for users who intentionally copy commands into external terminals.
   */
  showSessionCommandCopyActions: boolean;
  /**
   * CDXC:SidebarContextMenu 2026-06-11-23:08:
   * Copy details is an explicit session-card context-menu opt-in. Keep it hidden
   * by default because it copies project/session metadata, including paths and
   * provider ids, into the system clipboard.
   */
  showSessionDetailsCopyAction: boolean;
  /**
   * CDXC:SessionTagFilters 2026-06-13-17:50:
   * Settings owns the sidebar tag-filter presentation list: users can reorder
   * tags, move separators, hide rows, or disable selectable tag filters without
   * changing the durable session tag values stored on sessions.
   */
  sidebarSessionTagListItems: readonly SidebarSessionTagListItem[];
  /**
   * CDXC:AutoSleep 2026-05-28-08:06:
   * Auto Sleep is a settings-owned policy for retiring idle VS Code, Git,
   * Project, Manage, browser, and agent sessions through their native sleep paths.
   * Keep each surface independently configurable so users can preserve existing
   * editor behavior while opting agent terminals in separately.
   */
  autoSleepAgentSessionsEnabled: boolean;
  autoSleepAgentIdleMinutes: AutoSleepIdleMinutes;
  autoSleepBrowserSessionsEnabled: boolean;
  autoSleepBrowserIdleMinutes: AutoSleepIdleMinutes;
  autoSleepCodeEditorEnabled: boolean;
  autoSleepCodeEditorIdleMinutes: AutoSleepIdleMinutes;
  autoSleepGitEditorEnabled: boolean;
  autoSleepGitEditorIdleMinutes: AutoSleepIdleMinutes;
  autoSleepProjectEditorEnabled: boolean;
  autoSleepProjectEditorIdleMinutes: AutoSleepIdleMinutes;
  autoSleepRequireAgentResumeCommand: boolean;
  autoSleepFavoriteAgentSessions: boolean;
  keepAwakeActivateOnExternalDisplay: boolean;
  keepAwakeActivateOnLaunch: boolean;
  keepAwakeAllowDisplaySleep: boolean;
  keepAwakeBatteryThresholdPercent: number;
  keepAwakeDeactivateBelowBatteryThreshold: boolean;
  keepAwakeDeactivateOnLowPowerMode: boolean;
  keepAwakeDeactivateOnUserSwitch: boolean;
  keepAwakeDefaultDurationMinutes: KeepAwakeDurationMinutes;
  /**
   * CDXC:TitlebarKeepAwake 2026-06-23-08:20:
   * Users can opt into a Mac power hold while any session is Working, with the titlebar runtime extending that hold for a short reply window after work stops.
   */
  keepAwakeWhileWorkingSessions: boolean;
  keepAwakePreventLidSleep: boolean;
  hideKeepAwakeTitlebarControl: boolean;
  /**
   * CDXC:GlobalActions 2026-08-01:
   * The Agents tab strip ships New Terminal, New Chat, and New Browser Tab
   * buttons. Users who run those from Global Actions or hotkeys can hide each
   * one to make room in the strip. The pane overflow button is deliberately not
   * hideable — it is the only way to reach the rest of the pane actions.
   */
  hideTabStripNewTerminalButton: boolean;
  hideTabStripNewChatButton: boolean;
  hideTabStripNewBrowserButton: boolean;
  showMacOSAttentionNotifications: boolean;
  hideFloatingSessionStatusIndicators: boolean;
  hideMenuBarSessionStatusIndicators: boolean;
  petOverlayEnabled: boolean;
  selectedPetId: PetId;
  sessionStatusIndicatorSize: SessionStatusIndicatorSize;
  sessionPersistenceProvider: SessionPersistenceProvider;
  showSessionIdInTerminalPanes: boolean;
  /** Newly launched supported agents still start a terminal, then show this surface first. */
  preferredAgentInterface: PreferredAgentInterface;
  /**
   * CDXC:SidebarV2 2026-07-29:
   * The sidebar version selector is the rollout switch for the Inbox sidebar.
   * V1 stays the default everywhere; V2 is pure opt-in from Settings or the
   * sidebar Sort & Filter menu.
   */
  sidebarVersion: SidebarVersion;
  /**
   * CDXC:SidebarV2 2026-07-29:
   * Group by Project is a V2-only sub-mode. It is stored independently of
   * `sidebarVersion` so returning to V2 restores the last chosen layout.
   */
  sidebarV2Layout: SidebarV2Layout;
  /**
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * Days of inactivity before an untouched session auto-settles onto the Inbox
   * sidebar's Settled shelf. `null` disables inactivity auto-settle entirely.
   *
   * This key is read by BOTH ends: the client predicate in
   * `shared/sidebar-v2-lifecycle.ts` and gxserver-rs, which reads
   * `sidebarAutoSettleAfterDays` straight out of
   * `GHOSTEX_HOME/state/native-sidebar-settings.json` for its auto-settle sweep
   * (`gxserver-rs/src/session_lifecycle.rs`). The spelling is therefore part of
   * the server contract — renaming it silently reverts every user to the
   * 3-day default.
   */
  sidebarAutoSettleAfterDays: number | null;
  /**
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * Per-checkout override for cross-machine logical project grouping in Sidebar
   * V2. The default (an empty record) means every project follows the automatic
   * rule: merge checkouts that share a normalized git `origin` remote, and
   * never merge anything without one.
   *
   * The KEY is the module's physical project key
   * (`deriveSidebarV2ProjectGroupingOverrideKey` in
   * `shared/sidebar-v2-logical-project.ts`), i.e. `<machineId>:<path>`. Keying
   * by the physical checkout rather than by repository is deliberate: setting
   * "keep separate" on this Mac's copy must not silently re-group a colleague's
   * machine, and the key stays stable when a project is renamed.
   *
   * Values are the wire-contract spellings `"repository"` (merge every checkout
   * of the repo), `"repositoryPath"` (merge only checkouts at the same path
   * inside the repo), and `"separate"` (never merge). Unknown values and
   * malformed entries are dropped by normalization rather than defaulting, so a
   * hand-edited settings file cannot invent a grouping mode.
   */
  sidebarProjectGroupingOverrides: Readonly<Record<string, SidebarProjectGroupingMode>>;
  /**
   * CDXC:SidebarV2Worktree 2026-07-29:
   * Default environment for a new session started from Sidebar V2's "+".
   * "local" keeps the instant in-project session; "worktree" makes the same
   * click open the worktree popover pre-filled instead.
   *
   * This is GLOBAL rather than per-project on purpose (see the plan's P4
   * notes): per-project storage would mean a new `gitConfig` field, a new
   * settings message, a new projection, and a Projects-tab control, for a
   * preference users set once. A per-project override can be layered on later
   * without changing this key's meaning.
   */
  newSessionsDefaultEnvMode: SidebarNewSessionEnvMode;
  sidebarSide: SidebarSide;
  /** Duration for sidebar section, group, and project disclosure animations. */
  sidebarCollapseAnimationDurationMs: number;
  /**
   * CDXC:SidebarChrome 2026-06-05-04:40:
   * The sidebar default width is the reset target for a double-click on the
   * sidebar drag handle in Electron and native macOS. Restart hydration must
   * continue using the last persisted sidebarWidth so changing this default
   * does not erase the user's last manual resize.
   */
  sidebarDefaultWidthPx: number;
  /**
   * CDXC:ProjectSessionLists 2026-06-13-01:06:
   * The project header Show less action keeps a configurable number of project sessions visible. Default to ten visible sessions so active projects stay scannable before switching back to Show more.
   */
  projectSessionListCollapsedCount: number;
  /** Visual treatment for user-created project groups in the shared sidebar. */
  sidebarProjectGroupStyle: SidebarProjectGroupStyle;
  /** @deprecated Retained only so existing settings files remain readable. */
  sidebarGroupsOpacityPercent: number;
  /** @deprecated Retained only so existing settings files remain readable. */
  sidebarProjectsOpacityPercent: number;
  /**
   * CDXC:ProjectHotkeys 2026-06-15-11:12:
   * Jump to Project shortcuts should reveal the target project row when it was collapsed, because the keyboard action is also a navigation intent in the visible Projects sidebar area.
   */
  expandCollapsedProjectsOnJump: boolean;
  /**
   * CDXC:ProjectHotkeys 2026-06-15-11:12:
   * Some users want a project jump to reveal only the target project header plus the configured Show less slice after auto-expanding a collapsed project. Keep that secondary behavior opt-in and only meaningful when auto-expand is enabled.
   */
  showLessForExpandedProjectJumps: boolean;
  sidebarTheme: SidebarThemeSetting;
  /** Theme for chat content only; the surrounding Ghostex chrome stays dark. */
  sessionChatTheme: SessionChatTheme;
  /** CSS font-family used by chat messages and the prompt composer. */
  sessionChatFontFamily: string;
  /** Transcript width on a 64rem scale; 75% preserves the historical 48rem cap. */
  sessionChatTranscriptWidthPercent: number;
  /**
   * Reveal thinking-owned tool calls by default in Session Chat. Chats that
   * use the composer's Verbose pill store their own value and stop following
   * this (sidebar/chat/session-chat-verbose-override.ts).
   */
  sessionChatVerboseMode: boolean;
  /**
   * CDXC:SidebarTitlebarColors 2026-06-15-11:24:
   * Custom chrome colors are scoped to the sidebar and native titlebar only.
   * Keep these separate from theme tokens so modals, dropdowns, and the
   * disabled theme selector keep using Dark Gray/Dark 2 defaults.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-13:22:
   * Settings still carries a foreground field for compatibility with native
   * layout payloads and older stored settings, but normalization derives it
   * from the background instead of preserving user-entered foreground values.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-13:45:
   * Users now tune the custom sidebar/titlebar background through a contrast
   * slider. Keep the background color field as the computed dark protocol
   * value, not as a user-editable setting.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:15:
   * The user-facing Settings control is named Contrast, but this protocol keeps
   * its darkness key so stored settings and native payloads remain compatible.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:28:
   * Tint is stored as a separate web-picker color and folded into the computed
   * background hex. The native/sidebar consumers still receive one final
   * background color, preserving their existing contract.
   *
   * CDXC:SettingsTheming 2026-06-15-21:35:
   * The old custom sidebar/titlebar contrast toggle is retired from Settings.
   * Keep this compatibility field enabled after normalization so visible
   * Theming controls apply without a hidden or experimental gate.
   */
  customSidebarTitlebarColorsEnabled: boolean;
  customSidebarTitlebarForegroundColor: string;
  customSidebarTitlebarBackgroundTintColor: string;
  customSidebarTitlebarBackgroundDarknessPercent: number;
  customSidebarTitlebarBackgroundColor: string;
  terminalCursorStyle: TerminalCursorStyle;
  terminalCursorStyleBlink: boolean;
  terminalEngine: TerminalEngine;
  /**
   * Windows currently runs terminals only through WSL2. The optional
   * distribution override selects an exact initialized distro when automatic
   * discovery cannot choose the intended install.
   */
  windowsTerminalBackend: WindowsTerminalBackend;
  windowsWslDistribution: string;
  terminalFontFamily: string;
  terminalFontSize: number;
  terminalFontWeight: number;
  terminalGhosttyTheme: string;
  terminalBackgroundImage: string;
  terminalBackgroundImageOpacity: number;
  terminalBackgroundImageFit: TerminalBackgroundImageFit;
  terminalLetterSpacing: number;
  terminalLineHeight: number;
  /**
   * CDXC:TerminalPanePadding 2026-06-25-21:27:
   * Terminal pane padding is app layout, not a Ghostty config key. Store
   * separate horizontal and vertical pixel values so Settings can inset every
   * native terminal surface while preserving the pane titlebar, borders,
   * splitters, and Ghostty background color.
   */
  terminalPaneHorizontalPaddingPx: number;
  terminalPaneVerticalPaddingPx: number;
  terminalMouseScrollMultiplierDiscrete: number;
  terminalMouseScrollMultiplierPrecision: number;
  tmuxMode: boolean;
  terminalScrollToBottomWhenTyping: boolean;
  terminalScrollbackLimitMb: number;
  terminalCopyOnSelect: GhosttyCopyOnSelect;
  terminalConfirmCloseSurface: GhosttyConfirmCloseSurface;
  terminalClipboardTrimTrailingSpaces: boolean;
  terminalClipboardPasteProtection: boolean;
  /**
   * CDXC:TerminalImagePaste 2026-06-08-13:32:
   * Terminal image paste is app-owned behavior, not a Ghostty config key. Keep a
   * default-on setting so users can opt out of Cmd+V/Ctrl+V converting clipboard
   * images into previewable Markdown links that also render in Cmd-hover terminal
   * previews and the Ctrl+G Rich Prompt Editor.
   */
  terminalPastePreviewableImages: boolean;
  terminalMouseHideWhileTyping: boolean;
  terminalScrollbar: GhosttyScrollbar;
  /**
   * CDXC:TerminalDevServers 2026-06-23-19:22:
   * Dev-server discovery is app-owned terminal behavior, not a terminal emulator config key. Persist detection, a single open-target choice, and ignored ports with the main settings contract so Terminal settings stay focused on opening in the user's system browser or the internal browser instead of exposing per-browser checkboxes.
   */
  terminalDevServerDetectionEnabled: boolean;
  terminalDevServerOpenTarget: TerminalDevServerOpenTarget;
  terminalDevServerIgnoredPortRules: readonly string[];
  /**
   * CDXC:PortlessSettings 2026-06-22-22:35:
   * Portless is a global app contract, not project state. Keep one default-on toggle and one protocol setting so every project/worktree shares the same local proxy mode without per-project enablement keys.
   */
  portlessEnabled: boolean;
  portlessProtocol: PortlessProtocol;
  promptEditorBackend: PromptEditorBackend;
  customPromptEditorCommand: string;
  richPromptEditingWithGte: boolean;
  useGteForCtrlGPromptEditing: boolean;
  hotkeys: ghostexHotkeySettings;
  workspaceActivePaneBorderColor: string;
  workspaceBackgroundColor: string;
  /**
   * CDXC:SleepingPanePlaceholders 2026-06-13-01:44:
   * Sleeping native pane tabs should select their original split pane without
   * starting Ghostty immediately. Keep click-to-wake enabled by default so
   * users can inspect stable black placeholders and wake only by clicking the
   * pane body.
   */
  clickToWakeSleepingSessions: boolean;
  customWorkspaceOpenTargets: CustomWorkspaceOpenTarget[];
  workspaceOpenTargetAvailability: WorkspaceOpenTargetAvailability;
  workspaceOpenTargetHiddenIds: string[];
  workspacePaneGap: number;
  /**
   * CDXC:RemoteMachines 2026-06-02-23:47:
   * Settings owns the saved Remote machine list and its sidebar section order. Each machine requires a user-visible name and SSH host; live connection state, projects, sessions, and gxserver tokens stay outside settings so reconnect/start/install flows refresh from the remote daemon.
   */
  remoteMachines: RemoteMachineSettings[];
  /**
   * CDXC:CommandsPanel 2026-05-30-10:05:
   * Opening the command pane (F12, sidebar button) and double-clicking its top
   * resize rail must restore this pixel height, clamped to the same 5%-90%
   * workspace limits enforced during drag resize.
   */
  commandsPanelDefaultHeightPx: number;
  /**
   * Where the command pane docks in the desktop workspace: below the active
   * workspace (default) or as a column to its right. Terminal Actions and
   * F12 open the pane on the configured side.
   */
  commandsPanelSide: CommandsPanelSide;
};

export type ghostexSettingsPatch = Partial<ghostexSettings>;

export type ghostexSettingsUpdateSource =
  | "firstLaunch:preferences"
  | "settings:bulk"
  | "settings:control"
  | "settings:navigation"
  | "settings:remoteMachines"
  | "sidebar:remoteMachineOrder"
  /**
   * CDXC:SidebarV2 2026-07-29:
   * The sidebar Sort & Filter menu can switch the sidebar version and its
   * Group by Project sub-mode. Those writes come from the sidebar surface, not
   * from the Settings modal, so they carry their own source and must never be
   * treated as a remote-machine-capable save.
   */
  | "sidebar:sidebarVersion"
  /**
   * CDXC:SidebarV2Worktree 2026-07-29:
   * Sidebar V2's "+" menu can flip the default environment for new sessions.
   * Same reasoning as the version switch above: a sidebar-surface write, never
   * a remote-machine-capable save.
   */
  | "sidebar:newSessionsDefaultEnvMode"
  /**
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * Sidebar V2's project group header can change how one checkout merges with
   * other checkouts of the same repository. Same reasoning as the two sources
   * above: a sidebar-surface write, never a remote-machine-capable save.
   */
  | "sidebar:projectGrouping";

export function canSettingsUpdateSourceChangeRemoteMachines(
  source: ghostexSettingsUpdateSource | undefined,
): boolean {
  /*
   * CDXC:RemoteMachines 2026-06-30-15:18:
   * Remote machine settings must not be rewritten by broad Settings saves such
   * as tab, scroll, preset, or reset updates. Only explicit remote-machine UI
   * and sidebar ordering operations may replace the saved machine list.
   */
  return source === "settings:remoteMachines" || source === "sidebar:remoteMachineOrder";
}

export const SIDEBAR_SETTINGS_PRESET_KEYS = [
  "showProjectIcons",
  "hideSessionAgentIconUntilHover",
  "hideBrowserFaviconUntilHover",
  "showCloseButtonOnSessionCards",
  "hideLastActiveTimeOnSessionCards",
  "hideProjectHeaderDiffStats",
  "showProjectEditorDiffFileCount",
  "hideMenuBarSessionStatusIndicators",
] as const satisfies ReadonlyArray<keyof ghostexSettings>;

export type SidebarSettingsPresetKey = (typeof SIDEBAR_SETTINGS_PRESET_KEYS)[number];
export type SidebarSettingsPresetSettings = Pick<ghostexSettings, SidebarSettingsPresetKey>;

/**
 * CDXC:SidebarSettingsPresets 2026-05-16-10:11:
 * The Settings top row exposes Codex, Minimal, Detailed, and Recommended sidebar UI presets as toggle buttons.
 * Preset state is derived from the controlled sidebar settings instead of persisted separately, so manual deviations show Custom without adding another source of truth.
 *
 * CDXC:SidebarSettingsPresets 2026-06-12-07:10:
 * Superseded by CDXC:SidebarSettingsPresets 2026-06-30-22:29.
 *
 * CDXC:SidebarSettingsPresets 2026-06-13-01:06:
 * Superseded by CDXC:SidebarSettingsPresets 2026-06-30-22:29.
 *
 * CDXC:SidebarSettingsPresets 2026-06-13-15:42:
 * Recommended should keep the sidebar quieter by hiding session-card Last Active timestamps while preserving the rest of the detailed status chrome.
 *
 * CDXC:SessionStatusIndicators 2026-06-15-14:00:
 * Sidebar presets did not control the legacy macOS floating status indicator.
 *
 * CDXC:SessionStatusIndicators 2026-06-27-20:11:
 * The standalone floating status indicator was removed from macOS and GPUI.
 * Presets now tune only sidebar density and the menu-bar indicator; legacy
 * floating keys are normalized separately for old settings files only.
 *
 * CDXC:SidebarSettingsPresets 2026-06-23-08:20:
 * Every sidebar preset must show session-card close buttons on hover. Presets may still tune density, icons, timestamps, project stats, and menu-bar indicators, but they should not remove the primary per-session close affordance.
 *
 * CDXC:SidebarSettingsPresets 2026-06-30-22:29:
 * Recommended should match the user's current preset-controlled sidebar configuration: visible session agent icons, visible browser favicons, close button on hover, hidden Last Active timestamps, visible project git stats, hidden changed-file counts, and visible menu-bar session indicators.
 */
export const SIDEBAR_SETTINGS_PRESET_SETTINGS = {
  codex: {
    showProjectIcons: true,
    hideSessionAgentIconUntilHover: true,
    hideBrowserFaviconUntilHover: false,
    showCloseButtonOnSessionCards: true,
    hideLastActiveTimeOnSessionCards: false,
    hideProjectHeaderDiffStats: true,
    showProjectEditorDiffFileCount: false,
    hideMenuBarSessionStatusIndicators: true,
  },
  minimal: {
    showProjectIcons: false,
    hideSessionAgentIconUntilHover: true,
    hideBrowserFaviconUntilHover: true,
    showCloseButtonOnSessionCards: true,
    hideLastActiveTimeOnSessionCards: true,
    hideProjectHeaderDiffStats: true,
    showProjectEditorDiffFileCount: false,
    hideMenuBarSessionStatusIndicators: true,
  },
  detailed: {
    showProjectIcons: true,
    hideSessionAgentIconUntilHover: false,
    hideBrowserFaviconUntilHover: false,
    showCloseButtonOnSessionCards: true,
    hideLastActiveTimeOnSessionCards: false,
    hideProjectHeaderDiffStats: false,
    showProjectEditorDiffFileCount: false,
    hideMenuBarSessionStatusIndicators: false,
  },
  recommended: {
    showProjectIcons: true,
    hideSessionAgentIconUntilHover: false,
    hideBrowserFaviconUntilHover: false,
    showCloseButtonOnSessionCards: true,
    hideLastActiveTimeOnSessionCards: true,
    hideProjectHeaderDiffStats: false,
    showProjectEditorDiffFileCount: false,
    hideMenuBarSessionStatusIndicators: false,
  },
} as const satisfies Record<SidebarSettingsPresetId, SidebarSettingsPresetSettings>;

export const SIDEBAR_SETTINGS_PRESETS: ReadonlyArray<{
  id: SidebarSettingsPresetId;
  label: string;
  settings: SidebarSettingsPresetSettings;
}> = [
  {
    id: "recommended",
    label: "Recommended",
    settings: SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended,
  },
  { id: "codex", label: "Codex", settings: SIDEBAR_SETTINGS_PRESET_SETTINGS.codex },
  { id: "minimal", label: "Minimal", settings: SIDEBAR_SETTINGS_PRESET_SETTINGS.minimal },
  { id: "detailed", label: "Detailed", settings: SIDEBAR_SETTINGS_PRESET_SETTINGS.detailed },
];

export const DEFAULT_ghostex_SETTINGS: ghostexSettings = {
  /**
   * CDXC:CompletionSounds 2026-05-29-12:00:
   * Action-completion feedback should use the plain shamisen sound by default;
   * shamisen reverb remains available from Settings for users who prefer it.
   */
  actionCompletionSound: "shamisen",
  gpuiTitlebarActionCommandByProject: {},
  gpuiTitlebarOpenTargetByProject: {},
  /**
   * CDXC:AppShots 2026-06-13-19:51:
   * App Shots are a beta workflow and should be opt-in for first-run Settings
   * defaults and missing persisted settings. Keep the hotkey configured so
   * enabling the beta feature is a single explicit toggle.
   */
  appShotsEnabled: false,
  appShotsHotkey: "both-command",
  /*
   * CDXC:AppShots 2026-06-29-02:59:
   * App Shot prompts should paste only the image link by default. Window metadata is useful for debugging and context-heavy cases, but it must be an explicit Settings opt-in so routine image prompts stay compact.
   */
  appShotsMetadataEnabled: false,
  /**
   * CDXC:GxserverAgentSettings 2026-06-02-22:23:
   * New installs should start with gxserver-owned Accept All enabled so built-in
   * and custom agent launches inherit permission-bypass mode unless the user
   * turns it off.
   */
  agentAcceptAllEnabled: true,
  agentManagerZoomPercent: DEFAULT_AGENT_MANAGER_ZOOM_PERCENT,
  defaultPromptAgentId: "codex",
  sessionTitleGenerationAgent: "codex",
  customSessionTitleGenerationCommand: "",
  /**
   * Browser feedback always uses Agentation. Keep the normalized field for
   * compatibility with older host payloads, but do not expose it in Settings.
   */
  browserFeedbackTool: "agentation",
  /**
   * CDXC:BrowserPanes 2026-05-27-07:24
   * Browser actions should no longer expose or route through Chrome Canary attachment.
   * Normalize all browser-action launches to in-workspace browser panes so Settings and native startup do not preserve the old external Canary path.
   */
  browserOpenMode: "browser-pane",
  /**
   * CDXC:TerminalLinkInAppBrowser 2026-07-02-13:05:
   * In-app terminal link routing is the default so cmd-clicked web links land
   * in the project Browser view unless the user opts back into the system
   * browser in Settings.
   */
  openTerminalLinksInApp: true,
  /**
   * CDXC:SettingsAdvanced 2026-06-28-08:01:
   * New installs should start with ordinary Settings density, but an explicit
   * Show Advanced toggle is saved so restart hydration preserves the user's
   * last browsing mode.
   */
  showAdvancedSettings: false,
  /**
   * CDXC:SettingsNavigation 2026-06-29-17:54:
   * New installs start at General Settings. Once the user closes Settings, the
   * modal saves only navigation chrome state here, not search text or private
   * setting values beyond the already persisted preferences.
   *
   * CDXC:SettingsNavigation 2026-06-30-04:47:
   * Navigation writes can happen before close so native-window teardown does not
   * lose the last selected Settings page.
   */
  settingsModalNavigation: DEFAULT_SETTINGS_MODAL_NAVIGATION_STATE,
  /**
   * CDXC:ExperimentalFeatures 2026-06-28-07:41:
   * New installs and missing persisted settings should keep experimental
   * surfaces hidden until the user enables Enable Experimental Features from
   * Advanced Settings.
   *
   * CDXC:Automations 2026-07-01-03:24:
   * Automations Overview and project Automate start hidden behind their
   * coming-soon overlay until Enable Experimental Features is on.
   */
  showBetaFeatures: false,
  codeViewTabHidden: false,
  browserViewTabHidden: false,
  kanbanViewTabHidden: false,
  automateViewTabHidden: false,
  docsViewTabHidden: false,
  tipsAndTricksTitlebarButtonHidden: false,
  resourcesTitlebarButtonHidden: false,
  gitActionsTitlebarButtonHidden: false,
  quickActionsTitlebarButtonHidden: false,
  openInTitlebarButtonHidden: false,
  /**
   * CDXC:EditorPanes 2026-05-06-15:00
   * Embedded code-server editor panes can reuse the user's local VS Code
   * user settings. A separate Insiders toggle switches the linked source
   * directory without disabling the shared project editor runtime.
   *
   * CDXC:EditorPanes 2026-06-08-20:12:
   * New installs should use Ghostex-owned bundled editor settings by default
   * so the embedded VS Code surface starts on Dark 2026. Users can still opt
   * into local VS Code settings explicitly from Settings.
   */
  codeServerLinkVscodeUserConfig: false,
  codeServerUseVscodeInsidersUserConfig: false,
  /**
   * Legacy external-IDE preference keys remain normalized so existing settings
   * files and generic Open in IDE actions stay readable. They are no longer
   * exposed in Settings or used by Agents Hub, which opens files in Source.
   */
  customDefaultEditorCommand: "",
  // CDXC:AppIconPicker 2026-06-25-21:50: New installs use the default bundled app icon (empty source id).
  appIconSourceId: "",
  defaultEditorCommand: "code",
  /**
   * CDXC:ProjectDiffStats 2026-05-16-08:46:
   * Users can hide the project-header +added/-removed git summary completely
   * when they want project names to stay visually quiet. This is independent
   * from the existing changed-file count preference.
   *
   * CDXC:SidebarSettingsPresets 2026-06-13-01:06:
   * Recommended is the default sidebar preset, so new settings show project-header
   * git stats while keeping the changed-file count off unless the user enables it.
   */
  hideProjectHeaderDiffStats:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideProjectHeaderDiffStats,
  /**
   * CDXC:DocsSidebar 2026-06-30-19:47:
   * Additional Docs scan folders remain opt-in beyond the built-in ./docs,
   * ./artifacts, and ./ai roots plus root Markdown, HTML, and Excalidraw files.
   * A configured Docs directory adds its own tree on top of whatever this
   * lists (CDXC:DocsRootAdditive).
   */
  manageAdditionalDocsFolders: "",
  /**
   * CDXC:GlobalProjectDefaults 2026-08-02:
   * New installs ship every Global Default empty so project resolution stays
   * byte-for-byte identical to the pre-feature behavior until a user fills one in.
   */
  globalWorktreeCommand: "",
  globalBeadsDisplayKey: "",
  globalBeadsDirectory: "",
  globalDocsDirectory: "",
  /**
   * CDXC:ProjectDiffStats 2026-05-15-14:33:
   * Project-header git stats should hide the changed-file count by default and
   * show only added/removed line counts. Users can opt back into the file
   * number from Settings when they want the full diff summary.
   */
  showProjectEditorDiffFileCount:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.codex.showProjectEditorDiffFileCount,
  /**
   * CDXC:ProjectDiffStats 2026-05-27-09:25:
   * Match Starship-style tracked line counts by default. Users can opt in to
   * show untracked line totals only when tracked `git diff --numstat HEAD` is
   * +0 -0.
   */
  showUntrackedProjectDiffWhenNoTrackedChanges: false,
  /**
   * CDXC:CompletionSounds 2026-05-29-12:00:
   * The completion bell should be enabled by default so finished agent work is
   * audible without requiring users to discover the Sounds setting first.
   */
  completionBellEnabled: true,
  completionSound: DEFAULT_COMPLETION_SOUND,
  /**
   * CDXC:TerminalBellAttention 2026-07-01-01:13:
   * Plain terminal BEL events include ordinary shell feedback such as zsh
   * completion misses. Keep terminal-bell attention notifications opt-in so
   * Monaco prompt editing and agent completion alerts remain independent from
   * noisy terminal-emulator bells.
   */
  showNotificationOnTerminalBell: false,
  createSessionOnSidebarDoubleClick: false,
  debuggingMode: false,
  diagnosticLogging: {
    scenarios: DEFAULT_DIAGNOSTIC_LOGGING_SCENARIOS,
    version: 1,
  },
  renameSessionOnDoubleClick: false,
  showProjectIcons: SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.showProjectIcons,
  /**
   * CDXC:SidebarSessions 2026-05-16-08:46:
   * Agent identity remains configurable in Settings through an explicit
   * hover-only mode for quieter session lists.
   *
   * CDXC:SidebarSettingsPresets 2026-06-13-01:06:
   * Superseded by CDXC:SidebarSettingsPresets 2026-06-30-22:29.
   *
   * CDXC:SidebarSettingsPresets 2026-06-30-22:29:
   * Recommended is the first-run preset and keeps session agent icons visible
   * while showing detailed sidebar status chrome.
   */
  hideSessionAgentIconUntilHover:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideSessionAgentIconUntilHover,
  /**
   * CDXC:SidebarSessionAgentIcons 2026-06-29-23:58:
   * Preserve the existing monochrome sidebar until users opt into colored
   * session agent logos from Session Cards settings.
   *
   * CDXC:SidebarSessionAgentIcons 2026-06-30-22:40:
   * The same opt-in colors the selected agent launcher icon in project and
   * Quick headers, so the visible picker identity matches the session-card
   * agent-logo mode.
   */
  useColoredSessionAgentIcons: false,
  /**
   * CDXC:BrowserPanes 2026-05-28-07:38:
   * Browser page favicons are page identity, not agent chrome. Keep them
   * visible in the default Codex and Detailed presets even when agent icons are
   * hover-only, while Minimal can hide favicons until hover for a quieter list.
   */
  hideBrowserFaviconUntilHover:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideBrowserFaviconUntilHover,
  /**
   * CDXC:SidebarSessions 2026-05-09-17:00
   * Session-card close controls should be available out of the box. Users can
   * still turn the hover chrome off from Settings when they want quieter cards.
   */
  showCloseButtonOnSessionCards:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.showCloseButtonOnSessionCards,
  /**
   * CDXC:SidebarSessions 2026-06-13-15:42
   * Recommended is the default sidebar style and hides session-card Last Active
   * timestamps by default. Settings still owns an explicit toggle for users who
   * want the timestamp back, and the setting must not affect project-header git
   * diff stats.
   */
  hideLastActiveTimeOnSessionCards:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideLastActiveTimeOnSessionCards,
  showSessionCloseContextMenuAction: false,
  showSessionCommandCopyActions: false,
  showSessionDetailsCopyAction: false,
  /**
   * CDXC:SessionTagFilters 2026-06-13-17:50:
   * First-run sidebar tag filter settings should show the default triage tags,
   * the No tag filter, and the default separators. Users opt out by hiding or
   * disabling individual rows from the collapsed Sidebar Tags settings area.
   */
  sidebarSessionTagListItems: DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS,
  /**
   * CDXC:AutoSleep 2026-05-28-08:06:
   * Background VS Code, Project, and Git panes originally auto-slept after
   * fifteen minutes of idle time by default. Agent terminal auto-sleep starts
   * opt-in because it closes live user-created conversation surfaces.
   *
   * CDXC:AutoSleep 2026-06-15-18:31:
   * Heavy editor, Project, Git/Browser, and browser-session surfaces should
   * retire quickly by default because many awake webviews and code-server
   * processes make sidebar switching laggy. Use a five-minute idle window and
   * enable browser-session Auto Sleep while keeping agent terminals opt-in.
   *
   * CDXC:AutoSleep 2026-06-07-00:53:
   * Agent auto-sleep keeps its opt-in policy, but the default idle threshold is
   * now fifteen minutes so enabled agent sessions retire on the same window as
   * editor surfaces.
   *
   * CDXC:AutoSleep 2026-06-07-00:56:
   * Focused agent sessions must never auto-sleep and no longer have a Settings
   * override because sleeping the active conversation is not a supported UX.
   */
  autoSleepAgentSessionsEnabled: false,
  autoSleepAgentIdleMinutes: 15,
  autoSleepBrowserSessionsEnabled: true,
  autoSleepBrowserIdleMinutes: 5,
  autoSleepCodeEditorEnabled: true,
  autoSleepCodeEditorIdleMinutes: 5,
  autoSleepGitEditorEnabled: true,
  autoSleepGitEditorIdleMinutes: 5,
  autoSleepProjectEditorEnabled: true,
  autoSleepProjectEditorIdleMinutes: 5,
  autoSleepRequireAgentResumeCommand: true,
  autoSleepFavoriteAgentSessions: false,
  keepAwakeActivateOnExternalDisplay: false,
  keepAwakeActivateOnLaunch: false,
  keepAwakeAllowDisplaySleep: false,
  keepAwakeBatteryThresholdPercent: 20,
  keepAwakeDeactivateBelowBatteryThreshold: false,
  keepAwakeDeactivateOnLowPowerMode: false,
  keepAwakeDeactivateOnUserSwitch: false,
  keepAwakeDefaultDurationMinutes: 0,
  keepAwakeWhileWorkingSessions: false,
  /**
   * CDXC:TitlebarKeepAwake 2026-05-28-19:28:
   * Closing a MacBook lid is not covered by the standard caffeinate idle-sleep assertion.
   * Keep lid-close sleep prevention as an explicit opt-in because it changes the system-wide `pmset disablesleep` policy with administrator approval.
   */
  keepAwakePreventLidSleep: false,
  /**
   * CDXC:TitlebarKeepAwake 2026-05-27-07:32:
   * The titlebar keep-awake affordance is optional chrome. Keep the per-control
   * hide preference off by default, but persist a Power setting that can remove
   * the titlebar control completely for users who do not use Mac sleep
   * management from Ghostex.
   *
   * CDXC:ExperimentalFeatures 2026-06-28-07:41:
   * Keep Awake is an experimental macOS feature. Enable Experimental Features
   * must be enabled before the titlebar button or runtime automation is
   * available; this preference only hides the button again inside that enabled
   * state.
   */
  hideKeepAwakeTitlebarControl: false,
  /**
   * CDXC:GlobalActions 2026-08-01:
   * Every built-in tab strip button stays visible until the user hides it, so
   * adding Global Actions never silently removes a control someone relies on.
   */
  hideTabStripNewTerminalButton: false,
  hideTabStripNewChatButton: false,
  hideTabStripNewBrowserButton: false,
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
   * Floating and menu bar desktop status badges stay independently controlled.
   *
   * CDXC:SessionStatusIndicators 2026-06-15-02:01:
   * Floating session indicators previously started hidden for new installs, while the menu bar session indicator stayed visible unless that separate setting changed.
   *
   * CDXC:SessionStatusIndicators 2026-06-15-14:00:
   * Sidebar presets did not provide the legacy floating indicator value.
   *
   * CDXC:SessionStatusIndicators 2026-06-16-09:20:
   * The legacy floating indicator defaulted on before the surface was removed.
   *
   * CDXC:SessionStatusIndicators 2026-06-27-20:11:
   * The floating badge surface and Settings rows were removed from macOS and
   * GPUI. Keep this legacy key normalized so existing settings JSON remains
   * readable, but no current native or GPUI presentation should consume it.
   */
  hideFloatingSessionStatusIndicators: false,
  hideMenuBarSessionStatusIndicators:
    SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended.hideMenuBarSessionStatusIndicators,
  petOverlayEnabled: false,
  selectedPetId: DEFAULT_PET_ID,
  /**
   * CDXC:SessionStatusIndicators 2026-05-07-18:20
   * The legacy AppKit floating session indicator size was persisted as a named
   * value rather than raw pixels.
   *
   * CDXC:SessionStatusIndicators 2026-06-27-20:11:
   * Floating size is retained only as a legacy normalized value after removing
   * the floating badge from macOS and GPUI. Do not expose it in Settings or
   * send it through native/GPUI status payloads.
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
   * The session-id pane overlay preference is configurable, and the
   * native label itself must still render only for terminal panes that carry
   * zmx/tmux/zellij persistence metadata.
   *
   * CDXC:SessionPersistence 2026-06-06-05:47:
   * Provider session ids in terminal panes are opt-in chrome. Keep the setting
   * disabled for default settings so new users do not see top-right provider
   * identifiers unless they explicitly enable the pane overlay.
   */
  showSessionIdInTerminalPanes: false,
  preferredAgentInterface: "terminal",
  /**
   * CDXC:SidebarV2 2026-07-29:
   * The classic sidebar stays the default for every user. Sidebar V2 must be
   * chosen explicitly, so a settings file without this key keeps V1 behavior.
   */
  sidebarVersion: "v1",
  /**
   * CDXC:SidebarV2 2026-07-29:
   * Sidebar V2 opens as one flat, position-stable inbox. Group by Project is
   * the opt-in sub-mode.
   */
  sidebarV2Layout: "flat",
  /**
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * Three days is the agreed default window, and it matches
   * `DEFAULT_AUTO_SETTLE_AFTER_DAYS` in gxserver-rs so a settings file that has
   * never been written and one that carries the default behave identically.
   */
  sidebarAutoSettleAfterDays: 3,
  /**
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * No overrides: every project follows the automatic origin-remote rule, which
   * is a no-op for anyone with a single machine.
   */
  sidebarProjectGroupingOverrides: {},
  /**
   * CDXC:SidebarV2Worktree 2026-07-29:
   * The plain "+" keeps its historic meaning until the user asks otherwise, so
   * an untouched settings file starts instant local sessions exactly as today.
   */
  newSessionsDefaultEnvMode: "local",
  /**
   * CDXC:SidebarPlacement 2026-05-06-17:32
   * Sidebar side is a first-class setting so users can choose left or right
   * placement from Settings instead of relying on sidebar placement shortcuts.
   *
   * CDXC:SidebarCollapse 2026-06-12-02:23:
   * Cmd+B is reserved for complete sidebar collapse, so sidebar side placement
   * should remain an explicit setting or user-assigned command.
   */
  sidebarSide: "left",
  sidebarCollapseAnimationDurationMs: DEFAULT_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
  /**
   * CDXC:SidebarChrome 2026-06-05-04:40:
   * First-run reset target remains 235px, but users can change this Settings
   * value for explicit sidebar-handle double-click resets without changing the
   * last-width restore path used at app restart.
   */
  sidebarDefaultWidthPx: DEFAULT_SIDEBAR_DEFAULT_WIDTH_PX,
  projectSessionListCollapsedCount: DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  sidebarProjectGroupStyle: "branched",
  sidebarGroupsOpacityPercent: 0,
  sidebarProjectsOpacityPercent: 0,
  expandCollapsedProjectsOnJump: true,
  showLessForExpandedProjectJumps: false,
  /**
   * CDXC:SidebarTheme 2026-06-15-02:29:
   * Theme selection is disabled again until the full theme system is ready.
   * Use Dark 2 as the active app theme and present it to users as Dark Gray.
   */
  sidebarTheme: "dark-2",
  sessionChatTheme: "dark",
  sessionChatFontFamily: "",
  sessionChatTranscriptWidthPercent: DEFAULT_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
  sessionChatVerboseMode: false,
  /**
   * CDXC:SidebarTitlebarColors 2026-06-15-11:24:
   * Custom sidebar/titlebar colors are scoped to the sidebar and titlebar.
   * The default background matches Dark Gray chrome without changing modal or
   * dropdown color tokens.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-13:22:
   * Foreground is derived from background luminance, so the default foreground
   * remains light for Dark Gray and flips to the dark foreground on light
   * custom backgrounds.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-13:45:
   * The custom background contrast slider defaults near Dark Gray and is
   * restricted to dark applied values to avoid arbitrary bright color blends
   * in sidebar rows.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:01:
   * Clamp the slider to 85-100 per visual review; lighter values made the
   * sidebar feel too gray.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:15:
   * Keep this persisted field named darkness for compatibility while Settings
   * labels the same control Background Contrast.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-15:28:
   * The tint picker originally defaulted to neutral #808080. The tint
   * algorithm now maps picker colors to very dark chrome backgrounds, so
   * neutral same-channel tints do not change Dark Gray chrome.
   *
   * CDXC:SidebarTitlebarColors 2026-06-16-14:28:
   * The custom chrome default is now 95 contrast with white #FFFFFF tint.
   * Store the computed default background with those controls so Settings,
   * native startup, and protocol snapshots agree.
   *
   * CDXC:SidebarTitlebarColors 2026-07-22:
   * Default app chrome to neutral #808080 at 90 Background Contrast,
   * resolving to #1c1c1c.
   *
   * CDXC:SettingsTheming 2026-06-15-21:35:
   * Background Contrast and Background Tint are standard Theming controls.
   * Enable the retained protocol field by default so the removed toggle cannot
   * make those visible controls inert.
   */
  customSidebarTitlebarColorsEnabled: true,
  customSidebarTitlebarForegroundColor: DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_FOREGROUND_COLOR,
  customSidebarTitlebarBackgroundTintColor:
    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR,
  customSidebarTitlebarBackgroundDarknessPercent:
    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
  customSidebarTitlebarBackgroundColor: getSidebarTitlebarBackgroundForDarkness(
    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR,
  ),
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
  windowsTerminalBackend: "wsl",
  windowsWslDistribution: "",
  terminalFontFamily: "JetBrains Mono",
  terminalFontSize: 13,
  terminalFontWeight: 300,
  terminalGhosttyTheme: "GitHub Dark",
  terminalBackgroundImage: "",
  terminalBackgroundImageOpacity: 1,
  terminalBackgroundImageFit: "cover",
  terminalLetterSpacing: 0,
  terminalLineHeight: 1.2,
  terminalPaneHorizontalPaddingPx: DEFAULT_TERMINAL_PANE_PADDING_PX,
  terminalPaneVerticalPaddingPx: DEFAULT_TERMINAL_PANE_PADDING_PX,
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
  terminalConfirmCloseSurface: "false",
  terminalClipboardTrimTrailingSpaces: true,
  terminalClipboardPasteProtection: true,
  terminalPastePreviewableImages: true,
  terminalMouseHideWhileTyping: false,
  terminalScrollbar: "system",
  /**
   * CDXC:TerminalDevServers 2026-06-23-19:22:
   * New installs should discover local dev servers from terminal output, open detected URLs with the user's system default browser unless changed to the internal browser, and start with no ignored ports.
   */
  terminalDevServerDetectionEnabled: true,
  terminalDevServerOpenTarget: DEFAULT_TERMINAL_DEV_SERVER_OPEN_TARGET,
  terminalDevServerIgnoredPortRules: DEFAULT_TERMINAL_DEV_SERVER_IGNORED_PORT_RULES,
  /**
   * CDXC:PortlessSettingsDisabled 2026-07-25:
   * Keep the Portless settings contract available for a later return, but new
   * and legacy settings snapshots must not opt into an app integration that is
   * currently hidden and disabled.
   */
  portlessEnabled: false,
  portlessProtocol: "https",
  /**
   * CDXC:PromptEditorBackend 2026-05-13-15:58
   * Ctrl+G rich prompt editing originally defaulted to the floating Monaco editor.
   *
   * CDXC:PromptEditorBackend 2026-05-25-11:31:
   * Monaco is the out-of-the-box Ctrl+G prompt editor again. New installs should open the floating Monaco editor for local app terminals.
   *
   * CDXC:PromptEditorBackend 2026-06-30-00:08:
   * Settings must expose only Monaco and the user's machine default editor. Removed gte and custom selections migrate to inherit so Ctrl+G stops injecting a Ghostex-owned editor command when users choose the machine default path.
   */
  promptEditorBackend: "monaco",
  customPromptEditorCommand: "code --wait",
  /**
   * CDXC:GtePromptEditing 2026-05-22-09:56
   * The boolean mirrors keep legacy Ctrl+G prompt-editor settings readable while promptEditorBackend remains the source of truth for launch behavior.
   *
   * CDXC:PromptEditorBackend 2026-06-30-00:08:
   * The Settings UI no longer offers gte. New normalized settings keep these legacy mirrors false so removed gte choices cannot keep exporting a gte editor command.
   */
  richPromptEditingWithGte: false,
  useGteForCtrlGPromptEditing: false,
  hotkeys: DEFAULT_ghostex_HOTKEYS,
  workspaceActivePaneBorderColor: "#3b82f6",
  /**
   * CDXC:WorkspaceLayout 2026-06-07-16:53:
   * Black is the fallback workspace background when Ghostty has no readable terminal background. Native layout sync treats this default as automatic so the macOS workarea can use the loaded Ghostty `background` color instead of forcing a separate app gray.
   */
  workspaceBackgroundColor: "#000000",
  clickToWakeSleepingSessions: true,
  /**
   * CDXC:TitlebarOpenIn 2026-05-11-00:22
   * The titlebar Open In menu is configurable: built-in editor targets can be
   * hidden and user-defined command targets can be appended without changing
   * the default editor catalog.
   */
  customWorkspaceOpenTargets: [],
  /**
   * CDXC:TitlebarOpenIn 2026-05-11-02:03
   * First launch starts with only ghostex/Open Folder until the native sidebar performs
   * its one startup installed-target scan and persists the detected IDE list.
   *
   * CDXC:TitlebarOpenIn 2026-06-04-13:39:
   * The default folder target should be described with OS-agnostic Open Folder copy even though the persisted target id remains finder for compatibility.
   */
  workspaceOpenTargetAvailability: DEFAULT_WORKSPACE_OPEN_TARGET_AVAILABILITY,
  workspaceOpenTargetHiddenIds: [],
  /**
   * CDXC:WorkspaceLayout 2026-05-30-07:24:
   * The macOS app no longer exposes Pane Gap as a user setting. Keep the
   * persisted field for settings compatibility, but normalize it to zero so
   * native panes always render without configurable spacing.
   */
  workspacePaneGap: 0,
  remoteMachines: [],
  commandsPanelDefaultHeightPx: DEFAULT_COMMANDS_PANEL_HEIGHT_PX,
  commandsPanelSide: "bottom",
};

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
  { label: "Dark Gray", value: "dark-2" },
];

export const SESSION_CHAT_THEME_OPTIONS: ReadonlyArray<{
  label: string;
  value: SessionChatTheme;
}> = [
  { label: "Light", value: "light" },
  { label: "Dark", value: "dark" },
];

export const TERMINAL_ENGINE_SETTING_OPTIONS: ReadonlyArray<{
  label: string;
  value: TerminalEngine;
}> = [{ label: "Ghostty Native", value: "ghostty-native" }];

export const WINDOWS_TERMINAL_BACKEND_OPTIONS: ReadonlyArray<{
  label: string;
  value: WindowsTerminalBackend;
}> = [{ label: "WSL2", value: "wsl" }];

export const BROWSER_OPEN_MODE_OPTIONS: ReadonlyArray<{
  label: string;
  value: BrowserOpenMode;
}> = [{ label: "Browser Panes", value: "browser-pane" }];

export const APP_SHOTS_HOTKEY_OPTIONS: ReadonlyArray<{
  label: string;
  value: AppShotsHotkey;
}> = [
  { label: "Both Command keys", value: "both-command" },
  { label: "Both Shift keys", value: "both-shift" },
  { label: "Both Option keys", value: "both-option" },
  { label: "Double-tap Left Shift", value: "double-left-shift" },
  { label: "Double-tap Left Option", value: "double-left-option" },
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

export const COMMANDS_PANEL_SIDE_OPTIONS: ReadonlyArray<{
  label: string;
  value: CommandsPanelSide;
}> = [
  { label: "Bottom", value: "bottom" },
  { label: "Right", value: "right" },
];

export const SIDEBAR_PROJECT_GROUP_STYLE_OPTIONS: ReadonlyArray<{
  label: string;
  value: SidebarProjectGroupStyle;
}> = [
  { label: "Quiet rail", value: "quiet" },
  { label: "Header rail", value: "header" },
  { label: "Branched rail", value: "branched" },
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
  { label: "Standard", value: "v1" },
  { label: "Inbox (Beta)", value: "v2" },
];

export const PREFERRED_AGENT_INTERFACE_OPTIONS: ReadonlyArray<{
  label: string;
  value: PreferredAgentInterface;
}> = [
  { label: "Terminal", value: "terminal" },
  { label: "Chat", value: "chat" },
];

export const SIDEBAR_V2_LAYOUT_OPTIONS: ReadonlyArray<{
  label: string;
  value: SidebarV2Layout;
}> = [
  { label: "Flat inbox", value: "flat" },
  { label: "Group by project", value: "byProject" },
];

/**
 * CDXC:SidebarV2Lifecycle 2026-07-29:
 * Auto-settle windows offered in Settings. Presets rather than a free number
 * field because the useful values are few and the wrong one is expensive: a
 * mistyped "0.5" would sweep a whole inbox onto the settled shelf overnight.
 * `SIDEBAR_AUTO_SETTLE_OFF_VALUE` is the select's stand-in for `null`.
 */
export const SIDEBAR_AUTO_SETTLE_OFF_VALUE = "off";

export const SIDEBAR_AUTO_SETTLE_AFTER_DAYS_OPTIONS: ReadonlyArray<{
  label: string;
  value: string;
}> = [
  { label: "After 1 day", value: "1" },
  { label: "After 3 days", value: "3" },
  { label: "After 7 days", value: "7" },
  { label: "After 14 days", value: "14" },
  { label: "After 30 days", value: "30" },
  { label: "Off", value: SIDEBAR_AUTO_SETTLE_OFF_VALUE },
];

/** Select value for the current setting. An unlisted custom window (hand-edited
    settings file) falls back to Off rather than silently showing a preset it is
    not using. */
export function sidebarAutoSettleAfterDaysSelectValue(
  value: number | null,
): string {
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
  { label: "X-Large", value: "x-large" },
  { label: "Large", value: "large" },
  { label: "Medium", value: "medium" },
  { label: "Small", value: "small" },
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
  { label: "Until turned off", value: 0 },
  { label: "2 hours", value: 120 },
  { label: "5 hours", value: 300 },
];

export const AUTO_SLEEP_IDLE_MINUTE_OPTIONS: ReadonlyArray<{
  label: string;
  value: AutoSleepIdleMinutes;
}> = [
  { label: "5 minutes", value: 5 },
  { label: "10 minutes", value: 10 },
  { label: "15 minutes", value: 15 },
  { label: "30 minutes", value: 30 },
  { label: "1 hour", value: 60 },
  { label: "2 hours", value: 120 },
  { label: "5 hours", value: 300 },
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
  /**
   * CDXC:PromptEditorBackend 2026-06-30-00:08:
   * Ctrl+G Settings should be a two-choice dropdown: use the bundled Monaco prompt editor or leave $EDITOR/$VISUAL to the user's machine defaults. gte install/use and custom command controls are intentionally absent.
   */
  { label: "Monaco editor", value: "monaco" },
  { label: "Use default from this machine", value: "inherit" },
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

export function isDiagnosticLoggingScenarioEnabled(
  diagnosticLogging: DiagnosticLoggingSettings | undefined,
  scenarioId: DiagnosticLoggingScenarioId,
  now: Date = new Date(),
): boolean {
  const scenario = diagnosticLogging?.scenarios[scenarioId];
  if (!scenario?.enabled) {
    return false;
  }
  if (!scenario.expiresAt) {
    return true;
  }
  const expiresAtMs = Date.parse(scenario.expiresAt);
  return Number.isFinite(expiresAtMs) && expiresAtMs > now.getTime();
}

export function setDiagnosticLoggingScenario(
  diagnosticLogging: DiagnosticLoggingSettings,
  scenarioId: DiagnosticLoggingScenarioId,
  state: DiagnosticLoggingScenarioState | undefined,
): DiagnosticLoggingSettings {
  const scenarios = { ...diagnosticLogging.scenarios };
  const normalizedState = normalizeDiagnosticLoggingScenarioState(state);
  if (normalizedState) {
    scenarios[scenarioId] = normalizedState;
  } else {
    delete scenarios[scenarioId];
  }
  return normalizeDiagnosticLoggingSettings({
    scenarios,
    version: 1,
  });
}

export function areDiagnosticLoggingSettingsEqual(
  lhs: DiagnosticLoggingSettings,
  rhs: DiagnosticLoggingSettings,
): boolean {
  return JSON.stringify(normalizeDiagnosticLoggingSettings(lhs)) ===
    JSON.stringify(normalizeDiagnosticLoggingSettings(rhs));
}

function normalizeTitlebarProjectSelectionMap(candidate: unknown): Record<string, string> {
  if (!isRecord(candidate)) {
    return {};
  }
  const normalized: Record<string, string> = {};
  for (const [rawProjectId, rawSelection] of Object.entries(candidate).slice(0, 256)) {
    const projectId = rawProjectId.trim();
    const selection = typeof rawSelection === "string" ? rawSelection.trim() : "";
    if (
      projectId.length === 0 ||
      projectId.length > 512 ||
      selection.length === 0 ||
      selection.length > 512
    ) {
      continue;
    }
    normalized[projectId] = selection;
  }
  return normalized;
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
  const terminalDevServerOpenTarget = normalizeTerminalDevServerOpenTarget(
    source.terminalDevServerOpenTarget,
    source.terminalDevServerDefaultBrowserId,
  );
  const rawLegacyCustomSidebarTitlebarBackgroundColor =
    source.customSidebarTitlebarBackgroundColor;
  const hasValidLegacyCustomSidebarTitlebarBackgroundColor =
    typeof rawLegacyCustomSidebarTitlebarBackgroundColor === "string" &&
    /^#[0-9a-f]{6}$/u.test(rawLegacyCustomSidebarTitlebarBackgroundColor.trim().toLowerCase());
  const legacyCustomSidebarTitlebarBackgroundColor = normalizeSidebarTitlebarHexColor(
    readString(
      source,
      "customSidebarTitlebarBackgroundColor",
      DEFAULT_ghostex_SETTINGS.customSidebarTitlebarBackgroundColor,
    ),
    DEFAULT_ghostex_SETTINGS.customSidebarTitlebarBackgroundColor,
  );
  /**
   * CDXC:SidebarTitlebarColors 2026-06-16-14:28:
   * Missing Settings should use the explicit 95 contrast default instead of
   * reverse-mapping the default background hex, because that reverse mapping
   * cannot exactly invert the slider's channel curve. Only valid legacy saved
   * background colors should continue to seed the slider during migration.
   */
  const customSidebarTitlebarBackgroundDarknessFallback =
    hasValidLegacyCustomSidebarTitlebarBackgroundColor
      ? getSidebarTitlebarBackgroundDarknessForColor(legacyCustomSidebarTitlebarBackgroundColor)
      : DEFAULT_ghostex_SETTINGS.customSidebarTitlebarBackgroundDarknessPercent;
  const customSidebarTitlebarBackgroundDarknessPercent =
    clampSidebarTitlebarBackgroundDarknessPercent(
      readNumber(
        source,
        "customSidebarTitlebarBackgroundDarknessPercent",
        customSidebarTitlebarBackgroundDarknessFallback,
      ),
    );
  const customSidebarTitlebarBackgroundTintColor = normalizeSidebarTitlebarHexColor(
    readString(
      source,
      "customSidebarTitlebarBackgroundTintColor",
      DEFAULT_ghostex_SETTINGS.customSidebarTitlebarBackgroundTintColor,
    ),
    DEFAULT_ghostex_SETTINGS.customSidebarTitlebarBackgroundTintColor,
  );
  const customSidebarTitlebarBackgroundColor = getSidebarTitlebarBackgroundForDarkness(
    customSidebarTitlebarBackgroundDarknessPercent,
    customSidebarTitlebarBackgroundTintColor,
  );
  return {
    actionCompletionSound: clampCompletionSoundSetting(
      readString(source, "actionCompletionSound", DEFAULT_ghostex_SETTINGS.actionCompletionSound),
    ),
    gpuiTitlebarActionCommandByProject: normalizeTitlebarProjectSelectionMap(
      source.gpuiTitlebarActionCommandByProject,
    ),
    gpuiTitlebarOpenTargetByProject: normalizeTitlebarProjectSelectionMap(
      source.gpuiTitlebarOpenTargetByProject,
    ),
    appShotsEnabled: readBoolean(
      source,
      "appShotsEnabled",
      DEFAULT_ghostex_SETTINGS.appShotsEnabled,
    ),
    appShotsHotkey: normalizeAppShotsHotkey(
      readString(source, "appShotsHotkey", DEFAULT_ghostex_SETTINGS.appShotsHotkey),
    ),
    appShotsMetadataEnabled: readBoolean(
      source,
      "appShotsMetadataEnabled",
      DEFAULT_ghostex_SETTINGS.appShotsMetadataEnabled,
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
     * CDXC:PromptAgents 2026-05-28-07:15:
     * Keep the selected default prompt agent as a plain agent id so built-in,
     * reordered, hidden-restored, and custom agents can all be selected without
     * coupling settings normalization to the runtime agent registry.
     */
    defaultPromptAgentId: normalizeDefaultPromptAgentId(
      readString(source, "defaultPromptAgentId", DEFAULT_ghostex_SETTINGS.defaultPromptAgentId),
    ),
    sessionTitleGenerationAgent: normalizeSessionTitleGenerationAgent(
      readString(
        source,
        "sessionTitleGenerationAgent",
        DEFAULT_ghostex_SETTINGS.sessionTitleGenerationAgent,
      ),
    ),
    customSessionTitleGenerationCommand: normalizeCustomSessionTitleGenerationCommand(
      readString(
        source,
        "customSessionTitleGenerationCommand",
        DEFAULT_ghostex_SETTINGS.customSessionTitleGenerationCommand,
      ),
    ),
    openTerminalLinksInApp: readBoolean(
      source,
      "openTerminalLinksInApp",
      DEFAULT_ghostex_SETTINGS.openTerminalLinksInApp,
    ),
    /** Normalize legacy feedback-tool settings to the sole supported tool. */
    browserFeedbackTool: normalizeBrowserFeedbackTool(
      readString(source, "browserFeedbackTool", DEFAULT_ghostex_SETTINGS.browserFeedbackTool),
    ),
    /**
     * CDXC:BrowserPanes 2026-05-27-07:24
     * Existing settings files may still contain the deleted Chrome Canary value.
     * Treat every stored value as Browser Panes so the old attachment route cannot reappear after reload.
     */
    browserOpenMode: normalizeBrowserOpenMode(
      readString(source, "browserOpenMode", DEFAULT_ghostex_SETTINGS.browserOpenMode),
    ),
    /**
     * CDXC:SettingsAdvanced 2026-06-28-08:01:
     * Persist the Show Advanced density switch with other Settings so advanced
     * rows stay visible after a restart until the user disables the switch.
     */
    showAdvancedSettings: readBoolean(
      source,
      "showAdvancedSettings",
      DEFAULT_ghostex_SETTINGS.showAdvancedSettings,
    ),
    /**
     * CDXC:SettingsNavigation 2026-06-29-17:54:
     * Restart restore reads Settings location from the shared settings file, so
     * normalize active tab and scroll offsets at the storage boundary before
     * React uses them to choose the initial Settings surface.
     */
    settingsModalNavigation: normalizeSettingsModalNavigationState(
      source.settingsModalNavigation,
    ),
    /**
     * CDXC:BetaFeatures 2026-06-16-13:08:
     * Normalize the beta gate as a strict boolean so stale or malformed settings
     * cannot expose beta-only OS Integration or browser address-bar controls.
     */
    showBetaFeatures: readBoolean(
      source,
      "showBetaFeatures",
      DEFAULT_ghostex_SETTINGS.showBetaFeatures,
    ),
    codeViewTabHidden: readBoolean(
      source,
      "codeViewTabHidden",
      DEFAULT_ghostex_SETTINGS.codeViewTabHidden,
    ),
    browserViewTabHidden: readBoolean(
      source,
      "browserViewTabHidden",
      DEFAULT_ghostex_SETTINGS.browserViewTabHidden,
    ),
    kanbanViewTabHidden: readBoolean(
      source,
      "kanbanViewTabHidden",
      DEFAULT_ghostex_SETTINGS.kanbanViewTabHidden,
    ),
    automateViewTabHidden: readBoolean(
      source,
      "automateViewTabHidden",
      DEFAULT_ghostex_SETTINGS.automateViewTabHidden,
    ),
    docsViewTabHidden: readBoolean(
      source,
      "docsViewTabHidden",
      DEFAULT_ghostex_SETTINGS.docsViewTabHidden,
    ),
    tipsAndTricksTitlebarButtonHidden: readBoolean(
      source,
      "tipsAndTricksTitlebarButtonHidden",
      DEFAULT_ghostex_SETTINGS.tipsAndTricksTitlebarButtonHidden,
    ),
    resourcesTitlebarButtonHidden: readBoolean(
      source,
      "resourcesTitlebarButtonHidden",
      DEFAULT_ghostex_SETTINGS.resourcesTitlebarButtonHidden,
    ),
    gitActionsTitlebarButtonHidden: readBoolean(
      source,
      "gitActionsTitlebarButtonHidden",
      DEFAULT_ghostex_SETTINGS.gitActionsTitlebarButtonHidden,
    ),
    quickActionsTitlebarButtonHidden: readBoolean(
      source,
      "quickActionsTitlebarButtonHidden",
      DEFAULT_ghostex_SETTINGS.quickActionsTitlebarButtonHidden,
    ),
    openInTitlebarButtonHidden: readBoolean(
      source,
      "openInTitlebarButtonHidden",
      DEFAULT_ghostex_SETTINGS.openInTitlebarButtonHidden,
    ),
    /**
     * CDXC:EditorPanes 2026-06-08-20:12:
     * Normalize the code-server VS Code settings-link toggles on every read so
     * missing values use the bundled editor defaults while explicit local VS
     * Code settings choices remain persisted.
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
    // CDXC:AppIconPicker 2026-06-25-21:50: Coerce stored app icon source id to a trimmed string, defaulting to the bundled icon.
    appIconSourceId: normalizeAppIconSourceId(
      readString(source, "appIconSourceId", DEFAULT_ghostex_SETTINGS.appIconSourceId),
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
    manageAdditionalDocsFolders: normalizeManageAdditionalDocsFolders(
      readString(
        source,
        "manageAdditionalDocsFolders",
        DEFAULT_ghostex_SETTINGS.manageAdditionalDocsFolders,
      ),
    ),
    // CDXC:GlobalProjectDefaults 2026-08-02: Global Defaults for the Projects page fields.
    globalWorktreeCommand: normalizeGlobalWorktreeCommand(
      readString(source, "globalWorktreeCommand", DEFAULT_ghostex_SETTINGS.globalWorktreeCommand),
    ),
    globalBeadsDisplayKey: normalizeGlobalBeadsDisplayKey(
      readString(source, "globalBeadsDisplayKey", DEFAULT_ghostex_SETTINGS.globalBeadsDisplayKey),
    ),
    globalBeadsDirectory: normalizeGlobalBeadsDirectory(
      readString(source, "globalBeadsDirectory", DEFAULT_ghostex_SETTINGS.globalBeadsDirectory),
    ),
    globalDocsDirectory: normalizeGlobalDocsDirectory(
      readString(source, "globalDocsDirectory", DEFAULT_ghostex_SETTINGS.globalDocsDirectory),
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
    showUntrackedProjectDiffWhenNoTrackedChanges: readBoolean(
      source,
      "showUntrackedProjectDiffWhenNoTrackedChanges",
      DEFAULT_ghostex_SETTINGS.showUntrackedProjectDiffWhenNoTrackedChanges,
    ),
    completionBellEnabled: readBoolean(
      source,
      "completionBellEnabled",
      DEFAULT_ghostex_SETTINGS.completionBellEnabled,
    ),
    completionSound: clampCompletionSoundSetting(
      readString(source, "completionSound", DEFAULT_ghostex_SETTINGS.completionSound),
    ),
    showNotificationOnTerminalBell: readBoolean(
      source,
      "showNotificationOnTerminalBell",
      DEFAULT_ghostex_SETTINGS.showNotificationOnTerminalBell,
    ),
    createSessionOnSidebarDoubleClick: readBoolean(
      source,
      "createSessionOnSidebarDoubleClick",
      DEFAULT_ghostex_SETTINGS.createSessionOnSidebarDoubleClick,
    ),
    debuggingMode: readBoolean(source, "debuggingMode", DEFAULT_ghostex_SETTINGS.debuggingMode),
    diagnosticLogging: normalizeDiagnosticLoggingSettings(source.diagnosticLogging),
    renameSessionOnDoubleClick: readBoolean(
      source,
      "renameSessionOnDoubleClick",
      DEFAULT_ghostex_SETTINGS.renameSessionOnDoubleClick,
    ),
    showProjectIcons: readBoolean(
      source,
      "showProjectIcons",
      DEFAULT_ghostex_SETTINGS.showProjectIcons,
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
    useColoredSessionAgentIcons: readBoolean(
      source,
      "useColoredSessionAgentIcons",
      DEFAULT_ghostex_SETTINGS.useColoredSessionAgentIcons,
    ),
    /**
     * CDXC:BrowserPanes 2026-05-28-07:38:
     * Missing browser-favicon visibility should follow the sidebar preset
     * independently from the older agent-icon hover-only setting so browser
     * page identity does not disappear just because agent logos are quiet.
     */
    hideBrowserFaviconUntilHover: readBoolean(
      source,
      "hideBrowserFaviconUntilHover",
      DEFAULT_ghostex_SETTINGS.hideBrowserFaviconUntilHover,
    ),
    showCloseButtonOnSessionCards: readBoolean(
      source,
      "showCloseButtonOnSessionCards",
      DEFAULT_ghostex_SETTINGS.showCloseButtonOnSessionCards,
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
    showSessionCloseContextMenuAction: readBoolean(
      source,
      "showSessionCloseContextMenuAction",
      DEFAULT_ghostex_SETTINGS.showSessionCloseContextMenuAction,
    ),
    showSessionCommandCopyActions: readBoolean(
      source,
      "showSessionCommandCopyActions",
      DEFAULT_ghostex_SETTINGS.showSessionCommandCopyActions,
    ),
    showSessionDetailsCopyAction: readBoolean(
      source,
      "showSessionDetailsCopyAction",
      DEFAULT_ghostex_SETTINGS.showSessionDetailsCopyAction,
    ),
    sidebarSessionTagListItems: normalizeSidebarSessionTagListItems(
      source.sidebarSessionTagListItems,
    ),
    /**
     * CDXC:AutoSleep 2026-05-28-08:06:
     * Normalize Auto Sleep policy independently from keep-awake so Mac power
     * assertions and Ghostex session retirement can be configured separately.
     */
    autoSleepAgentSessionsEnabled: readBoolean(
      source,
      "autoSleepAgentSessionsEnabled",
      DEFAULT_ghostex_SETTINGS.autoSleepAgentSessionsEnabled,
    ),
    autoSleepAgentIdleMinutes: normalizeAutoSleepIdleMinutes(
      readNumber(
        source,
        "autoSleepAgentIdleMinutes",
        DEFAULT_ghostex_SETTINGS.autoSleepAgentIdleMinutes,
      ),
      DEFAULT_ghostex_SETTINGS.autoSleepAgentIdleMinutes,
    ),
    autoSleepBrowserSessionsEnabled: readBoolean(
      source,
      "autoSleepBrowserSessionsEnabled",
      DEFAULT_ghostex_SETTINGS.autoSleepBrowserSessionsEnabled,
    ),
    autoSleepBrowserIdleMinutes: normalizeAutoSleepIdleMinutes(
      readNumber(
        source,
        "autoSleepBrowserIdleMinutes",
        DEFAULT_ghostex_SETTINGS.autoSleepBrowserIdleMinutes,
      ),
      DEFAULT_ghostex_SETTINGS.autoSleepBrowserIdleMinutes,
    ),
    autoSleepCodeEditorEnabled: readBoolean(
      source,
      "autoSleepCodeEditorEnabled",
      DEFAULT_ghostex_SETTINGS.autoSleepCodeEditorEnabled,
    ),
    autoSleepCodeEditorIdleMinutes: normalizeAutoSleepIdleMinutes(
      readNumber(
        source,
        "autoSleepCodeEditorIdleMinutes",
        DEFAULT_ghostex_SETTINGS.autoSleepCodeEditorIdleMinutes,
      ),
      DEFAULT_ghostex_SETTINGS.autoSleepCodeEditorIdleMinutes,
    ),
    autoSleepGitEditorEnabled: readBoolean(
      source,
      "autoSleepGitEditorEnabled",
      DEFAULT_ghostex_SETTINGS.autoSleepGitEditorEnabled,
    ),
    autoSleepGitEditorIdleMinutes: normalizeAutoSleepIdleMinutes(
      readNumber(
        source,
        "autoSleepGitEditorIdleMinutes",
        DEFAULT_ghostex_SETTINGS.autoSleepGitEditorIdleMinutes,
      ),
      DEFAULT_ghostex_SETTINGS.autoSleepGitEditorIdleMinutes,
    ),
    autoSleepProjectEditorEnabled: readBoolean(
      source,
      "autoSleepProjectEditorEnabled",
      DEFAULT_ghostex_SETTINGS.autoSleepProjectEditorEnabled,
    ),
    autoSleepProjectEditorIdleMinutes: normalizeAutoSleepIdleMinutes(
      readNumber(
        source,
        "autoSleepProjectEditorIdleMinutes",
        DEFAULT_ghostex_SETTINGS.autoSleepProjectEditorIdleMinutes,
      ),
      DEFAULT_ghostex_SETTINGS.autoSleepProjectEditorIdleMinutes,
    ),
    autoSleepRequireAgentResumeCommand: readBoolean(
      source,
      "autoSleepRequireAgentResumeCommand",
      DEFAULT_ghostex_SETTINGS.autoSleepRequireAgentResumeCommand,
    ),
    autoSleepFavoriteAgentSessions: readBoolean(
      source,
      "autoSleepFavoriteAgentSessions",
      DEFAULT_ghostex_SETTINGS.autoSleepFavoriteAgentSessions,
    ),
    keepAwakeActivateOnExternalDisplay: readBoolean(
      source,
      "keepAwakeActivateOnExternalDisplay",
      DEFAULT_ghostex_SETTINGS.keepAwakeActivateOnExternalDisplay,
    ),
    keepAwakeActivateOnLaunch: readBoolean(
      source,
      "keepAwakeActivateOnLaunch",
      DEFAULT_ghostex_SETTINGS.keepAwakeActivateOnLaunch,
    ),
    keepAwakeAllowDisplaySleep: readBoolean(
      source,
      "keepAwakeAllowDisplaySleep",
      DEFAULT_ghostex_SETTINGS.keepAwakeAllowDisplaySleep,
    ),
    keepAwakeBatteryThresholdPercent: clampNumber(
      readNumber(
        source,
        "keepAwakeBatteryThresholdPercent",
        DEFAULT_ghostex_SETTINGS.keepAwakeBatteryThresholdPercent,
      ),
      10,
      90,
      DEFAULT_ghostex_SETTINGS.keepAwakeBatteryThresholdPercent,
    ),
    keepAwakeDeactivateBelowBatteryThreshold: readBoolean(
      source,
      "keepAwakeDeactivateBelowBatteryThreshold",
      DEFAULT_ghostex_SETTINGS.keepAwakeDeactivateBelowBatteryThreshold,
    ),
    keepAwakeDeactivateOnLowPowerMode: readBoolean(
      source,
      "keepAwakeDeactivateOnLowPowerMode",
      DEFAULT_ghostex_SETTINGS.keepAwakeDeactivateOnLowPowerMode,
    ),
    keepAwakeDeactivateOnUserSwitch: readBoolean(
      source,
      "keepAwakeDeactivateOnUserSwitch",
      DEFAULT_ghostex_SETTINGS.keepAwakeDeactivateOnUserSwitch,
    ),
    keepAwakeDefaultDurationMinutes: normalizeKeepAwakeDurationMinutes(
      readNumber(
        source,
        "keepAwakeDefaultDurationMinutes",
        DEFAULT_ghostex_SETTINGS.keepAwakeDefaultDurationMinutes,
      ),
    ),
    keepAwakeWhileWorkingSessions: readBoolean(
      source,
      "keepAwakeWhileWorkingSessions",
      DEFAULT_ghostex_SETTINGS.keepAwakeWhileWorkingSessions,
    ),
    keepAwakePreventLidSleep: readBoolean(
      source,
      "keepAwakePreventLidSleep",
      DEFAULT_ghostex_SETTINGS.keepAwakePreventLidSleep,
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
      "hideKeepAwakeTitlebarControl",
      DEFAULT_ghostex_SETTINGS.hideKeepAwakeTitlebarControl,
    ),
    hideTabStripNewTerminalButton: readBoolean(
      source,
      "hideTabStripNewTerminalButton",
      DEFAULT_ghostex_SETTINGS.hideTabStripNewTerminalButton,
    ),
    hideTabStripNewChatButton: readBoolean(
      source,
      "hideTabStripNewChatButton",
      DEFAULT_ghostex_SETTINGS.hideTabStripNewChatButton,
    ),
    hideTabStripNewBrowserButton: readBoolean(
      source,
      "hideTabStripNewBrowserButton",
      DEFAULT_ghostex_SETTINGS.hideTabStripNewBrowserButton,
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
     *
     * CDXC:SessionStatusIndicators 2026-06-27-20:11:
     * After removing standalone floating session indicators from macOS and
     * GPUI, this normalizer is compatibility-only for existing settings JSON.
     * Do not use the value for menu bar or floating pet presentation.
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
     * Older settings should normalize the session-id overlay preference from
     * the canonical default while preserving explicit user choices.
     * The native pane still suppresses the actual label unless that terminal is
     * backed by zmx, tmux, or zellij.
     */
    showSessionIdInTerminalPanes: readBoolean(
      source,
      "showSessionIdInTerminalPanes",
      DEFAULT_ghostex_SETTINGS.showSessionIdInTerminalPanes,
    ),
    /**
     * CDXC:SidebarV2 2026-07-29:
     * Unknown or missing sidebar version values normalize to the classic
     * sidebar, so a corrupted or older settings file can never strand a user
     * inside the opt-in Inbox surface.
     */
    sidebarVersion: normalizeSidebarVersion(
      readString(source, "sidebarVersion", DEFAULT_ghostex_SETTINGS.sidebarVersion),
    ),
    preferredAgentInterface: normalizePreferredAgentInterface(
      readString(
        source,
        "preferredAgentInterface",
        DEFAULT_ghostex_SETTINGS.preferredAgentInterface,
      ),
    ),
    sidebarV2Layout: normalizeSidebarV2Layout(
      readString(source, "sidebarV2Layout", DEFAULT_ghostex_SETTINGS.sidebarV2Layout),
    ),
    sidebarAutoSettleAfterDays: normalizeSidebarAutoSettleAfterDays(
      source["sidebarAutoSettleAfterDays"],
    ),
    sidebarProjectGroupingOverrides: normalizeSidebarProjectGroupingOverrides(
      source["sidebarProjectGroupingOverrides"],
    ),
    newSessionsDefaultEnvMode: normalizeSidebarNewSessionEnvMode(
      readString(
        source,
        "newSessionsDefaultEnvMode",
        DEFAULT_ghostex_SETTINGS.newSessionsDefaultEnvMode,
      ),
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
    sidebarCollapseAnimationDurationMs: clampSidebarCollapseAnimationDurationMs(
      readNumber(
        source,
        "sidebarCollapseAnimationDurationMs",
        DEFAULT_ghostex_SETTINGS.sidebarCollapseAnimationDurationMs,
      ),
    ),
    sidebarDefaultWidthPx: clampSidebarDefaultWidthPx(
      readNumber(
        source,
        "sidebarDefaultWidthPx",
        DEFAULT_ghostex_SETTINGS.sidebarDefaultWidthPx,
      ),
    ),
    /**
     * CDXC:ProjectSessionLists 2026-06-13-01:06:
     * Missing settings should use the current ten-session Show less behavior, while explicit numeric values tune how many project sessions remain visible before the header toggle offers Show more.
     */
    projectSessionListCollapsedCount: clampProjectSessionListCollapsedCount(
      readNumber(
        source,
        "projectSessionListCollapsedCount",
        DEFAULT_ghostex_SETTINGS.projectSessionListCollapsedCount,
      ),
    ),
    sidebarProjectGroupStyle: normalizeSidebarProjectGroupStyle(
      readString(
        source,
        "sidebarProjectGroupStyle",
        DEFAULT_ghostex_SETTINGS.sidebarProjectGroupStyle,
      ),
    ),
    sidebarGroupsOpacityPercent: Math.min(
      100,
      Math.max(
        0,
        Math.round(
          readNumber(
            source,
            "sidebarGroupsOpacityPercent",
            DEFAULT_ghostex_SETTINGS.sidebarGroupsOpacityPercent,
          ),
        ),
      ),
    ),
    sidebarProjectsOpacityPercent: Math.min(
      100,
      Math.max(
        0,
        Math.round(
          readNumber(
            source,
            "sidebarProjectsOpacityPercent",
            DEFAULT_ghostex_SETTINGS.sidebarProjectsOpacityPercent,
          ),
        ),
      ),
    ),
    expandCollapsedProjectsOnJump: readBoolean(
      source,
      "expandCollapsedProjectsOnJump",
      DEFAULT_ghostex_SETTINGS.expandCollapsedProjectsOnJump,
    ),
    showLessForExpandedProjectJumps: readBoolean(
      source,
      "showLessForExpandedProjectJumps",
      DEFAULT_ghostex_SETTINGS.showLessForExpandedProjectJumps,
    ),
    sidebarTheme: clampSidebarThemeSetting(
      readString(source, "sidebarTheme", DEFAULT_ghostex_SETTINGS.sidebarTheme),
    ),
    sessionChatTheme: normalizeSessionChatTheme(source.sessionChatTheme),
    sessionChatFontFamily: readString(
      source,
      "sessionChatFontFamily",
      DEFAULT_ghostex_SETTINGS.sessionChatFontFamily,
    ).trim(),
    sessionChatTranscriptWidthPercent: clampSessionChatTranscriptWidthPercent(
      readNumber(
        source,
        "sessionChatTranscriptWidthPercent",
        DEFAULT_ghostex_SETTINGS.sessionChatTranscriptWidthPercent,
      ),
    ),
    sessionChatVerboseMode: readBoolean(
      source,
      "sessionChatVerboseMode",
      DEFAULT_ghostex_SETTINGS.sessionChatVerboseMode,
    ),
    customSidebarTitlebarColorsEnabled: true,
    customSidebarTitlebarForegroundColor: getSidebarTitlebarForegroundForBackground(
      customSidebarTitlebarBackgroundColor,
    ),
    customSidebarTitlebarBackgroundTintColor,
    customSidebarTitlebarBackgroundDarknessPercent,
    customSidebarTitlebarBackgroundColor,
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
    windowsTerminalBackend: normalizeWindowsTerminalBackend(
      readString(
        source,
        "windowsTerminalBackend",
        DEFAULT_ghostex_SETTINGS.windowsTerminalBackend,
      ),
    ),
    windowsWslDistribution: normalizeWindowsWslDistribution(
      readString(
        source,
        "windowsWslDistribution",
        DEFAULT_ghostex_SETTINGS.windowsWslDistribution,
      ),
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
    terminalBackgroundImage: readString(
      source,
      "terminalBackgroundImage",
      DEFAULT_ghostex_SETTINGS.terminalBackgroundImage,
    ).trim(),
    terminalBackgroundImageOpacity: clampNumber(
      readNumber(
        source,
        "terminalBackgroundImageOpacity",
        DEFAULT_ghostex_SETTINGS.terminalBackgroundImageOpacity,
      ),
      0,
      1,
      DEFAULT_ghostex_SETTINGS.terminalBackgroundImageOpacity,
    ),
    terminalBackgroundImageFit: normalizeTerminalBackgroundImageFit(
      readString(
        source,
        "terminalBackgroundImageFit",
        DEFAULT_ghostex_SETTINGS.terminalBackgroundImageFit,
      ),
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
     * CDXC:TerminalPanePadding 2026-06-25-21:27:
     * Missing or legacy settings should keep terminals edge-to-edge. Explicit
     * values are integer pixels clamped to the Settings slider range so native
     * layout receives bounded insets without reintroducing the removed pane-gap
     * spacing between adjacent panes.
     */
    terminalPaneHorizontalPaddingPx: clampTerminalPanePaddingPx(
      readNumber(
        source,
        "terminalPaneHorizontalPaddingPx",
        DEFAULT_ghostex_SETTINGS.terminalPaneHorizontalPaddingPx,
      ),
    ),
    terminalPaneVerticalPaddingPx: clampTerminalPanePaddingPx(
      readNumber(
        source,
        "terminalPaneVerticalPaddingPx",
        DEFAULT_ghostex_SETTINGS.terminalPaneVerticalPaddingPx,
      ),
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
    terminalPastePreviewableImages: readBoolean(
      source,
      "terminalPastePreviewableImages",
      DEFAULT_ghostex_SETTINGS.terminalPastePreviewableImages,
    ),
    terminalMouseHideWhileTyping: readBoolean(
      source,
      "terminalMouseHideWhileTyping",
      DEFAULT_ghostex_SETTINGS.terminalMouseHideWhileTyping,
    ),
    terminalScrollbar: normalizeGhosttyScrollbar(
      readString(source, "terminalScrollbar", DEFAULT_ghostex_SETTINGS.terminalScrollbar),
    ),
    /**
     * CDXC:TerminalDevServers 2026-06-23-19:22:
     * Dev-server settings normalize in the app layer because they are not Ghostty keys. Keep the launch choice to system default versus internal browser, migrate legacy per-browser defaults to system default, and canonicalize ignored port rules to sorted, merged strings.
     */
    terminalDevServerDetectionEnabled: readBoolean(
      source,
      "terminalDevServerDetectionEnabled",
      DEFAULT_ghostex_SETTINGS.terminalDevServerDetectionEnabled,
    ),
    terminalDevServerOpenTarget,
    terminalDevServerIgnoredPortRules: normalizeTerminalDevServerIgnoredPortRules(
      source.terminalDevServerIgnoredPortRules,
    ),
    /**
     * CDXC:PortlessSettings 2026-06-22-22:35:
     * Portless normalization accepts only explicit booleans and lowercase http/https. Missing, legacy, string-boolean, and invalid values fall back to enabled HTTPS without preserving project-scoped Portless keys.
     */
    portlessEnabled: readBoolean(
      source,
      "portlessEnabled",
      DEFAULT_ghostex_SETTINGS.portlessEnabled,
    ),
    portlessProtocol: normalizePortlessProtocol(
      readString(source, "portlessProtocol", DEFAULT_ghostex_SETTINGS.portlessProtocol),
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
     * Keep reading the old opt-in keys so older snapshots round-trip cleanly.
     *
     * CDXC:PromptEditorBackend 2026-06-30-00:08:
     * Removed gte Settings choices normalize to inherit, so legacy mirrors must no longer resurrect gte after the canonical backend has been migrated.
     */
    richPromptEditingWithGte: false,
    useGteForCtrlGPromptEditing: false,
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
    clickToWakeSleepingSessions: readBoolean(
      source,
      "clickToWakeSleepingSessions",
      DEFAULT_ghostex_SETTINGS.clickToWakeSleepingSessions,
    ),
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
    workspacePaneGap: 0,
    remoteMachines: normalizeRemoteMachineSettings(source.remoteMachines),
    commandsPanelDefaultHeightPx: clampCommandsPanelDefaultHeightPx(
      readNumber(
        source,
        "commandsPanelDefaultHeightPx",
        DEFAULT_ghostex_SETTINGS.commandsPanelDefaultHeightPx,
      ),
    ),
    commandsPanelSide: normalizeCommandsPanelSide(
      readString(source, "commandsPanelSide", DEFAULT_ghostex_SETTINGS.commandsPanelSide),
    ),
  };
}

export function normalizeRemoteMachineSettings(candidate: unknown): RemoteMachineSettings[] {
  if (!Array.isArray(candidate)) {
    return [];
  }
  const seenIds = new Set<string>();
  const normalized: RemoteMachineSettings[] = [];
  for (const item of candidate) {
    if (!isRecord(item)) {
      continue;
    }
    const name = readLooseString(item.name).slice(0, 80);
    const sshHost = readLooseString(item.sshHost).slice(0, 200);
    if (!name || !sshHost) {
      continue;
    }
    let id = normalizeRemoteMachineId(item.id);
    if (!id || seenIds.has(id)) {
      id = `remote-${normalized.length + 1}`;
      while (seenIds.has(id)) {
        id = `remote-${normalized.length + 1}-${seenIds.size + 1}`;
      }
    }
    seenIds.add(id);
    const sshUser = readLooseString(item.sshUser).slice(0, 120);
    const sshIdentityFile = readLooseString(item.sshIdentityFile).slice(0, 500);
    const sshPort = normalizeRemoteMachineSshPort(item.sshPort);
    const wslDistribution = normalizeRemoteMachineWslDistribution(item.wslDistribution);
    normalized.push({
      id,
      name,
      sshHost,
      /*
      CDXC:RemoteMachines 2026-06-09-18:23:
      Remote SSH passwords are stored only in macOS Keychain. Settings may keep
      this boolean marker so the UI can show a saved credential state, but raw
      password fields from drafts/imports must be ignored by normalization.
      */
      ...(item.sshPasswordSaved === true ? { sshPasswordSaved: true } : {}),
      ...(sshIdentityFile ? { sshIdentityFile } : {}),
      ...(sshPort ? { sshPort } : {}),
      ...(sshUser ? { sshUser } : {}),
      ...(wslDistribution ? { wslDistribution } : {}),
    });
  }
  return normalized;
}

function normalizeRemoteMachineWslDistribution(value: unknown): string {
  const distribution = readLooseString(value).slice(0, 120);
  if (
    !distribution ||
    distribution.startsWith("-") ||
    !/^[A-Za-z0-9][A-Za-z0-9._+() -]*$/u.test(distribution)
  ) {
    return "";
  }
  return distribution;
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

export function normalizeSettingsModalNavigationState(
  candidate: unknown,
): SettingsModalNavigationState {
  const source = isRecord(candidate) ? candidate : {};
  const rawActiveTab = readLooseString(source.activeTab);
  const activeTab = SETTINGS_MODAL_NAVIGATION_TAB_SET.has(rawActiveTab)
    ? (rawActiveTab as SettingsModalNavigationTab)
    : DEFAULT_SETTINGS_MODAL_NAVIGATION_STATE.activeTab;
  const scrollSource = isRecord(source.scrollTopByTab) ? source.scrollTopByTab : {};
  const scrollTopByTab: SettingsModalNavigationState["scrollTopByTab"] = {};
  for (const tab of SETTINGS_MODAL_NAVIGATION_TABS) {
    const scrollTop = scrollSource[tab];
    if (typeof scrollTop !== "number" || !Number.isFinite(scrollTop) || scrollTop <= 0) {
      continue;
    }
    scrollTopByTab[tab] = clampNumber(
      scrollTop,
      0,
      MAX_SETTINGS_MODAL_SCROLL_TOP,
      DEFAULT_SETTINGS_MODAL_NAVIGATION_STATE.scrollTopByTab[tab] ?? 0,
    );
  }
  return {
    activeTab,
    scrollTopByTab,
    version: 1,
  };
}

export function normalizeManageAdditionalDocsFolders(value: string | undefined): string {
  /*
   * CDXC:DocsSidebar 2026-06-30-19:47:
   * The Projects setting is typed as comma-separated text because folder names may contain spaces. Settings normalizes on every keystroke, so preserve the user's draft text here and let native trim comma boundaries and reject unsafe path shapes when scanning.
   */
  return (value ?? "").replace(/\0/gu, "").replace(/\r?\n/gu, ", ").slice(0, 1_000);
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
  return (value ?? "").replace(/\0/gu, "").slice(0, 16_384);
}

export function normalizeGlobalBeadsDisplayKey(value: string | undefined): string {
  return (value ?? "").toUpperCase().replace(/[^A-Z0-9]/gu, "").slice(0, 3);
}

export function normalizeGlobalBeadsDirectory(value: string | undefined): string {
  return (value ?? "").replace(/\0/gu, "").trim().slice(0, 1_000);
}

/*
 * CDXC:DocsRootDirectory 2026-08-09:
 * The Docs directory normalizes exactly like the Beads directory it sits next
 * to: one absolute folder path, trimmed, with no embedded NULs.
 */
export function normalizeGlobalDocsDirectory(value: string | undefined): string {
  return (value ?? "").replace(/\0/gu, "").trim().slice(0, 1_000);
}

function normalizeTerminalCursorStyle(value: string | undefined): TerminalCursorStyle {
  return value === "block" || value === "underline" ? value : "bar";
}

function normalizeWindowsTerminalBackend(
  _value: string | undefined,
): WindowsTerminalBackend {
  return "wsl";
}

function normalizeWindowsWslDistribution(value: string | undefined): string {
  return (value ?? "").replace(/\0/gu, "").replace(/\r?\n/gu, "").trim().slice(0, 128);
}

function normalizeBrowserOpenMode(value: string | undefined): BrowserOpenMode {
  return "browser-pane";
}

function normalizeBrowserFeedbackTool(_value: string | undefined): BrowserFeedbackTool {
  return "agentation";
}

function normalizeAppShotsHotkey(value: string | undefined): AppShotsHotkey {
  /*
   * CDXC:AppShots 2026-06-29-01:29:
   * App Shots hotkeys must support both physical Shift keys and both physical Option keys in addition to both Command and left-key double-taps, because modifier-only capture should be usable without overloading one hand.
   */
  return value === "both-shift" ||
    value === "both-option" ||
    value === "double-left-shift" ||
    value === "double-left-option"
    ? value
    : DEFAULT_ghostex_SETTINGS.appShotsHotkey;
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

/*
 * CDXC:AppIconPicker 2026-06-26-23:42:
 * Empty string means default icon; otherwise the persisted id must remain a
 * filename-only value that round-trips exactly after native confirms it. Reject
 * invalid/path-like ids instead of slicing or otherwise rewriting them.
 */
function normalizeAppIconSourceId(value: string | undefined): string {
  const normalized = (value ?? "").trim();
  if (normalized.length === 0) {
    return "";
  }
  if (normalized.length > 255) {
    return "";
  }
  if (normalized === "." || normalized === "..") {
    return "";
  }
  if (normalized.includes("/") || normalized.includes("\\") || normalized.includes("\0")) {
    return "";
  }
  return normalized;
}

function normalizeDefaultPromptAgentId(value: string | undefined): string {
  return ((value ?? "").trim() || DEFAULT_ghostex_SETTINGS.defaultPromptAgentId).slice(0, 120);
}

function normalizeSessionTitleGenerationAgent(
  value: string | undefined,
): SessionTitleGenerationAgent {
  return value === "cursor" || value === "claude" || value === "grok" || value === "custom"
    ? value
    : DEFAULT_ghostex_SETTINGS.sessionTitleGenerationAgent;
}

function normalizeCustomSessionTitleGenerationCommand(value: string | undefined): string {
  return (value ?? "").trim().slice(0, 240);
}

export function getSessionTitleGenerationCommandPreview(
  agent: SessionTitleGenerationAgent,
  options: { command?: string } = {},
): string {
  const command = readSessionTitleGenerationPreviewCommand(agent, options.command);
  const prompt = SESSION_TITLE_GENERATION_PROMPT_PLACEHOLDER;
  switch (agent) {
    case "codex":
      /*
      CDXC:SessionTitleSettings 2026-06-07-01:57:
      Settings must preview the same internal Codex title-generation command gxserver runs. Include `--ephemeral` so users see that generated titles do not create restorable Codex sessions.
      */
      return createSessionTitleGenerationHereDocPreview(
        `${command} exec --ephemeral --skip-git-repo-check -m gpt-5.6-luna -c 'model_reasoning_effort="low"'`,
        prompt,
      );
    case "cursor":
      return `${command} --print --yolo --trust --model cursor-grok-4.5-low --output-format text '${prompt}'`;
    case "claude":
      return createSessionTitleGenerationHereDocPreview(
        `${command} -p --model haiku --effort low`,
        prompt,
      );
    case "grok":
      return `${command} --model grok-4.5 --reasoning-effort low --output-format plain --no-alt-screen --no-plan --no-subagents --disable-web-search --max-turns 1 --single '${prompt}'`;
    case "custom":
      return createSessionTitleGenerationHereDocPreview(command, prompt);
  }
}

function readSessionTitleGenerationPreviewCommand(
  agent: SessionTitleGenerationAgent,
  command: string | undefined,
): string {
  const configured = command?.trim();
  if (configured) {
    return configured;
  }
  switch (agent) {
    case "codex":
      return "codex";
    case "cursor":
      return "cursor-agent";
    case "claude":
      return "claude";
    case "grok":
      return "grok";
    case "custom":
      return "<custom command>";
  }
}

function createSessionTitleGenerationHereDocPreview(command: string, prompt: string): string {
  return `${command} <<'PROMPT'\n${prompt}\nPROMPT`;
}

function normalizeCustomPromptEditorCommand(value: string | undefined): string {
  return ((value ?? "").trim() || DEFAULT_ghostex_SETTINGS.customPromptEditorCommand).slice(0, 240);
}

function normalizeTerminalDevServerOpenTarget(
  candidate: unknown,
  legacyDefaultBrowserId: unknown,
): TerminalDevServerOpenTarget {
  const value = readLooseString(candidate);
  if (TERMINAL_DEV_SERVER_OPEN_TARGET_SET.has(value as TerminalDevServerOpenTarget)) {
    return value as TerminalDevServerOpenTarget;
  }

  const legacyValue = readLooseString(legacyDefaultBrowserId);
  if (legacyValue !== undefined) {
    return "system-default-browser";
  }

  return DEFAULT_TERMINAL_DEV_SERVER_OPEN_TARGET;
}

type TerminalDevServerPortRule = {
  lowerBound: number;
  upperBound: number;
};

export function normalizeTerminalDevServerIgnoredPortRuleInput(
  value: string,
): string | undefined {
  return parseTerminalDevServerPortRule(value)?.canonicalString;
}

export function normalizeTerminalDevServerIgnoredPortRules(candidate: unknown): readonly string[] {
  if (!Array.isArray(candidate)) {
    return DEFAULT_TERMINAL_DEV_SERVER_IGNORED_PORT_RULES;
  }
  const mergedRules = mergeTerminalDevServerPortRules(
    candidate.map(readLooseString).flatMap((value) => {
      const rule = parseTerminalDevServerPortRule(value);
      return rule ? [rule] : [];
    }),
  ).map((rule) => rule.canonicalString);

  return mergedRules.length === 0 ? DEFAULT_TERMINAL_DEV_SERVER_IGNORED_PORT_RULES : mergedRules;
}

function parseTerminalDevServerPortRule(value: string): (TerminalDevServerPortRule & {
  canonicalString: string;
}) | undefined {
  const trimmedValue = value.trim();
  if (!trimmedValue) {
    return undefined;
  }
  const rangeMatch = trimmedValue.match(/^(\d+)(?:\s*-\s*(\d+))?$/u);
  if (!rangeMatch) {
    return undefined;
  }
  const lowerBound = Number(rangeMatch[1]);
  const upperBound = rangeMatch[2] === undefined ? lowerBound : Number(rangeMatch[2]);
  if (
    !Number.isInteger(lowerBound) ||
    !Number.isInteger(upperBound) ||
    lowerBound < 1 ||
    upperBound > 65535 ||
    lowerBound > upperBound
  ) {
    return undefined;
  }
  return {
    lowerBound,
    upperBound,
    canonicalString:
      lowerBound === upperBound ? String(lowerBound) : `${lowerBound}-${upperBound}`,
  };
}

function mergeTerminalDevServerPortRules(
  rules: ReadonlyArray<TerminalDevServerPortRule>,
): Array<TerminalDevServerPortRule & { canonicalString: string }> {
  const mergedRules: TerminalDevServerPortRule[] = [];
  for (const rule of [...rules].sort((left, right) =>
    left.lowerBound === right.lowerBound
      ? left.upperBound - right.upperBound
      : left.lowerBound - right.lowerBound,
  )) {
    const previousRule = mergedRules.at(-1);
    if (!previousRule || rule.lowerBound > previousRule.upperBound + 1) {
      mergedRules.push({ lowerBound: rule.lowerBound, upperBound: rule.upperBound });
      continue;
    }
    previousRule.upperBound = Math.max(previousRule.upperBound, rule.upperBound);
  }

  return mergedRules.map((rule) => ({
    ...rule,
    canonicalString:
      rule.lowerBound === rule.upperBound
        ? String(rule.lowerBound)
        : `${rule.lowerBound}-${rule.upperBound}`,
  }));
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

function normalizeCommandsPanelSide(value: string | undefined): CommandsPanelSide {
  return value === "right" ? "right" : DEFAULT_ghostex_SETTINGS.commandsPanelSide;
}

function normalizeSidebarProjectGroupStyle(
  value: string | undefined,
): SidebarProjectGroupStyle {
  return value === "quiet" || value === "header" || value === "branched"
    ? value
    : DEFAULT_ghostex_SETTINGS.sidebarProjectGroupStyle;
}

function normalizeSidebarVersion(value: string | undefined): SidebarVersion {
  return value === "v2" ? "v2" : DEFAULT_ghostex_SETTINGS.sidebarVersion;
}

function normalizePreferredAgentInterface(
  value: string | undefined,
): PreferredAgentInterface {
  return value === "chat" ? "chat" : DEFAULT_ghostex_SETTINGS.preferredAgentInterface;
}

function normalizeSidebarV2Layout(value: string | undefined): SidebarV2Layout {
  return value === "byProject" ? "byProject" : DEFAULT_ghostex_SETTINGS.sidebarV2Layout;
}

/* An unknown value falls back to "local": the worktree default must be an
   explicit choice, never something a corrupted settings file can turn on. */
function normalizeSidebarNewSessionEnvMode(
  value: string | undefined,
): SidebarNewSessionEnvMode {
  return value === "worktree" ? "worktree" : DEFAULT_ghostex_SETTINGS.newSessionsDefaultEnvMode;
}

/*
CDXC:SidebarV2Lifecycle 2026-07-29:
Exact twin of gxserver-rs `normalize_auto_settle_after_days`. The two ends must
agree or the shelf a user sees and the shelf the daemon writes drift apart:
  missing key            -> the default window
  explicit null          -> auto-settle disabled
  finite number > 0      -> that window
  any other number       -> disabled (0 and negatives mean "never", not "always")
  a non-number value     -> the default window
*/
function normalizeSidebarAutoSettleAfterDays(value: unknown): number | null {
  if (value === undefined) {
    return DEFAULT_ghostex_SETTINGS.sidebarAutoSettleAfterDays;
  }
  if (value === null) {
    return null;
  }
  if (typeof value !== "number") {
    return DEFAULT_ghostex_SETTINGS.sidebarAutoSettleAfterDays;
  }
  return Number.isFinite(value) && value > 0 ? value : null;
}

/*
CDXC:SidebarV2LogicalProjects 2026-07-29:
Grouping overrides are user-visible state that a user can also hand-edit, so
normalization drops anything it does not recognize instead of substituting a
default. An unrecognized value must not silently become "repository": that
would re-merge checkouts the user explicitly separated. Keys are kept verbatim
because they are opaque physical-project keys minted by the sidebar module.
*/
function normalizeSidebarProjectGroupingOverrides(
  value: unknown,
): Readonly<Record<string, SidebarProjectGroupingMode>> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return DEFAULT_ghostex_SETTINGS.sidebarProjectGroupingOverrides;
  }
  const normalized: Record<string, SidebarProjectGroupingMode> = {};
  for (const [key, mode] of Object.entries(value as Record<string, unknown>)) {
    if (key.trim().length === 0) {
      continue;
    }
    if (mode === "repository" || mode === "repositoryPath" || mode === "separate") {
      normalized[key] = mode;
    }
  }
  return normalized;
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

function normalizeKeepAwakeDurationMinutes(value: number): KeepAwakeDurationMinutes {
  return KEEP_AWAKE_DURATION_OPTIONS.some((option) => option.value === value)
    ? (value as KeepAwakeDurationMinutes)
    : DEFAULT_ghostex_SETTINGS.keepAwakeDefaultDurationMinutes;
}

function normalizeAutoSleepIdleMinutes(
  value: number,
  fallback: AutoSleepIdleMinutes,
): AutoSleepIdleMinutes {
  return AUTO_SLEEP_IDLE_MINUTE_OPTIONS.some((option) => option.value === value)
    ? (value as AutoSleepIdleMinutes)
    : fallback;
}

function normalizePromptEditorBackend(source: Record<string, unknown>): PromptEditorBackend {
  const backend = readString(source, "promptEditorBackend", "");
  if (backend === "inherit" || backend === "monaco") {
    return backend;
  }
  if (backend === "gte" || backend === "custom") {
    return "inherit";
  }
  if (
    readBoolean(source, "useGteForCtrlGPromptEditing", false) ||
    readBoolean(source, "richPromptEditingWithGte", false)
  ) {
    return "inherit";
  }
  return DEFAULT_ghostex_SETTINGS.promptEditorBackend;
}

function normalizeRemoteMachineId(input: unknown): string | undefined {
  const id = readLooseString(input).slice(0, 80);
  return /^remote-[a-z0-9_-]+$/iu.test(id) ? id : undefined;
}

function normalizeRemoteMachineSshPort(input: unknown): number | undefined {
  if (input === undefined || input === null || input === "") {
    return undefined;
  }
  const value = typeof input === "number" ? input : Number(input);
  if (!Number.isInteger(value) || value < 1 || value > 65535) {
    return undefined;
  }
  return value;
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
  return value === "false" || value === "true" || value === "always"
    ? value
    : DEFAULT_ghostex_SETTINGS.terminalConfirmCloseSurface;
}

function normalizeGhosttyScrollbar(value: string | undefined): GhosttyScrollbar {
  return value === "never" ? "never" : "system";
}

function normalizeTerminalBackgroundImageFit(
  value: string | undefined,
): TerminalBackgroundImageFit {
  return value === "contain" || value === "stretch" || value === "natural"
    ? value
    : DEFAULT_ghostex_SETTINGS.terminalBackgroundImageFit;
}

function normalizePortlessProtocol(value: string | undefined): PortlessProtocol {
  return value === "http" || value === "https"
    ? value
    : DEFAULT_ghostex_SETTINGS.portlessProtocol;
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

export function normalizeDiagnosticLoggingSettings(
  candidate: unknown,
): DiagnosticLoggingSettings {
  const source = isRecord(candidate) ? candidate : {};
  const scenariosSource = isRecord(source.scenarios) ? source.scenarios : {};
  const scenarios: DiagnosticLoggingSettings["scenarios"] = {
    ...DEFAULT_DIAGNOSTIC_LOGGING_SCENARIOS,
  };
  for (const [scenarioId, rawState] of Object.entries(scenariosSource)) {
    if (!DIAGNOSTIC_LOGGING_SCENARIO_IDS.has(scenarioId)) {
      continue;
    }
    const state = normalizeDiagnosticLoggingScenarioState(rawState);
    if (state) {
      scenarios[scenarioId as DiagnosticLoggingScenarioId] = state;
    }
  }
  return {
    scenarios,
    version: 1,
  };
}

function normalizeDiagnosticLoggingScenarioState(
  candidate: unknown,
): DiagnosticLoggingScenarioState | undefined {
  /*
   * CDXC:ChromeResponsivenessDiagnostics 2026-06-30-23:52:
   * Default-on diagnostic scenarios need a durable Off state. Preserve explicit
   * enabled:false values so Settings can disable routine chrome/lag logging
   * without reset-to-default immediately turning it back on.
   */
  if (candidate === true) {
    return { enabled: true };
  }
  if (candidate === false) {
    return { enabled: false };
  }
  if (!isRecord(candidate)) {
    return undefined;
  }
  if (candidate.enabled === false) {
    return { enabled: false };
  }
  if (candidate.enabled !== true) {
    return undefined;
  }
  const expiresAt =
    typeof candidate.expiresAt === "string" && isValidDiagnosticLoggingExpiry(candidate.expiresAt)
      ? candidate.expiresAt
      : undefined;
  return expiresAt ? { enabled: true, expiresAt } : { enabled: true };
}

function isValidDiagnosticLoggingExpiry(value: string): boolean {
  /*
   * CDXC:DiagnosticsSettings 2026-06-27-22:07:
   * Time-limited logging scenarios persist as ISO timestamps produced by
   * Date.toISOString. Normalize only parseable absolute times so native Swift
   * and GPUI can evaluate expiry without accepting arbitrary strings into the
   * support-logging contract.
   */
  return Number.isFinite(Date.parse(value));
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

function readLooseString(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}
