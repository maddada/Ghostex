/*
 * Default simulated environment + fake on-disk state file.
 *
 * These are the "brand new machine" values: nothing installed, a healthy
 * gxserver, no projects. Presets in ./presets.ts derive every other scenario
 * from here.
 */
import {
  BUNDLED_SKILL_IDS,
  SIM_AGENT_IDS,
  type BundledSkillId,
  type FirstRunOnboardingStateFile,
  type SimAgentId,
  type SimAgentState,
  type SimEnvState,
  type SimHookState,
} from '../state/types';

export function createAgentStates(cliInstalled: boolean, hookState: SimHookState): Record<SimAgentId, SimAgentState> {
  return Object.fromEntries(SIM_AGENT_IDS.map((agentId) => [agentId, { cliInstalled, hookState }])) as Record<
    SimAgentId,
    SimAgentState
  >;
}

export function createBundledSkills(installed: boolean): Record<BundledSkillId, boolean> {
  return Object.fromEntries(BUNDLED_SKILL_IDS.map((skillId) => [skillId, installed])) as Record<
    BundledSkillId,
    boolean
  >;
}

export function createDefaultEnv(): SimEnvState {
  return {
    platform: 'macos',
    agents: createAgentStates(false, 'notInstalled'),
    ghostexCli: { installed: false, gxUsable: false, gxBlockedByExistingCommand: false },
    bundledSkills: createBundledSkills(false),
    cuaDriver: {
      appInstalled: false,
      cliInstalled: false,
      accessibilityPermission: false,
      screenRecordingPermission: false,
    },
    gxserver: { scenario: 'healthyToolsAvailable', respawnFixesHealth: true },
    projectCount: 0,
    updateAvailable: false,
    settings: { debuggingMode: false, sessionPersistenceOff: false },
    timing: {
      gxserverProbeMs: 400,
      cefInitMs: 900,
      installActionMs: 1200,
      hookStatusPerAgentMs: 350,
    },
  };
}

/** Fresh `gpui-first-run-onboarding-state.json` — the file does not exist yet. */
export function createDefaultStateFile(): FirstRunOnboardingStateFile {
  return {
    tipsAndTricksSeen: false,
    highlightedFeaturesSeenRevision: null,
    firstLaunchSetupSeenRevision: null,
    osIntegrationOnboardingSeen: false,
    firstLaunchSetupComplete: false,
    windowsTerminalSetupComplete: false,
  };
}
