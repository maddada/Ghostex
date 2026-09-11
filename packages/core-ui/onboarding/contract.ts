import type { ghostexSettings } from '@/packages/shared/ghostex-settings';
import type {
  SidebarAgentHookStatusMessage,
  SidebarGhostexCliStatusMessage,
} from '@/packages/shared/session-grid-contract-sidebar';
import type { SidebarTheme } from '@/packages/shared/session-grid-contract-core';

/**
 * CDXC:Onboarding 2026-09-11 DECISION:
 * This is the new five-panel onboarding ported from the B4 "Extensions" prototype at
 * github.com/banozz0/ghostex-onboarding-prototypes. User: "lots of changes on the onboarding so let's keep the old one
 * for now": first run and the Tips "Setup" button open the older FirstLaunchSetupModal until this one is finished;
 * this modal is reachable only by its `onboarding` modal id (openAppModal) meanwhile.
 * This file is the props contract between the React port (`onboarding-modal.tsx` and its panels) and the
 * modal-host adapter that feeds it real app state; both sides build against it, so change it deliberately.
 */

/** Sidebar agent id (`'claude'`, `'codex'`, `'cursor'`, ...) or `'terminal'` for a plain shell session. */
export type OnboardingAgentChoice = string;

export type OnboardingDetectedAgent = {
  /** Sidebar agent id, e.g. `'claude'`. */
  agentId: string;
  /** Display name, e.g. `'Claude Code'`. */
  name: string;
  /** Short account line shown under the name, e.g. `'uses your Claude account'`. */
  accountLabel?: string;
  /** True when the CLI binary was found on this computer. */
  installed: boolean;
  /** True when the Ghostex helper (agent hooks) is already installed for this agent. */
  hooksInstalled: boolean;
  /** Native detail line (path / error) for tooltips. */
  detail?: string;
};

/** The workspace views the Workspace panel toggles. Each maps to `<view>ViewTabHidden` in settings. */
export type OnboardingViewKey = 'browser' | 'docs' | 'code' | 'kanban' | 'automate';

export type OnboardingSessionView = 'chat' | 'terminal' | 'last';

/** Lifecycle of the Computer Use switch on the Agents panel. */
export type OnboardingComputerUseState =
  | 'off'
  | 'installing'
  /** Cua Driver is installed but macOS Accessibility / Screen Recording grants are still missing. */
  | 'permissions'
  | 'on';

export type OnboardingModalProps = {
  isOpen: boolean;
  theme?: SidebarTheme;
  /** True when the sidebar already lists at least one project: the last panel ends with Finish instead of
   * requiring a folder, and the modal may be dismissed. */
  hasProjects?: boolean;
  /**
   * CDXC:Onboarding 2026-09-11 DECISION:
   * User: "if it's first run ever browser/docs is actually applied as default enabled; for regular current
   * users if they revisit the onboarding by clicking on the setup button in the tips dropdown this shouldn't
   * happen." True only for the automatic first-run open (native adds `firstRun` to that open message).
   */
  firstRun?: boolean;
  /** Current settings snapshot; the modal reads view visibility, notifications, default agent and
   * preferred interface from here and writes back through `onChange` with a full settings object. */
  settings?: ghostexSettings;
  onChange: (settings: ghostexSettings) => void;
  /** Persist completion and close the window. Called by "Open Ghostex", "I already know Ghostex" and the
   * finished screen. */
  onClose: () => void;

  // Agents panel
  agents: readonly OnboardingDetectedAgent[];
  agentsLoading: boolean;
  agentHookStatus?: SidebarAgentHookStatusMessage;
  /** Re-run native agent detection (`requestAgentHookStatus`). */
  onRescanAgents: () => void;
  /** Install the Ghostex helper (agent hooks) for the given agents. */
  onInstallAgentHooks: (agentIds: readonly string[]) => void;
  /** Open the external "install an agent" guide. */
  onOpenInstallGuide?: (url: string) => void;

  // Computer Use (Cua Driver + Computer Use skill)
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading?: boolean;
  computerUseState: OnboardingComputerUseState;
  onInstallComputerUse: () => void;
  onOpenAccessibilityPreferences?: () => void;
  onOpenScreenRecordingPreferences?: () => void;

  // Workspace panel: the browser skill row
  browserSkillInstalled: boolean;
  onInstallBrowserSkill: () => void;
  onUninstallBrowserSkill: () => void;

  // Mobile panel
  /** Opens Settings -> Remote (Easy Connect / pairing QR). The modal calls it only when it is finishing
   * (right before `onClose`), because opening Settings replaces the onboarding window. */
  onOpenRemoteSettings: () => void;
  onOpenExternalUrl: (url: string) => void;

  // Get started panel
  /** Opens the native folder picker; the result arrives through `pickedProjectFolder`. */
  onPickProjectFolder: () => void;
  pickedProjectFolder?: string;
  /** Registers the folder as a project and opens its first session. Resolves once the host confirms the
   * project and session exist; rejects with the host's error message, in which case the Get started panel
   * stays put and shows it. The finished screen is only shown after it resolves. */
  onFinishFirstLaunch: (options: { agentId: OnboardingAgentChoice; path: string }) => Promise<void>;
  /** Open the full Settings modal ("Advanced settings later"). */
  onOpenSettings?: () => void;
};
