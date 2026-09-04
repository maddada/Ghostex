/*
CDXC:RepoStructure 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  getGpuiWorkspaceSessionSubgroups,
  moveGpuiWorkspaceSessionToSubgroup,
  parseGpuiWorkspaceSessionSubgroupId,
} from '../workspace-session-groups';
import {
  GPUI_GXSERVER_CHATS_GROUP_ID,
  GPUI_QUICK_AUTOMATIONS_PROJECT_ID,
  GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_TYPE,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_VERSION,
  SESSION_LIFECYCLE_FAILURE_TITLES,
} from './constants';
import type { GpuiSidebarRuntime } from './core';
import { gpuiBrowserSidebarSessionId } from './helpers/browser-tabs';
import { isGpuiInactiveProjectPresentationSession } from './helpers/close-after-done';
import {
  isGpuiPresentationChatDomainProject,
  isGpuiPresentationChatProjectPath,
} from './helpers/presentation-projection';
import { createGpuiSidebarSettings } from './helpers/bootstrap';
import { normalizeNonEmptyString } from './helpers/records';
import {
  createGpuiRemotePresentationGroupId,
  createGpuiRemotePresentationSessionId,
  parseGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationSessionId,
} from './helpers/remote-presentation';
import { shouldApplyGpuiLocalWorkspaceTransition } from './helpers/terminal-lifecycle';
import type { GpuiWorkspaceTerminalFocusPlacement } from './types-and-protocol';
import { closeAppModal, openAppModal, postAppModalHostMessage } from '@/packages/core-ui/app-modal-host-bridge';
import type { PreferredAgentInterface } from '@/packages/shared/ghostex-settings';
import { reorderPresentationProjectSessions } from '@/packages/shared/gxserver-presentation-cache';
import {
  createGxserverPresentationProjectGroupId,
  createGxserverPresentationProjectSessionId,
  createGxserverPresentationSessionsByProjectFromGroups,
  parseGxserverPresentationProjectGroupId,
  parseGxserverPresentationProjectSessionId,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type {
  GxserverEndpointPath,
  GxserverForkSessionResult,
  GxserverProjectId,
  GxserverSessionId,
  GxserverSessionRenameRequestResult,
  GxserverSessionTransitionResult,
} from '@/packages/shared/gxserver-protocol';
import type { SidebarToExtensionMessage } from '@/packages/shared/session-grid-contract';
import type { SidebarSessionTag } from '@/packages/shared/session-tags';

/*
CDXC:RepoStructure 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeSessionFocusMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeSessionFocusMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeSessionFocusMethods {
  focusGroup(groupId: string, originalMessage: SidebarToExtensionMessage): void;
  openQuickAutomationsPage(): void;
  ensureQuickAutomationsProject(): void;
  focusQuickAutomationsProject(): void;
  closeQuickAutomationsProject(): void;
  focusSession(sessionId: string, originalMessage?: SidebarToExtensionMessage): Promise<void>;
  postSidebarSessionFocusConfirmation(sessionId: string): void;
  focusLocalWorkspaceSession(
    projectId: string,
    sessionId: string,
    options?: {
      forceRemount?: boolean;
      placement?: GpuiWorkspaceTerminalFocusPlacement;
      preferredInterface?: PreferredAgentInterface;
    }
  ): void;
  postLocalWorkspaceTerminalFocus(
    projectId: string,
    sessionId: string,
    placementTargetSessionId?: string,
    options?: {
      forceRemount?: boolean;
      placement?: GpuiWorkspaceTerminalFocusPlacement;
      preferredInterface?: PreferredAgentInterface;
      startupRestore?: boolean;
    }
  ): void;
  transitionSession(sessionId: string, action: 'close' | 'sleep'): Promise<void>;
  copySessionDetails(message: Extract<SidebarToExtensionMessage, { type: 'copySessionDetails' }>): void;
  fullReloadSession(sessionId: string): Promise<void>;
  splitSessionRight(sessionId: string): Promise<void>;
  switchSessionAgent(sessionId: string, agentId: string): Promise<void>;
  fullReloadProjectZmxSessions(groupId: string): Promise<void>;
  fullReloadWorkspaceGroup(groupId: string): Promise<void>;
  resolveLocalProjectListTransitionFocusTarget(projectId: string, removedSessionId: string): string | undefined;
  localProjectTransitionSessionIds(projectId: string, removedSessionId: string): string[];
  isRunningLocalPresentationSession(projectId: string, sessionId: string): boolean;
  isSleepingLocalPresentationSession(projectId: string, sessionId: string): boolean;
  forkSession(sessionId: string): Promise<void>;
  renameSession(message: Extract<SidebarToExtensionMessage, { type: 'renameSession' }>): Promise<void>;
  updateSessionFlags(
    sessionId: string,
    flags: { isFavorite?: boolean; isParked?: boolean; isPinned?: boolean; sessionTag?: SidebarSessionTag | null }
  ): Promise<void>;
  setSessionParked(sessionId: string, parked: boolean): Promise<void>;
  openSessionNoteEditor(sessionId: string): void;
  saveSessionNote(sessionId: string, note: string): Promise<void>;
  runSessionLifecycleCommand(
    sessionId: string,
    path: Extract<
      GxserverEndpointPath,
      '/api/settleSession' | '/api/snoozeSession' | '/api/unsettleSession' | '/api/unsnoozeSession'
    >,
    params: Record<string, unknown>
  ): Promise<void>;
  syncSessionOrder(groupId: string, sessionIds: readonly string[]): Promise<void>;
  focusProjectId(projectId: string): void;
  focusBrowserTabProject(projectId: string): void;
  focusRemotePresentationProject(reference: { machineId: string; projectId: string }): boolean;
  setLocalPresentationSessionFocus(
    projectId: string,
    sessionId: string,
    targetGroupId?: string,
    exactVisibleSessionIds?: readonly string[]
  ): void;
  nextVisibleSessionIdsForLocalFocus(projectId: string, sessionId: string): Set<string>;
  currentVisibleSessionIdsForLocalProject(projectId: string): string[];
  isGpuiPresentationChatProjectId(projectId: string): boolean;
  setRemotePresentationSessionFocus(reference: { machineId: string; projectId: string; sessionId: string }): void;
  dropRemotePresentationSessionFocus(machineId: string): void;
}

export const gpuiSidebarRuntimeSessionFocusMethods = {
  focusGroup(this: GpuiSidebarRuntime, groupId: string, originalMessage: SidebarToExtensionMessage): void {
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      const target = this.selectRemoteGroupAttachTarget(remoteGroup);
      if (!target) {
        this.postRemoteToast('info', 'Remote attach unavailable', {
          description: 'This remote project has no attachable sessions.',
        });
        return;
      }
      if (this.postRemoteSessionNativeAction('openRemoteSessionTerminal', target, originalMessage)) {
        this.setRemotePresentationSessionFocus(target);
        this.publishRemotePresentationPatch();
      }
      return;
    }
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (subgroup) {
      if (!parseGpuiRemotePresentationProjectId(subgroup.projectId)) {
        this.activeProjectId = subgroup.projectId;
      }
      this.activeGroupId = groupId;
      this.refreshSidebarHudFromClient();
      if (this.presentation) {
        this.publishPresentation('patch');
      } else {
        this.publishRemotePresentationPatch();
      }
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (projectId) {
      this.focusProjectId(projectId);
    } else {
      this.activeGroupId = groupId;
      this.refreshSidebarHudFromClient();
    }
    this.publishPresentation('patch');
  },

  openQuickAutomationsPage(this: GpuiSidebarRuntime): void {
    this.ensureQuickAutomationsProject();
    this.focusQuickAutomationsProject();
  },

  ensureQuickAutomationsProject(this: GpuiSidebarRuntime): void {
    /*
    CDXC:Automations 2026-07-08:
    GPUI mirrors macOS `ensureQuickAutomationsProject` without daemon storage:
    macOS writes a client registry row, while GPUI keeps this overview as a
    session-local runtime projection until its synthetic Quick row is closed.
    */
    this.quickAutomationsOverviewOpen = true;
  },

  focusQuickAutomationsProject(this: GpuiSidebarRuntime): void {
    /*
    CDXC:Automations 2026-07-08:
    Mirror macOS `focusQuickAutomationsProject`: selecting the synthetic
    quick-automations project activates the Quick group and focused overview row;
    Rust receives the Automate workarea through the active-project context post.
    */
    this.activeProjectId = GPUI_QUICK_AUTOMATIONS_PROJECT_ID;
    this.activeGroupId = GPUI_GXSERVER_CHATS_GROUP_ID;
    this.focusedSessionId = GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID;
    this.visibleSessionIds = new Set([...this.visibleSessionIds, GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID]);
    if (this.presentation) {
      this.publishPresentation('patch');
      return;
    }
    this.postActiveProjectContext();
  },

  closeQuickAutomationsProject(this: GpuiSidebarRuntime): void {
    this.quickAutomationsOverviewOpen = false;
    this.visibleSessionIds.delete(GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID);
    if (this.focusedSessionId === GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID) {
      this.focusedSessionId = undefined;
    }
    if (this.activeProjectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID) {
      this.activeProjectId = undefined;
      this.activeGroupId = undefined;
    }
    if (this.presentation) {
      this.publishPresentation('patch');
      return;
    }
    this.postActiveProjectContext();
  },

  async focusSession(
    this: GpuiSidebarRuntime,
    sessionId: string,
    originalMessage?: SidebarToExtensionMessage
  ): Promise<void> {
    const browserTab = this.browserTabs.find((candidate) => gpuiBrowserSidebarSessionId(candidate) === sessionId);
    if (browserTab) {
      /*
      A Browser row becomes the presentation focus owner when clicked. Clear
      the previous terminal owner before publishing the project change;
      otherwise ensureActiveProject resolves that stale terminal and switches
      the sidebar straight back to the terminal's project.
      */
      this.focusedSessionId = undefined;
      this.focusBrowserTabProject(browserTab.projectId);
      const post = window.ghostexGpui?.postBrowserTabFocus;
      if (typeof post === 'function') {
        post(
          JSON.stringify({
            projectId: browserTab.projectId,
            tabId: browserTab.tabId,
            type: 'ghostex.gpui.sidebar.browserTabFocus',
            version: 1,
          })
        );
      }
      return;
    }
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      this.acknowledgeSessionAttention(sessionId, 'sidebar-focus');
      if (
        this.postRemoteSessionNativeAction(
          'openRemoteSessionTerminal',
          remoteSession,
          originalMessage ?? { sessionId, type: 'focusSession' }
        )
      ) {
        this.setRemotePresentationSessionFocus(remoteSession);
        this.publishRemotePresentationPatch();
      }
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (reference?.projectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID) {
      if (this.isQuickAutomationsSidebarSessionId(sessionId)) {
        this.ensureQuickAutomationsProject();
        this.focusQuickAutomationsProject();
      }
      return;
    }
    if (!reference || !this.client) {
      return;
    }
    this.acknowledgeSessionAttention(sessionId, 'sidebar-focus');
    if (this.isSleepingLocalPresentationSession(reference.projectId, reference.sessionId)) {
      /*
      CDXC:FocusRouting 2026-06-26-23:24:
      Sleeping local session-card clicks must match macOS session activation by committing gxserver `/api/wakeSession` before the Rust workspace materializes the terminal. A plain focus bridge can select the tab but leaves gxserver sleeping, so route this branch through the same Wake path as the sidebar sleep toggle.
      */
      await this.setSessionSleeping(sessionId, false);
      return;
    }
    /*
    CDXC:FocusRouting 2026-06-26-04:42:
    Local GPUI sidebar clicks must match the macOS sidebar ownership model: the SidebarApp adapter applies local focus immediately and publishes the CEF bootstrap focus hint, but it must not call gxserver `/api/focusSession`. That endpoint is an external renderer-command route and can bounce focus when another renderer is the first open gxserver subscriber.
    */
    this.focusLocalWorkspaceSession(reference.projectId, reference.sessionId);
    this.publishPresentation('patch');
  },

  /*
  CDXC:Git 2026-08-16:
  The SidebarApp applies focus optimistically (pendingFocusedSessionId) and
  waits for a groups message containing the session to confirm or correct it.
  Full-tree publishes used to provide that confirmation implicitly on every
  patch; now that patches carry only changed groups, a focus request whose
  projection ends up identical (clicking the already-focused session) would
  never re-deliver the group and the pending marker could go stale, letting a
  later native-driven focus change get visually yanked back. Re-send the
  authoritative group(s) holding the requested session after every explicit
  sidebar focus request, even when unchanged.
  */
  postSidebarSessionFocusConfirmation(this: GpuiSidebarRuntime, sessionId: string): void {
    if (!this.hasHydrated) {
      return;
    }
    const groups = this.latestGroups.filter((group) =>
      group.sessions.some((session) => session.sessionId === sessionId)
    );
    if (groups.length === 0) {
      return;
    }
    this.messageSource.postMessage({
      groupOrder: this.latestGroups.map((group) => group.groupId),
      groups,
      removedGroupIds: [],
      removedSessionIds: [],
      revision: ++this.revision,
      type: 'sidebarGroupsChanged',
    });
  },

  focusLocalWorkspaceSession(
    this: GpuiSidebarRuntime,
    projectId: string,
    sessionId: string,
    options?: {
      forceRemount?: boolean;
      placement?: GpuiWorkspaceTerminalFocusPlacement;
      preferredInterface?: PreferredAgentInterface;
    }
  ): void {
    /*
    CDXC:FocusRouting 2026-06-26-06:18:
    Any successful local GPUI activation that makes a gxserver workspace session current must update both the reused SidebarApp presentation focus and the real GPUI Agents workspace. This matches macOS create, fork, restore, App Shot, and session-click behavior instead of requiring a second sidebar click to show the newly focused terminal.
    */
    const normalizedProjectId = normalizeNonEmptyString(projectId);
    const normalizedSessionId = normalizeNonEmptyString(sessionId);
    if (!normalizedProjectId || !normalizedSessionId) {
      return;
    }
    this.setLocalPresentationSessionFocus(normalizedProjectId, normalizedSessionId);
    this.postLocalWorkspaceTerminalFocus(normalizedProjectId, normalizedSessionId, undefined, options);
  },

  postLocalWorkspaceTerminalFocus(
    this: GpuiSidebarRuntime,
    projectId: string,
    sessionId: string,
    placementTargetSessionId?: string,
    options?: {
      forceRemount?: boolean;
      placement?: GpuiWorkspaceTerminalFocusPlacement;
      preferredInterface?: PreferredAgentInterface;
      startupRestore?: boolean;
    }
  ): void {
    /*
    CDXC:FocusRouting 2026-06-26-06:08:
    Local GPUI session-card clicks must drive the real Agents workspace the way macOS does: after React updates gxserver presentation focus, send only bounded project/session ids to Rust so Rust can select or materialize the corresponding terminal tab from gxserver attach metadata. Do not pass labels, titles, commands, paths, terminal content, or daemon responses through the renderer bridge.
    */
    const postFocus = window.ghostexGpui?.postWorkspaceTerminalFocus;
    if (typeof postFocus !== 'function') {
      return;
    }
    const payload = JSON.stringify({
      ...(placementTargetSessionId ? { placementTargetSessionId } : {}),
      ...(options?.forceRemount ? { forceRemount: true } : {}),
      ...(options?.placement ? { placement: options.placement } : {}),
      ...(options?.preferredInterface ? { preferredInterface: options.preferredInterface } : {}),
      ...(options?.startupRestore ? { startupRestore: true } : {}),
      projectId,
      sessionId,
      type: GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_VERSION,
    });
    postFocus(payload);
  },

  async transitionSession(this: GpuiSidebarRuntime, sessionId: string, action: 'close' | 'sleep'): Promise<void> {
    const browserTab = this.browserTabs.find((candidate) => gpuiBrowserSidebarSessionId(candidate) === sessionId);
    if (browserTab) {
      if (action === 'close') {
        window.ghostexGpui?.postBrowserTabFocus?.(
          JSON.stringify({
            close: true,
            projectId: browserTab.projectId,
            tabId: browserTab.tabId,
            type: 'ghostex.gpui.sidebar.browserTabFocus',
            version: 1,
          })
        );
      }
      return;
    }
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      this.postRemoteGxserverSidebarRequest(
        remoteSession.machineId,
        action === 'close' ? '/api/killSession' : '/api/sleepSession',
        {
          projectId: remoteSession.projectId,
          reason: 'gpui-sidebar',
          sessionId: remoteSession.sessionId,
        }
      );
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (reference?.projectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID) {
      if (action === 'close' && this.isQuickAutomationsSidebarSessionId(sessionId)) {
        this.closeQuickAutomationsProject();
      }
      return;
    }
    if (!reference || !this.client) {
      return;
    }
    const replacementFocusSessionId = this.resolveLocalProjectListTransitionFocusTarget(
      reference.projectId,
      reference.sessionId
    );
    if (action === 'close') {
      this.removePresentationSession(reference.projectId, reference.sessionId);
      if (replacementFocusSessionId) {
        this.focusLocalWorkspaceSession(reference.projectId, replacementFocusSessionId);
        this.publishPresentation('patch');
      }
      await this.client
        .rpc<GxserverSessionTransitionResult>('/api/transitionSession', {
          action,
          projectId: reference.projectId,
          reason: 'gpui-sidebar',
          sessionId: reference.sessionId,
        })
        .catch(() => undefined);
      return;
    }
    const result = await this.client.rpc<GxserverSessionTransitionResult>('/api/transitionSession', {
      action,
      projectId: reference.projectId,
      reason: 'gpui-sidebar',
      sessionId: reference.sessionId,
    });
    if (!shouldApplyGpuiLocalWorkspaceTransition(result, action)) {
      return;
    }
    this.patchPresentationSession(reference.projectId, reference.sessionId, {
      lifecycleState: 'sleeping',
    });
    if (replacementFocusSessionId) {
      this.focusLocalWorkspaceSession(reference.projectId, replacementFocusSessionId);
      this.publishPresentation('patch');
    }
  },

  copySessionDetails(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'copySessionDetails' }>
  ): void {
    const detailsText = normalizeNonEmptyString(message.detailsText);
    if (!detailsText) {
      this.handleUnsupportedSidebarMessage(message);
      return;
    }
    try {
      postAppModalHostMessage({ detailsText, type: 'copySessionDetails' }, 'GPUISidebarActions:copySessionDetails');
    } catch {
      this.handleUnsupportedSidebarMessage(message);
    }
  },

  async fullReloadSession(this: GpuiSidebarRuntime, sessionId: string): Promise<void> {
    /*
    CDXC:CefRuntime 2026-07-12:
    Full reload must really cycle the provider: `/api/sleepSession` zmx-kills
    the daemon (and the CLI inside it) and `/api/wakeSession` respawns it with
    the restore command. The local surface in the Rust workspace is now dead,
    but Rust only learns about the sleep through presentation snapshots, so a
    plain wake focus can race ahead and re-select the dead mounted terminal.
    `forceRemount` makes the wake focus tear down the stale local terminal
    owner synchronously before running the ordinary attach pipeline, so the
    reused tab deterministically re-attaches to the freshly restored daemon.
    */
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (!remoteSession && (!reference || !this.client)) {
      return;
    }
    await this.setSessionSleeping(sessionId, true);
    await this.setSessionSleeping(sessionId, false, { forceRemount: true });
  },

  async splitSessionRight(this: GpuiSidebarRuntime, sessionId: string): Promise<void> {
    /*
    CDXC:Workarea 2026-09-04 DECISION:
    User: Advanced > Split Right in the session menu opens the session in a new
    pane to the right of the focused agents pane. It is a sidebar focus with a
    placement: a sleeping row wakes first exactly like a click, and the same
    focus bridge carries `placement: 'splitRight'` so Rust either moves the
    already-open tab into a new right-hand leaf or attaches the session there.
    */
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      // Remote rows open through the native project-path action bridge, the
      // same route as a remote session click; Rust wakes and attaches there.
      this.acknowledgeSessionAttention(sessionId, 'sidebar-focus');
      if (
        this.postRemoteSessionNativeAction(
          'openRemoteSessionTerminal',
          remoteSession,
          { sessionId, type: 'splitSessionRight' },
          { placement: 'splitRight' }
        )
      ) {
        this.setRemotePresentationSessionFocus(remoteSession);
        this.publishRemotePresentationPatch();
      }
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (!reference || !this.client || reference.projectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID) {
      return;
    }
    this.acknowledgeSessionAttention(sessionId, 'sidebar-focus');
    if (this.isSleepingLocalPresentationSession(reference.projectId, reference.sessionId)) {
      await this.setSessionSleeping(sessionId, false, { placement: 'splitRight' });
      return;
    }
    this.focusLocalWorkspaceSession(reference.projectId, reference.sessionId, { placement: 'splitRight' });
    this.publishPresentation('patch');
  },

  async switchSessionAgent(this: GpuiSidebarRuntime, sessionId: string, agentId: string): Promise<void> {
    /*
    CDXC:AgentProviders 2026-09-03:
    Resume the same conversation under another same-family agent configuration
    (another account). The owning daemon rewrites the row's launch identity;
    the provider cycle that follows is Full Reload itself, so the wake resumes
    through the ordinary restore path with the new agent's command. A refused
    switch (incompatible agent, draft, old daemon) leaves the row untouched, so
    nothing is reloaded and the daemon's own sentence is what the user sees.
    */
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    try {
      if (remoteSession) {
        await this.requestRemoteGxserver(remoteSession.machineId, '/api/switchSessionAgent', {
          agentId,
          projectId: remoteSession.projectId,
          sessionId: remoteSession.sessionId,
        });
      } else if (reference && this.client) {
        await this.client.rpc('/api/switchSessionAgent', {
          agentId,
          projectId: reference.projectId,
          sessionId: reference.sessionId,
        });
      } else {
        return;
      }
    } catch (error) {
      this.postRemoteToast('error', 'Could not switch account', {
        description: error instanceof Error ? error.message : String(error),
      });
      return;
    }
    await this.fullReloadSession(sessionId);
  },

  async fullReloadProjectZmxSessions(this: GpuiSidebarRuntime, groupId: string): Promise<void> {
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      const presentation = this.remotePresentations.get(remoteGroup.machineId);
      const remoteSessionIds = (presentation?.sessions ?? [])
        .filter(
          (session) =>
            session.projectId === remoteGroup.projectId &&
            session.sessionPersistenceProvider === 'zmx' &&
            isGpuiInactiveProjectPresentationSession(session)
        )
        .map((session) =>
          createGpuiRemotePresentationSessionId(remoteGroup.machineId, remoteGroup.projectId, session.sessionId)
        );
      for (const reloadSessionId of remoteSessionIds) {
        await this.fullReloadSession(reloadSessionId);
      }
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (!projectId || !this.presentation) {
      return;
    }
    const sessionIds = this.presentation.sessions
      .filter(
        (session) =>
          session.projectId === projectId &&
          session.sessionPersistenceProvider === 'zmx' &&
          isGpuiInactiveProjectPresentationSession(session)
      )
      .map((session) => createGxserverPresentationProjectSessionId(projectId, session.sessionId));
    for (const reloadSessionId of sessionIds) {
      await this.fullReloadSession(reloadSessionId);
    }
  },

  async fullReloadWorkspaceGroup(this: GpuiSidebarRuntime, groupId: string): Promise<void> {
    const subgroup = parseGpuiWorkspaceSessionSubgroupId(groupId);
    if (!subgroup) {
      await this.fullReloadProjectZmxSessions(groupId);
      return;
    }
    const remoteProject = parseGpuiRemotePresentationProjectId(subgroup.projectId);
    const memberIds =
      getGpuiWorkspaceSessionSubgroups(this.workspaceGroups, subgroup.projectId).find(
        (group) => group.groupId === subgroup.groupId
      )?.sessionIds ?? [];
    for (const sessionId of memberIds) {
      await this.fullReloadSession(
        remoteProject
          ? createGpuiRemotePresentationSessionId(remoteProject.machineId, remoteProject.projectId, sessionId)
          : createGxserverPresentationProjectSessionId(subgroup.projectId, sessionId)
      );
    }
  },

  resolveLocalProjectListTransitionFocusTarget(
    this: GpuiSidebarRuntime,
    projectId: string,
    removedSessionId: string
  ): string | undefined {
    /*
    CDXC:FocusRouting 2026-06-26-06:34:
    Sidebar-origin local close/sleep must follow the macOS project-list focus rule: background transitions do not steal focus, while closing or sleeping the focused session selects the next running row from the same displayed local project order and routes it through the workspace focus bridge.
    */
    const normalizedProjectId = normalizeNonEmptyString(projectId);
    const normalizedRemovedSessionId = normalizeNonEmptyString(removedSessionId);
    if (!normalizedProjectId || !normalizedRemovedSessionId || this.focusedSessionId !== normalizedRemovedSessionId) {
      return undefined;
    }
    const orderedSessionIds = this.localProjectTransitionSessionIds(normalizedProjectId, normalizedRemovedSessionId);
    const removedIndex = orderedSessionIds.indexOf(normalizedRemovedSessionId);
    const candidates =
      removedIndex >= 0
        ? [...orderedSessionIds.slice(removedIndex + 1), ...orderedSessionIds.slice(0, removedIndex)]
        : orderedSessionIds;
    const replacementSessionId = candidates.find(
      (candidateSessionId) =>
        candidateSessionId !== normalizedRemovedSessionId &&
        this.isRunningLocalPresentationSession(normalizedProjectId, candidateSessionId)
    );
    return replacementSessionId;
  },

  localProjectTransitionSessionIds(this: GpuiSidebarRuntime, projectId: string, removedSessionId: string): string[] {
    const orderedSessionIds: string[] = [];
    const addSessionId = (sessionId: string | undefined): void => {
      const normalizedSessionId = normalizeNonEmptyString(sessionId);
      if (!normalizedSessionId || orderedSessionIds.includes(normalizedSessionId)) {
        return;
      }
      orderedSessionIds.push(normalizedSessionId);
    };
    for (const group of this.latestGroups) {
      for (const session of group.sessions) {
        if (parseGpuiRemotePresentationSessionId(session.sessionId)) {
          continue;
        }
        const reference = parseGxserverPresentationProjectSessionId(session.sessionId);
        if (reference?.projectId === projectId) {
          addSessionId(reference.sessionId);
        }
      }
    }
    for (const session of this.presentation?.sessions ?? []) {
      if (session.projectId === projectId) {
        addSessionId(session.sessionId);
      }
    }
    addSessionId(removedSessionId);
    return orderedSessionIds;
  },

  isRunningLocalPresentationSession(this: GpuiSidebarRuntime, projectId: string, sessionId: string): boolean {
    return (
      this.presentation?.sessions.some(
        (session) =>
          session.projectId === projectId && session.sessionId === sessionId && session.lifecycleState === 'running'
      ) ?? false
    );
  },

  isSleepingLocalPresentationSession(this: GpuiSidebarRuntime, projectId: string, sessionId: string): boolean {
    const presentationSleeping =
      this.presentation?.sessions.some(
        (session) =>
          session.projectId === projectId && session.sessionId === sessionId && session.lifecycleState === 'sleeping'
      ) ?? false;
    if (presentationSleeping) {
      return true;
    }
    const sidebarSessionId = createGxserverPresentationProjectSessionId(projectId, sessionId);
    if (this.sleepingLocalSidebarSessionIds.has(sidebarSessionId)) {
      return true;
    }
    return this.latestGroups.some((group) =>
      group.sessions.some(
        (session) =>
          session.sessionId === sidebarSessionId &&
          (session.lifecycleState === 'sleeping' || session.isSleeping === true)
      )
    );
  },

  async forkSession(this: GpuiSidebarRuntime, sessionId: string): Promise<void> {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      /*
      CDXC:RemoteMachines 2026-06-24-17:19:
      Remote fork authority comes only from a machine-prefixed session id already present in the remote presentation snapshot. Route the project/session ids to `/api/forkSession` on that machine; do not derive ids from labels or terminal text.

      CDXC:SessionFork 2026-07-10:
      Match macOS remote Fork exactly: the owning gxserver creates the fork and
      the refreshed remote presentation renders it without moving focus away
      from the session the user was viewing.
      */
      try {
        await this.requestRemoteGxserver(remoteSession.machineId, '/api/forkSession', {
          projectId: remoteSession.projectId,
          reason: 'gpui-sidebar',
          sessionId: remoteSession.sessionId,
        });
        await this.refreshRemotePresentationFromGxserver(remoteSession.machineId).catch(() => undefined);
      } catch (error) {
        this.postRemoteToast('error', 'Remote fork failed', {
          description: error instanceof Error ? error.message : String(error),
        });
      }
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (!reference || !this.client) {
      return;
    }
    if (
      !this.presentation?.sessions.some(
        (session) => session.projectId === reference.projectId && session.sessionId === reference.sessionId
      )
    ) {
      return;
    }

    const sourceGroupId =
      this.workspaceSubgroupSidebarIdForSession(reference.projectId, reference.sessionId) ??
      createGxserverPresentationProjectGroupId(reference.projectId);
    if (this.activeProjectId !== reference.projectId || this.activeGroupId !== sourceGroupId) {
      /*
      CDXC:SessionFork 2026-07-10:
      macOS focuses the clicked session's project before awaiting gxserver.
      GPUI also activates its clicked sidebar subgroup so Rust has the source
      tab-group mapping before the fork result arrives.
      */
      this.activeProjectId = reference.projectId;
      this.activeGroupId = sourceGroupId;
      this.refreshSidebarHudFromClient();
      this.publishPresentation('patch');
    }

    try {
      /*
      CDXC:SessionFork 2026-07-10:
      `/api/forkSession` returns `{ fork }`, exactly as the macOS gxserver
      client unwraps it. The previous GPUI code treated the result itself as
      the fork payload, so `response.session` was undefined and the action
      could not materialize or focus the returned G-session.
      */
      const { fork } = await this.client.rpc<{ fork: GxserverForkSessionResult }>('/api/forkSession', {
        projectId: reference.projectId,
        reason: 'gpui-sidebar',
        sessionId: reference.sessionId,
      });
      const forkedSessionId = normalizeNonEmptyString(fork?.session.sessionId);
      if (!forkedSessionId) {
        throw new Error('gxserver did not return the forked session.');
      }

      const sourceSubgroup = parseGpuiWorkspaceSessionSubgroupId(sourceGroupId);
      if (sourceSubgroup) {
        this.workspaceGroups = moveGpuiWorkspaceSessionToSubgroup(
          this.workspaceGroups,
          reference.projectId,
          forkedSessionId,
          sourceSubgroup.groupId
        );
        this.persistWorkspaceGroups();
      }

      this.setLocalPresentationSessionFocus(reference.projectId, forkedSessionId, sourceGroupId);
      this.publishPresentation('patch');
      /*
      The placement target is the clicked source session, not whichever pane
      happens to be focused when the RPC completes. Rust resolves this bounded
      id to the existing pane and appends the fork there before mounting the
      gxserver attach plan, matching macOS appendToTabGroup behavior.
      */
      this.postLocalWorkspaceTerminalFocus(reference.projectId, forkedSessionId, reference.sessionId);
      await this.refreshDomainPresentationSnapshotFromClient('patch').catch(() => undefined);
    } catch (error) {
      this.postSidebarActionToast('error', 'Could not fork session', {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  },

  async renameSession(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'renameSession' }>
  ): Promise<void> {
    const remoteSession = parseGpuiRemotePresentationSessionId(message.sessionId);
    if (remoteSession) {
      /*
      CDXC:SessionTitles 2026-07-29:
      Empty-title Generate Name is a local-transcript flow; a remote machine's
      transcripts are not readable here, and a blank direct rename would erase
      the remote title.
      */
      if (!message.title.trim()) {
        return;
      }
      /*
      CDXC:RemoteMachines 2026-08-12:
      Remote agent sessions must use the same pending-metadata rename contract
      as local sessions. The remote gxserver owns that session's zmx provider,
      so ask it to submit the provider-specific slash command itself instead of
      only updating sidebar metadata or trying to use GPUI's local Ghostty
      surface bridge.
      */
      this.postRemoteGxserverSidebarRequest(remoteSession.machineId, '/api/requestSessionRename', {
        ...(message.agentId ? { agentName: message.agentId } : {}),
        projectId: remoteSession.projectId,
        reason: 'gpui-sidebar',
        sessionId: remoteSession.sessionId,
        submitAgentRenameCommand: true,
        title: message.title,
        titleSource: 'user',
      });
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(message.sessionId);
    if (!reference || !this.client) {
      return;
    }
    if (message.shouldGenerateTitle) {
      /*
      CDXC:Sessions 2026-07-29:
      Generate Name reuses the first-message auto-title UX end to end:
      gxserver marks the session generating (the card shows the same
      "Generating title…" chrome), summarizes the pasted text with the chosen
      generation agent, stages the agent rename command through zmx with the
      same delayed real Enter, and applies the generated title. The long
      pasted text must never reach `/api/requestSessionRename` as a literal
      title.
      */
      const generationAgent = this.resolveSidebarAgent(message.agentId ?? '');
      const generationCommand = generationAgent?.command?.trim();
      await this.client.rpc('/api/generateSessionTitle', {
        ...(message.agentId ? { agentId: message.agentId } : {}),
        ...(generationCommand ? { command: generationCommand } : {}),
        projectId: reference.projectId,
        sessionId: reference.sessionId,
        text: message.title,
      });
      return;
    }
    const result = await this.client.rpc<GxserverSessionRenameRequestResult>('/api/requestSessionRename', {
      agentName: message.agentId,
      projectId: reference.projectId,
      reason: 'gpui-sidebar',
      sessionId: reference.sessionId,
      title: message.title,
      titleSource: 'user',
    });
    /*
    CDXC:Sessions 2026-08-18:
    Session cards render `displayTitle`, so patching only `title` moved the
    row's alias without changing the text on the card. Apply gxserver's own
    title projection instead — the same fields presentation publishes — so the
    card, its tooltip, and the alias stay one consistent title. Agent sessions
    keep the previous title here until the Agent CLI confirms the rename; the
    confirmed title lands through the normal presentation delta.
    */
    this.patchPresentationSession(reference.projectId, reference.sessionId, result.projection);
    /*
    CDXC:Sessions 2026-07-28:
    gxserver keeps agent-session renames pending until the Agent CLI itself is
    renamed, and it answers `shouldSendAgentRenameCommand` so the client stages
    `/rename <title>` (Pi uses `/name`; Hermes Agent uses `/title`) into the
    mapped terminal — the same contract macOS follows.
    */
    if (result.shouldSendAgentRenameCommand) {
      this.postLocalWorkspaceTerminalRenameCommand(reference.projectId, reference.sessionId, message.title);
    }
  },

  async updateSessionFlags(
    this: GpuiSidebarRuntime,
    sessionId: string,
    flags: { isFavorite?: boolean; isParked?: boolean; isPinned?: boolean; sessionTag?: SidebarSessionTag | null }
  ): Promise<void> {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      this.postRemoteGxserverSidebarRequest(remoteSession.machineId, '/api/updateSession', {
        ...flags,
        projectId: remoteSession.projectId,
        sessionId: remoteSession.sessionId,
      });
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (!reference || !this.client) {
      return;
    }
    await this.client.rpc('/api/updateSession', {
      ...flags,
      projectId: reference.projectId,
      sessionId: reference.sessionId,
    });
    /*
    `/api/updateSession` clears a tag with an explicit `null`, but a
    presentation session models "no tag" as an absent field. Translate the
    clear so the optimistic patch writes the same shape the daemon will send
    back, and leave the field untouched when the caller did not name it.
    */
    const { sessionTag, ...presentationFlags } = flags;
    this.patchPresentationSession(reference.projectId, reference.sessionId, {
      ...presentationFlags,
      ...(sessionTag === undefined ? {} : { sessionTag: sessionTag ?? undefined }),
    });
  },

  async setSessionParked(this: GpuiSidebarRuntime, sessionId: string, parked: boolean): Promise<void> {
    const shouldSleep = parked && createGpuiSidebarSettings(this.runtimeSettings).sleepSessionWhenParking;
    const remoteSession = shouldSleep ? parseGpuiRemotePresentationSessionId(sessionId) : undefined;
    if (remoteSession) {
      /*
      A fire-and-forget remote update can race a response-backed sleep request.
      When parking also sleeps, await the durable flag mutation before entering
      the existing remote sleep lifecycle.
      */
      await this.requestRemoteGxserver(remoteSession.machineId, '/api/updateSession', {
        isParked: parked,
        projectId: remoteSession.projectId,
        sessionId: remoteSession.sessionId,
      });
    } else {
      await this.updateSessionFlags(sessionId, { isParked: parked });
    }
    if (shouldSleep) {
      await this.setSessionSleeping(sessionId, true);
    }
  },

  /*
  CDXC:SessionNotes 2026-08-25:
  Open the note editor for a session, as the session row's context menu does.
  This exists so the desktop terminal's agent action bar opens the SAME dialog
  the sidebar opens rather than a second note surface of its own: the bar is a
  native gpui control with no modal host and no gxserver client, so it sends the
  intent here, where the presentation that holds the current note text lives.

  The note text is read from the presentation the row itself renders from — a
  dialog that opened empty over an existing note would overwrite it on confirm,
  so a session whose note cannot be resolved does not open one at all. Local and
  remote resolve from their own machine's presentation for the same reason
  `saveSessionNote` writes to its own machine's daemon.
  */
  openSessionNoteEditor(this: GpuiSidebarRuntime, sessionId: string): void {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    const presentation = remoteSession ? this.remotePresentations.get(remoteSession.machineId) : this.presentation;
    const reference = remoteSession ?? parseGxserverPresentationProjectSessionId(sessionId);
    if (!reference || !presentation) {
      return;
    }
    const session = presentation.sessions.find(
      (candidate) => candidate.projectId === reference.projectId && candidate.sessionId === reference.sessionId
    );
    if (!session) {
      return;
    }
    const sessionTitle =
      normalizeNonEmptyString(session.primaryTitle) ??
      normalizeNonEmptyString(session.title) ??
      normalizeNonEmptyString(session.terminalTitle);
    // Same modal-host contract as the row's own entry: the note editor replaces
    // whatever modal is open instead of stacking behind it.
    closeAppModal('SettingsDismissal:terminalActionBarNote');
    openAppModal({
      initialNote: session.sessionNote ?? '',
      modal: 'sessionNote',
      sessionId,
      ...(sessionTitle ? { sessionTitle } : {}),
      type: 'open',
    });
  },

  /*
  CDXC:SessionNotes 2026-08-24:
  Save (or, with an empty note, clear) this session's free-text note.

  - The note is filed against the session's PROVIDER conversation id, which only
    the daemon can resolve, so the client sends the ghostex session reference and
    nothing else. That is also why a note survives closing the row: resuming the
    same conversation lands on the same note.
  - No optimistic patch. gxserver schedules a presentation delta after a
    successful save, and that delta is what puts the note on the row; guessing
    here would paint a note the daemon may have refused (a session that never
    captured a conversation id has nothing to file against).
  - A remote row routes to ITS machine's daemon, exactly like every other
    session mutation. A daemon that predates session notes rejects the call, so
    the failure is surfaced as a toast instead of a silently lost note.
  */
  async saveSessionNote(this: GpuiSidebarRuntime, sessionId: string, note: string): Promise<void> {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    try {
      if (remoteSession) {
        await this.requestRemoteGxserver(remoteSession.machineId, '/api/saveSessionAgentNote', {
          note,
          projectId: remoteSession.projectId,
          sessionId: remoteSession.sessionId,
        });
        return;
      }
      const reference = parseGxserverPresentationProjectSessionId(sessionId);
      if (!reference || !this.client) {
        return;
      }
      await this.client.rpc('/api/saveSessionAgentNote', {
        note,
        projectId: reference.projectId,
        sessionId: reference.sessionId,
      });
    } catch {
      this.postSidebarActionToast('warning', 'Could not save the session note', {
        description: 'gxserver refused the note. This session may not have an agent conversation yet.',
      });
    }
  },

  /*
  CDXC:StateSync 2026-07-29:
  One code path for settle/unsettle/snooze/unsnooze, local and remote.

  - Routing mirrors `updateSessionFlags`: a remote-prefixed sidebar session id
    resolves to (machineId, projectId, sessionId) and goes over the Rust remote
    bridge to THAT machine's daemon; anything else is local. The renderer never
    picks a daemon by anything other than the id the host itself minted.
  - The response is awaited (not fire-and-forget) so a guard rejection — settling
    a working session, snoozing a session that is blocked on the user, a wake
    time in the past — surfaces as a toast instead of a row that silently never
    moves. The toast carries no session title, path, or daemon body.
  - No local presentation patch: gxserver emits the delta, and inventing one
    here would fight the server's guards and desync the settled/snoozed shelves.
  */
  async runSessionLifecycleCommand(
    this: GpuiSidebarRuntime,
    sessionId: string,
    path: Extract<
      GxserverEndpointPath,
      '/api/settleSession' | '/api/snoozeSession' | '/api/unsettleSession' | '/api/unsnoozeSession'
    >,
    params: Record<string, unknown>
  ): Promise<void> {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    try {
      if (remoteSession) {
        await this.requestRemoteGxserver(remoteSession.machineId, path, {
          ...params,
          projectId: remoteSession.projectId,
          sessionId: remoteSession.sessionId,
        });
        return;
      }
      const reference = parseGxserverPresentationProjectSessionId(sessionId);
      if (!reference || !this.client) {
        return;
      }
      await this.client.rpc(path, {
        ...params,
        projectId: reference.projectId,
        sessionId: reference.sessionId,
      });
    } catch {
      this.postSidebarActionToast('warning', SESSION_LIFECYCLE_FAILURE_TITLES[path], {
        description: 'gxserver refused the change. The session may be working or waiting on you.',
      });
    }
  },

  async syncSessionOrder(this: GpuiSidebarRuntime, groupId: string, sessionIds: readonly string[]): Promise<void> {
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (!projectId || !this.client || !this.presentation) {
      return;
    }
    const gxserverSessionIds = sessionIds.flatMap((sessionId) => {
      const reference = parseGxserverPresentationProjectSessionId(sessionId);
      return reference?.projectId === projectId ? [reference.sessionId] : [];
    });
    if (gxserverSessionIds.length === 0) {
      return;
    }
    this.presentation = reorderPresentationProjectSessions(
      this.presentation,
      projectId as GxserverProjectId,
      gxserverSessionIds as GxserverSessionId[]
    );
    this.publishPresentation('patch');
    await this.client.rpc('/api/updateSessionOrder', {
      projectId,
      sessionIds: gxserverSessionIds,
    });
  },

  focusProjectId(this: GpuiSidebarRuntime, projectId: string): void {
    const normalizedProjectId = normalizeNonEmptyString(projectId);
    if (!normalizedProjectId) {
      return;
    }
    this.activeProjectId = normalizedProjectId;
    this.activeGroupId = this.isGpuiPresentationChatProjectId(normalizedProjectId)
      ? GPUI_GXSERVER_CHATS_GROUP_ID
      : createGxserverPresentationProjectGroupId(normalizedProjectId);
    this.refreshSidebarHudFromClient();
  },

  focusBrowserTabProject(this: GpuiSidebarRuntime, projectId: string): void {
    /*
    The sidebar lists Browser rows for every project, local and remote, so a
    row click is a project switch whenever the row belongs to another project.
    Both kinds take the same two steps the rest of the runtime takes: select
    the owning project, then publish the projection. Rust learns the new active
    project only from the active-project context the publish posts, and that
    payload is rebuilt from `latestGroups`, which only a publish refreshes — so
    changing the selection without publishing would leave Rust on the previous
    project and strand the tab focus that follows this call.
    */
    const remoteProject = parseGpuiRemotePresentationProjectId(projectId);
    if (remoteProject) {
      if (!this.focusRemotePresentationProject(remoteProject)) {
        return;
      }
    } else {
      /*
      A Browser row hangs off its project's own group, so "already selected"
      means that group is active, not merely that the project id matches: with
      a named session group of the same project active, the row's group is not
      the active one and its focus highlight belongs to the parent group.
      */
      if (
        this.activeProjectId === projectId &&
        this.activeGroupId === createGxserverPresentationProjectGroupId(projectId)
      ) {
        return;
      }
      this.focusProjectId(projectId);
    }
    /*
    A local presentation is not required for this switch: browser rows outlive
    a gxserver outage and remote rows never needed one. `publishPresentation`
    would report gxserver as unavailable in that state, so use the
    remote-aware publish, which rebuilds the same projection from whichever
    presentations exist.
    */
    if (this.presentation) {
      this.publishPresentation('patch');
      return;
    }
    this.publishRemotePresentationPatch();
  },

  focusRemotePresentationProject(
    this: GpuiSidebarRuntime,
    reference: { machineId: string; projectId: string }
  ): boolean {
    /*
    The machine-scoped group id is the only representation of a remote
    selection: `activeProjectId` stays a local-only field, and the remote group
    projection, the presentation focus-state bridge, and the active-project
    context all read the selection back out of `activeGroupId`. Resolve the
    chat-collection group the same way `setRemotePresentationSessionFocus`
    does so both remote entry points land on the same group.
    */
    const machineId = normalizeNonEmptyString(reference.machineId);
    const projectId = normalizeNonEmptyString(reference.projectId);
    if (!machineId || !projectId) {
      return false;
    }
    const project = this.remotePresentations
      .get(machineId)
      ?.projects.find((candidate) => candidate.projectId === projectId);
    const scopedGroupId = createGpuiRemotePresentationGroupId(
      machineId,
      isGpuiPresentationChatProjectPath(project?.path) ? GPUI_GXSERVER_CHATS_GROUP_ID : projectId
    );
    if (this.activeGroupId === scopedGroupId) {
      return false;
    }
    this.activeGroupId = scopedGroupId;
    return true;
  },

  setLocalPresentationSessionFocus(
    this: GpuiSidebarRuntime,
    projectId: string,
    sessionId: string,
    targetGroupId?: string,
    exactVisibleSessionIds?: readonly string[]
  ): void {
    const normalizedProjectId = normalizeNonEmptyString(projectId);
    const normalizedSessionId = normalizeNonEmptyString(sessionId);
    if (!normalizedProjectId || !normalizedSessionId) {
      return;
    }
    this.activeProjectId = normalizedProjectId;
    this.activeGroupId =
      targetGroupId ??
      (this.isGpuiPresentationChatProjectId(normalizedProjectId)
        ? GPUI_GXSERVER_CHATS_GROUP_ID
        : createGxserverPresentationProjectGroupId(normalizedProjectId));
    this.refreshSidebarHudFromClient();
    this.focusedSessionId = normalizedSessionId;
    this.visibleSessionIds = exactVisibleSessionIds
      ? new Set(exactVisibleSessionIds)
      : this.nextVisibleSessionIdsForLocalFocus(normalizedProjectId, normalizedSessionId);
    this.postGxserverPresentationFocusState();
  },

  nextVisibleSessionIdsForLocalFocus(this: GpuiSidebarRuntime, projectId: string, sessionId: string): Set<string> {
    /*
    CDXC:FocusRouting 2026-06-26-04:42:
    GPUI local session focus should follow the macOS sidebar rule that a click selects the target within the current visible workspace projection instead of replacing all visible ownership with a singleton. Preserve live local visible ids and remote ids, materialize the current project's projected visible row, then add the clicked session so last-activity resorting cannot make a second session steal focus back.
    */
    const liveLocalSessionIds = new Set<string>(
      (this.presentation?.sessions ?? []).map((session) => session.sessionId)
    );
    const nextVisibleSessionIds = new Set(
      [...this.visibleSessionIds].filter(
        (visibleSessionId) =>
          parseGpuiRemotePresentationSessionId(visibleSessionId) || liveLocalSessionIds.has(visibleSessionId)
      )
    );
    const projectVisibleSessionIds = this.currentVisibleSessionIdsForLocalProject(projectId);
    for (const visibleSessionId of projectVisibleSessionIds) {
      nextVisibleSessionIds.add(visibleSessionId);
    }
    nextVisibleSessionIds.add(sessionId);
    return nextVisibleSessionIds;
  },

  currentVisibleSessionIdsForLocalProject(this: GpuiSidebarRuntime, projectId: string): string[] {
    const presentation = this.presentation;
    if (!presentation) {
      return [];
    }
    const sessions = createGxserverPresentationSessionsByProjectFromGroups({ presentation }).get(projectId) ?? [];
    return sessions.flatMap((session, index) =>
      this.visibleSessionIds.has(session.sessionId) || index === 0 ? [session.sessionId] : []
    );
  },

  isGpuiPresentationChatProjectId(this: GpuiSidebarRuntime, projectId: string): boolean {
    return (
      isGpuiPresentationChatDomainProject(this.domainProjectById(projectId)) ||
      isGpuiPresentationChatProjectPath(
        this.presentation?.projects.find((project) => project.projectId === projectId)?.path
      )
    );
  },

  setRemotePresentationSessionFocus(
    this: GpuiSidebarRuntime,
    reference: {
      machineId: string;
      projectId: string;
      sessionId: string;
    }
  ): void {
    const machineId = normalizeNonEmptyString(reference.machineId);
    const projectId = normalizeNonEmptyString(reference.projectId);
    const sessionId = normalizeNonEmptyString(reference.sessionId);
    if (!machineId || !projectId || !sessionId) {
      return;
    }
    const scopedSessionId = createGpuiRemotePresentationSessionId(machineId, projectId, sessionId);
    const project = this.remotePresentations
      .get(machineId)
      ?.projects.find((candidate) => candidate.projectId === projectId);
    const scopedGroupId = createGpuiRemotePresentationGroupId(
      machineId,
      isGpuiPresentationChatProjectPath(project?.path) ? GPUI_GXSERVER_CHATS_GROUP_ID : projectId
    );
    this.activeGroupId = scopedGroupId;
    this.focusedSessionId = scopedSessionId;
    this.visibleSessionIds = new Set([scopedSessionId]);
    this.postGxserverPresentationFocusState();
  },

  dropRemotePresentationSessionFocus(this: GpuiSidebarRuntime, machineId: string): void {
    if (this.focusedSessionId && parseGpuiRemotePresentationSessionId(this.focusedSessionId)?.machineId === machineId) {
      this.focusedSessionId = undefined;
    }
    this.visibleSessionIds = new Set(
      [...this.visibleSessionIds].filter(
        (sessionId) => parseGpuiRemotePresentationSessionId(sessionId)?.machineId !== machineId
      )
    );
  },
};

const gpuiSidebarRuntimeSessionFocusMethodsShapeCheck: GpuiSidebarRuntimeSessionFocusMethods =
  gpuiSidebarRuntimeSessionFocusMethods;
void gpuiSidebarRuntimeSessionFocusMethodsShapeCheck;
