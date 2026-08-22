/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  getGpuiWorkspaceSessionSubgroups,
  moveGpuiWorkspaceSessionToSubgroup,
  parseGpuiWorkspaceSessionSubgroupId,
} from "../workspace-session-groups";
import {
  GPUI_GXSERVER_CHATS_GROUP_ID,
  GPUI_QUICK_AUTOMATIONS_PROJECT_ID,
  GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_TYPE,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_VERSION,
  SESSION_LIFECYCLE_FAILURE_TITLES,
} from "./constants";
import type { GpuiSidebarRuntime } from "./core";
import { gpuiBrowserSidebarSessionId } from "./helpers/browser-tabs";
import { isGpuiInactiveProjectPresentationSession } from "./helpers/close-after-done";
import {
  isGpuiPresentationChatDomainProject,
  isGpuiPresentationChatProjectPath,
} from "./helpers/presentation-projection";
import { normalizeNonEmptyString } from "./helpers/records";
import {
  createGpuiRemotePresentationGroupId,
  createGpuiRemotePresentationSessionId,
  parseGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationSessionId,
} from "./helpers/remote-presentation";
import { shouldApplyGpuiLocalWorkspaceTransition } from "./helpers/terminal-lifecycle";
import { postAppModalHostMessage } from "@/packages/core-ui/app-modal-host-bridge";
import type { PreferredAgentInterface } from "@/packages/shared/ghostex-settings";
import { reorderPresentationProjectSessions } from "@/packages/shared/gxserver-presentation-cache";
import {
  createGxserverPresentationProjectGroupId,
  createGxserverPresentationProjectSessionId,
  createGxserverPresentationSessionsByProjectFromGroups,
  parseGxserverPresentationProjectGroupId,
  parseGxserverPresentationProjectSessionId,
} from "@/packages/shared/gxserver-presentation-sidebar-projection";
import type {
  GxserverEndpointPath,
  GxserverForkSessionResult,
  GxserverProjectId,
  GxserverSessionId,
  GxserverSessionRenameRequestResult,
  GxserverSessionTransitionResult,
} from "@/packages/shared/gxserver-protocol";
import type { SidebarToExtensionMessage } from "@/packages/shared/session-grid-contract";
import type { SidebarSessionTag } from "@/packages/shared/session-tags";

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
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
  focusLocalWorkspaceSession(projectId: string, sessionId: string, options?: { forceRemount?: boolean; preferredInterface?: PreferredAgentInterface }): void;
  postLocalWorkspaceTerminalFocus(projectId: string, sessionId: string, placementTargetSessionId?: string, options?: { forceRemount?: boolean; preferredInterface?: PreferredAgentInterface }): void;
  transitionSession(sessionId: string, action: "close" | "sleep"): Promise<void>;
  copySessionDetails(message: Extract<SidebarToExtensionMessage, { type: "copySessionDetails" }>): void;
  fullReloadSession(sessionId: string): Promise<void>;
  fullReloadProjectZmxSessions(groupId: string): Promise<void>;
  fullReloadWorkspaceGroup(groupId: string): Promise<void>;
  resolveLocalProjectListTransitionFocusTarget(projectId: string, removedSessionId: string): string | undefined;
  localProjectTransitionSessionIds(projectId: string, removedSessionId: string): string[];
  isRunningLocalPresentationSession(projectId: string, sessionId: string): boolean;
  isSleepingLocalPresentationSession(projectId: string, sessionId: string): boolean;
  forkSession(sessionId: string): Promise<void>;
  renameSession(message: Extract<SidebarToExtensionMessage, { type: "renameSession" }>): Promise<void>;
  updateSessionFlags(sessionId: string, flags: { isFavorite?: boolean; isPinned?: boolean; sessionTag?: SidebarSessionTag | null }): Promise<void>;
  runSessionLifecycleCommand(sessionId: string, path: Extract<
      GxserverEndpointPath,
      | "/api/settleSession"
      | "/api/snoozeSession"
      | "/api/unsettleSession"
      | "/api/unsnoozeSession"
    >, params: Record<string, unknown>): Promise<void>;
  syncSessionOrder(groupId: string, sessionIds: readonly string[]): Promise<void>;
  focusProjectId(projectId: string): void;
  setLocalPresentationSessionFocus(projectId: string, sessionId: string, targetGroupId?: string, exactVisibleSessionIds?: readonly string[]): void;
  nextVisibleSessionIdsForLocalFocus(projectId: string, sessionId: string): Set<string>;
  currentVisibleSessionIdsForLocalProject(projectId: string): string[];
  isGpuiPresentationChatProjectId(projectId: string): boolean;
  setRemotePresentationSessionFocus(reference: {
    machineId: string;
    projectId: string;
    sessionId: string;
  }): void;
  dropLocalPresentationSessionFocus(): void;
  dropRemotePresentationSessionFocus(machineId: string): void;
}

export const gpuiSidebarRuntimeSessionFocusMethods = {

  focusGroup(this: GpuiSidebarRuntime, groupId: string, originalMessage: SidebarToExtensionMessage): void {
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      const target = this.selectRemoteGroupAttachTarget(remoteGroup);
      if (!target) {
        this.postRemoteToast("info", "Remote attach unavailable", {
          description: "This remote project has no attachable sessions.",
        });
        return;
      }
      if (
        this.postRemoteSessionNativeAction("openRemoteSessionTerminal", target, originalMessage)
      ) {
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
        this.publishPresentation("patch");
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
    this.publishPresentation("patch");
  },

  openQuickAutomationsPage(this: GpuiSidebarRuntime): void {
    this.ensureQuickAutomationsProject();
    this.focusQuickAutomationsProject();
  },

  ensureQuickAutomationsProject(this: GpuiSidebarRuntime): void {
    /*
    CDXC:GPUIAutomationsOverview 2026-07-08:
    GPUI mirrors macOS `ensureQuickAutomationsProject` without daemon storage:
    macOS writes a client registry row, while GPUI keeps this overview as a
    session-local runtime projection until its synthetic Quick row is closed.
    */
    this.quickAutomationsOverviewOpen = true;
  },

  focusQuickAutomationsProject(this: GpuiSidebarRuntime): void {
    /*
    CDXC:GPUIAutomationsOverview 2026-07-08:
    Mirror macOS `focusQuickAutomationsProject`: selecting the synthetic
    quick-automations project activates the Quick group and focused overview row;
    Rust receives the Automate workarea through the active-project context post.
    */
    this.activeProjectId = GPUI_QUICK_AUTOMATIONS_PROJECT_ID;
    this.activeGroupId = GPUI_GXSERVER_CHATS_GROUP_ID;
    this.focusedSessionId = GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID;
    this.visibleSessionIds = new Set([
      ...this.visibleSessionIds,
      GPUI_QUICK_AUTOMATIONS_SIDEBAR_SESSION_ID,
    ]);
    if (this.presentation) {
      this.publishPresentation("patch");
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
      this.publishPresentation("patch");
      return;
    }
    this.postActiveProjectContext();
  },

  async focusSession(this: GpuiSidebarRuntime,
    sessionId: string,
    originalMessage?: SidebarToExtensionMessage,
  ): Promise<void> {
    const browserTab = this.browserTabs.find(
      (candidate) => gpuiBrowserSidebarSessionId(candidate) === sessionId,
    );
    if (browserTab) {
      /*
      A Browser row becomes the presentation focus owner when clicked. Clear
      the previous terminal owner before publishing the project change;
      otherwise ensureActiveProject resolves that stale terminal and switches
      the sidebar straight back to the terminal's project.
      */
      this.focusedSessionId = undefined;
      const remoteBrowserProject = parseGpuiRemotePresentationProjectId(browserTab.projectId);
      if (remoteBrowserProject) {
        const remoteGroupId = createGpuiRemotePresentationGroupId(
          remoteBrowserProject.machineId,
          remoteBrowserProject.projectId,
        );
        if (this.activeGroupId !== remoteGroupId) {
          this.activeGroupId = remoteGroupId;
          if (this.presentation) {
            this.publishPresentation("patch");
          } else {
            this.publishRemotePresentationPatch();
          }
        }
      } else if (this.activeProjectId !== browserTab.projectId) {
        this.focusProjectId(browserTab.projectId);
        if (this.presentation) {
          this.publishPresentation("patch");
        }
      }
      const post = window.ghostexGpui?.postBrowserTabFocus;
      if (typeof post === "function") {
        post(
          JSON.stringify({
            projectId: browserTab.projectId,
            tabId: browserTab.tabId,
            type: "ghostex.gpui.sidebar.browserTabFocus",
            version: 1,
          }),
        );
      }
      return;
    }
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      this.acknowledgeSessionAttention(sessionId, "sidebar-focus");
      if (
        this.postRemoteSessionNativeAction(
          "openRemoteSessionTerminal",
          remoteSession,
          originalMessage ?? { sessionId, type: "focusSession" },
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
    this.acknowledgeSessionAttention(sessionId, "sidebar-focus");
    if (this.isSleepingLocalPresentationSession(reference.projectId, reference.sessionId)) {
      /*
      CDXC:GPUIWorkspaceSessionFocus 2026-06-26-23:24:
      Sleeping local session-card clicks must match macOS session activation by committing gxserver `/api/wakeSession` before the Rust workspace materializes the terminal. A plain focus bridge can select the tab but leaves gxserver sleeping, so route this branch through the same Wake path as the sidebar sleep toggle.
      */
      await this.setSessionSleeping(sessionId, false);
      return;
    }
    /*
    CDXC:GPUISidebarSessionFocus 2026-06-26-04:42:
    Local GPUI sidebar clicks must match the macOS sidebar ownership model: the SidebarApp adapter applies local focus immediately and publishes the CEF bootstrap focus hint, but it must not call gxserver `/api/focusSession`. That endpoint is an external renderer-command route and can bounce focus when another renderer is the first open gxserver subscriber.
    */
    this.focusLocalWorkspaceSession(reference.projectId, reference.sessionId);
    this.publishPresentation("patch");
  },

  /*
  CDXC:SidebarDiffStatsChurn 2026-08-16:
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
      group.sessions.some((session) => session.sessionId === sessionId),
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
      type: "sidebarGroupsChanged",
    });
  },

  focusLocalWorkspaceSession(this: GpuiSidebarRuntime,
    projectId: string,
    sessionId: string,
    options?: { forceRemount?: boolean; preferredInterface?: PreferredAgentInterface },
  ): void {
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-06:18:
    Any successful local GPUI activation that makes a gxserver workspace session current must update both the reused SidebarApp presentation focus and the real GPUI Agents workspace. This matches macOS create, fork, restore, App Shot, and session-click behavior instead of requiring a second sidebar click to show the newly focused terminal.
    */
    const normalizedProjectId = normalizeNonEmptyString(projectId);
    const normalizedSessionId = normalizeNonEmptyString(sessionId);
    if (!normalizedProjectId || !normalizedSessionId) {
      return;
    }
    this.setLocalPresentationSessionFocus(normalizedProjectId, normalizedSessionId);
    this.postLocalWorkspaceTerminalFocus(
      normalizedProjectId,
      normalizedSessionId,
      undefined,
      options,
    );
  },

  postLocalWorkspaceTerminalFocus(this: GpuiSidebarRuntime,
    projectId: string,
    sessionId: string,
    placementTargetSessionId?: string,
    options?: { forceRemount?: boolean; preferredInterface?: PreferredAgentInterface },
  ): void {
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-06:08:
    Local GPUI session-card clicks must drive the real Agents workspace the way macOS does: after React updates gxserver presentation focus, send only bounded project/session ids to Rust so Rust can select or materialize the corresponding terminal tab from gxserver attach metadata. Do not pass labels, titles, commands, paths, terminal content, or daemon responses through the renderer bridge.
    */
    const postFocus = window.ghostexGpui?.postWorkspaceTerminalFocus;
    if (typeof postFocus !== "function") {
      return;
    }
    const payload = JSON.stringify({
      ...(placementTargetSessionId ? { placementTargetSessionId } : {}),
      ...(options?.forceRemount ? { forceRemount: true } : {}),
      ...(options?.preferredInterface
        ? { preferredInterface: options.preferredInterface }
        : {}),
      projectId,
      sessionId,
      type: GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_VERSION,
    });
    postFocus(payload);
  },

  async transitionSession(this: GpuiSidebarRuntime, sessionId: string, action: "close" | "sleep"): Promise<void> {
    const browserTab = this.browserTabs.find(
      (candidate) => gpuiBrowserSidebarSessionId(candidate) === sessionId,
    );
    if (browserTab) {
      if (action === "close") {
        window.ghostexGpui?.postBrowserTabFocus?.(
          JSON.stringify({
            close: true,
            projectId: browserTab.projectId,
            tabId: browserTab.tabId,
            type: "ghostex.gpui.sidebar.browserTabFocus",
            version: 1,
          }),
        );
      }
      return;
    }
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      this.postRemoteGxserverSidebarRequest(
        remoteSession.machineId,
        action === "close" ? "/api/killSession" : "/api/sleepSession",
        {
          projectId: remoteSession.projectId,
          reason: "gpui-sidebar",
          sessionId: remoteSession.sessionId,
        },
      );
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (reference?.projectId === GPUI_QUICK_AUTOMATIONS_PROJECT_ID) {
      if (action === "close" && this.isQuickAutomationsSidebarSessionId(sessionId)) {
        this.closeQuickAutomationsProject();
      }
      return;
    }
    if (!reference || !this.client) {
      return;
    }
    const replacementFocusSessionId = this.resolveLocalProjectListTransitionFocusTarget(
      reference.projectId,
      reference.sessionId,
    );
    if (action === "close") {
      this.removePresentationSession(reference.projectId, reference.sessionId);
      if (replacementFocusSessionId) {
        this.focusLocalWorkspaceSession(reference.projectId, replacementFocusSessionId);
        this.publishPresentation("patch");
      }
      await this.client
        .rpc<GxserverSessionTransitionResult>("/api/transitionSession", {
          action,
          projectId: reference.projectId,
          reason: "gpui-sidebar",
          sessionId: reference.sessionId,
        })
        .catch(() => undefined);
      return;
    }
    const result = await this.client.rpc<GxserverSessionTransitionResult>(
      "/api/transitionSession",
      {
        action,
        projectId: reference.projectId,
        reason: "gpui-sidebar",
        sessionId: reference.sessionId,
      },
    );
    if (!shouldApplyGpuiLocalWorkspaceTransition(result, action)) {
      return;
    }
    this.patchPresentationSession(reference.projectId, reference.sessionId, {
      lifecycleState: "sleeping",
    });
    if (replacementFocusSessionId) {
      this.focusLocalWorkspaceSession(reference.projectId, replacementFocusSessionId);
      this.publishPresentation("patch");
    }
  },

  copySessionDetails(this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: "copySessionDetails" }>,
  ): void {
    const detailsText = normalizeNonEmptyString(message.detailsText);
    if (!detailsText) {
      this.handleUnsupportedSidebarMessage(message);
      return;
    }
    try {
      postAppModalHostMessage(
        { detailsText, type: "copySessionDetails" },
        "GPUISidebarActions:copySessionDetails",
      );
    } catch {
      this.handleUnsupportedSidebarMessage(message);
    }
  },

  async fullReloadSession(this: GpuiSidebarRuntime, sessionId: string): Promise<void> {
    /*
    CDXC:GPUIFullReload 2026-07-12:
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

  async fullReloadProjectZmxSessions(this: GpuiSidebarRuntime, groupId: string): Promise<void> {
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      const presentation = this.remotePresentations.get(remoteGroup.machineId);
      const remoteSessionIds = (presentation?.sessions ?? [])
        .filter(
          (session) =>
            session.projectId === remoteGroup.projectId &&
            session.sessionPersistenceProvider === "zmx" &&
            isGpuiInactiveProjectPresentationSession(session),
        )
        .map((session) =>
          createGpuiRemotePresentationSessionId(
            remoteGroup.machineId,
            remoteGroup.projectId,
            session.sessionId,
          ),
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
          session.sessionPersistenceProvider === "zmx" &&
          isGpuiInactiveProjectPresentationSession(session),
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
        (group) => group.groupId === subgroup.groupId,
      )?.sessionIds ?? [];
    for (const sessionId of memberIds) {
      await this.fullReloadSession(
        remoteProject
          ? createGpuiRemotePresentationSessionId(
              remoteProject.machineId,
              remoteProject.projectId,
              sessionId,
            )
          : createGxserverPresentationProjectSessionId(subgroup.projectId, sessionId),
      );
    }
  },

  resolveLocalProjectListTransitionFocusTarget(this: GpuiSidebarRuntime,
    projectId: string,
    removedSessionId: string,
  ): string | undefined {
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-06:34:
    Sidebar-origin local close/sleep must follow the macOS project-list focus rule: background transitions do not steal focus, while closing or sleeping the focused session selects the next running row from the same displayed local project order and routes it through the workspace focus bridge.
    */
    const normalizedProjectId = normalizeNonEmptyString(projectId);
    const normalizedRemovedSessionId = normalizeNonEmptyString(removedSessionId);
    if (
      !normalizedProjectId ||
      !normalizedRemovedSessionId ||
      this.focusedSessionId !== normalizedRemovedSessionId
    ) {
      return undefined;
    }
    const orderedSessionIds = this.localProjectTransitionSessionIds(
      normalizedProjectId,
      normalizedRemovedSessionId,
    );
    const removedIndex = orderedSessionIds.indexOf(normalizedRemovedSessionId);
    const candidates =
      removedIndex >= 0
        ? [
            ...orderedSessionIds.slice(removedIndex + 1),
            ...orderedSessionIds.slice(0, removedIndex),
          ]
        : orderedSessionIds;
    const replacementSessionId = candidates.find(
      (candidateSessionId) =>
        candidateSessionId !== normalizedRemovedSessionId &&
        this.isRunningLocalPresentationSession(normalizedProjectId, candidateSessionId),
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
          session.projectId === projectId &&
          session.sessionId === sessionId &&
          session.lifecycleState === "running",
      ) ?? false
    );
  },

  isSleepingLocalPresentationSession(this: GpuiSidebarRuntime, projectId: string, sessionId: string): boolean {
    const presentationSleeping =
      this.presentation?.sessions.some(
        (session) =>
          session.projectId === projectId &&
          session.sessionId === sessionId &&
          session.lifecycleState === "sleeping",
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
          (session.lifecycleState === "sleeping" || session.isSleeping === true),
      ),
    );
  },

  async forkSession(this: GpuiSidebarRuntime, sessionId: string): Promise<void> {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      /*
      CDXC:GPUIRemoteSessions 2026-06-24-17:19:
      Remote fork authority comes only from a machine-prefixed session id already present in the remote presentation snapshot. Route the project/session ids to `/api/forkSession` on that machine; do not derive ids from labels or terminal text.

      CDXC:GPUIForkParity 2026-07-10:
      Match macOS remote Fork exactly: the owning gxserver creates the fork and
      the refreshed remote presentation renders it without moving focus away
      from the session the user was viewing.
      */
      try {
        await this.requestRemoteGxserver(remoteSession.machineId, "/api/forkSession", {
          projectId: remoteSession.projectId,
          reason: "gpui-sidebar",
          sessionId: remoteSession.sessionId,
        });
        await this.refreshRemotePresentationFromGxserver(remoteSession.machineId).catch(
          () => undefined,
        );
      } catch (error) {
        this.postRemoteToast("error", "Remote fork failed", {
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
        (session) =>
          session.projectId === reference.projectId && session.sessionId === reference.sessionId,
      )
    ) {
      return;
    }

    const sourceGroupId =
      this.workspaceSubgroupSidebarIdForSession(reference.projectId, reference.sessionId) ??
      createGxserverPresentationProjectGroupId(reference.projectId);
    if (this.activeProjectId !== reference.projectId || this.activeGroupId !== sourceGroupId) {
      /*
      CDXC:GPUIForkParity 2026-07-10:
      macOS focuses the clicked session's project before awaiting gxserver.
      GPUI also activates its clicked sidebar subgroup so Rust has the source
      tab-group mapping before the fork result arrives.
      */
      this.activeProjectId = reference.projectId;
      this.activeGroupId = sourceGroupId;
      this.refreshSidebarHudFromClient();
      this.publishPresentation("patch");
    }

    try {
      /*
      CDXC:GPUIForkParity 2026-07-10:
      `/api/forkSession` returns `{ fork }`, exactly as the macOS gxserver
      client unwraps it. The previous GPUI code treated the result itself as
      the fork payload, so `response.session` was undefined and the action
      could not materialize or focus the returned G-session.
      */
      const { fork } = await this.client.rpc<{ fork: GxserverForkSessionResult }>(
        "/api/forkSession",
        {
          projectId: reference.projectId,
          reason: "gpui-sidebar",
          sessionId: reference.sessionId,
        },
      );
      const forkedSessionId = normalizeNonEmptyString(fork?.session.sessionId);
      if (!forkedSessionId) {
        throw new Error("gxserver did not return the forked session.");
      }

      const sourceSubgroup = parseGpuiWorkspaceSessionSubgroupId(sourceGroupId);
      if (sourceSubgroup) {
        this.workspaceGroups = moveGpuiWorkspaceSessionToSubgroup(
          this.workspaceGroups,
          reference.projectId,
          forkedSessionId,
          sourceSubgroup.groupId,
        );
        this.persistWorkspaceGroups();
      }

      this.setLocalPresentationSessionFocus(reference.projectId, forkedSessionId, sourceGroupId);
      this.publishPresentation("patch");
      /*
      The placement target is the clicked source session, not whichever pane
      happens to be focused when the RPC completes. Rust resolves this bounded
      id to the existing pane and appends the fork there before mounting the
      gxserver attach plan, matching macOS appendToTabGroup behavior.
      */
      this.postLocalWorkspaceTerminalFocus(
        reference.projectId,
        forkedSessionId,
        reference.sessionId,
      );
      await this.refreshDomainPresentationSnapshotFromClient("patch").catch(() => undefined);
    } catch (error) {
      this.postSidebarActionToast("error", "Could not fork session", {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  },

  async renameSession(this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: "renameSession" }>,
  ): Promise<void> {
    const remoteSession = parseGpuiRemotePresentationSessionId(message.sessionId);
    if (remoteSession) {
      /*
      CDXC:SessionHistoryTitleSource 2026-07-29:
      Empty-title Generate Name is a local-transcript flow; a remote machine's
      transcripts are not readable here, and a blank direct rename would erase
      the remote title.
      */
      if (!message.title.trim()) {
        return;
      }
      /*
      CDXC:GPUIRemoteSessionRename 2026-08-12:
      Remote agent sessions must use the same pending-metadata rename contract
      as local sessions. The remote gxserver owns that session's zmx provider,
      so ask it to submit the provider-specific slash command itself instead of
      only updating sidebar metadata or trying to use GPUI's local Ghostty
      surface bridge.
      */
      this.postRemoteGxserverSidebarRequest(remoteSession.machineId, "/api/requestSessionRename", {
        ...(message.agentId ? { agentName: message.agentId } : {}),
        projectId: remoteSession.projectId,
        reason: "gpui-sidebar",
        sessionId: remoteSession.sessionId,
        submitAgentRenameCommand: true,
        title: message.title,
        titleSource: "user",
      });
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(message.sessionId);
    if (!reference || !this.client) {
      return;
    }
    if (message.shouldGenerateTitle) {
      /*
      CDXC:GPUISidebarRename 2026-07-29:
      Generate Name reuses the first-message auto-title UX end to end:
      gxserver marks the session generating (the card shows the same
      "Generating title…" chrome), summarizes the pasted text with the chosen
      generation agent, stages the agent rename command through zmx with the
      same delayed real Enter, and applies the generated title. The long
      pasted text must never reach `/api/requestSessionRename` as a literal
      title.
      */
      const generationAgent = this.resolveSidebarAgent(message.agentId ?? "");
      const generationCommand = generationAgent?.command?.trim();
      await this.client.rpc("/api/generateSessionTitle", {
        ...(message.agentId ? { agentId: message.agentId } : {}),
        ...(generationCommand ? { command: generationCommand } : {}),
        projectId: reference.projectId,
        sessionId: reference.sessionId,
        text: message.title,
      });
      return;
    }
    const result = await this.client.rpc<GxserverSessionRenameRequestResult>(
      "/api/requestSessionRename",
      {
        agentName: message.agentId,
        projectId: reference.projectId,
        reason: "gpui-sidebar",
        sessionId: reference.sessionId,
        title: message.title,
        titleSource: "user",
      },
    );
    /*
    CDXC:GPUISidebarRename 2026-08-18:
    Session cards render `displayTitle`, so patching only `title` moved the
    row's alias without changing the text on the card. Apply gxserver's own
    title projection instead — the same fields presentation publishes — so the
    card, its tooltip, and the alias stay one consistent title. Agent sessions
    keep the previous title here until the Agent CLI confirms the rename; the
    confirmed title lands through the normal presentation delta.
    */
    this.patchPresentationSession(
      reference.projectId,
      reference.sessionId,
      result.projection,
    );
    /*
    CDXC:GPUISidebarRename 2026-07-28:
    gxserver keeps agent-session renames pending until the Agent CLI itself is
    renamed, and it answers `shouldSendAgentRenameCommand` so the client stages
    `/rename <title>` (Pi uses `/name`; Hermes Agent uses `/title`) into the
    mapped terminal — the same contract macOS follows.
    */
    if (result.shouldSendAgentRenameCommand) {
      this.postLocalWorkspaceTerminalRenameCommand(
        reference.projectId,
        reference.sessionId,
        message.title,
      );
    }
  },

  async updateSessionFlags(this: GpuiSidebarRuntime,
    sessionId: string,
    flags: { isFavorite?: boolean; isPinned?: boolean; sessionTag?: SidebarSessionTag | null },
  ): Promise<void> {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      this.postRemoteGxserverSidebarRequest(remoteSession.machineId, "/api/updateSession", {
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
    await this.client.rpc("/api/updateSession", {
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

  /*
  CDXC:SidebarV2Lifecycle 2026-07-29:
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
  async runSessionLifecycleCommand(this: GpuiSidebarRuntime,
    sessionId: string,
    path: Extract<
      GxserverEndpointPath,
      | "/api/settleSession"
      | "/api/snoozeSession"
      | "/api/unsettleSession"
      | "/api/unsnoozeSession"
    >,
    params: Record<string, unknown>,
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
      this.postSidebarActionToast("warning", SESSION_LIFECYCLE_FAILURE_TITLES[path], {
        description: "gxserver refused the change. The session may be working or waiting on you.",
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
      gxserverSessionIds as GxserverSessionId[],
    );
    this.publishPresentation("patch");
    await this.client.rpc("/api/updateSessionOrder", {
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

  setLocalPresentationSessionFocus(this: GpuiSidebarRuntime,
    projectId: string,
    sessionId: string,
    targetGroupId?: string,
    exactVisibleSessionIds?: readonly string[],
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
    CDXC:GPUISidebarSessionFocus 2026-06-26-04:42:
    GPUI local session focus should follow the macOS sidebar rule that a click selects the target within the current visible workspace projection instead of replacing all visible ownership with a singleton. Preserve live local visible ids and remote ids, materialize the current project's projected visible row, then add the clicked session so last-activity resorting cannot make a second session steal focus back.
    */
    const liveLocalSessionIds = new Set<string>(
      (this.presentation?.sessions ?? []).map((session) => session.sessionId),
    );
    const nextVisibleSessionIds = new Set(
      [...this.visibleSessionIds].filter(
        (visibleSessionId) =>
          parseGpuiRemotePresentationSessionId(visibleSessionId) ||
          liveLocalSessionIds.has(visibleSessionId),
      ),
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
    const sessions =
      createGxserverPresentationSessionsByProjectFromGroups({ presentation }).get(projectId) ?? [];
    return sessions.flatMap((session, index) =>
      this.visibleSessionIds.has(session.sessionId) || index === 0 ? [session.sessionId] : [],
    );
  },

  isGpuiPresentationChatProjectId(this: GpuiSidebarRuntime, projectId: string): boolean {
    return (
      isGpuiPresentationChatDomainProject(this.domainProjectById(projectId)) ||
      isGpuiPresentationChatProjectPath(
        this.presentation?.projects.find((project) => project.projectId === projectId)?.path,
      )
    );
  },

  setRemotePresentationSessionFocus(this: GpuiSidebarRuntime, reference: {
    machineId: string;
    projectId: string;
    sessionId: string;
  }): void {
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
      isGpuiPresentationChatProjectPath(project?.path)
        ? GPUI_GXSERVER_CHATS_GROUP_ID
        : projectId,
    );
    this.activeGroupId = scopedGroupId;
    this.focusedSessionId = scopedSessionId;
    this.visibleSessionIds = new Set([scopedSessionId]);
    this.postGxserverPresentationFocusState();
  },

  dropLocalPresentationSessionFocus(this: GpuiSidebarRuntime): void {
    if (this.focusedSessionId && !parseGpuiRemotePresentationSessionId(this.focusedSessionId)) {
      this.focusedSessionId = undefined;
    }
    this.visibleSessionIds = new Set(
      [...this.visibleSessionIds].filter((sessionId) =>
        Boolean(parseGpuiRemotePresentationSessionId(sessionId)),
      ),
    );
  },

  dropRemotePresentationSessionFocus(this: GpuiSidebarRuntime, machineId: string): void {
    if (
      this.focusedSessionId &&
      parseGpuiRemotePresentationSessionId(this.focusedSessionId)?.machineId === machineId
    ) {
      this.focusedSessionId = undefined;
    }
    this.visibleSessionIds = new Set(
      [...this.visibleSessionIds].filter(
        (sessionId) => parseGpuiRemotePresentationSessionId(sessionId)?.machineId !== machineId,
      ),
    );
  },
};

const gpuiSidebarRuntimeSessionFocusMethodsShapeCheck: GpuiSidebarRuntimeSessionFocusMethods = gpuiSidebarRuntimeSessionFocusMethods;
void gpuiSidebarRuntimeSessionFocusMethodsShapeCheck;
