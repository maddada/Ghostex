import { isRecord } from "./primitives";

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
export const DEFAULT_DIAGNOSTIC_LOGGING_SCENARIOS: DiagnosticLoggingSettings["scenarios"] = {};
const DIAGNOSTIC_LOGGING_SCENARIO_IDS = new Set<string>(
  DIAGNOSTIC_LOGGING_SCENARIOS.map((scenario) => scenario.id),
);

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
