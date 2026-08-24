/*
 * localStorage persistence for the sandbox.
 *
 * Mirrors what survives a real app restart: the fake
 * `~/.local/state/ghostex/gpui-first-run-onboarding-state.json`, the simulated
 * machine environment, and the launch counter. In-memory suppressions (the
 * portless "suppressed until restart" flag, the single-modal slot, the Windows
 * first-launch followup) are deliberately NOT persisted.
 */
import type { FirstRunOnboardingStateFile, SimEnvState } from '../state/types';
import { createBundledSkills, createDefaultEnv, createDefaultStateFile } from './env-defaults';
import { SIM_AGENT_IDS, BUNDLED_SKILL_IDS } from '../state/types';

const ENV_KEY = 'ghostex.onboardingSandbox.env';
const STATE_FILE_KEY = 'ghostex.onboardingSandbox.stateFile';
const LAUNCH_COUNT_KEY = 'ghostex.onboardingSandbox.launchCount';

function readJson(key: string): unknown {
  try {
    const raw = window.localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as unknown) : undefined;
  } catch {
    return undefined;
  }
}

function writeJson(key: string, value: unknown): void {
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* Private-mode localStorage failures must never break the simulation. */
  }
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : undefined;
}

/**
 * Merge a stored env over the defaults key by key. A stored blob from an older
 * sandbox build must never produce an env with missing agents or skills, so
 * every nested record is rebuilt from the current id lists.
 */
export function loadEnv(): SimEnvState {
  const defaults = createDefaultEnv();
  const stored = asRecord(readJson(ENV_KEY));
  if (!stored) {
    return defaults;
  }
  const storedAgents = asRecord(stored.agents) ?? {};
  const storedSkills = asRecord(stored.bundledSkills) ?? {};
  const agents = { ...defaults.agents };
  for (const agentId of SIM_AGENT_IDS) {
    const entry = asRecord(storedAgents[agentId]);
    if (!entry) {
      continue;
    }
    agents[agentId] = {
      cliInstalled: entry.cliInstalled === true,
      hookState: entry.hookState === 'installed' || entry.hookState === 'outdated' ? entry.hookState : 'notInstalled',
    };
  }
  const bundledSkills = createBundledSkills(false);
  for (const skillId of BUNDLED_SKILL_IDS) {
    bundledSkills[skillId] = storedSkills[skillId] === true;
  }
  return {
    ...defaults,
    ...stored,
    agents,
    bundledSkills,
    ghostexCli: { ...defaults.ghostexCli, ...(asRecord(stored.ghostexCli) ?? {}) },
    cuaDriver: { ...defaults.cuaDriver, ...(asRecord(stored.cuaDriver) ?? {}) },
    gxserver: { ...defaults.gxserver, ...(asRecord(stored.gxserver) ?? {}) },
    settings: { ...defaults.settings, ...(asRecord(stored.settings) ?? {}) },
    timing: { ...defaults.timing, ...(asRecord(stored.timing) ?? {}) },
  } as SimEnvState;
}

export function saveEnv(env: SimEnvState): void {
  writeJson(ENV_KEY, env);
}

export function loadStateFile(): FirstRunOnboardingStateFile {
  const defaults = createDefaultStateFile();
  const stored = asRecord(readJson(STATE_FILE_KEY));
  if (!stored) {
    return defaults;
  }
  const revision = (key: string): string | null => (typeof stored[key] === 'string' ? (stored[key] as string) : null);
  return {
    tipsAndTricksSeen: stored.tipsAndTricksSeen === true,
    highlightedFeaturesSeenRevision: revision('highlightedFeaturesSeenRevision'),
    firstLaunchSetupSeenRevision: revision('firstLaunchSetupSeenRevision'),
    osIntegrationOnboardingSeen: stored.osIntegrationOnboardingSeen === true,
    firstLaunchSetupComplete: stored.firstLaunchSetupComplete === true,
    windowsTerminalSetupComplete: stored.windowsTerminalSetupComplete === true,
  };
}

export function saveStateFile(stateFile: FirstRunOnboardingStateFile): void {
  writeJson(STATE_FILE_KEY, stateFile);
}

export function loadLaunchCount(): number {
  const stored = readJson(LAUNCH_COUNT_KEY);
  return typeof stored === 'number' && Number.isFinite(stored) && stored >= 0 ? Math.floor(stored) : 0;
}

export function saveLaunchCount(launchCount: number): void {
  writeJson(LAUNCH_COUNT_KEY, launchCount);
}
