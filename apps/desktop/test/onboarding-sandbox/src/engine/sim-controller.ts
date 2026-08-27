/*
 * The simulation engine: a faithful TS port of the gpui startup/onboarding
 * sequencing, bugs included.
 *
 * Reference flow (macOS):
 *   main → cx.open_window
 *     ├─ Track A  start_gpui_local_gxserver_bootstrap (apps/desktop/src/app/os_integration/gxserver_bootstrap.rs:33)
 *     │    healthy+tools → replay bootstrap → portless check → first-run onboarding
 *     │    any other health → toast + daemon restart → portless check +
 *     │                       first-run onboarding on the healed path
 *     └─ Track B  initialize_cef → sidebar surface ready (apps/desktop/src/app/terminal_sync/cef_and_command_terminal_focus.rs:222)
 *                  → first-run onboarding
 *
 * start_gpui_first_run_onboarding (apps/desktop/src/app/os_integration/first_run_onboarding.rs:82) returns immediately
 * when `self.sidebar` is None, so a Track A attempt that wins the race against
 * CEF is a pure no-op — nothing is consumed and nothing is shown.
 *
 * 2026-08-18 fixes mirrored here (see SPEC.md "2026-08-18 fixes"):
 * - Only the two silent legacy markers are burned in the background pass; the
 *   tutorial-video and OS-integration markers are persisted AFTER their surface
 *   is displayed, so a dropped auto-open or a crash no longer eats onboarding.
 * - A once-per-launch in-memory guard (not the markers) dedups the Track A,
 *   Track B and post-respawn attempts.
 * - The daemon-respawn success path re-runs onboarding instead of requiring a
 *   restart, and an occupied app-modal slot defers the portless prompt check
 *   (re-run on modal close) instead of dropping it.
 */
import type { StoreApi } from 'zustand';
import type { SidebarHydrateMessage } from '@/packages/shared/session-grid-contract';
import { sendToModalWindow, setModalOutboundHandler } from '../bridge/modal-connections';
import {
  FIRST_LAUNCH_SETUP_SEEN_REVISION,
  HIGHLIGHTED_FEATURES_SEEN_REVISION,
  PRIORITY_AGENT_IDS,
  SIM_AGENT_IDS,
  type BundledSkillId,
  type FirstRunOnboardingStateFile,
  type ModalHostInboundMessage,
  type ModalHostOutboundMessage,
  type SandboxActions,
  type SandboxModalKind,
  type SandboxState,
  type SandboxStore,
  type SimAgentId,
  type SimEnvState,
  type SimEvent,
  type SimModalWindow,
  type SimToast,
} from '../state/types';
import { answerAddProjectRequest } from './add-project';
import { createDefaultStateFile } from './env-defaults';
import { modalChrome, modalOpenPayload } from './modal-chrome';
import { loadEnv, loadLaunchCount, loadStateFile, saveEnv, saveLaunchCount, saveStateFile } from './persistence';
import { SCENARIO_PRESETS } from './presets';
import {
  areFirstLaunchAgentHooksReady,
  areFirstLaunchBundledSkillsInstalled,
  createAgentHookStatusMessage,
  createGhostexCliStatusMessage,
  createSandboxHydrateMessage,
  deriveAgentHookStatus,
  orderedHookStatusAgentIds,
} from './status-messages';
import {
  CODE_REFS,
  GPUI_APP_TOAST_DEFAULT_DURATION_MS,
  GXSERVER_BRANCH_TOASTS,
  GXSERVER_LOADING_TOAST,
  GXSERVER_RESPAWN_FAILURE_TOAST,
  OS_INTEGRATION_TOAST,
  scenarioRestartsDaemon,
} from './startup-sequence';
import { TITLEBAR_TIP_IDS, probedTipsNotices, settingsDerivedTipsNotices, tipsBadgeCount } from './tips';

export function createInitialSandboxState(): SandboxState {
  const env = loadEnv();
  const notices = settingsDerivedTipsNotices(env);
  return {
    env,
    stateFile: loadStateFile(),
    appPhase: 'notRunning',
    launchCount: loadLaunchCount(),
    toasts: [],
    modalWindows: [],
    tipsPanelOpen: false,
    tipsBadgeCount: tipsBadgeCount(TITLEBAR_TIP_IDS.length, notices),
    tipsNotices: notices,
    events: [],
  };
}

type PendingWindow = {
  ready: boolean;
  requiresSidebarState: boolean;
  /** The hydrate this window opened with, replayed on its own `ready`. */
  hydrate: SidebarHydrateMessage;
  /** Replayed on every `ready` so a reloaded iframe re-opens its modal. */
  openMessage: ModalHostInboundMessage;
  queue: ModalHostInboundMessage[];
};

const SKILL_INSTALL_COMMANDS: Record<string, BundledSkillId> = {
  installBrowserControl: 'embeddedBrowser',
  installBrowserUseSkill: 'browser',
  installComputerUseSkill: 'computerUse',
  installCliSkill: 'cli',
  installFable56OrchestrationSkill: 'fable56Orchestration',
  installManageBeadsSkill: 'manageBeads',
  installGenerateTitleSkill: 'generateTitle',
  installManageBeadsSkill: 'manageBeads',
  installMoveCodexSessionSkill: 'moveCodexSession',
};

/** packages/shared/ghostex-agent-skills.ts BundledGhostexAgentSkillId → sandbox skill id. */
const BUNDLED_SKILL_ID_BY_CONTRACT_ID: Record<string, BundledSkillId> = {
  browserUse: 'browser',
  embeddedBrowserUse: 'embeddedBrowser',
  computerUse: 'computerUse',
  cli: 'cli',
  fable56Orchestration: 'fable56Orchestration',
  manageBeads: 'manageBeads',
  generateTitle: 'generateTitle',
  manageBeads: 'manageBeads',
  moveCodexSession: 'moveCodexSession',
};

export function createEngineActions(
  set: StoreApi<SandboxStore>['setState'],
  get: StoreApi<SandboxStore>['getState'],
  _api: StoreApi<SandboxStore>
): SandboxActions {
  /* ---------------------------------------------------------------- runtime */
  /** Bumped on every quit/launch so stale timers from a dead run cannot fire. */
  let epoch = 0;
  let launchStartedAt = Date.now();
  let eventSeq = 0;
  let modalSeq = 0;
  /** Mirrors `self.sidebar.is_some()` — the first-run onboarding guard. */
  let sidebarReady = false;
  /**
   * Once-per-launch in-memory guard (fixed 2026-08-18). The markers are now
   * persisted only after their surface is displayed, so they can no longer act
   * as the dedup between the Track A, Track B and post-respawn attempts —
   * this flag does, exactly like the real app's run-scoped guard.
   */
  let firstRunOnboardingRanThisLaunch = false;
  /** Memory-only for one app run, like `portless_setup_prompt_suppressed_until_restart`. */
  let portlessSuppressedUntilRestart = false;
  /**
   * Fixed 2026-08-18: an occupied app-modal slot defers the portless prompt
   * check instead of dropping it — the check re-runs when the modal closes.
   */
  let portlessCheckDeferredUntilModalCloses = false;
  /** `agent_hook_status_request_in_flight` single-flight guard. */
  let hookStatusInFlight = false;
  /**
   * True once the Tips panel ran the runtime probe. The titlebar keeps the
   * probed `ghostexCliStatus`/`agentHookStatus` in its project state afterwards,
   * so the CLI and hook notices stay visible once they have appeared.
   */
  let tipsRuntimeProbed = false;
  const pendingWindows = new Map<string, PendingWindow>();
  const timers = new Set<number>();

  function later(ms: number, run: () => void): void {
    const startedEpoch = epoch;
    const id = window.setTimeout(
      () => {
        timers.delete(id);
        if (startedEpoch !== epoch) {
          return;
        }
        run();
      },
      Math.max(0, ms)
    );
    timers.add(id);
  }

  function clearTimers(): void {
    for (const id of timers) {
      window.clearTimeout(id);
    }
    timers.clear();
  }

  function emit(kind: SimEvent['kind'], label: string, detail?: string, codeRef?: string): void {
    eventSeq += 1;
    const event: SimEvent = {
      id: eventSeq,
      at: Math.max(0, Date.now() - launchStartedAt),
      launchIndex: get().launchCount,
      kind,
      label,
      ...(detail ? { detail } : {}),
      ...(codeRef ? { codeRef } : {}),
    };
    set((state) => ({ events: [...state.events, event] }));
  }

  function updateEnv(mutate: (env: SimEnvState) => SimEnvState): SimEnvState {
    const next = mutate(get().env);
    saveEnv(next);
    set(() => ({ env: next }));
    refreshTipsNotices();
    return next;
  }

  function updateStateFile(next: FirstRunOnboardingStateFile): void {
    saveStateFile(next);
    set(() => ({ stateFile: next }));
  }

  /* ------------------------------------------------------------------ toasts */

  function showToast(toast: Omit<SimToast, 'id'> & { id: string }): void {
    set((state) => ({
      toasts: [...state.toasts.filter((entry) => entry.id !== toast.id), toast],
    }));
    emit('toast', `Toast: ${toast.title}`, toast.message, CODE_REFS.toast);
    if (toast.autoDismissMs !== null) {
      later(toast.autoDismissMs, () => {
        set((state) => ({ toasts: state.toasts.filter((entry) => entry.id !== toast.id) }));
      });
    }
  }

  /* ------------------------------------------------------------- modal plumbing */

  function deliver(windowId: string, detail: ModalHostInboundMessage): void {
    const pending = pendingWindows.get(windowId);
    if (pending && !pending.ready) {
      pending.queue.push(detail);
      return;
    }
    sendToModalWindow(windowId, detail);
  }

  function dispatchSidebarStateToOpenModals(message: unknown): void {
    for (const modalWindow of get().modalWindows) {
      deliver(modalWindow.windowId, { type: 'sidebarState', message });
    }
  }

  function openModalWindow(
    modal: SandboxModalKind,
    extraPayload: Record<string, unknown>,
    forced: boolean
  ): string | undefined {
    const state = get();
    if (!forced && state.modalWindows.length > 0) {
      emit(
        'warning',
        `Auto-open of "${modal}" DROPPED`,
        `GPUI hosts exactly one app-modal child window and "${state.modalWindows[0].modal}" already owns it, so this queued auto-open is lost for this launch.`,
        CODE_REFS.modalSlot
      );
      return undefined;
    }
    if (forced && state.modalWindows.length > 0) {
      emit(
        'modal',
        `Force-open "${modal}" bypasses the single-window slot`,
        'The real app reuses or replaces its one app-modal window here; the gallery opens a second panel so several modals can be compared side by side.',
        CODE_REFS.modalSlot
      );
    }
    const chrome = modalChrome(modal);
    modalSeq += 1;
    const windowId = `sandbox-modal-${modalSeq}`;
    const payload: Record<string, unknown> = { ...modalOpenPayload(modal), ...extraPayload };

    if (chrome.nonReactHostUrl) {
      /*
       * The tutorial video is the one kind with
       * `uses_react_modal_host() == false`: gpui opens the child window on
       * GHOSTEX_TUTORIAL_VIDEO_URL itself, constructs the host with
       * `is_ready: !uses_react_modal_host` (true), and never sends hydrate or
       * `{type:"open"}` — there is no React bundle in that window to receive
       * them, and no `presented` ever comes back.
       */
      const nonReactRecord: SimModalWindow = {
        windowId,
        modal,
        title: chrome.title,
        width: chrome.width,
        height: chrome.height === 'fit' ? 'fit' : chrome.height,
        presented: true,
        forced,
        openPayload: payload,
        nonReactHostUrl: chrome.nonReactHostUrl,
      };
      set((current) => ({ modalWindows: [...current.modalWindows, nonReactRecord] }));
      emit(
        'modal',
        `Open modal window "${modal}" (${chrome.title}) — non-React host`,
        `${chrome.width}×${chrome.height === 'fit' ? 'fit' : chrome.height}. The child window's top-level document IS the YouTube watch page (sandbox: ${chrome.nonReactHostUrl}, proxied so the same page can be framed).`,
        CODE_REFS.modalOpen
      );
      emit(
        'modal',
        'is_ready = !uses_react_modal_host → the window shows immediately',
        'No modal-host bundle, so no hydrate, no {type:"open"} delivery, no ready/presented handshake and no ready-timeout retry. YouTube rejects the embed player when it is framed from the file:// modal host (missing referrer identity), which is exactly why this kind navigates to the watch page instead.',
        CODE_REFS.tutorialVideoUrl
      );
      return windowId;
    }

    const hydrate = createSandboxHydrateMessage(state.env);
    const openMessage: ModalHostInboundMessage = {
      type: 'open',
      modal,
      ...payload,
      ...(chrome.requiresSidebarState ? { latestSidebarStateMessage: hydrate } : {}),
    };
    pendingWindows.set(windowId, {
      ready: false,
      requiresSidebarState: chrome.requiresSidebarState,
      hydrate,
      openMessage,
      queue: [],
    });
    const record: SimModalWindow = {
      windowId,
      modal,
      title: chrome.title,
      width: chrome.width,
      height: chrome.height === 'fit' ? 'fit' : chrome.height,
      presented: false,
      forced,
      openPayload: payload,
    };
    set((current) => ({ modalWindows: [...current.modalWindows, record] }));
    emit(
      'modal',
      `Open modal window "${modal}" (${chrome.title})`,
      `${chrome.width}×${chrome.height === 'fit' ? 'fit' : chrome.height}${
        chrome.requiresSidebarState ? ' · hydrate sent before open' : ''
      }. The native panel stays hidden until the host posts "presented".`,
      CODE_REFS.modalOpen
    );
    return windowId;
  }

  /**
   * `GpuiAppModalHost::receive_bridge_message` on `ready`: hydrate first when the
   * modal needs a populated sidebar store, then the queued open message. A second
   * `ready` means the iframe document was replaced (dev reload), so the whole
   * opening handshake is replayed instead of leaving an empty panel.
   */
  function flushPendingWindow(windowId: string): void {
    const pending = pendingWindows.get(windowId);
    if (!pending) {
      return;
    }
    pending.ready = true;
    if (pending.requiresSidebarState) {
      sendToModalWindow(windowId, { type: 'sidebarState', message: pending.hydrate });
    }
    sendToModalWindow(windowId, pending.openMessage);
    const queued = pending.queue;
    pending.queue = [];
    for (const message of queued) {
      sendToModalWindow(windowId, message);
    }
  }

  /* --------------------------------------------------------------- status feeds */

  function sendGhostexCliStatus(detailOverride?: string): void {
    dispatchSidebarStateToOpenModals(createGhostexCliStatusMessage(get().env, detailOverride));
  }

  function runProgressiveAgentHookStatus(requestedAgentIds?: readonly string[]): void {
    if (hookStatusInFlight) {
      emit(
        'message',
        'requestAgentHookStatus dropped — a status walk is already in flight',
        'One status request runs at a time; overlapping requests are dropped exactly like the macOS in-flight guard.',
        CODE_REFS.progressiveHooks
      );
      return;
    }
    const ordered = orderedHookStatusAgentIds(requestedAgentIds);
    if (ordered.length === 0) {
      return;
    }
    hookStatusInFlight = true;
    emit(
      'flow',
      `Progressive agent hook status for ${ordered.length} agent(s)`,
      `Priority order first (${PRIORITY_AGENT_IDS.join(', ')}), one merged agentHookStatus message per probed agent.`,
      CODE_REFS.progressiveHooks
    );
    const step = (index: number): void => {
      if (index >= ordered.length) {
        hookStatusInFlight = false;
        return;
      }
      later(get().env.timing.hookStatusPerAgentMs, () => {
        const env = get().env;
        const probed = ordered.slice(0, index + 1);
        const agentId = ordered[index];
        dispatchSidebarStateToOpenModals(createAgentHookStatusMessage(env, probed));
        emit(
          'message',
          `agentHookStatus + ${agentId}: ${deriveAgentHookStatus(env, agentId)}`,
          `Merged payload now carries ${probed.length} of ${ordered.length} agents.`,
          CODE_REFS.progressiveHooks
        );
        step(index + 1);
      });
    };
    step(0);
  }

  function requestedAgentIds(command: Record<string, unknown>): SimAgentId[] {
    const raw = command.agentIds;
    const ids = Array.isArray(raw) ? raw.filter((id): id is string => typeof id === 'string') : [];
    return orderedHookStatusAgentIds(ids.length > 0 ? ids : undefined);
  }

  function runHookInstallAction(command: Record<string, unknown>, install: boolean): void {
    const agentIds = requestedAgentIds(command);
    emit(
      'flow',
      `${install ? 'Install' : 'Uninstall'} agent hooks: ${agentIds.join(', ')}`,
      undefined,
      CODE_REFS.sidebarCommand
    );
    later(get().env.timing.installActionMs, () => {
      const skipped: SimAgentId[] = [];
      const env = updateEnv((current) => {
        const agents = { ...current.agents };
        for (const agentId of agentIds) {
          if (!agents[agentId].cliInstalled) {
            skipped.push(agentId);
            continue;
          }
          agents[agentId] = {
            ...agents[agentId],
            hookState: install ? 'installed' : 'notInstalled',
          };
        }
        return { ...current, agents };
      });
      if (skipped.length > 0) {
        emit(
          'message',
          `No-op for ${skipped.join(', ')} — CLI missing`,
          'gxserver reports cliMissing for agents whose CLI is not on PATH; the hook file is never written.',
          'server/src/agent_hooks/api.rs read_hook_status'
        );
      }
      dispatchSidebarStateToOpenModals(createAgentHookStatusMessage(env, agentIds));
      emit('message', 'agentHookStatus refreshed after install action', undefined, CODE_REFS.sidebarCommand);
    });
  }

  function runCliSettingsAction(commandType: string, command: Record<string, unknown>): void {
    emit('flow', `Integration action: ${commandType}`, undefined, CODE_REFS.cliSettingsAction);
    later(get().env.timing.installActionMs, () => {
      updateEnv((current) => {
        if (commandType === 'installGhostexCli') {
          return {
            ...current,
            ghostexCli: {
              ...current.ghostexCli,
              installed: true,
              gxUsable: !current.ghostexCli.gxBlockedByExistingCommand,
            },
          };
        }
        if (commandType === 'installCuaDriver') {
          return {
            ...current,
            cuaDriver: { ...current.cuaDriver, appInstalled: true, cliInstalled: true },
          };
        }
        if (commandType === 'uninstallBundledAgentSkills') {
          const bundledSkills = { ...current.bundledSkills };
          for (const skillId of Object.keys(bundledSkills) as BundledSkillId[]) {
            bundledSkills[skillId] = false;
          }
          return { ...current, bundledSkills };
        }
        if (commandType === 'uninstallBundledAgentSkill') {
          const contractId = typeof command.skillId === 'string' ? command.skillId : '';
          const skillId = BUNDLED_SKILL_ID_BY_CONTRACT_ID[contractId];
          if (!skillId) {
            return current;
          }
          return {
            ...current,
            bundledSkills: { ...current.bundledSkills, [skillId]: false },
          };
        }
        const installedSkill = SKILL_INSTALL_COMMANDS[commandType];
        if (installedSkill) {
          return {
            ...current,
            bundledSkills: { ...current.bundledSkills, [installedSkill]: true },
          };
        }
        return current;
      });
      sendGhostexCliStatus();
      const env = get().env;
      emit(
        'message',
        'ghostexCliStatus refreshed',
        `First-launch gates → hooks ready: ${areFirstLaunchAgentHooksReady(env)}, all bundled skills installed: ${areFirstLaunchBundledSkillsInstalled(env)}.`,
        CODE_REFS.cliSettingsAction
      );
    });
  }

  function applySettingsCommand(command: Record<string, unknown>): void {
    const settings = command.settings;
    if (!settings || typeof settings !== 'object') {
      return;
    }
    const record = settings as Record<string, unknown>;
    const debuggingMode = record.debuggingMode === true;
    updateEnv((current) => ({
      ...current,
      settings: { debuggingMode },
    }));
    emit(
      'state',
      'updateSettings applied to the simulated settings service',
      `debuggingMode: ${debuggingMode}.`,
      'apps/desktop/src/app/remote_conn/settings_and_install_probe.rs:6 handle_gpui_app_modal_update_settings_message'
    );
    const hydrate = createSandboxHydrateMessage(get().env);
    dispatchSidebarStateToOpenModals(hydrate);
  }

  /* ---------------------------------------------------------------------- tips */

  function refreshTipsNotices(): void {
    const state = get();
    const notices = tipsRuntimeProbed ? probedTipsNotices(state.env) : settingsDerivedTipsNotices(state.env);
    set(() => ({
      tipsNotices: notices,
      tipsBadgeCount: tipsBadgeCount(TITLEBAR_TIP_IDS.length, notices),
    }));
  }

  /* ------------------------------------------------------------- launch tracks */

  function runPortlessSetupPromptCheck(source: string): void {
    if (portlessSuppressedUntilRestart) {
      emit(
        'flow',
        `Portless prompt check skipped (${source})`,
        'Already suppressed for this app run; the suppression resets only on relaunch.',
        CODE_REFS.portlessCheck
      );
      return;
    }
    portlessSuppressedUntilRestart = true;
    emit(
      'flow',
      `Portless setup prompt check (${source}) — compile-time disabled`,
      'GPUI_PORTLESS_APP_INTEGRATION_ENABLED is false (apps/desktop/src/app/helpers/os_cli/process_and_constants.rs:230), so the check immediately suppresses the prompt for this run and disables Portless state. No modal can ever appear here today.',
      CODE_REFS.portlessCheck
    );
    maybeOpenPortlessSetupPrompt(source);
  }

  /**
   * `maybe_open_gpui_portless_setup_prompt`. GPUI hosts exactly one app-modal
   * child window, so auto-opening the prompt while another modal owns the slot
   * would replace it. Fixed 2026-08-18: that case now DEFERS the check (it
   * re-runs when the slot frees up) instead of dropping it for the whole run.
   */
  function maybeOpenPortlessSetupPrompt(source: string): void {
    if (get().modalWindows.length === 0) {
      return;
    }
    portlessCheckDeferredUntilModalCloses = true;
    emit(
      'flow',
      `Portless prompt auto-open DEFERRED — the app-modal slot is busy (${source})`,
      `"${get().modalWindows[0].modal}" owns the single app-modal window. Fixed 2026-08-18: the check is re-armed and re-runs on modal close instead of being dropped for this launch. It still resolves to nothing today because the feature is compile-time disabled.`,
      CODE_REFS.portlessCheck
    );
  }

  function runGxserverRespawn(): void {
    emit(
      'flow',
      'stop_gpui_local_gxserver_from_titlebar(restart) → respawn poll',
      'The daemon is restarted and re-probed every 500ms for up to 20s.',
      CODE_REFS.bootstrap
    );
    showToast({
      id: 'gpui-gxserver-bootstrap',
      kind: GXSERVER_LOADING_TOAST.kind,
      title: GXSERVER_LOADING_TOAST.title,
      message: GXSERVER_LOADING_TOAST.message,
      autoDismissMs: null,
    });
    later(Math.max(300, get().env.timing.gxserverProbeMs * 2), () => {
      if (!get().env.gxserver.respawnFixesHealth) {
        showToast({
          id: 'gpui-gxserver-bootstrap',
          kind: GXSERVER_RESPAWN_FAILURE_TOAST.kind,
          title: GXSERVER_RESPAWN_FAILURE_TOAST.title,
          message: GXSERVER_RESPAWN_FAILURE_TOAST.message,
          autoDismissMs: null,
        });
        return;
      }
      updateEnv((current) => ({
        ...current,
        gxserver: { ...current.gxserver, scenario: 'healthyToolsAvailable' },
      }));
      set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== 'gpui-gxserver-bootstrap') }));
      emit(
        'flow',
        'Respawned gxserver is healthy → replay_sidebar_gxserver_bootstrap',
        'Sessions and projects load now. The environment healed, so the NEXT launch takes the healthy branch.',
        CODE_REFS.bootstrap
      );
      runPortlessSetupPromptCheck('after respawn');
      emit(
        'flow',
        'Respawn success path also re-runs first-run onboarding',
        'Fixed 2026-08-18: the healed branch now calls start_gpui_first_run_onboarding alongside replay_sidebar_gxserver_bootstrap + the portless check. It is idempotent — the once-per-launch guard and the persisted markers make a second call a no-op — so a user whose daemon needed an upgrade finally sees onboarding on THIS launch instead of having to restart.',
        CODE_REFS.bootstrap
      );
      runFirstRunOnboarding('gxserver respawn (healed)');
    });
  }

  function runTrackAGxserverBootstrap(): void {
    const env = get().env;
    const scenario = env.gxserver.scenario;
    emit('flow', `Track A: gxserver health probe resolved → ${scenario}`, undefined, CODE_REFS.healthProbe);
    if (scenario === 'healthyToolsAvailable') {
      emit('flow', 'replay_sidebar_gxserver_bootstrap — sidebar loads projects', undefined, CODE_REFS.bootstrap);
      runPortlessSetupPromptCheck('healthy bootstrap');
      runFirstRunOnboarding('Track A (gxserver bootstrap)');
      return;
    }
    const toast = GXSERVER_BRANCH_TOASTS[scenario];
    if (toast) {
      showToast({
        id: 'gpui-gxserver-bootstrap',
        kind: toast.kind,
        title: toast.title,
        message: toast.message,
        autoDismissMs: null,
      });
    }
    emit(
      'flow',
      `Unhealthy gxserver branch ("${scenario}") defers onboarding to the repair path`,
      'This branch still returns before start_gpui_first_run_onboarding — the CEF attempt (Track B) normally owns onboarding. Fixed 2026-08-18: when the daemon respawn heals the environment, that success path re-runs first-run onboarding too, so onboarding is no longer lost until the next app restart.',
      CODE_REFS.bootstrap
    );
    if (scenario === 'spawnFailure') {
      return;
    }
    if (scenarioRestartsDaemon(scenario)) {
      runGxserverRespawn();
      return;
    }
    /* protocolMismatch: macOS shows the error toast and stops. */
    if (get().env.gxserver.respawnFixesHealth) {
      emit(
        'flow',
        'Sandbox extension: respawn-heals is on, so the daemon is restarted anyway',
        'Real macOS returns immediately on a protocol mismatch (apps/desktop/src/app/os_integration/gxserver_bootstrap.rs:33) — no respawn, no further work this launch.',
        CODE_REFS.bootstrap
      );
      runGxserverRespawn();
    }
  }

  function runTrackBCefInit(): void {
    sidebarReady = true;
    set(() => ({ appPhase: 'running' }));
    emit(
      'flow',
      'Track B: CEF initialized — sidebar surface exists (self.sidebar = Some)',
      undefined,
      CODE_REFS.cefReady
    );
    runFirstRunOnboarding('Track B (CEF sidebar ready)');
  }

  function runFirstRunOnboarding(source: string): void {
    emit('flow', `start_gpui_first_run_onboarding — attempt from ${source}`, undefined, CODE_REFS.firstRun);
    if (!sidebarReady) {
      emit(
        'warning',
        'Attempt dropped: self.sidebar is None',
        'The sidebar guard is the FIRST statement of the function, so this attempt is a pure no-op: no flag is read, nothing is persisted, and nothing is queued for later. Whichever attempt runs after CEF is ready owns the whole onboarding.',
        CODE_REFS.firstRun
      );
      return;
    }
    if (firstRunOnboardingRanThisLaunch) {
      emit(
        'flow',
        'Attempt skipped — first-run onboarding already ran this launch',
        'Fixed 2026-08-18: the once-per-launch in-memory guard is checked by BOTH tracks (and by the post-respawn re-run). It replaces the old flag-based dedup, which stopped working once the markers moved behind the display.',
        CODE_REFS.firstRun
      );
      return;
    }
    firstRunOnboardingRanThisLaunch = true;
    const env = get().env;
    const current = get().stateFile;
    const next: FirstRunOnboardingStateFile = { ...current };
    let changed = false;

    if (!next.tipsAndTricksSeen) {
      next.tipsAndTricksSeen = true;
      changed = true;
      emit(
        'state',
        'Flag burned: tipsAndTricksSeen = true',
        'Marked seen silently. No Tips & Tricks modal is ever opened by this path, so this marker is still written in the background pass.',
        CODE_REFS.firstRun
      );
    }
    if (next.highlightedFeaturesSeenRevision !== HIGHLIGHTED_FEATURES_SEEN_REVISION) {
      next.highlightedFeaturesSeenRevision = HIGHLIGHTED_FEATURES_SEEN_REVISION;
      changed = true;
      emit(
        'state',
        `Flag burned: highlightedFeaturesSeenRevision = "${HIGHLIGHTED_FEATURES_SEEN_REVISION}"`,
        'Marked seen silently — the Discover Ghostex tour is never auto-shown anymore, yet the revision is consumed on first launch. Silent marker, so it also stays in the background pass.',
        CODE_REFS.firstRun
      );
    }
    const showOsIntegrationToast = !next.osIntegrationOnboardingSeen;
    /*
     * Fixed 2026-08-19: first run raises exactly ONE modal. The tutorial video
     * used to be its own window, opened before (Windows) or instead of
     * (everywhere else) the setup modal; it is the setup modal's first page
     * now. Two markers gate it and they mean different things: the revision
     * marker says "this revision's setup was presented" (written when the
     * window exists, so a revision bump shows it again), and `complete` says
     * "the user closed it" (written on close, so quitting mid-setup brings it
     * back next launch).
     */
    const needsFirstLaunchSetup =
      next.firstLaunchSetupSeenRevision !== FIRST_LAUNCH_SETUP_SEEN_REVISION || !next.firstLaunchSetupComplete;
    if (changed) {
      updateStateFile(next);
      emit(
        'state',
        'persist_gpui_first_run_onboarding_state — silent legacy markers only',
        'Fixed 2026-08-18: the background pass still burns the two markers whose surfaces are never shown, but firstLaunchSetupSeenRevision and osIntegrationOnboardingSeen are NOT written here anymore. They are persisted after their surface is actually on screen, so a crash, a quit, or a dropped auto-open no longer swallows the onboarding forever.',
        CODE_REFS.persistState
      );
    }

    if (!showOsIntegrationToast && !needsFirstLaunchSetup) {
      emit(
        'flow',
        'Nothing to show — onboarding returns early',
        'Every marker was already consumed by an earlier launch (or by the earlier attempt in this same launch).',
        CODE_REFS.firstRun
      );
      return;
    }

    if (showOsIntegrationToast) {
      showToast({
        id: OS_INTEGRATION_TOAST.id,
        kind: 'info',
        title: OS_INTEGRATION_TOAST.title,
        message: OS_INTEGRATION_TOAST.message,
        autoDismissMs: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
      });
      updateStateFile({ ...get().stateFile, osIntegrationOnboardingSeen: true });
      emit(
        'state',
        'Marker persisted AFTER the toast is shown: osIntegrationOnboardingSeen = true',
        'Fixed 2026-08-18: the write follows the display instead of preceding it, so the toast can only be consumed once it has actually been raised.',
        CODE_REFS.persistState
      );
    }
    if (needsFirstLaunchSetup) {
      const setupWindowId = openModalWindow('firstLaunchSetup', {}, false);
      if (setupWindowId) {
        updateStateFile({
          ...get().stateFile,
          firstLaunchSetupSeenRevision: FIRST_LAUNCH_SETUP_SEEN_REVISION,
        });
        emit(
          'state',
          `Marker persisted AFTER the setup window exists: firstLaunchSetupSeenRevision = "${FIRST_LAUNCH_SETUP_SEEN_REVISION}"`,
          'Fixed 2026-08-18: the marker is written only once the child window was actually created. `firstLaunchSetupComplete` follows separately, when the user closes the modal.',
          CODE_REFS.persistState
        );
        emit(
          'flow',
          "One modal on startup: the tutorial video is the setup modal's first page",
          'Fixed 2026-08-19: the video used to be its own child window opened before or instead of the setup modal. It is an iframe on page 1 now, pointed at a player page the host serves from a real origin (a file:// document cannot embed YouTube — Error 153).',
          CODE_REFS.firstLaunchSetup
        );
      } else {
        emit(
          'warning',
          'Setup window was NOT created — firstLaunchSetupSeenRevision stays unburned',
          'Fixed 2026-08-18: a dropped auto-open (the single app-modal slot was busy) no longer consumes the marker, so first-run setup is offered again on the next launch.',
          CODE_REFS.persistState
        );
      }
    }
  }

  /* ------------------------------------------------------------ outbound router */

  function handleAddProjectRequest(windowId: string, message: ModalHostOutboundMessage): void {
    const requestId = typeof message.requestId === 'string' ? message.requestId : '';
    emit(
      'message',
      `addProjectDialogRequest: ${String(message.operation ?? 'unknown')}`,
      undefined,
      CODE_REFS.addProject
    );
    void answerAddProjectRequest(message).then((answer) => {
      if (answer.addedProject) {
        updateEnv((current) => ({ ...current, projectCount: current.projectCount + 1 }));
        emit('state', `Project added — projectCount = ${get().env.projectCount}`, undefined, CODE_REFS.addProject);
      }
      deliver(windowId, {
        type: 'addProjectDialogResult',
        requestId,
        ok: answer.ok,
        ...(answer.result === undefined ? {} : { result: answer.result }),
        ...(answer.error ? { error: answer.error } : {}),
      });
    });
  }

  function handleSidebarCommand(windowId: string, message: ModalHostOutboundMessage): void {
    const command = message.message;
    if (!command || typeof command !== 'object') {
      return;
    }
    const record = command as Record<string, unknown>;
    const commandType = typeof record.type === 'string' ? record.type : '';
    switch (commandType) {
      case 'requestAgentHookStatus': {
        const ids = Array.isArray(record.agentIds)
          ? record.agentIds.filter((id): id is string => typeof id === 'string')
          : undefined;
        runProgressiveAgentHookStatus(ids);
        return;
      }
      case 'requestGhostexCliStatus':
        emit('flow', 'requestGhostexCliStatus', undefined, CODE_REFS.sidebarCommand);
        later(120, () => sendGhostexCliStatus());
        return;
      case 'installAgentHooks':
        runHookInstallAction(record, true);
        return;
      case 'uninstallAgentHooks':
        runHookInstallAction(record, false);
        return;
      case 'installGhostexCli':
      case 'installBrowserControl':
      case 'installBrowserUseSkill':
      case 'installComputerUseSkill':
      case 'installCliSkill':
      case 'installFable56OrchestrationSkill':
      case 'installManageBeadsSkill':
      case 'installGenerateTitleSkill':
      case 'installManageBeadsSkill':
      case 'installMoveCodexSessionSkill':
      case 'installCuaDriver':
      case 'uninstallBundledAgentSkill':
      case 'uninstallBundledAgentSkills':
        runCliSettingsAction(commandType, record);
        return;
      case 'updateSettings':
        applySettingsCommand(record);
        return;
      case 'openExternalUrl':
        emit(
          'flow',
          `Open external URL: ${String(record.url ?? '')}`,
          'The real app hands this to the system browser (the trycua/cua link on the skills page comes through here).',
          CODE_REFS.sidebarCommand
        );
        return;
      case 'openWorkspaceWelcome':
        emit('flow', 'Tips → Welcome opens firstLaunchSetup', undefined, CODE_REFS.sidebarCommand);
        openModalWindow('firstLaunchSetup', {}, true);
        return;
      case 'openGhostexTutorialVideo':
        emit('flow', 'Tips → Tutorial video', undefined, CODE_REFS.sidebarCommand);
        openModalWindow('watchGhostexVideo', {}, true);
        return;
      default:
        emit(
          'message',
          `Unhandled sidebarCommand: ${commandType || '(no type)'}`,
          `From ${windowId}. The real app routes this through handle_gpui_app_modal_sidebar_command; the sandbox only simulates the onboarding-relevant commands.`,
          CODE_REFS.sidebarCommand
        );
    }
  }

  function handleOutbound(windowId: string, message: ModalHostOutboundMessage): void {
    switch (message.type) {
      case 'ready':
        emit(
          'message',
          `Modal host ready (${windowId})`,
          'The host posts {type:"ready"} on mount; queued sidebarState + open messages are dispatched now.',
          CODE_REFS.modalReady
        );
        flushPendingWindow(windowId);
        return;
      case 'presented': {
        set((state) => ({
          modalWindows: state.modalWindows.map((entry) =>
            entry.windowId === windowId ? { ...entry, presented: true } : entry
          ),
        }));
        emit(
          'modal',
          `Presented: ${String(message.modal ?? '')}`,
          'The native panel becomes visible only now — before this the window is hidden.',
          CODE_REFS.modalReady
        );
        return;
      }
      case 'contentHeightMeasured':
        emit(
          'modal',
          `contentHeightMeasured: ${String(message.height ?? '')}px`,
          'One-shot fit-height measurement; the real app clamps it to 200..850 and resizes the child window once.',
          CODE_REFS.modalReady
        );
        return;
      case 'close':
        closeModalWindow(windowId);
        return;
      case 'toastDismissed':
        if (message.keepOpen !== true) {
          closeModalWindow(windowId);
        }
        return;
      case 'sidebarCommand':
        handleSidebarCommand(windowId, message);
        return;
      case 'addProjectDialogRequest':
        handleAddProjectRequest(windowId, message);
        return;
      case 'debugLog':
        emit(
          'message',
          `debugLog: ${String(message.event ?? '')}`,
          typeof message.details === 'string' ? message.details : undefined
        );
        return;
      case 'downloadGhostexUpdate':
      case 'restartAndUpdateGhostex':
        emit(
          'message',
          `Update action: ${message.type}`,
          'Sparkle owns this in the real app; the sandbox only records it.'
        );
        return;
      default:
        emit('message', `Unrouted host message: ${message.type}`, `From ${windowId}.`);
    }
  }

  setModalOutboundHandler(handleOutbound);

  /* ----------------------------------------------------------------- lifecycle */

  function closeModalWindow(windowId: string): void {
    const modalWindow = get().modalWindows.find((entry) => entry.windowId === windowId);
    if (!modalWindow) {
      return;
    }
    pendingWindows.delete(windowId);
    set((state) => ({
      modalWindows: state.modalWindows.filter((entry) => entry.windowId !== windowId),
    }));
    emit('modal', `Closed modal window "${modalWindow.modal}"`, undefined, CODE_REFS.modalSlot);
    if (portlessCheckDeferredUntilModalCloses && get().modalWindows.length === 0) {
      portlessCheckDeferredUntilModalCloses = false;
      emit(
        'flow',
        'Deferred portless prompt check re-runs now that the app-modal slot is free',
        'Fixed 2026-08-18: closing the modal re-arms the check that used to be dropped. It reports the compile-time disable below, which is the only reason nothing opens.',
        CODE_REFS.portlessCheck
      );
      runPortlessSetupPromptCheck('deferred re-check after modal close');
    }
    if (modalWindow.modal === 'firstLaunchSetup') {
      const stateFile = get().stateFile;
      if (!stateFile.firstLaunchSetupComplete) {
        updateStateFile({ ...stateFile, firstLaunchSetupComplete: true });
        emit(
          'state',
          'Flag burned: firstLaunchSetupComplete = true',
          'First-launch setup counts as complete when the dialog closes, however far the user got.',
          'apps/desktop/src/app/modals.rs:1353 complete_first_launch_setup'
        );
      }
    }
  }

  function launchApp(): void {
    if (get().appPhase !== 'notRunning') {
      emit('warning', 'Launch ignored — the app is already running');
      return;
    }
    epoch += 1;
    clearTimers();
    pendingWindows.clear();
    sidebarReady = false;
    firstRunOnboardingRanThisLaunch = false;
    portlessSuppressedUntilRestart = false;
    portlessCheckDeferredUntilModalCloses = false;
    hookStatusInFlight = false;
    tipsRuntimeProbed = false;
    launchStartedAt = Date.now();
    const launchCount = get().launchCount + 1;
    saveLaunchCount(launchCount);
    const env = get().env;
    const notices = settingsDerivedTipsNotices(env);
    set(() => ({
      appPhase: 'launching',
      launchCount,
      toasts: [],
      modalWindows: [],
      tipsPanelOpen: false,
      tipsNotices: notices,
      tipsBadgeCount: tipsBadgeCount(TITLEBAR_TIP_IDS.length, notices),
    }));
    emit(
      'flow',
      `Launch #${launchCount} — cx.open_window`,
      `Platform: ${env.platform}. Two detached tracks start in parallel.`,
      'apps/desktop/src/main.rs main'
    );
    emit(
      'flow',
      `Track A scheduled: gxserver bootstrap in ${env.timing.gxserverProbeMs}ms`,
      undefined,
      CODE_REFS.bootstrap
    );
    emit(
      'flow',
      `Track B scheduled: CEF init in ${env.timing.cefInitMs}ms`,
      'Whichever track finishes first calls start_gpui_first_run_onboarding.',
      CODE_REFS.cefReady
    );
    emit(
      'flow',
      `Tips badge starts at ${TITLEBAR_TIP_IDS.length} unread tips + ${notices.length} settings notice(s)`,
      'CLI and agent-hook notices are NOT probed at startup — they appear only when the Tips panel is opened.',
      CODE_REFS.tipsRuntimeStatus
    );
    later(env.timing.gxserverProbeMs, runTrackAGxserverBootstrap);
    later(env.timing.cefInitMs, runTrackBCefInit);
  }

  function quitApp(): void {
    if (get().appPhase === 'notRunning') {
      return;
    }
    epoch += 1;
    clearTimers();
    pendingWindows.clear();
    sidebarReady = false;
    firstRunOnboardingRanThisLaunch = false;
    portlessCheckDeferredUntilModalCloses = false;
    hookStatusInFlight = false;
    tipsRuntimeProbed = false;
    set(() => ({
      appPhase: 'notRunning',
      toasts: [],
      modalWindows: [],
      tipsPanelOpen: false,
    }));
    emit(
      'flow',
      'Quit — every window closed',
      'The state file survives; in-memory suppressions (portless suppressed-until-restart, the modal slot, the Windows followup) reset.',
      'apps/desktop/src/app/modals.rs:2028 flush_gpui_quit_persistence'
    );
    refreshTipsNotices();
  }

  /* -------------------------------------------------------------------- actions */

  return {
    patchEnv: (patch) => {
      updateEnv((current) => ({ ...current, ...patch }));
    },
    setAgentState: (agentId, patch) => {
      updateEnv((current) => ({
        ...current,
        agents: { ...current.agents, [agentId]: { ...current.agents[agentId], ...patch } },
      }));
    },
    patchStateFile: (patch) => {
      updateStateFile({ ...get().stateFile, ...patch });
    },
    wipeStateFile: () => {
      updateStateFile(createDefaultStateFile());
      emit(
        'state',
        'Wiped gpui-first-run-onboarding-state.json',
        'The next launch behaves exactly like a brand-new user.',
        CODE_REFS.persistState
      );
    },
    applyPreset: (presetId) => {
      const preset = SCENARIO_PRESETS.find((entry) => entry.id === presetId);
      if (!preset) {
        emit('warning', `Unknown preset "${presetId}"`);
        return;
      }
      updateEnv((current) =>
        typeof preset.apply.env === 'function' ? preset.apply.env(current) : { ...current, ...preset.apply.env }
      );
      if (preset.apply.wipeStateFile) {
        updateStateFile(createDefaultStateFile());
      }
      if (preset.apply.stateFile) {
        updateStateFile({ ...get().stateFile, ...preset.apply.stateFile });
      }
      emit('state', `Preset applied: ${preset.label}`, preset.description);
    },
    launchApp,
    quitApp,
    relaunchApp: () => {
      quitApp();
      launchApp();
    },
    forceOpenModal: (kind, payload) => {
      emit(
        'modal',
        `Gallery force-open: ${kind}`,
        'The real app has no way to do this: every modal here is reachable only through its own trigger, and the single app-modal window would replace whatever is open.',
        CODE_REFS.modalSlot
      );
      openModalWindow(kind, payload ?? {}, true);
    },
    closeModalWindow,
    dismissToast: (toastId) => {
      set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== toastId) }));
    },
    setTipsPanelOpen: (open) => {
      set(() => ({ tipsPanelOpen: open }));
      if (!open) {
        refreshTipsNotices();
        return;
      }
      emit(
        'flow',
        'Tips panel opened → request_gpui_titlebar_tips_runtime_status',
        'Only now does the app probe ghostexCliStatus and walk the agent hook status, so the CLI and missing-hook notices can appear.',
        CODE_REFS.tipsRuntimeStatus
      );
      tipsRuntimeProbed = true;
      const env = get().env;
      const notices = probedTipsNotices(env);
      set(() => ({
        tipsNotices: notices,
        tipsBadgeCount: tipsBadgeCount(TITLEBAR_TIP_IDS.length, notices),
      }));
      for (const notice of notices) {
        emit('message', `Tips notice: ${notice.title}`, notice.body, CODE_REFS.tipsRuntimeStatus);
      }
      if (notices.length === 0) {
        emit('message', 'Tips runtime probe found no notices', undefined, CODE_REFS.tipsRuntimeStatus);
      }
    },
    clearEvents: () => {
      set(() => ({ events: [] }));
    },
    emitEvent: (event) => {
      emit(event.kind, event.label, event.detail, event.codeRef);
    },
  };
}

/** Exported for the control panel's agent grid ordering. */
export const SANDBOX_AGENT_ORDER: readonly SimAgentId[] = [
  ...PRIORITY_AGENT_IDS,
  ...SIM_AGENT_IDS.filter((agentId) => !PRIORITY_AGENT_IDS.includes(agentId)),
];
