/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import { parseGpuiWorkspaceSessionSubgroupId } from '../workspace-session-groups';
import {
  APP_SHOT_PROMPT_INSERT_RESULT_TIMEOUT_MS,
  APP_SHOT_RECENT_TARGET_MS,
  GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_TYPE,
  GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_VERSION,
  GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_TYPE,
  GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_VERSION,
  GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_TYPE,
  GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_VERSION,
  GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE,
  GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
  GPUI_SIDEBAR_OPEN_BROWSER_URL_MAX_CHARS,
  GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE,
  GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION,
} from './constants';
import type { GpuiSidebarRuntime } from './core';
import {
  formatGpuiNativeAppShotPrompt,
  isNativeAppShotAgentSession,
  localGxserverProjectIdForSidebarSession,
  localGxserverSessionIdForSidebarSession,
  nativeAppShotPromptSessionIdForSidebarSession,
  normalizeGpuiNativeAppShotCapture,
  normalizeGpuiNativeAppShotPromptResult,
} from './helpers/app-shot';
import { createGpuiSidebarSettings } from './helpers/bootstrap';
import {
  normalizeGpuiCommandPaletteRunSidebarCommand,
  normalizeGpuiCommandPaletteSessionFocus,
} from './helpers/command-palette';
import { normalizeNonEmptyString, readGpuiRecordString } from './helpers/records';
import {
  createGpuiRemotePresentationGroupId,
  createGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationSessionId,
} from './helpers/remote-presentation';
import {
  normalizeGpuiRendererCommandRenameTitle,
  parseGpuiRendererCommandGlobalSessionRef,
  readGpuiRendererCommandSessionTarget,
} from './helpers/renderer-commands';
import {
  gpuiMenuBarStatusSessionFocusRoutingId,
  normalizeGpuiMenuBarProjectActivation,
  normalizeGpuiMenuBarSessionActivation,
  normalizeGpuiStatusPetActivation,
} from './helpers/status-indicators';
import type {
  GpuiPendingNativeAppShotPromptInsertion,
  GpuiRendererCommandResolvedSession,
  GpuiSidebarNativeProjectPathAction,
} from './types-and-protocol';
import { openAppModal, postAppModalHostMessage } from '@/packages/core-ui/app-modal-host-bridge';
import type { AppToastLevel } from '@/packages/shared/app-toast-contract';
import { createAppToastRequest } from '@/packages/shared/app-toast-contract';
import type { PreferredAgentInterface } from '@/packages/shared/ghostex-settings';
import {
  createGxserverPresentationProjectSessionId,
  parseGxserverPresentationProjectSessionId,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type { GxserverRendererCommand } from '@/packages/shared/gxserver-protocol';
import type { NavigationHistoryEntry } from '@/packages/shared/navigation-history/navigation-history-contract';
import type { NavigationHistoryRpc } from '@/packages/shared/navigation-history/navigation-history-controller';
import type { SidebarSessionItem, SidebarToExtensionMessage } from '@/packages/shared/session-grid-contract';
import type { SidebarCommandButton } from '@/packages/shared/sidebar-commands';
import { isSidebarCommandConfigured, isSidebarCommandRunMode } from '@/packages/shared/sidebar-commands';

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeAppShotAndMiscMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeAppShotAndMiscMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeAppShotAndMiscMethods {
  handleGpuiStatusPetActivation(payload: unknown): void;
  handleGpuiMenuBarProjectActivation(payload: unknown): void;
  handleGpuiMenuBarSessionActivation(payload: unknown): Promise<void>;
  handleGpuiCommandPaletteSessionFocus(payload: unknown): Promise<void>;
  handleGpuiCommandPaletteRunSidebarCommand(payload: unknown): void;
  handleNativeAppShotCaptured(payload: unknown): Promise<void>;
  stageNativeAppShotInAgentSession(prompt: string): Promise<{ ok: true } | { description: string; ok: false }>;
  stageNativeAppShotInExistingAgentSession(session: SidebarSessionItem, prompt: string): Promise<boolean>;
  resolveNativeAppShotTargetSession(): SidebarSessionItem | undefined;
  findNativeAppShotSessionByPresentationSessionId(sessionId: string): SidebarSessionItem | undefined;
  findNativeAppShotSessionByLocalGxserverSessionId(sessionId: string): SidebarSessionItem | undefined;
  findNativeAppShotSessionByRemotePresentationSessionId(sessionId: string): SidebarSessionItem | undefined;
  postNativeAppShotPromptToSession(sessionId: string, prompt: string): Promise<boolean>;
  handleNativeAppShotPromptResult(payload: unknown): void;
  resolvePendingNativeAppShotPromptInsertion(pending: GpuiPendingNativeAppShotPromptInsertion, ok: boolean): void;
  rememberNativeAppShotTargetSessionId(sessionId: string): void;
  handleGxserverRendererCommand(command: GxserverRendererCommand): Promise<Record<string, unknown>>;
  runGxserverRendererCommandButton(
    rawCommandId: string | undefined,
    rendererCommand: GxserverRendererCommand
  ): Record<string, unknown>;
  openEmbeddedBrowserFromRendererCommand(command: GxserverRendererCommand): Record<string, unknown>;
  resolveEmbeddedBrowserRendererCommandProjectId(scope: {
    groupId?: string;
    projectId?: string;
    projectPath?: string;
  }): string | undefined;
  resolveEmbeddedBrowserKnownProjectId(projectId: string): string | undefined;
  resolveGxserverRendererCommandSession(
    payload: Record<string, unknown>
  ): GpuiRendererCommandResolvedSession | undefined;
  hasGpuiRendererCommandLocalSession(projectId: string, sessionId: string): boolean;
  createNavigationHistoryEntry(): NavigationHistoryEntry | undefined;
  navigationHistoryRpc(): NavigationHistoryRpc | undefined;
  activateNavigationHistoryEntry(entry: NavigationHistoryEntry): boolean;
  postNavigationHistoryState(state: { canGoBack: boolean; canGoForward: boolean }): void;
  postAppShotToast(
    level: AppToastLevel,
    title: string,
    options?: {
      description?: string;
    }
  ): void;
  postSidebarActionToast(level: AppToastLevel, title: string, options?: { description?: string }): void;
  copyWorkspaceProjectRemoteUrl(
    message: Extract<SidebarToExtensionMessage, { type: 'copyWorkspaceProjectRemoteUrl' }>
  ): void;
  postProjectPathActionForGroup(
    action: Extract<
      GpuiSidebarNativeProjectPathAction,
      'copyWorkspaceProjectPath' | 'openWorkspaceProjectInFinder' | 'openWorkspaceProjectInIde'
    >,
    groupId: string,
    originalMessage: SidebarToExtensionMessage
  ): void;
  postActiveProjectPathAction(
    action: Extract<
      GpuiSidebarNativeProjectPathAction,
      'openActiveWorkspaceProjectInFinder' | 'openActiveWorkspaceProjectInVscode' | 'openActiveWorkspaceProjectInZed'
    >,
    originalMessage: SidebarToExtensionMessage
  ): void;
  postNativeProjectPathAction(
    action: GpuiSidebarNativeProjectPathAction,
    projectId: string,
    originalMessage: SidebarToExtensionMessage,
    options?: { filePath?: string; preferredInterface?: PreferredAgentInterface }
  ): boolean;
  postSidebarCommandAction(
    command: SidebarCommandButton,
    selectionMessage: Extract<SidebarToExtensionMessage, { type: 'runSidebarCommand' }>
  ): boolean;
  postGhostexHotkeyAction(
    originalMessage: Extract<SidebarToExtensionMessage, { type: 'runGhostexHotkeyAction' }>
  ): boolean;
  postSidebarCommandRunEnd(commandId: string, originalMessage: SidebarToExtensionMessage): boolean;
  saveSidebarSettingsPatch(message: Extract<SidebarToExtensionMessage, { type: 'updateSettingsPatch' }>): void;
  openExternalUrl(message: Extract<SidebarToExtensionMessage, { type: 'openExternalUrl' }>): void;
  openAppModal(modal: 'firstLaunchSetup' | 'settings' | 'watchGhostexVideo'): void;
  saveScratchPad(content: string): Promise<void>;
  savePinnedPrompt(message: Extract<SidebarToExtensionMessage, { type: 'savePinnedPrompt' }>): Promise<void>;
  publishAppUserDataHydrate(): void;
}

export const gpuiSidebarRuntimeAppShotAndMiscMethods = {
  copyWorkspaceProjectRemoteUrl(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'copyWorkspaceProjectRemoteUrl' }>
  ): void {
    const remoteUrl = normalizeNonEmptyString(message.remoteUrl);
    if (!remoteUrl) {
      this.handleUnsupportedSidebarMessage(message);
      return;
    }
    try {
      postAppModalHostMessage(
        { detailsText: remoteUrl, type: 'copySessionDetails' },
        'GPUISidebarActions:copyRemoteUrl'
      );
    } catch {
      this.handleUnsupportedSidebarMessage(message);
    }
  },

  handleGpuiStatusPetActivation(this: GpuiSidebarRuntime, payload: unknown): void {
    const activation = normalizeGpuiStatusPetActivation(payload);
    if (!activation) {
      return;
    }
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-05:07:
    Visible GPUI status activation, and later pet activation, must re-enter the sidebar runtime's existing focusSession route. Keep this as a fixed callback with one bounded session id so local focus stays local, remote focus uses the reviewed remote native action path, and Rust never creates or wakes unrelated sessions for indicator clicks.
    */
    void this.focusSession(activation.sessionId, {
      sessionId: activation.sessionId,
      type: 'focusSession',
    });
  },

  handleGpuiMenuBarProjectActivation(this: GpuiSidebarRuntime, payload: unknown): void {
    const activation = normalizeGpuiMenuBarProjectActivation(payload);
    if (!activation) {
      return;
    }
    /*
    CDXC:GPUIMenuBarStatusItem 2026-06-26-06:05:
    Running Agents project rows should behave like focusing the matching sidebar project group. Reuse local focusProjectId or the remote group projection plus the normal presentation publish instead of creating a native-only project switch path, and accept only the bounded project id from Rust.
    */
    const remoteProject = parseGpuiRemotePresentationProjectId(activation.projectId);
    if (remoteProject) {
      this.activeGroupId = createGpuiRemotePresentationGroupId(remoteProject.machineId, remoteProject.projectId);
      this.publishRemotePresentationPatch();
      return;
    }
    this.focusProjectId(activation.projectId);
    this.publishPresentation('patch');
  },

  async handleGpuiMenuBarSessionActivation(this: GpuiSidebarRuntime, payload: unknown): Promise<void> {
    const activation = normalizeGpuiMenuBarSessionActivation(payload);
    if (!activation) {
      return;
    }
    /*
    CDXC:GPUIMenuBarStatusItem 2026-06-26-06:05:
    Running Agents session rows should behave like sidebar session-card clicks. Normalize raw local gxserver ids into the existing project-scoped presentation id when needed, then reuse focusSession so local clicks update presentation focus and post WorkspaceTerminalFocus back to Rust for terminal selection/materialization.
    */
    const sessionId = gpuiMenuBarStatusSessionFocusRoutingId(activation.projectId, activation.sessionId);
    await this.focusSession(sessionId, {
      sessionId,
      type: 'focusSession',
    });
  },

  async handleGpuiCommandPaletteSessionFocus(this: GpuiSidebarRuntime, payload: unknown): Promise<void> {
    /*
    Command-palette current-session rows post {type:"focusSession"} from the
    app-modal host window; Rust forwards only the bounded projected session id
    here so palette selection reuses the same reviewed focusSession routing as
    sidebar card clicks (local materialize/wake, remote-shaped ids included).
    */
    const sessionId = normalizeGpuiCommandPaletteSessionFocus(payload);
    if (!sessionId) {
      return;
    }
    await this.focusSession(sessionId, {
      sessionId,
      type: 'focusSession',
    });
  },

  handleGpuiCommandPaletteRunSidebarCommand(this: GpuiSidebarRuntime, payload: unknown): void {
    /*
    Command-palette Action rows post {type:"runSidebarCommand"} from the
    app-modal host window; Rust forwards only the selector (command id +
    optional runMode). Execution resolves the trusted saved/HUD command and
    goes through the same strict SidebarCommandAction bridge as sidebar-surface
    Action clicks.
    */
    const selection = normalizeGpuiCommandPaletteRunSidebarCommand(payload);
    if (!selection) {
      return;
    }
    this.runSidebarCommand(selection.message.commandId, selection.message, selection.scope);
  },

  async handleNativeAppShotCaptured(this: GpuiSidebarRuntime, payload: unknown): Promise<void> {
    const appShot = normalizeGpuiNativeAppShotCapture(payload);
    if (!appShot) {
      this.postAppShotToast('warning', 'App Shot Failed', {
        description: 'Could not read the native App Shot.',
      });
      return;
    }

    const prompt = formatGpuiNativeAppShotPrompt(
      appShot,
      createGpuiSidebarSettings(this.runtimeSettings).appShotsMetadataEnabled
    );
    const staged = await this.stageNativeAppShotInAgentSession(prompt);
    if (!staged.ok) {
      this.postAppShotToast('warning', 'App Shot Failed', {
        description: staged.description,
      });
      return;
    }

    this.postAppShotToast('success', 'App Shot Added', {
      description: appShot.appName,
    });
  },

  async stageNativeAppShotInAgentSession(
    this: GpuiSidebarRuntime,
    prompt: string
  ): Promise<{ ok: true } | { description: string; ok: false }> {
    /*
    CDXC:GPUIAppShots 2026-06-25-23:28:
    GPUI App Shots mirror macOS target order for local sessions: reuse the last successful local App Shot target for 60 seconds when it is still a live local agent row, otherwise use the focused/visible local agent row, and create a default prompt-agent session only when the exact local insert bridge declines. Keep command-pane, sleeping, stale, non-agent, and sidebar-only rows out of insertion.

    CDXC:GPUIAppShots 2026-06-26-04:27:
    Existing-session App Shot targeting now accepts live remote agent rows by their machine-scoped presentation session id, but only as an insertion request to Rust. React must not wake, materialize, or open remote attach tabs for App Shots; Rust may write only when that exact remote attach surface is already mounted.
    */
    const targetSession = this.resolveNativeAppShotTargetSession();
    if (targetSession && (await this.stageNativeAppShotInExistingAgentSession(targetSession, prompt))) {
      return { ok: true };
    }

    if (!this.client) {
      return {
        description: 'The local agent service is not ready.',
        ok: false,
      };
    }
    const project = this.activeDomainProject();
    if (!project) {
      return {
        description: 'Open a project before using App Shots.',
        ok: false,
      };
    }
    const agent = this.resolveDefaultPromptAgent();
    if (!agent?.command?.trim()) {
      return {
        description: 'Choose a configured default prompt agent before using App Shots.',
        ok: false,
      };
    }

    try {
      const sessionId = await this.createAgentSessionForProject(project, agent, prompt);
      this.rememberNativeAppShotTargetSessionId(sessionId);
      return { ok: true };
    } catch {
      return {
        description: 'Could not stage the App Shot in an agent session.',
        ok: false,
      };
    }
  },

  async stageNativeAppShotInExistingAgentSession(
    this: GpuiSidebarRuntime,
    session: SidebarSessionItem,
    prompt: string
  ): Promise<boolean> {
    const sessionId = nativeAppShotPromptSessionIdForSidebarSession(session);
    if (!sessionId) {
      return false;
    }
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      this.setRemotePresentationSessionFocus(remoteSession);
    } else {
      const projectId = localGxserverProjectIdForSidebarSession(session, this.presentation);
      if (projectId) {
        this.focusLocalWorkspaceSession(projectId, sessionId);
      } else {
        this.focusedSessionId = sessionId;
        this.visibleSessionIds = new Set([sessionId]);
        this.postGxserverPresentationFocusState();
      }
    }
    const inserted = await this.postNativeAppShotPromptToSession(sessionId, prompt);
    if (inserted) {
      this.rememberNativeAppShotTargetSessionId(sessionId);
    }
    return inserted;
  },

  resolveNativeAppShotTargetSession(this: GpuiSidebarRuntime): SidebarSessionItem | undefined {
    const now = Date.now();
    const recentTarget =
      this.lastAppShotTargetSessionId && now - this.lastAppShotTargetAt <= APP_SHOT_RECENT_TARGET_MS
        ? this.findNativeAppShotSessionByPresentationSessionId(this.lastAppShotTargetSessionId)
        : undefined;
    if (isNativeAppShotAgentSession(recentTarget)) {
      return recentTarget;
    }

    const focusedSession = this.focusedSessionId
      ? this.findNativeAppShotSessionByPresentationSessionId(this.focusedSessionId)
      : undefined;
    if (isNativeAppShotAgentSession(focusedSession)) {
      return focusedSession;
    }

    for (const sessionId of this.visibleSessionIds) {
      const visibleSession = this.findNativeAppShotSessionByPresentationSessionId(sessionId);
      if (visibleSession?.isVisible && isNativeAppShotAgentSession(visibleSession)) {
        return visibleSession;
      }
    }
    return undefined;
  },

  findNativeAppShotSessionByPresentationSessionId(
    this: GpuiSidebarRuntime,
    sessionId: string
  ): SidebarSessionItem | undefined {
    const normalizedSessionId = normalizeNonEmptyString(sessionId);
    if (!normalizedSessionId) {
      return undefined;
    }
    if (parseGpuiRemotePresentationSessionId(normalizedSessionId)) {
      return this.findNativeAppShotSessionByRemotePresentationSessionId(normalizedSessionId);
    }
    return this.findNativeAppShotSessionByLocalGxserverSessionId(normalizedSessionId);
  },

  findNativeAppShotSessionByLocalGxserverSessionId(
    this: GpuiSidebarRuntime,
    sessionId: string
  ): SidebarSessionItem | undefined {
    const normalizedSessionId = normalizeNonEmptyString(sessionId);
    if (!normalizedSessionId || parseGpuiRemotePresentationSessionId(normalizedSessionId)) {
      return undefined;
    }
    for (const group of this.latestGroups) {
      if (group.remoteMachineContext) {
        continue;
      }
      const session = group.sessions.find(
        (candidate) => localGxserverSessionIdForSidebarSession(candidate) === normalizedSessionId
      );
      if (session) {
        return session;
      }
    }
    return undefined;
  },

  findNativeAppShotSessionByRemotePresentationSessionId(
    this: GpuiSidebarRuntime,
    sessionId: string
  ): SidebarSessionItem | undefined {
    const normalizedSessionId = normalizeNonEmptyString(sessionId);
    if (!normalizedSessionId || !parseGpuiRemotePresentationSessionId(normalizedSessionId)) {
      return undefined;
    }
    for (const group of this.latestGroups) {
      if (!group.remoteMachineContext) {
        continue;
      }
      const session = group.sessions.find((candidate) => candidate.sessionId === normalizedSessionId);
      if (session) {
        return session;
      }
    }
    return undefined;
  },

  async postNativeAppShotPromptToSession(
    this: GpuiSidebarRuntime,
    sessionId: string,
    prompt: string
  ): Promise<boolean> {
    const postPrompt = window.ghostexGpui?.postNativeAppShotPromptToSession;
    if (typeof postPrompt !== 'function') {
      return false;
    }
    const payload = JSON.stringify({
      prompt,
      sessionId,
      type: GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_NATIVE_APP_SHOT_PROMPT_MESSAGE_VERSION,
    });

    return await new Promise<boolean>((resolve) => {
      const pending: GpuiPendingNativeAppShotPromptInsertion = {
        resolve,
        sessionId,
        timeoutId: 0,
      };
      pending.timeoutId = window.setTimeout(() => {
        this.resolvePendingNativeAppShotPromptInsertion(pending, false);
      }, APP_SHOT_PROMPT_INSERT_RESULT_TIMEOUT_MS);
      this.pendingNativeAppShotPromptInsertions.push(pending);
      let sent = false;
      try {
        sent = postPrompt(payload) === true;
      } catch {
        sent = false;
      }
      if (!sent) {
        this.resolvePendingNativeAppShotPromptInsertion(pending, false);
      }
    });
  },

  handleNativeAppShotPromptResult(this: GpuiSidebarRuntime, payload: unknown): void {
    const result = normalizeGpuiNativeAppShotPromptResult(payload);
    if (!result) {
      return;
    }
    const pending = this.pendingNativeAppShotPromptInsertions.find(
      (candidate) => candidate.sessionId === result.sessionId
    );
    if (!pending) {
      return;
    }
    this.resolvePendingNativeAppShotPromptInsertion(pending, result.ok);
  },

  resolvePendingNativeAppShotPromptInsertion(
    this: GpuiSidebarRuntime,
    pending: GpuiPendingNativeAppShotPromptInsertion,
    ok: boolean
  ): void {
    const index = this.pendingNativeAppShotPromptInsertions.indexOf(pending);
    if (index >= 0) {
      this.pendingNativeAppShotPromptInsertions.splice(index, 1);
    }
    window.clearTimeout(pending.timeoutId);
    pending.resolve(ok);
  },

  rememberNativeAppShotTargetSessionId(this: GpuiSidebarRuntime, sessionId: string): void {
    const normalizedSessionId = normalizeNonEmptyString(sessionId);
    if (!normalizedSessionId) {
      return;
    }
    this.lastAppShotTargetSessionId = normalizedSessionId;
    this.lastAppShotTargetAt = Date.now();
  },

  async handleGxserverRendererCommand(
    this: GpuiSidebarRuntime,
    command: GxserverRendererCommand
  ): Promise<Record<string, unknown>> {
    switch (command.action) {
      case 'focusSession': {
        const resolvedSession = this.resolveGxserverRendererCommandSession(command.payload);
        if (!resolvedSession) {
          throw new Error('No matching session was found.');
        }
        await this.focusSession(resolvedSession.sidebarSessionId, {
          sessionId: resolvedSession.sidebarSessionId,
          type: 'focusSession',
        });
        return {
          ok: true,
          session: {
            ghostexId: resolvedSession.sidebarSessionId,
            projectId: resolvedSession.projectId,
            sessionId: resolvedSession.sessionId,
          },
        };
      }
      case 'renameCommand': {
        const resolvedSession = this.resolveGxserverRendererCommandSession(command.payload);
        if (!resolvedSession) {
          throw new Error('No matching session was found.');
        }
        const title = normalizeGpuiRendererCommandRenameTitle(command.payload);
        if (!title) {
          throw new Error('Invalid renderer command title.');
        }
        this.postLocalWorkspaceTerminalRenameCommand(resolvedSession.projectId, resolvedSession.sessionId, title);
        return {
          accepted: true,
          action: 'renameCommand',
          ok: true,
          session: {
            ghostexId: resolvedSession.sidebarSessionId,
            projectId: resolvedSession.projectId,
            sessionId: resolvedSession.sessionId,
          },
        };
      }
      case 'runCommand':
        return this.runGxserverRendererCommandButton(readGpuiRecordString(command.payload, 'commandId'), command);
      case 'openBrowser':
      case 'openBrowserPane':
        return this.openEmbeddedBrowserFromRendererCommand(command);
      case 'clickButton': {
        const kind = readGpuiRecordString(command.payload, 'kind')?.trim();
        if (kind !== 'command') {
          throw new Error('Unsupported renderer command.');
        }
        return this.runGxserverRendererCommandButton(readGpuiRecordString(command.payload, 'id'), command);
      }
      default:
        throw new Error('Unsupported renderer command.');
    }
  },

  runGxserverRendererCommandButton(
    this: GpuiSidebarRuntime,
    rawCommandId: string | undefined,
    rendererCommand: GxserverRendererCommand
  ): Record<string, unknown> {
    /*
    CDXC:GxserverRendererCommands 2026-06-27-05:51:
    gxserver `runCommand` and `clickButton(kind:"command")` must launch the same trusted project Action button as native. Treat renderer payloads as selectors only; command text, URLs, close-on-exit normalization, completion-sound preference, cwd/env, paths, output, and logs must come from the live HUD command and fixed Rust command-action bridge.
    */
    const commandId = normalizeNonEmptyString(rawCommandId)?.trim();
    if (!commandId) {
      throw new Error('Unsupported renderer command.');
    }
    const command = this.resolveSidebarCommand(commandId);
    if (!command || !isSidebarCommandConfigured(command)) {
      throw new Error('Unsupported renderer command.');
    }
    const selectionMessage: Extract<SidebarToExtensionMessage, { type: 'runSidebarCommand' }> = {
      commandId,
      type: 'runSidebarCommand',
    };
    if (!this.postSidebarCommandAction(command, selectionMessage)) {
      throw new Error('Renderer command bridge unavailable.');
    }
    return {
      accepted: true,
      action: rendererCommand.action,
      ok: true,
    };
  },

  openEmbeddedBrowserFromRendererCommand(
    this: GpuiSidebarRuntime,
    command: GxserverRendererCommand
  ): Record<string, unknown> {
    /*
    macOS `openNativeBrowserPaneFromCli` parity for `ghostex browser open` /
    `gx ln`. Resolve CLI project selectors against the live sidebar project
    model, then forward only the validated project key; Rust re-normalizes the
    address and owns project-model swapping plus tab reuse/creation. An
    untargeted `--active-project` open keeps using the current Browser model.
    */
    const post = window.ghostexGpui?.postOpenBrowserUrl;
    if (typeof post !== 'function') {
      throw new Error('Renderer command bridge unavailable.');
    }
    const url = readGpuiRecordString(command.payload, 'url')?.trim() ?? '';
    if (url.length > GPUI_SIDEBAR_OPEN_BROWSER_URL_MAX_CHARS) {
      throw new Error('Invalid renderer command URL.');
    }
    const rawReuse = readGpuiRecordString(command.payload, 'reuse')?.trim().toLowerCase();
    const reuse = rawReuse === 'exact' || rawReuse === 'none' ? rawReuse : 'similar';
    const groupId = readGpuiRecordString(command.payload, 'groupId')?.trim();
    const requestedProjectId = readGpuiRecordString(command.payload, 'projectId')?.trim();
    const projectPath = readGpuiRecordString(command.payload, 'projectPath')?.trim();
    const projectId = this.resolveEmbeddedBrowserRendererCommandProjectId({
      groupId,
      projectId: requestedProjectId,
      projectPath,
    });
    if ((groupId || requestedProjectId || projectPath) && !projectId) {
      throw new Error('No matching project was found.');
    }
    const payload = JSON.stringify({
      ...(projectId ? { projectId } : {}),
      reuse,
      type: GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE,
      url,
      version: GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION,
    });
    if (!post(payload)) {
      throw new Error('Renderer command bridge unavailable.');
    }
    return {
      accepted: true,
      action: command.action,
      ok: true,
    };
  },

  resolveEmbeddedBrowserRendererCommandProjectId(
    this: GpuiSidebarRuntime,
    scope: {
      groupId?: string;
      projectId?: string;
      projectPath?: string;
    }
  ): string | undefined {
    if (scope.groupId) {
      const groupProjectId = this.resolveWorkspaceGroupProjectId(scope.groupId);
      return groupProjectId ? this.resolveEmbeddedBrowserKnownProjectId(groupProjectId) : undefined;
    }
    if (scope.projectId) {
      return this.resolveEmbeddedBrowserKnownProjectId(scope.projectId);
    }
    return this.resolveDomainProjectScope({ projectPath: scope.projectPath })?.projectId;
  },

  resolveEmbeddedBrowserKnownProjectId(this: GpuiSidebarRuntime, projectId: string): string | undefined {
    const remoteScope = this.resolveRemotePresentationProjectScope({ projectId });
    if (remoteScope) {
      return createGpuiRemotePresentationProjectId(remoteScope.machineId, remoteScope.projectId);
    }
    return this.domainProjectById(projectId)?.projectId;
  },

  resolveGxserverRendererCommandSession(
    this: GpuiSidebarRuntime,
    payload: Record<string, unknown>
  ): GpuiRendererCommandResolvedSession | undefined {
    /*
    CDXC:GxserverRendererCommands 2026-06-27-02:05:
    gxserver renderer commands can target local sessions with raw project/session ids in `sessionTarget`, while the reused GPUI SidebarApp renders combined `combined-session:<project>:<session>` ids. Resolve those raw ids to the same combined sidebar id before invoking runtime focus logic, and keep the command result bounded to ids/status rather than paths, titles, command text, URLs, tokens, terminal output, or renderer payload echoes.
    */
    const target = readGpuiRendererCommandSessionTarget(payload);
    const globalReference = parseGpuiRendererCommandGlobalSessionRef(
      readGpuiRecordString(target, 'globalRef') ?? readGpuiRecordString(payload, 'globalRef')
    );
    const projectId =
      readGpuiRecordString(target, 'projectId')?.trim() ||
      readGpuiRecordString(payload, 'projectId')?.trim() ||
      globalReference?.projectId;
    const sessionId =
      readGpuiRecordString(target, 'sessionId')?.trim() ||
      readGpuiRecordString(payload, 'sessionId')?.trim() ||
      globalReference?.sessionId;
    if (!sessionId) {
      return undefined;
    }
    const scopedSession = parseGxserverPresentationProjectSessionId(sessionId);
    if (scopedSession) {
      if (projectId && scopedSession.projectId !== projectId) {
        return undefined;
      }
      if (!this.hasGpuiRendererCommandLocalSession(scopedSession.projectId, scopedSession.sessionId)) {
        return undefined;
      }
      return {
        projectId: scopedSession.projectId,
        sessionId: scopedSession.sessionId,
        sidebarSessionId: sessionId,
      };
    }
    if (!projectId) {
      return undefined;
    }
    if (!this.hasGpuiRendererCommandLocalSession(projectId, sessionId)) {
      return undefined;
    }
    return {
      projectId,
      sessionId,
      sidebarSessionId: createGxserverPresentationProjectSessionId(projectId, sessionId),
    };
  },

  hasGpuiRendererCommandLocalSession(this: GpuiSidebarRuntime, projectId: string, sessionId: string): boolean {
    if (
      this.presentation?.sessions.some((session) => session.projectId === projectId && session.sessionId === sessionId)
    ) {
      return true;
    }
    return this.latestGroups.some((group) =>
      group.sessions.some((session) => {
        const reference = parseGxserverPresentationProjectSessionId(session.sessionId);
        return reference?.projectId === projectId && reference.sessionId === sessionId;
      })
    );
  },

  /*
  CDXC:NavigationHistory 2026-08-19:
  Trail stops are recorded from the SAME projection the titlebar label reads,
  so "where the user is" can never disagree between the label and Back. A stop
  needs a real project: Quick/projectless and the synthetic Chats collection
  publish nothing rather than pushing a stop that cannot be returned to.
  */
  createNavigationHistoryEntry(this: GpuiSidebarRuntime): NavigationHistoryEntry | undefined {
    const activeGroup = this.activeProjectContextGroups().find((group) => group.isActive);
    if (!activeGroup) {
      return undefined;
    }
    const projectId =
      activeGroup.projectContext?.editor.projectId ??
      parseGpuiWorkspaceSessionSubgroupId(activeGroup.groupId)?.projectId;
    if (!projectId) {
      return undefined;
    }
    const focusedSession = activeGroup.sessions.find((session) => session.isFocused);
    const sessionLabel = focusedSession
      ? (focusedSession.displayTitle ?? focusedSession.primaryTitle ?? focusedSession.alias)
      : undefined;
    return {
      groupId: activeGroup.groupId,
      projectId,
      ...(activeGroup.title ? { projectLabel: activeGroup.title } : {}),
      ...(focusedSession ? { sessionId: focusedSession.sessionId } : {}),
      ...(sessionLabel ? { sessionLabel } : {}),
    };
  },

  navigationHistoryRpc(this: GpuiSidebarRuntime): NavigationHistoryRpc | undefined {
    const client = this.client;
    if (!client) {
      return undefined;
    }
    return (path, params) => client.rpc<unknown>(path, params);
  },

  /**
   * Focus a trail stop, or report it as gone so the daemon drops it and Back
   * keeps walking. Sessions win over their project: the stop recorded a session
   * because that is what the user was looking at.
   */
  activateNavigationHistoryEntry(this: GpuiSidebarRuntime, entry: NavigationHistoryEntry): boolean {
    if (entry.sessionId) {
      const exists = this.latestGroups.some((group) =>
        group.sessions.some((session) => session.sessionId === entry.sessionId)
      );
      if (!exists) {
        return false;
      }
      void this.focusSession(entry.sessionId, {
        sessionId: entry.sessionId,
        type: 'focusSession',
      });
      return true;
    }
    const groupId = entry.groupId;
    if (!groupId || !this.latestGroups.some((group) => group.groupId === groupId)) {
      return false;
    }
    this.focusGroup(groupId, { groupId, type: 'focusGroup' });
    return true;
  },

  /**
   * The native titlebar renders the two buttons from this cached state; it must
   * never issue an RPC of its own on a render pass. Deduplicated so a publish
   * storm cannot turn into a bridge-message storm.
   */
  postNavigationHistoryState(
    this: GpuiSidebarRuntime,
    state: {
      canGoBack: boolean;
      canGoForward: boolean;
    }
  ): void {
    /*
    CDXC:NavigationHistory 2026-08-19:
    Availability only. The native arrows have no hover tooltip, so sending the
    destination labels would wake the bridge — and a titlebar repaint check —
    every time a back/forward target's title changed, for pixels that cannot
    move.
    */
    const message = {
      canGoBack: state.canGoBack,
      canGoForward: state.canGoForward,
      type: 'navigationHistoryState',
    };
    const payload = JSON.stringify(message);
    if (payload === this.lastNavigationHistoryStatePayload) {
      return;
    }
    this.lastNavigationHistoryStatePayload = payload;
    window.webkit?.messageHandlers?.ghostexNativeHost?.postMessage(message);
  },

  postAppShotToast(
    this: GpuiSidebarRuntime,
    level: AppToastLevel,
    title: string,
    options: {
      description?: string;
    } = {}
  ): void {
    try {
      postAppModalHostMessage(createAppToastRequest(level, title, options.description), 'AppModals:gpuiAppShotToast');
    } catch {
      /*
      CDXC:GPUIAppShots 2026-06-25-23:07:
      App Shots user feedback must not depend on toast-host availability and must not log raw app names, window titles, image paths, project paths, command text, terminal content, URLs, or tokens when presentation is unavailable.
      */
    }
  },

  postSidebarActionToast(
    this: GpuiSidebarRuntime,
    level: AppToastLevel,
    title: string,
    options: { description?: string } = {}
  ): void {
    try {
      postAppModalHostMessage(createAppToastRequest(level, title, options.description), 'GPUISidebarActions:toast');
    } catch {
      // Toast-host availability must never gate the underlying action.
    }
  },

  postProjectPathActionForGroup(
    this: GpuiSidebarRuntime,
    action: Extract<
      GpuiSidebarNativeProjectPathAction,
      'copyWorkspaceProjectPath' | 'openWorkspaceProjectInFinder' | 'openWorkspaceProjectInIde'
    >,
    groupId: string,
    originalMessage: SidebarToExtensionMessage
  ): void {
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      if (action === 'copyWorkspaceProjectPath') {
        this.postRemoteProjectNativeAction('copyRemoteProjectPath', remoteGroup, originalMessage);
        return;
      }
      if (action === 'openWorkspaceProjectInIde') {
        this.postRemoteProjectNativeAction('openRemoteWorkspaceProjectInIde', remoteGroup, originalMessage);
        return;
      }
      this.postRemoteToast('warning', 'Remote project open unavailable', {
        description: 'GPUI does not open remote project paths in local Finder.',
      });
      return;
    }
    const projectId = this.resolveProjectIdForGroup(groupId);
    if (!projectId) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return;
    }
    this.postNativeProjectPathAction(action, projectId, originalMessage);
  },

  postActiveProjectPathAction(
    this: GpuiSidebarRuntime,
    action: Extract<
      GpuiSidebarNativeProjectPathAction,
      'openActiveWorkspaceProjectInFinder' | 'openActiveWorkspaceProjectInVscode' | 'openActiveWorkspaceProjectInZed'
    >,
    originalMessage: SidebarToExtensionMessage
  ): void {
    const remoteGroup = this.activeGroupId ? parseGpuiRemotePresentationGroupId(this.activeGroupId) : undefined;
    if (remoteGroup) {
      if (action === 'openActiveWorkspaceProjectInVscode') {
        this.postRemoteProjectNativeAction('openRemoteWorkspaceProjectInVscode', remoteGroup, originalMessage);
        return;
      }
      if (action === 'openActiveWorkspaceProjectInZed') {
        this.postRemoteProjectNativeAction('openRemoteWorkspaceProjectInZed', remoteGroup, originalMessage);
        return;
      }
      this.postRemoteToast('warning', 'Remote project open unavailable', {
        description:
          action === 'openActiveWorkspaceProjectInFinder'
            ? 'GPUI does not open remote project paths in local Finder.'
            : 'That editor is not supported for GPUI remote project opens.',
      });
      return;
    }
    const projectId = this.activeProjectId;
    if (!projectId || !this.domainProjectById(projectId)) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return;
    }
    this.postNativeProjectPathAction(action, projectId, originalMessage);
  },

  postNativeProjectPathAction(
    this: GpuiSidebarRuntime,
    action: GpuiSidebarNativeProjectPathAction,
    projectId: string,
    originalMessage: SidebarToExtensionMessage,
    options: { filePath?: string; preferredInterface?: PreferredAgentInterface } = {}
  ): boolean {
    const normalizedProjectId = projectId.trim();
    if (!normalizedProjectId) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
    const bridge = window.ghostexGpui?.postNativeProjectPathAction;
    if (!bridge) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
    const payload = JSON.stringify({
      action,
      ...(options.filePath ? { filePath: options.filePath } : {}),
      ...(options.preferredInterface ? { preferredInterface: options.preferredInterface } : {}),
      projectId: normalizedProjectId,
      type: GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
    });
    try {
      if (!bridge(payload)) {
        this.handleUnsupportedSidebarMessage(originalMessage);
        return false;
      }
      return true;
    } catch {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
  },

  postSidebarCommandAction(
    this: GpuiSidebarRuntime,
    command: SidebarCommandButton,
    selectionMessage: Extract<SidebarToExtensionMessage, { type: 'runSidebarCommand' }>
  ): boolean {
    const bridge = window.ghostexGpui?.postSidebarCommandAction;
    if (!bridge) {
      this.handleUnsupportedSidebarMessage(selectionMessage);
      return false;
    }
    const payload = JSON.stringify({
      actionType: command.actionType,
      commandId: command.commandId,
      name: command.name,
      /*
      CDXC:GPUICommandPane 2026-06-27-07:54:
      `runSidebarCommand` reaches the launch bridge only after GPUI rebuilds it as a selector-shaped object. Forward an own, validated runMode only for terminal Actions so Rust can create the visible debug workspace terminal like macOS while all other launch metadata stays resolved from the trusted HUD command.
      */
      ...(command.actionType === 'terminal' &&
      selectionMessage.runMode &&
      isSidebarCommandRunMode(selectionMessage.runMode)
        ? { runMode: selectionMessage.runMode }
        : {}),
      ...(command.actionType === 'terminal'
        ? {
            /*
            CDXC:GPUICommandPane 2026-06-27-07:54:
            GPUI command-pane Action launches must match native `runNativeSidebarCommand`: default command-pane runtime forces terminal close-on-exit off even when trusted saved/HUD Action definitions preserve older close-on-exit metadata. Renderer `runSidebarCommand` messages cannot supply this field, and Browser Actions must continue omitting the terminal-only boolean.
            */
            closeTerminalOnExit: false,
            playCompletionSound: command.playCompletionSound,
          }
        : {}),
      ...(command.actionType === 'terminal' && command.command ? { command: command.command } : {}),
      ...(command.actionType === 'terminal' && command.links && command.links.length > 0
        ? { links: command.links.map((link) => ({ target: link.target, url: link.url })) }
        : {}),
      ...(command.actionType === 'browser' && command.url ? { url: command.url } : {}),
      type: GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_VERSION,
    });
    try {
      if (!bridge(payload)) {
        this.handleUnsupportedSidebarMessage(selectionMessage);
        return false;
      }
      return true;
    } catch {
      this.handleUnsupportedSidebarMessage(selectionMessage);
      return false;
    }
  },

  postGhostexHotkeyAction(
    this: GpuiSidebarRuntime,
    originalMessage: Extract<SidebarToExtensionMessage, { type: 'runGhostexHotkeyAction' }>
  ): boolean {
    const bridge = window.ghostexGpui?.postGhostexHotkeyAction;
    if (!bridge) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
    /*
    CDXC:GPUICommandPalette 2026-06-27-08:11:
    Shared SidebarApp and Command Palette hotkey rows emit `runGhostexHotkeyAction` through the reused GPUI runtime, not directly to Rust. Forward only the fixed action-id selector so Open Commands Panel, focused-pane routes, Settings, and modal hotkeys share Rust's native dispatcher without renderer-owned session ids, paths, command text, URLs, or launch metadata.
    */
    if (
      Object.keys(originalMessage).some((key) => key !== 'type' && key !== 'actionId') ||
      typeof originalMessage.actionId !== 'string' ||
      originalMessage.actionId.trim() === ''
    ) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
    const payload = JSON.stringify({
      actionId: originalMessage.actionId,
      type: 'runGhostexHotkeyAction',
    });
    try {
      if (!bridge(payload)) {
        this.handleUnsupportedSidebarMessage(originalMessage);
        return false;
      }
      return true;
    } catch {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
  },

  postSidebarCommandRunEnd(
    this: GpuiSidebarRuntime,
    commandId: string,
    originalMessage: SidebarToExtensionMessage
  ): boolean {
    const bridge = window.ghostexGpui?.postSidebarCommandRunEnd;
    if (!bridge) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
    const normalizedCommandId = commandId.trim();
    if (!normalizedCommandId) {
      return false;
    }
    const payload = JSON.stringify({
      commandId: normalizedCommandId,
      /*
      CDXC:GPUICommandPane 2026-06-27-05:59:
      `endSidebarCommandRun` is a separate fixed GPUI bridge from Action launch because Rust only needs the selected command id to close the mapped command-pane run. Rebuild the payload here so renderer command text, URLs, close-on-exit flags, cwd/env, paths, logs, output, status-file paths, and run ids never cross the run-end bridge.
      */
      type: GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_VERSION,
    });
    try {
      if (!bridge(payload)) {
        this.handleUnsupportedSidebarMessage(originalMessage);
        return false;
      }
      return true;
    } catch {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return false;
    }
  },

  saveSidebarSettingsPatch(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'updateSettingsPatch' }>
  ): void {
    /*
    CDXC:SidebarV2 2026-07-29:
    Sidebar-origin settings writes (sidebar version, Group by Project, remote
    machine ordering) are real Settings saves, so they take the same route the
    Settings modal uses: the app-modal host bridge installed on the GPUI sidebar
    surface, where Rust merges the patch onto the stored snapshot and hydrates
    every surface back. Do not persist settings inside this adapter.
    */
    try {
      postAppModalHostMessage({ message, type: 'sidebarCommand' }, 'GPUISidebarActions:updateSettingsPatch');
    } catch {
      this.handleUnsupportedSidebarMessage(message);
    }
  },

  openExternalUrl(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'openExternalUrl' }>
  ): void {
    /*
    CDXC:SidebarDiscord 2026-08-07:
    The shared sidebar's external links must enter the same native command
    route as Settings and first-launch links. The GPUI sidebar adapter used to
    drop openExternalUrl as unsupported after the React click had already
    closed the menu, so Join Discord appeared inert. Forward the typed command
    through the existing app-modal host bridge; Rust remains responsible for
    validating and opening the http/https URL.
    */
    try {
      postAppModalHostMessage({ message, type: 'sidebarCommand' }, 'GPUISidebarActions:openExternalUrl');
    } catch {
      this.handleUnsupportedSidebarMessage(message);
    }
  },

  openAppModal(this: GpuiSidebarRuntime, modal: 'firstLaunchSetup' | 'settings' | 'watchGhostexVideo'): void {
    /*
    CDXC:GPUISidebarAppModalBridge 2026-06-24-11:40:
    Sidebar-origin Settings, first-launch welcome, and tutorial-video requests in GPUI must use the shared app-modal host bridge installed by the CEF sidebar surface. Do not fork Settings React UI, duplicate modal state, or route these first-party modals through fixture/sidebar-only alternate paths.
    */
    try {
      openAppModal({ modal, type: 'open' });
    } catch {
      this.handleUnsupportedSidebarMessage({ type: 'openSettings' });
    }
  },

  async saveScratchPad(this: GpuiSidebarRuntime, content: string): Promise<void> {
    const client = this.client;
    if (!client) {
      return;
    }
    this.appUserData = await client.saveScratchPad(content);
    this.publishAppUserDataHydrate();
  },

  async savePinnedPrompt(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'savePinnedPrompt' }>
  ): Promise<void> {
    const client = this.client;
    if (!client) {
      return;
    }
    this.appUserData = await client.savePinnedPrompt({
      content: message.content,
      promptId: message.promptId,
      title: message.title,
    });
    this.publishAppUserDataHydrate();
  },

  publishAppUserDataHydrate(this: GpuiSidebarRuntime): void {
    if (!this.hasHydrated) {
      return;
    }
    this.messageSource.postMessage(this.createHydrateMessage(this.latestGroups, this.latestHud));
  },
};

const gpuiSidebarRuntimeAppShotAndMiscMethodsShapeCheck: GpuiSidebarRuntimeAppShotAndMiscMethods =
  gpuiSidebarRuntimeAppShotAndMiscMethods;
void gpuiSidebarRuntimeAppShotAndMiscMethodsShapeCheck;
