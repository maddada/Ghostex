import type { ghostexSettings } from '@/packages/shared/ghostex-settings';
import type { OnboardingDetectedAgent, OnboardingModalProps, OnboardingViewKey } from './contract';

/** README section listing every supported agent CLI; the Install guide popup and the finished screen open it. */
export const ONBOARDING_INSTALL_GUIDE_URL = 'https://github.com/maddada/Ghostex#supports-all-of-the-popular-agent-clis';

/** Choices that live only for the length of the flow (everything else is read from and written to settings). */
export type OnboardingFlowState = {
  /** The "Ghostex integration" switch on the Agents panel. */
  integrationOn: boolean;
  /** Set once "Connect & continue" asked the host to install the agent helper. */
  hooksRequested: boolean;
  /** "Open after onboarding" in the Install guide popup. */
  installQueued: boolean;
  /** Agent id or `'terminal'` for the first session; undefined until the user picks one. */
  startWith?: string;
  /** Local progress of the Mobile panel's step list (which rows the user acted on). */
  mobileInstallOpened: boolean;
  mobilePairingOpened: boolean;
  /**
   * CDXC:Onboarding 2026-09-11 WHY:
   * "Pair this computer" used to open Settings -> Remote at once, which replaces the onboarding window and
   * ends the flow before the Get started panel. The click now only queues the phone step (the prototype's
   * "phone queued" idea) and `finishOnboarding` opens Remote settings right before closing.
   */
  phoneQueued: boolean;
  /** The folder handed to `onFinishFirstLaunch`; shown on the finished screen. */
  finishedPath?: string;
  /** Shows the "You're set" screen in place of the last panel; any `go()` clears it. */
  finished: boolean;
};

export const INITIAL_FLOW_STATE: OnboardingFlowState = {
  integrationOn: true,
  hooksRequested: false,
  installQueued: false,
  mobileInstallOpened: false,
  mobilePairingOpened: false,
  phoneQueued: false,
  finished: false,
};

/** Ends the flow: opens Settings -> Remote first when the phone step was queued, then records completion. */
export function finishOnboarding(props: OnboardingModalProps, flow: OnboardingFlowState): void {
  if (flow.phoneQueued) props.onOpenRemoteSettings();
  props.onClose();
}

export type PanelProps = {
  props: OnboardingModalProps;
  flow: OnboardingFlowState;
  setFlow: (patch: Partial<OnboardingFlowState>) => void;
  go: (panel: number) => void;
  toast: (message: string) => void;
};

export const VIEW_KEYS: readonly OnboardingViewKey[] = ['browser', 'docs', 'code', 'kanban', 'automate'];

const VIEW_HIDDEN_KEY: Record<OnboardingViewKey, keyof ghostexSettings> = {
  browser: 'browserViewTabHidden',
  docs: 'docsViewTabHidden',
  code: 'codeViewTabHidden',
  kanban: 'kanbanViewTabHidden',
  automate: 'automateViewTabHidden',
};

/** The prototype's defaults, used only while the settings snapshot has not arrived. */
const FALLBACK_VIEWS: Record<OnboardingViewKey, boolean> = {
  browser: true,
  docs: true,
  code: false,
  kanban: false,
  automate: false,
};

export function isViewOn(settings: ghostexSettings | undefined, key: OnboardingViewKey): boolean {
  if (!settings) return FALLBACK_VIEWS[key];
  return !settings[VIEW_HIDDEN_KEY[key]];
}

export function withViewsOn(
  settings: ghostexSettings,
  changes: Partial<Record<OnboardingViewKey, boolean>>
): ghostexSettings {
  const next: Record<string, unknown> = { ...settings };
  for (const [key, on] of Object.entries(changes)) {
    if (on === undefined) continue;
    next[VIEW_HIDDEN_KEY[key as OnboardingViewKey]] = !on;
  }
  return next as ghostexSettings;
}

export function installedAgents(agents: readonly OnboardingDetectedAgent[]): OnboardingDetectedAgent[] {
  return agents.filter((agent) => agent.installed);
}

export function missingAgents(agents: readonly OnboardingDetectedAgent[]): OnboardingDetectedAgent[] {
  return agents.filter((agent) => !agent.installed);
}

/** The default prompt agent when it is installed, otherwise the first installed agent. */
export function defaultAgentId(
  settings: ghostexSettings | undefined,
  agents: readonly OnboardingDetectedAgent[]
): string | undefined {
  const installed = installedAgents(agents);
  const configured = settings?.defaultPromptAgentId;
  if (configured && installed.some((agent) => agent.agentId === configured)) return configured;
  return installed[0]?.agentId;
}

export function agentDisplayName(agents: readonly OnboardingDetectedAgent[], agentId: string | undefined): string {
  if (!agentId) return 'No agent';
  if (agentId === 'terminal') return 'Terminal';
  return agents.find((agent) => agent.agentId === agentId)?.name ?? agentId;
}

export function allInstalledHaveHooks(agents: readonly OnboardingDetectedAgent[]): boolean {
  const installed = installedAgents(agents);
  return installed.length > 0 && installed.every((agent) => agent.hooksInstalled);
}
