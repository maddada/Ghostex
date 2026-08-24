/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import { runGpuiSidebarBulkSleepPaced } from '../bulk-sleep-pacing';
import { getGpuiWorkspaceSessionSubgroups, parseGpuiWorkspaceSessionSubgroupId } from '../workspace-session-groups';
import { GPUI_AUTO_SLEEP_MONITOR_INTERVAL_MS, GPUI_QUICK_AUTOMATIONS_PROJECT_ID } from './constants';
import type { GpuiSidebarRuntime } from './core';
import { createGpuiAutoSleepAgentSessionIds, gxserverSleepWasDeclined } from './helpers/auto-sleep';
import { createGpuiSidebarSettings } from './helpers/bootstrap';
import { gpuiBrowserSidebarSessionId } from './helpers/browser-tabs';
import { isGpuiInactiveProjectPresentationSession } from './helpers/close-after-done';
import {
  createGpuiRemotePresentationProjectId,
  createGpuiRemotePresentationSessionId,
  parseGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationSessionId,
} from './helpers/remote-presentation';
import {
  createGxserverPresentationProjectSessionId,
  parseGxserverPresentationProjectGroupId,
  parseGxserverPresentationProjectSessionId,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type { GxserverSleepSessionResult } from '@/packages/shared/gxserver-protocol';

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeAutoSleepMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeAutoSleepMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeAutoSleepMethods {
  startGpuiAutoSleepMonitor(): void;
  runGpuiAutoSleepMonitor(_source: 'interval' | 'settings-change' | 'startup'): Promise<void>;
  sleepInactiveSessionsFromTitlebar(): Promise<void>;
  sleepAllLocalDaemonSessions(): Promise<void>;
  setGroupSleeping(groupId: string, sleeping: boolean): Promise<void>;
  setSessionsSleeping(sessionIds: readonly string[], sleeping: boolean): Promise<void>;
  setSessionSleeping(
    sessionId: string,
    sleeping: boolean,
    options?: { automatic?: boolean; forceRemount?: boolean }
  ): Promise<void>;
  closeInactiveProjectSessions(groupId: string): Promise<void>;
  sleepInactiveProjectSessions(groupId: string): Promise<void>;
  collectInactiveProjectSessionIds(groupId: string): string[];
  wakeProjectSleepingSessions(groupId: string): Promise<void>;
}

export const gpuiSidebarRuntimeAutoSleepMethods = {
  startGpuiAutoSleepMonitor(this: GpuiSidebarRuntime): void {
    if (this.autoSleepMonitorIntervalId !== undefined) {
      return;
    }
    /*
    CDXC:GPUISidebarAutoSleep 2026-06-27-01:24:
    GPUI owns only the SidebarApp/gxserver runtime policy loop for agent terminal Auto Sleep. Run a small idempotent monitor from the runtime lifecycle, use the normalized shared settings snapshot, and route every sleep through the existing gxserver session lifecycle path instead of adding Browser, project-editor, native-pane, or renderer-local sleep behavior.
    */
    this.autoSleepMonitorIntervalId = window.setInterval(() => {
      void this.runGpuiAutoSleepMonitor('interval');
    }, GPUI_AUTO_SLEEP_MONITOR_INTERVAL_MS);
    void this.runGpuiAutoSleepMonitor('startup');
  },

  async runGpuiAutoSleepMonitor(
    this: GpuiSidebarRuntime,
    _source: 'interval' | 'settings-change' | 'startup'
  ): Promise<void> {
    if (this.autoSleepMonitorRunning) {
      return;
    }
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    if (!settings.autoSleepAgentSessionsEnabled || !this.presentation) {
      return;
    }
    const sessionIdsToSleep = createGpuiAutoSleepAgentSessionIds({
      activeProjectId: this.activeProjectId,
      commandPaneSessions: this.commandPaneSessions,
      delayedSendSessionIds: [...this.workspaceSessionDelayedSends.keys()],
      displayedWorkspaceSessionIds: this.displayedWorkspaceSessionIds,
      focusedSessionId: this.focusedSessionId,
      groups: this.latestGroups,
      nowMs: Date.now(),
      presentation: this.presentation,
      settings,
    });
    if (sessionIdsToSleep.length === 0) {
      return;
    }
    this.autoSleepMonitorRunning = true;
    try {
      /*
      CDXC:GPUISidebarAutoSleep 2026-06-27-02:05:
      Auto Sleep must match native bulk sleep pacing: eligible agent sessions sleep one at a time with a 350 ms gap so gxserver and terminal teardown are not hit concurrently. Use the shared aggregate-count helper and ignore its private-data-free result because monitor progress is already reflected by gxserver presentation updates.
      */
      await runGpuiSidebarBulkSleepPaced(sessionIdsToSleep, async (sessionId) => {
        /*
        CDXC:MobileKeepAwake 2026-08-19:
        The sweep marks itself automatic so gxserver can decline it for a session
        another client (Ghostex mobile) is attached to. This client cannot see
        those attachments, and the daemon can.
        */
        await this.setSessionSleeping(sessionId, true, { automatic: true });
      });
    } finally {
      this.autoSleepMonitorRunning = false;
    }
  },

  async sleepInactiveSessionsFromTitlebar(this: GpuiSidebarRuntime): Promise<void> {
    /*
    macOS's titlebar Resources shortcut revalidates and sleeps every inactive
    awake terminal. GPUI derives the same set from the shared inactive-session
    filter used by per-project bulk sleep, across the local daemon and every
    connected remote presentation.
    */
    const sessionIds: string[] = [];
    for (const tab of this.browserTabs) {
      if (!tab.isSleeping && !tab.isVisible) {
        sessionIds.push(gpuiBrowserSidebarSessionId(tab));
      }
    }
    for (const session of this.presentation?.sessions ?? []) {
      if (isGpuiInactiveProjectPresentationSession(session)) {
        sessionIds.push(createGxserverPresentationProjectSessionId(session.projectId, session.sessionId));
      }
    }
    for (const [machineId, presentation] of this.remotePresentations) {
      for (const session of presentation.sessions ?? []) {
        if (isGpuiInactiveProjectPresentationSession(session)) {
          sessionIds.push(createGpuiRemotePresentationSessionId(machineId, session.projectId, session.sessionId));
        }
      }
    }
    if (sessionIds.length === 0) {
      return;
    }
    await this.setSessionsSleeping(sessionIds, true);
  },

  /*
  macOS killTerminalDaemon parity: since the gxserver cutover the Running
  Sessions daemon-stop control is a local-first bulk sleep — macOS routes
  every awake gxserver-presented terminal through the shared sleep path and
  leaves the shared daemon process running. GPUI sleeps every non-sleeping
  local daemon session the same way; remote presentations are untouched
  because the modal lists local daemon state.
  */
  async sleepAllLocalDaemonSessions(this: GpuiSidebarRuntime): Promise<void> {
    const sessionIds = this.browserTabs.filter((tab) => !tab.isSleeping).map(gpuiBrowserSidebarSessionId);
    for (const session of this.presentation?.sessions ?? []) {
      if (session.lifecycleState !== 'sleeping') {
        sessionIds.push(createGxserverPresentationProjectSessionId(session.projectId, session.sessionId));
      }
    }
    if (sessionIds.length === 0) {
      return;
    }
    await this.setSessionsSleeping(sessionIds, true);
  },

  async setGroupSleeping(this: GpuiSidebarRuntime, groupId: string, sleeping: boolean): Promise<void> {
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (subgroup) {
      const remoteProject = parseGpuiRemotePresentationProjectId(subgroup.projectId);
      const memberIds =
        getGpuiWorkspaceSessionSubgroups(this.workspaceGroups, subgroup.projectId).find(
          (group) => group.groupId === subgroup.groupId
        )?.sessionIds ?? [];
      await this.setSessionsSleeping(
        memberIds.map((sessionId) =>
          remoteProject
            ? createGpuiRemotePresentationSessionId(remoteProject.machineId, remoteProject.projectId, sessionId)
            : createGxserverPresentationProjectSessionId(subgroup.projectId, sessionId)
        ),
        sleeping
      );
      return;
    }
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      const presentation = this.remotePresentations.get(remoteGroup.machineId);
      const scopedProjectId = createGpuiRemotePresentationProjectId(remoteGroup.machineId, remoteGroup.projectId);
      const sessionIds = this.browserTabs
        .filter((tab) => tab.projectId === scopedProjectId)
        .map(gpuiBrowserSidebarSessionId)
        .concat(
          (presentation?.sessions ?? [])
            .filter((session) => session.projectId === remoteGroup.projectId)
            .map((session) =>
              createGpuiRemotePresentationSessionId(remoteGroup.machineId, remoteGroup.projectId, session.sessionId)
            )
        );
      /*
      CDXC:GPUISidebarBulkSleep 2026-06-27-02:05:
      Group sleep shares the same native-parity pacing as explicit multi-select sleep, while Wake remains concurrent because restoring sessions does not need terminal teardown throttling.
      */
      await this.setSessionsSleeping(sessionIds, sleeping);
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (!projectId || !this.presentation) {
      return;
    }
    const sessionIds = this.browserTabs
      .filter((tab) => tab.projectId === projectId)
      .map(gpuiBrowserSidebarSessionId)
      .concat(
        this.presentation.sessions
          .filter((session) => session.projectId === projectId)
          .map((session) => createGxserverPresentationProjectSessionId(projectId, session.sessionId))
      );
    /*
    CDXC:GPUISidebarBulkSleep 2026-06-27-02:05:
    Local project group sleep uses the shared private-data-free pacing helper through setSessionsSleeping, preserving the existing per-session focus replacement behavior inside setSessionSleeping.
    */
    await this.setSessionsSleeping(sessionIds, sleeping);
  },

  async setSessionsSleeping(this: GpuiSidebarRuntime, sessionIds: readonly string[], sleeping: boolean): Promise<void> {
    if (!sleeping) {
      await Promise.all(sessionIds.map((sessionId) => this.setSessionSleeping(sessionId, false)));
      return;
    }
    /*
    CDXC:GPUISidebarBulkSleep 2026-06-27-02:05:
    GPUI sleep bulk actions must mirror native pacing by starting one sleep request at a time with a 350 ms interval. Use the shared aggregate-count helper so per-operation failures continue without exposing ids, titles, paths, commands, URLs, or user text.
    */
    await runGpuiSidebarBulkSleepPaced(sessionIds, async (sessionId) => {
      await this.setSessionSleeping(sessionId, true);
    });
  },

  async setSessionSleeping(
    this: GpuiSidebarRuntime,
    sessionId: string,
    sleeping: boolean,
    options?: { automatic?: boolean; forceRemount?: boolean }
  ): Promise<void> {
    const browserTab = this.browserTabs.find((candidate) => gpuiBrowserSidebarSessionId(candidate) === sessionId);
    if (browserTab) {
      window.ghostexGpui?.postBrowserTabFocus?.(
        JSON.stringify({
          projectId: browserTab.projectId,
          sleeping,
          tabId: browserTab.tabId,
          type: 'ghostex.gpui.sidebar.browserTabFocus',
          version: 1,
        })
      );
      return;
    }
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      await this.requestRemoteGxserver(remoteSession.machineId, sleeping ? '/api/sleepSession' : '/api/wakeSession', {
        projectId: remoteSession.projectId,
        reason: 'gpui-sidebar',
        sessionId: remoteSession.sessionId,
        ...(sleeping && options?.automatic ? { sleepTrigger: 'automatic' } : {}),
      });
      await this.refreshRemotePresentationFromGxserver(remoteSession.machineId);
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (reference?.projectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID) {
      return;
    }
    if (!reference || !this.client) {
      return;
    }
    const replacementFocusSessionId = sleeping
      ? this.resolveLocalProjectListTransitionFocusTarget(reference.projectId, reference.sessionId)
      : undefined;
    const lifecycleResult = await this.client.rpc<GxserverSleepSessionResult | undefined>(
      sleeping ? '/api/sleepSession' : '/api/wakeSession',
      {
        projectId: reference.projectId,
        reason: 'gpui-sidebar',
        sessionId: reference.sessionId,
        ...(sleeping && options?.automatic ? { sleepTrigger: 'automatic' } : {}),
      }
    );
    /*
    CDXC:MobileKeepAwake 2026-08-19:
    A declined automatic sleep left the session running, so the optimistic
    "sleeping" patch below would publish a row state the daemon never entered.
    */
    if (gxserverSleepWasDeclined(lifecycleResult)) {
      return;
    }
    if (sleeping) {
      this.patchPresentationSession(reference.projectId, reference.sessionId, {
        lifecycleState: 'sleeping',
      });
      if (replacementFocusSessionId) {
        this.focusLocalWorkspaceSession(reference.projectId, replacementFocusSessionId);
        this.publishPresentation('patch');
      }
      return;
    }
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-06:34:
    A local sidebar Wake action is also a workspace activation in the macOS app: the row becomes running and the corresponding workspace terminal is selected/restored through the same focus path as a direct session click. GPUI must use the local focus bridge here, not gxserver `/api/focusSession`.
    */
    this.patchPresentationSession(reference.projectId, reference.sessionId, {
      lifecycleState: 'running',
    });
    this.focusLocalWorkspaceSession(reference.projectId, reference.sessionId, options);
    this.publishPresentation('patch');
  },

  async closeInactiveProjectSessions(this: GpuiSidebarRuntime, groupId: string): Promise<void> {
    const sessionIds = this.collectInactiveProjectSessionIds(groupId);
    await Promise.all(sessionIds.map((sessionId) => this.transitionSession(sessionId, 'close')));
  },

  async sleepInactiveProjectSessions(this: GpuiSidebarRuntime, groupId: string): Promise<void> {
    const sessionIds = this.collectInactiveProjectSessionIds(groupId);
    await this.setSessionsSleeping(sessionIds, true);
  },

  collectInactiveProjectSessionIds(this: GpuiSidebarRuntime, groupId: string): string[] {
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      const presentation = this.remotePresentations.get(remoteGroup.machineId);
      const scopedProjectId = createGpuiRemotePresentationProjectId(remoteGroup.machineId, remoteGroup.projectId);
      return this.browserTabs
        .filter((tab) => tab.projectId === scopedProjectId && !tab.isSleeping && !tab.isVisible)
        .map(gpuiBrowserSidebarSessionId)
        .concat(
          (presentation?.sessions ?? [])
            .filter((session) => session.projectId === remoteGroup.projectId)
            .filter(isGpuiInactiveProjectPresentationSession)
            .map((session) =>
              createGpuiRemotePresentationSessionId(remoteGroup.machineId, remoteGroup.projectId, session.sessionId)
            )
        );
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (!projectId || !this.presentation) {
      return [];
    }
    return this.browserTabs
      .filter((tab) => tab.projectId === projectId && !tab.isSleeping && !tab.isVisible)
      .map(gpuiBrowserSidebarSessionId)
      .concat(
        this.presentation.sessions
          .filter((session) => session.projectId === projectId)
          .filter(isGpuiInactiveProjectPresentationSession)
          .map((session) => createGxserverPresentationProjectSessionId(projectId, session.sessionId))
      );
  },

  async wakeProjectSleepingSessions(this: GpuiSidebarRuntime, groupId: string): Promise<void> {
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      const presentation = this.remotePresentations.get(remoteGroup.machineId);
      const scopedProjectId = createGpuiRemotePresentationProjectId(remoteGroup.machineId, remoteGroup.projectId);
      const sessionIds = this.browserTabs
        .filter((tab) => tab.projectId === scopedProjectId && tab.isSleeping)
        .map(gpuiBrowserSidebarSessionId)
        .concat(
          (presentation?.sessions ?? [])
            .filter((session) => session.projectId === remoteGroup.projectId && session.lifecycleState === 'sleeping')
            .map((session) =>
              createGpuiRemotePresentationSessionId(remoteGroup.machineId, remoteGroup.projectId, session.sessionId)
            )
        );
      await this.setSessionsSleeping(sessionIds, false);
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (!projectId || !this.presentation) {
      return;
    }
    this.focusProjectId(projectId);
    const sessionIds = this.browserTabs
      .filter((tab) => tab.projectId === projectId && tab.isSleeping)
      .map(gpuiBrowserSidebarSessionId)
      .concat(
        this.presentation.sessions
          .filter((session) => session.projectId === projectId && session.lifecycleState === 'sleeping')
          .map((session) => createGxserverPresentationProjectSessionId(projectId, session.sessionId))
      );
    await this.setSessionsSleeping(sessionIds, false);
  },
};

const gpuiSidebarRuntimeAutoSleepMethodsShapeCheck: GpuiSidebarRuntimeAutoSleepMethods =
  gpuiSidebarRuntimeAutoSleepMethods;
void gpuiSidebarRuntimeAutoSleepMethodsShapeCheck;
