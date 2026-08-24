/*
 * Shared contracts for the onboarding sandbox. All modules import from here.
 * Extend ADDITIVELY only — renaming/removing breaks parallel agents. See SPEC.md.
 */
import type { AppModalKind } from '@/packages/core-ui/app-modal-host-bridge';

/** All modal kinds the sandbox can open. `updateAvailable` exists only in the
 * modal host's own union, not in the bridge export, so it is added here. */
export type SandboxModalKind = AppModalKind | 'updateAvailable';

/** Loose envelopes for the modal-host message protocol. The real host validates
 * shapes at runtime; the sandbox does not re-model the full unions. */
export type ModalHostInboundMessage = { type: string } & Record<string, unknown>;
export type ModalHostOutboundMessage = { type: string } & Record<string, unknown>;

// ---------------------------------------------------------------------------
// Simulated environment (the "user's OS")
// ---------------------------------------------------------------------------

/** The 17 agent CLIs gxserver knows about (server/src/agent_hooks/). */
export const SIM_AGENT_IDS = [
  'codex',
  'claude',
  'opencode',
  'pi',
  'cursor',
  'gemini',
  'kiro',
  'copilot',
  'droid',
  'grok',
  'antigravity',
  'amp',
  'omp',
  'rovodev',
  'hermes-agent',
  'codebuddy',
  'qoder',
] as const;
export type SimAgentId = (typeof SIM_AGENT_IDS)[number];

/** Agents whose hook state satisfies the first-launch "ready" gate. */
export const PRIORITY_AGENT_IDS: readonly SimAgentId[] = ['codex', 'claude', 'opencode', 'pi'];

/** Raw on-disk hook state; the engine derives the contract status
 * (cliMissing/missing/updateRequired/installed) from this + cliInstalled. */
export type SimHookState = 'notInstalled' | 'installed' | 'outdated';

export interface SimAgentState {
  cliInstalled: boolean;
  hookState: SimHookState;
}

export const BUNDLED_SKILL_IDS = [
  'browser',
  'embeddedBrowser',
  'computerUse',
  'cli',
  'fable56Orchestration',
  'manageBeads',
  'generateTitle',
  'moveCodexSession',
] as const;
export type BundledSkillId = (typeof BUNDLED_SKILL_IDS)[number];

export type GxserverHealthScenario =
  'healthyToolsAvailable' | 'healthyToolsUnavailable' | 'buildMismatch' | 'protocolMismatch' | 'spawnFailure';

export interface SimTiming {
  /** Track A: delay before the gxserver health probe resolves. */
  gxserverProbeMs: number;
  /** Track B: delay before the CEF sidebar surface is ready. */
  cefInitMs: number;
  /** Latency applied to install actions triggered from modals. */
  installActionMs: number;
  /** Gap between per-agent progressive hook-status updates. */
  hookStatusPerAgentMs: number;
}

export interface SimEnvState {
  platform: 'macos' | 'windows';
  agents: Record<SimAgentId, SimAgentState>;
  ghostexCli: {
    installed: boolean;
    gxUsable: boolean;
    gxBlockedByExistingCommand: boolean;
  };
  bundledSkills: Record<BundledSkillId, boolean>;
  cuaDriver: {
    appInstalled: boolean;
    cliInstalled: boolean;
    accessibilityPermission: boolean;
    screenRecordingPermission: boolean;
  };
  gxserver: {
    scenario: GxserverHealthScenario;
    /** If true, a simulated daemon respawn heals the scenario to healthy. */
    respawnFixesHealth: boolean;
  };
  projectCount: number;
  updateAvailable: boolean;
  /** Mirrors settings that feed tips notices. */
  settings: {
    debuggingMode: boolean;
    sessionPersistenceOff: boolean;
  };
  timing: SimTiming;
}

/** Fake ~/.local/state/ghostex/gpui-first-run-onboarding-state.json */
export interface FirstRunOnboardingStateFile {
  tipsAndTricksSeen: boolean;
  highlightedFeaturesSeenRevision: string | null;
  firstLaunchSetupSeenRevision: string | null;
  osIntegrationOnboardingSeen: boolean;
  firstLaunchSetupComplete: boolean;
  windowsTerminalSetupComplete: boolean;
}

/** Revision constants mirrored from apps/desktop/src/app/helpers/board_gxserver/focus_state.rs. */
export const FIRST_LAUNCH_SETUP_SEEN_REVISION = '2026-06-18-short-first-launch';
export const HIGHLIGHTED_FEATURES_SEEN_REVISION = '2026-06-16-highlighted-features-launch';

// ---------------------------------------------------------------------------
// Simulated app runtime
// ---------------------------------------------------------------------------

export type SimAppPhase = 'notRunning' | 'launching' | 'running';

export interface SimToast {
  id: string;
  title: string;
  message: string;
  kind: 'info' | 'warning' | 'error';
  /** null = sticky until dismissed */
  autoDismissMs: number | null;
}

export interface SimModalWindow {
  windowId: string;
  modal: SandboxModalKind;
  title: string;
  width: number;
  /** "fit" = one-shot fit-height modal; frame resizes on contentHeightMeasured. */
  height: number | 'fit';
  /** False until the modal host posts {type:"presented"} — frame stays hidden/loading. */
  presented: boolean;
  /** True when opened from the gallery, bypassing the single-modal slot. */
  forced: boolean;
  /** Extra fields merged into the {type:"open"} payload. */
  openPayload: Record<string, unknown>;
  /**
   * Set for the ONE modal kind that does not use the React modal host
   * (`GpuiAppModalKind::uses_react_modal_host`, apps/desktop/src/app/model/app_modal_kind.rs): the
   * tutorial video window loads GHOSTEX_TUTORIAL_VIDEO_URL as its top-level
   * document instead of modal-host.html. Such windows have no bridge and no
   * ready/presented handshake — the frame points its iframe straight here.
   */
  nonReactHostUrl?: string;
}

export type SimEventKind = 'flow' | 'state' | 'modal' | 'toast' | 'message' | 'warning';

export interface SimEvent {
  id: number;
  /** ms since the current launch started (or since page load when not running). */
  at: number;
  launchIndex: number;
  kind: SimEventKind;
  label: string;
  detail?: string;
  /** Real-code anchor, e.g. "apps/desktop/src/app/os_integration.rs start_gpui_first_run_onboarding". */
  codeRef?: string;
}

export interface SimTipsNotice {
  id: string;
  severity: 'info' | 'warning';
  title: string;
  body: string;
}

// ---------------------------------------------------------------------------
// Store contract
// ---------------------------------------------------------------------------

export interface ScenarioPreset {
  id: string;
  label: string;
  description: string;
  apply: {
    env: Partial<SimEnvState> | ((current: SimEnvState) => SimEnvState);
    stateFile?: Partial<FirstRunOnboardingStateFile>;
    wipeStateFile?: boolean;
  };
}

export interface SandboxState {
  env: SimEnvState;
  stateFile: FirstRunOnboardingStateFile;
  appPhase: SimAppPhase;
  launchCount: number;
  toasts: SimToast[];
  modalWindows: SimModalWindow[];
  tipsPanelOpen: boolean;
  tipsBadgeCount: number;
  tipsNotices: SimTipsNotice[];
  events: SimEvent[];
}

export interface SandboxActions {
  // environment / persistence
  patchEnv(patch: Partial<SimEnvState>): void;
  setAgentState(agentId: SimAgentId, patch: Partial<SimAgentState>): void;
  patchStateFile(patch: Partial<FirstRunOnboardingStateFile>): void;
  wipeStateFile(): void;
  applyPreset(presetId: string): void;
  // lifecycle
  launchApp(): void;
  quitApp(): void;
  relaunchApp(): void;
  // modals & chrome
  forceOpenModal(kind: SandboxModalKind, payload?: Record<string, unknown>): void;
  closeModalWindow(windowId: string): void;
  dismissToast(toastId: string): void;
  setTipsPanelOpen(open: boolean): void;
  // event log
  clearEvents(): void;
  /** Engine-internal: append an event (auto id/at/launchIndex). */
  emitEvent(event: Omit<SimEvent, 'id' | 'at' | 'launchIndex'>): void;
}

export type SandboxStore = SandboxState & SandboxActions;
