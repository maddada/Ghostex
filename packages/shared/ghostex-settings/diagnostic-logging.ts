import { isRecord } from './primitives';

export type DiagnosticLoggingScenarioId = (typeof DIAGNOSTIC_LOGGING_SCENARIOS)[number]['id'];
export type DiagnosticLoggingScenarioState = {
  enabled: boolean;
  expiresAt?: string;
};
export type DiagnosticLoggingSettings = {
  scenarios: Partial<Record<DiagnosticLoggingScenarioId, DiagnosticLoggingScenarioState>>;
  version: 1;
};
export type DiagnosticLoggingScenarioDefinition = {
  description: string;
  id: string;
  label: string;
};
/**
 * CDXC:Diagnostics 2026-09-26 DECISION:
 * User: the Debugging page had far too many log switches. Scenarios nothing writes any more are deleted, the rest are merged into one switch per feature area, all switches share one turn-off timer, and rows show no log file names. Desktop performance profiling has no switch: launching with --profile is its opt-in.
 * Each id is the one an area kept from before the merge, so the Rust writers in apps/desktop/src/support_logs.rs and server/src/logging.rs still read the same strings.
 */
export const DIAGNOSTIC_LOGGING_SCENARIOS = [
  {
    description:
      'Terminal focus and keyboard routing, pane tabs, terminal attach and sync (local and remote), and zmx refreshes.',
    id: 'native.terminal.focus',
    label: 'Terminals, panes, and keyboard focus',
  },
  {
    description: 'Sidebar bootstrap and focus, browser and code editor panes, Quick Access, and chat frame readiness.',
    id: 'native.sidebar.refresh',
    label: 'Sidebar, browser, and editor panes',
  },
  {
    description: 'Chat view state, composer draft saves, sends, clears, and restart recovery (desktop and gxserver).',
    id: 'gpui.sessionChat.viewState',
    label: 'Chat',
  },
  {
    description: 'App modal host lifecycle, Settings hydration, and modal errors.',
    id: 'gpui.app.modal',
    label: 'Modals and Settings',
  },
  {
    description: 'Project board create/start, title generation, Beads, and worktree setup.',
    id: 'native.project.board',
    label: 'Project board',
  },
  {
    description: 'Remote gxserver install: approval, SSH setup, package upload, token read, and tunnel.',
    id: 'native.remote.gxserver.install',
    label: 'Remote machines',
  },
  {
    description: 'Agent detection and working/idle/attention transitions from hooks and terminal titles.',
    id: 'gxserver.agentActivity',
    label: 'Agent activity',
  },
  {
    description: 'Prompt editor window and composer lifecycle.',
    id: 'native.prompt.editor',
    label: 'Prompt editor',
  },
  {
    description: 'Desktop app and gxserver startup, activation, window close, and shutdown.',
    id: 'native.host.lifecycle',
    label: 'App and server lifecycle',
  },
  {
    description: 'gxserver API requests, typed operations, repository cloning, and Portless.',
    id: 'gxserver.requests',
    label: 'Server requests',
  },
] as const satisfies readonly DiagnosticLoggingScenarioDefinition[];

// Routine diagnostic disk logging is opt-in. Agents enable only the scenario
// needed for a repro and restore it when collection is complete.
export const DEFAULT_DIAGNOSTIC_LOGGING_SCENARIOS: DiagnosticLoggingSettings['scenarios'] = {};
const DIAGNOSTIC_LOGGING_SCENARIO_IDS = new Set<string>(DIAGNOSTIC_LOGGING_SCENARIOS.map((scenario) => scenario.id));

export function isDiagnosticLoggingScenarioEnabled(
  diagnosticLogging: DiagnosticLoggingSettings | undefined,
  scenarioId: DiagnosticLoggingScenarioId,
  now: Date = new Date()
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
  state: DiagnosticLoggingScenarioState | undefined
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
  rhs: DiagnosticLoggingSettings
): boolean {
  return (
    JSON.stringify(normalizeDiagnosticLoggingSettings(lhs)) === JSON.stringify(normalizeDiagnosticLoggingSettings(rhs))
  );
}

export function normalizeDiagnosticLoggingSettings(candidate: unknown): DiagnosticLoggingSettings {
  const source = isRecord(candidate) ? candidate : {};
  const scenariosSource = isRecord(source.scenarios) ? source.scenarios : {};
  const scenarios: DiagnosticLoggingSettings['scenarios'] = {
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

function normalizeDiagnosticLoggingScenarioState(candidate: unknown): DiagnosticLoggingScenarioState | undefined {
  /*
   * CDXC:Diagnostics 2026-06-30-23:52:
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
    typeof candidate.expiresAt === 'string' && isValidDiagnosticLoggingExpiry(candidate.expiresAt)
      ? candidate.expiresAt
      : undefined;
  return expiresAt ? { enabled: true, expiresAt } : { enabled: true };
}

function isValidDiagnosticLoggingExpiry(value: string): boolean {
  /*
   * CDXC:Diagnostics 2026-06-27-22:07:
   * Time-limited logging scenarios persist as ISO timestamps produced by
   * Date.toISOString. Normalize only parseable absolute times so native Swift
   * and GPUI can evaluate expiry without accepting arbitrary strings into the
   * support-logging contract.
   */
  return Number.isFinite(Date.parse(value));
}
