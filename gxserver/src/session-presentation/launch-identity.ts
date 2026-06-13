import type { GxserverSessionDomainState } from "../../protocol/index.js";
import { normalizeAgentId, normalizeText } from "./identity.js";

export interface GxserverLaunchAgentMismatch {
  currentAgentId?: string;
  hasForkSource: boolean;
  hasLaunchAgentId: boolean;
  hasLaunchPlan: boolean;
  incomingAgentId: string;
  lockedAgentId: string;
}

/*
CDXC:GxserverSessionIdentity 2026-06-09-21:59:
Hook/session-state events are passive observations, but gxserver-launched panes already have an authoritative launch agent. Lock launched and forked sessions to that agent so a global hook from another CLI cannot rewrite identity or pin explicit working on the wrong session.
*/
export function resolveSessionLaunchAgentId(session: GxserverSessionDomainState): string | undefined {
  const runtimeLaunchAgentId = normalizeAgentId(session.runtimeSettings.launchAgentId);
  if (runtimeLaunchAgentId) {
    return runtimeLaunchAgentId;
  }
  const launchPlan = normalizeRecord(session.launchSettings.agentLaunchPlan);
  const launchPlanAgentId =
    inferAgentIdFromCommand(launchPlan.agentCommand) ??
    inferAgentIdFromCommand(launchPlan.command);
  if (launchPlanAgentId) {
    return launchPlanAgentId;
  }
  /*
  CDXC:GxserverSessionIdentity 2026-06-13-09:08:
  Startup text such as `codex --yolo` is launch-owned identity evidence even when no structured launch plan was stored. Use it as the passive-event lock so stale hooks cannot relabel a gxserver-started agent session before live process evidence refreshes every client.
  */
  const startupTextAgentId = inferAgentIdFromCommand(session.launchSettings.startupText);
  if (startupTextAgentId) {
    return startupTextAgentId;
  }
  if (hasForkSource(session)) {
    return inferAgentIdFromCommand(session.runtimeSettings.agentCommand);
  }
  return undefined;
}

export function resolveSessionLaunchAgentMismatch(
  session: GxserverSessionDomainState,
  incomingAgentIdValue: unknown,
): GxserverLaunchAgentMismatch | undefined {
  const incomingAgentId = normalizeAgentId(incomingAgentIdValue);
  const lockedAgentId = resolveSessionLaunchAgentId(session);
  if (!incomingAgentId || !lockedAgentId || incomingAgentId === lockedAgentId) {
    return undefined;
  }
  return {
    currentAgentId: normalizeAgentId(session.agentId),
    hasForkSource: hasForkSource(session),
    hasLaunchAgentId: normalizeAgentId(session.runtimeSettings.launchAgentId) !== undefined,
    hasLaunchPlan: Object.keys(normalizeRecord(session.launchSettings.agentLaunchPlan)).length > 0,
    incomingAgentId,
    lockedAgentId,
  };
}

export function inferAgentIdFromCommand(value: unknown): string | undefined {
  const command = normalizeText(value)?.toLowerCase();
  if (!command) {
    return undefined;
  }
  /*
  CDXC:GxserverSessionIdentity 2026-06-12-12:09:
  Live zmx process evidence can report agent CLIs as absolute executable paths, for example a Node shim running `/.../bin/codex --yolo resume` after macOS `ps` omits the resume id. Treat path separators as command-token boundaries so gxserver can repair a stale cross-agent hook label from the active process tree and clear the old transcript identity for every client.
  */
  const patterns: Array<[string, RegExp]> = [
    ["cursor", /(?:^|[\s;&|()/])cursor-agent(?:$|[\s;&|()/])/u],
    ["hermes-agent", /(?:^|[\s;&|()/])hermes(?:$|[\s;&|()/])/u],
    ["codebuddy", /(?:^|[\s;&|()/])codebuddy(?:$|[\s;&|()/])/u],
    ["antigravity", /(?:^|[\s;&|()/])agy(?:$|[\s;&|()/])/u],
    ["opencode", /(?:^|[\s;&|()/])opencode(?:$|[\s;&|()/])/u],
    ["rovodev", /(?:^|[\s;&|()/])(?:acli\s+rovodev\s+run|rovodev)(?:$|[\s;&|()/])/u],
    ["qoder", /(?:^|[\s;&|()/])qodercli(?:$|[\s;&|()/])/u],
    ["claude", /(?:^|[\s;&|()/])claude(?:$|[\s;&|()/])/u],
    ["copilot", /(?:^|[\s;&|()/])copilot(?:$|[\s;&|()/])/u],
    ["gemini", /(?:^|[\s;&|()/])gemini(?:$|[\s;&|()/])/u],
    ["codex", /(?:^|[\s;&|()/])codex(?:$|[\s;&|()/])/u],
    ["droid", /(?:^|[\s;&|()/])droid(?:$|[\s;&|()/])/u],
    ["grok", /(?:^|[\s;&|()/])grok(?:$|[\s;&|()/])/u],
    ["amp", /(?:^|[\s;&|()/])amp(?:$|[\s;&|()/])/u],
    ["pi", /(?:^|[\s;&|()/])pi(?:$|[\s;&|()/])/u],
  ];
  for (const [agentId, pattern] of patterns) {
    if (pattern.test(command)) {
      return agentId;
    }
  }
  return undefined;
}

function hasForkSource(session: GxserverSessionDomainState): boolean {
  return (
    normalizeText(session.hiddenMetadata.restoredFromSessionId) !== undefined ||
    normalizeText(session.launchSettings.forkedFromSessionId) !== undefined ||
    normalizeText(session.runtimeSettings.forkedFromSessionId) !== undefined
  );
}

function normalizeRecord(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : {};
}
