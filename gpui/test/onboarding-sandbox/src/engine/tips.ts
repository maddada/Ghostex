/*
 * Tips & Tricks badge + notices.
 *
 * Tip ids: gpui/src/main.rs:1057 TITLEBAR_TIP_IDS (12 tips, all unread on a
 * fresh profile — gpui/src/main.rs:25658 seeds the badge with the full count).
 *
 * Notices: native/sidebar/titlebar-host.tsx:833-975. Only the settings-derived
 * notices (persistence off, debugging mode) exist before the panel is opened;
 * the CLI and missing-hook notices need `agentHookStatus`/`ghostexCliStatus`,
 * which are requested ONLY when the panel opens
 * (gpui/src/main.rs:31568 request_gpui_titlebar_tips_runtime_status).
 */
import type { SimEnvState, SimTipsNotice } from "../state/types";
import { SIM_AGENT_IDS } from "../state/types";
import { agentDisplayName, deriveAgentHookStatus } from "./status-messages";

export const TITLEBAR_TIP_IDS: readonly string[] = [
  "command-palette-all-actions",
  "customize-sidebar-layout-and-tools",
  "sleep-idle-sessions-from-resources",
  "attach-browser-pane-to-task",
  "use-ghostex-computer-use-skill",
  "use-ghostex-browser-use-skill",
  "use-ghostex-embedded-browser-use-skill",
  "use-ghostex-auto-rename-session-skill",
  "recommend-faster-chrome-devtools-skill",
  "find-session-by-prompt-text",
  "pin-important-workspaces",
  "add-todos-to-kanban-page",
];

const PERSISTENCE_OFF_NOTICE: SimTipsNotice = {
  id: "session-persistence-off-mobile-attach",
  severity: "warning",
  title: "Mobile attach needs persistence",
  body: "Android and iOS attach can have issues while Session Persistence is Off. Enable zmx persistence so mobile clients reconnect to durable terminal sessions.",
};

const DEBUGGING_MODE_NOTICE: SimTipsNotice = {
  id: "debugging-mode-enabled",
  severity: "warning",
  title: "Debug mode is on",
  body: "Ghostex is showing debug UI controls. Routine disk logging is controlled by Diagnostic disk logging scenarios in Settings.",
};

/** native/sidebar/titlebar-host.tsx createTitlebarGhostexCliNotice:859 */
function ghostexCliNotice(env: SimEnvState): SimTipsNotice | undefined {
  if (env.ghostexCli.installed && env.ghostexCli.gxUsable) {
    return undefined;
  }
  return {
    id: "ghostex-cli-not-accessible",
    severity: "warning",
    title: "Ghostex CLI is not accessible",
    body: "Install or repair the CLI to use ghostex/gx in any terminal, attach mobile clients, and install Browser/Computer/Orchestration agent skills.",
  };
}

function formatNameList(names: readonly string[]): string {
  if (names.length <= 1) {
    return names[0] ?? "";
  }
  if (names.length === 2) {
    return `${names[0]} and ${names[1]}`;
  }
  return `${names.slice(0, -1).join(", ")}, and ${names[names.length - 1]}`;
}

/** native/sidebar/titlebar-host.tsx createTitlebarMissingAgentHooksNotice:884 */
function missingAgentHooksNotice(env: SimEnvState): SimTipsNotice | undefined {
  const outdated: string[] = [];
  const missing: string[] = [];
  for (const agentId of SIM_AGENT_IDS) {
    if (!env.agents[agentId].cliInstalled) {
      continue;
    }
    const status = deriveAgentHookStatus(env, agentId);
    if (status === "installed" || status === "notRequired" || status === "cliMissing") {
      continue;
    }
    if (status === "updateRequired") {
      outdated.push(agentDisplayName(agentId));
    } else {
      missing.push(agentDisplayName(agentId));
    }
  }
  const names = [...outdated, ...missing];
  if (names.length === 0) {
    return undefined;
  }
  const action = outdated.length > 0 && missing.length > 0 ? "setup" : outdated.length > 0 ? "update" : "install";
  const actionLabel = action === "setup" ? "install or update" : action;
  const actionVerb = action === "setup" ? "installed or updated" : action === "update" ? "updated" : "installed";
  return {
    id: `agent-hooks-${action}`,
    severity: "warning",
    title: "Warning: Agent hooks aren't installed for agent CLIs",
    body: `Open Settings > Integrations to ${actionLabel} agent hooks for ${formatNameList(names)}. Automatic session renaming, In Progress/Needs Attention status, and sleeping or resuming agent sessions will not work correctly until hooks are ${actionVerb}.`,
  };
}

/** Notices available without a runtime probe (startup badge). */
export function settingsDerivedTipsNotices(env: SimEnvState): SimTipsNotice[] {
  return [
    ...(env.settings.sessionPersistenceOff ? [PERSISTENCE_OFF_NOTICE] : []),
    ...(env.settings.debuggingMode ? [DEBUGGING_MODE_NOTICE] : []),
  ];
}

/**
 * Full notice list, in the order the titlebar renders it: CLI, persistence,
 * debugging, missing hooks. Only reachable after the runtime status probe.
 */
export function probedTipsNotices(env: SimEnvState): SimTipsNotice[] {
  const cli = ghostexCliNotice(env);
  const hooks = missingAgentHooksNotice(env);
  return [
    ...(cli ? [cli] : []),
    ...settingsDerivedTipsNotices(env),
    ...(hooks ? [hooks] : []),
  ];
}

export function tipsBadgeCount(unreadTipCount: number, notices: readonly SimTipsNotice[]): number {
  return unreadTipCount + notices.length;
}
