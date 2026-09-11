import type { OnboardingComputerUseState, OnboardingDetectedAgent } from '@/packages/core-ui/onboarding/contract';
import { DEFAULT_SIDEBAR_AGENTS } from '@/packages/shared/sidebar-agents';
import type {
  SidebarAgentHookStatusMessage,
  SidebarGhostexCliStatusMessage,
} from '@/packages/shared/session-grid-contract';

/**
 * CDXC:Onboarding 2026-09-11 SEE-ALSO:
 * Pure adapters between the native status payloads the modal host already receives for
 * FirstLaunchSetupModal (`agentHookStatus`, `ghostexCliStatus`) and the props contract of the
 * new OnboardingModal (packages/core-ui/onboarding/contract.ts). Keeping them here keeps the
 * modal host's Onboarding render block as thin as the FirstLaunchSetup one.
 */

const ONBOARDING_AGENT_DISPLAY_NAMES: Readonly<Record<string, string>> = {
  claude: 'Claude Code',
  codex: 'Codex CLI',
};

const ONBOARDING_AGENT_ACCOUNT_LABELS: Readonly<Record<string, string>> = {
  claude: 'uses your Claude account',
  codex: 'uses your ChatGPT account',
  cursor: 'uses your Cursor account',
};

export function buildOnboardingDetectedAgents(
  agentHookStatus: SidebarAgentHookStatusMessage | undefined
): OnboardingDetectedAgent[] {
  if (!agentHookStatus) {
    return [];
  }
  const catalogById = new Map<string, { name: string }>(DEFAULT_SIDEBAR_AGENTS.map((agent) => [agent.agentId, agent]));
  return agentHookStatus.agents.map((status) => {
    const catalogEntry = catalogById.get(status.agentId);
    const accountLabel = ONBOARDING_AGENT_ACCOUNT_LABELS[status.agentId];
    const detail = status.detail.trim();
    return {
      agentId: status.agentId,
      name: ONBOARDING_AGENT_DISPLAY_NAMES[status.agentId] ?? catalogEntry?.name ?? status.agentId,
      ...(accountLabel ? { accountLabel } : {}),
      installed: status.cliInstalled,
      hooksInstalled: status.hookInstalled,
      ...(detail ? { detail } : {}),
    };
  });
}

/**
 * CDXC:Onboarding 2026-09-11 WHY:
 * The Trycua install runs as a command-pane Action that takes minutes, and every skill-install reply in
 * between refreshes `ghostexCliStatus` (clearing the loading marker). `installing` therefore hangs on the
 * host's `installRequested` flag alone, which the modal host resets when the driver reports installed or
 * the install fails, never on the loading marker. The permission probe is optional: both flags undefined
 * means it did not run, not that a grant is missing.
 */
export function deriveOnboardingComputerUseState({
  ghostexCliStatus,
  installRequested,
}: {
  ghostexCliStatus: SidebarGhostexCliStatusMessage | undefined;
  installRequested: boolean;
}): OnboardingComputerUseState {
  const installed =
    ghostexCliStatus?.cuaDriverInstalled === true && ghostexCliStatus?.computerUseSkillInstalled === true;
  if (installed) {
    const permissionMissing =
      ghostexCliStatus.cuaDriverAccessibilityPermissionGranted === false ||
      ghostexCliStatus.cuaDriverScreenRecordingPermissionGranted === false;
    return permissionMissing ? 'permissions' : 'on';
  }
  if (installRequested) {
    return 'installing';
  }
  return 'off';
}
