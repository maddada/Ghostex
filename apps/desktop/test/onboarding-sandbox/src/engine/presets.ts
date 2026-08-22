/*
 * Scenario presets. Each one is a complete machine description, so applying a
 * preset never leaves fields from the previous scenario behind: the env callback
 * starts from `createDefaultEnv()` and keeps only the timing sliders the user
 * has been tuning.
 *
 * `FIRST_LAUNCH_SETUP_SEEN_REVISION` / `HIGHLIGHTED_FEATURES_SEEN_REVISION` are
 * the exact strings gpui writes (gpui/src/main.rs), so a "returning user" state
 * file is byte-identical to a real one.
 */
import {
  FIRST_LAUNCH_SETUP_SEEN_REVISION,
  HIGHLIGHTED_FEATURES_SEEN_REVISION,
  type ScenarioPreset,
  type SimAgentId,
  type SimEnvState,
  type SimHookState,
} from "../state/types";
import { createAgentStates, createBundledSkills, createDefaultEnv } from "./env-defaults";

/** Every scenario keeps the user's current timing sliders. */
function baseEnv(current: SimEnvState): SimEnvState {
  return { ...createDefaultEnv(), timing: current.timing, platform: current.platform };
}

function withAgents(
  env: SimEnvState,
  agentIds: readonly SimAgentId[],
  hookState: SimHookState,
): SimEnvState {
  const agents = { ...env.agents };
  for (const agentId of agentIds) {
    agents[agentId] = { cliInstalled: true, hookState };
  }
  return { ...env, agents };
}

const RETURNING_USER_STATE_FILE = {
  tipsAndTricksSeen: true,
  highlightedFeaturesSeenRevision: HIGHLIGHTED_FEATURES_SEEN_REVISION,
  firstLaunchSetupSeenRevision: FIRST_LAUNCH_SETUP_SEEN_REVISION,
  osIntegrationOnboardingSeen: true,
  firstLaunchSetupComplete: false,
  windowsTerminalSetupComplete: false,
};

export const SCENARIO_PRESETS: ScenarioPreset[] = [
  {
    id: "brand-new-user",
    label: "Brand-new user",
    description:
      "Fresh machine: no agent CLIs, no Ghostex CLI, no skills, healthy gxserver, state file wiped. Every first-run marker is still unburned.",
    apply: {
      env: (current) => baseEnv(current),
      wipeStateFile: true,
    },
  },
  {
    id: "new-user-gxserver-upgrade",
    label: "New user, gxserver needs upgrade",
    description:
      "Brand-new state file, but the running daemon belongs to a different build. Track A restarts it; since the 2026-08-18 fix the healed respawn path re-runs first-run onboarding on THIS launch instead of requiring a restart.",
    apply: {
      env: (current) => ({
        ...baseEnv(current),
        gxserver: { scenario: "buildMismatch", respawnFixesHealth: true },
      }),
      wipeStateFile: true,
    },
  },
  {
    id: "returning-user-nothing-installed",
    label: "Returning user, nothing installed",
    description:
      "Onboarding markers already burned by an earlier launch, on a machine that still has no agent CLIs, no Ghostex CLI, and no skills. Since the 2026-08-19 fix this is the upgrade case: the one first-run modal (tutorial video on page 1, then hooks and skills) opens once so those installs finally get offered.",
    apply: {
      env: (current) => ({ ...baseEnv(current), projectCount: 2 }),
      stateFile: RETURNING_USER_STATE_FILE,
    },
  },
  {
    id: "power-user-all-installed",
    label: "Power user (all installed)",
    description:
      "Every agent CLI present with current hooks, Ghostex CLI usable, all 8 bundled skills installed, Cua Driver ready with both permissions. Fully onboarded, so nothing auto-opens.",
    apply: {
      env: (current) => ({
        ...baseEnv(current),
        agents: createAgentStates(true, "installed"),
        bundledSkills: createBundledSkills(true),
        ghostexCli: { installed: true, gxUsable: true, gxBlockedByExistingCommand: false },
        cuaDriver: {
          appInstalled: true,
          cliInstalled: true,
          accessibilityPermission: true,
          screenRecordingPermission: true,
        },
        projectCount: 6,
      }),
      stateFile: { ...RETURNING_USER_STATE_FILE, firstLaunchSetupComplete: true },
    },
  },
  {
    id: "hooks-outdated",
    label: "Hooks outdated",
    description:
      "The four priority agent CLIs are installed but carry stale Ghostex hooks, so every hook status reports updateRequired and Tips shows the 'outdated' warning.",
    apply: {
      env: (current) => ({
        ...withAgents(baseEnv(current), ["codex", "claude", "opencode", "pi"], "outdated"),
        bundledSkills: createBundledSkills(true),
        ghostexCli: { installed: true, gxUsable: true, gxBlockedByExistingCommand: false },
        projectCount: 3,
      }),
      stateFile: RETURNING_USER_STATE_FILE,
    },
  },
  {
    id: "cli-shadowed",
    label: "CLI shadowed (gx blocked)",
    description:
      "Ghostex CLI is installed but another gx command already owns the name, so gxUsable stays false and the Tips CLI notice never clears.",
    apply: {
      env: (current) => ({
        ...withAgents(baseEnv(current), ["codex", "claude"], "installed"),
        ghostexCli: { installed: true, gxUsable: false, gxBlockedByExistingCommand: true },
        projectCount: 1,
      }),
      stateFile: RETURNING_USER_STATE_FILE,
    },
  },
];
