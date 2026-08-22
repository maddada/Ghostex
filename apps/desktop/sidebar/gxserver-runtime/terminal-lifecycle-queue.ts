/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_TYPE,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_VERSION,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_TYPE,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_VERSION,
  GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_SETTLE_MS,
  GPUI_WORKSPACE_TERMINAL_LIFECYCLE_BRIDGE_RETRY_DELAY_MS,
} from "./constants";
import type { GpuiSidebarRuntime } from "./core";
import { createGpuiSidebarSettings } from "./helpers/bootstrap";
import { normalizeGpuiWorkspaceTabSessionSelection } from "./helpers/command-palette";
import { normalizeNonEmptyString } from "./helpers/records";
import {
  createGpuiRemotePresentationSessionId,
  parseGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationSessionId,
} from "./helpers/remote-presentation";
import {
  gpuiWorkspaceTerminalTitleCommandForAgent,
  normalizeGpuiWorkspaceFirstPromptTitleGenerationCancel,
  normalizeGpuiWorkspaceTerminalBell,
  normalizeGpuiWorkspaceTerminalLifecycleRequest,
  normalizeGpuiWorkspaceTerminalRuntimeAction,
  normalizeGpuiWorkspaceTerminalTitleChanged,
  normalizeQueuedGpuiWorkspaceTerminalLifecycleRequest,
  shouldApplyGpuiLocalWorkspaceTransition,
} from "./helpers/terminal-lifecycle";
import type {
  GpuiRemoteProjectReference,
  GpuiWorkspaceTerminalLifecycleRequest,
  GpuiWorkspaceTerminalTitleChangedPayload,
} from "./types-and-protocol";
import {
  createGxserverPresentationProjectSessionId,
} from "@/packages/shared/gxserver-presentation-sidebar-projection";
import type {
  GxserverSessionTransitionResult,
  GxserverTerminalTitleEventResult,
} from "@/packages/shared/gxserver-protocol";
import { getVisibleTerminalTitle } from "@/packages/shared/session-grid-contract";

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeTerminalLifecycleMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeTerminalLifecycleMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeTerminalLifecycleMethods {
  handleGpuiWorkspaceTabSessionSelected(payload: unknown): void;
  handleGpuiWorkspaceFirstPromptTitleGenerationCancel(payload: unknown): Promise<void>;
  clearLocalPresentationSessionFirstPromptTitleGeneration(projectId: string, sessionId: string): boolean;
  handleGpuiWorkspaceTerminalBell(payload: unknown): Promise<void>;
  handleGpuiWorkspaceTerminalTitleChanged(payload: unknown): void;
  ingestGpuiWorkspaceTerminalTitle(observation: GpuiWorkspaceTerminalTitleChangedPayload): Promise<void>;
  handleGpuiWorkspaceTerminalRuntimeAction(payload: unknown): Promise<void>;
  postLocalWorkspaceTerminalRenameCommand(projectId: string, sessionId: string, title: string): void;
  transitionWorkspaceTerminalLifecycleClose(request: GpuiWorkspaceTerminalLifecycleRequest, fallbackReplacementSessionId: string | undefined): boolean;
  workspaceTerminalLifecycleResultBridgeReady(): boolean;
  handleOrQueueWorkspaceTerminalLifecycleRequest(payload: unknown): void;
  queuePendingWorkspaceTerminalLifecycleRequest(request: GpuiWorkspaceTerminalLifecycleRequest): void;
  scheduleWorkspaceTerminalLifecycleBridgeRetry(): void;
  drainPendingWorkspaceTerminalLifecycleRequests(queuedRequests?: readonly unknown[]): void;
  handleNormalizedWorkspaceTerminalLifecycleRequest(request: GpuiWorkspaceTerminalLifecycleRequest): Promise<void>;
  applyWorkspaceTerminalLifecycleRequest(request: GpuiWorkspaceTerminalLifecycleRequest): Promise<boolean>;
  applyRemoteWorkspaceTerminalLifecycleRequest(request: GpuiWorkspaceTerminalLifecycleRequest, remoteProject: GpuiRemoteProjectReference): Promise<boolean>;
  postWorkspaceTerminalLifecycleResult(requestId: number, ok: boolean): void;
}

export const gpuiSidebarRuntimeTerminalLifecycleMethods = {

  handleGpuiWorkspaceTabSessionSelected(this: GpuiSidebarRuntime, payload: unknown): void {
    const selection = normalizeGpuiWorkspaceTabSessionSelection(payload);
    if (!selection) {
      return;
    }
    const remoteSession = parseGpuiRemotePresentationSessionId(selection.sessionId);
    const remoteProject = parseGpuiRemotePresentationProjectId(selection.projectId);
    if (remoteSession || remoteProject) {
      if (
        !remoteSession ||
        !remoteProject ||
        remoteSession.machineId !== remoteProject.machineId ||
        remoteSession.projectId !== remoteProject.projectId
      ) {
        return;
      }
      this.setRemotePresentationSessionFocus(remoteSession);
      this.publishRemotePresentationPatch();
      return;
    }
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-08:01:
    A GPUI workspace tab click has already selected the native tab in Rust. Match macOS `paneTabSelected` by updating the sidebar's local or machine-scoped remote presentation focus and publishing only the corresponding sidebar patch; do not post `workspaceTerminalFocus` back to Rust or call gxserver `/api/focusSession`.

    CDXC:GPUIWorkspaceSessionFocus 2026-06-27-00:33:
    MacOS reconciles stale native sleeping pane tabs when gxserver presentation already reports the canonical P/G session running. Preserve the one-way tab-selection path for ordinary clicks, but if Rust marks the selected mapped tab as locally sleeping and the current presentation row is running, post one bounded WorkspaceTerminalFocus so Rust reuses and attaches that existing tab instead of leaving an inert sleeping placeholder.

    CDXC:GPUIWorkspaceSessionFocus 2026-07-11:
    A restored-after-restart Running tab can carry no local terminal runtime at all (no live owner, parked owner, or pending attach payload); Rust reports that as `localRuntimeMissing`. Reconcile it exactly like the stale sleeping case: when gxserver presentation still reports the canonical P/G session running, post one bounded WorkspaceTerminalFocus so Rust materializes the tab through the ordinary gxserver attach pipeline instead of leaving an empty body behind the selected tab.
    */
    const shouldReconcileRunningPresentation =
      (selection.localWasSleeping === true || selection.localRuntimeMissing === true) &&
      this.presentation?.sessions.some(
        (session) =>
          session.projectId === selection.projectId &&
          session.sessionId === selection.sessionId &&
          session.lifecycleState === "running",
      ) === true;
    this.setLocalPresentationSessionFocus(
      selection.projectId,
      selection.sessionId,
      undefined,
      selection.visibleSessionIds,
    );
    if (shouldReconcileRunningPresentation) {
      this.postLocalWorkspaceTerminalFocus(selection.projectId, selection.sessionId);
    }
    this.publishPresentation("patch");
  },

  async handleGpuiWorkspaceFirstPromptTitleGenerationCancel(this: GpuiSidebarRuntime,
    payload: unknown,
  ): Promise<void> {
    /*
    CDXC:GPUISessionTitleOverlay 2026-07-26:
    Escape inside the blocking "Generating title" pane overlay cancels the
    gxserver-owned first-prompt title job, matching the managed macOS pane.
    Rust only reports the suppressed-pane Escape; the sidebar runtime owns the
    decision and the gxserver call. Clear the local presentation flag first so
    the overlay and terminal input suppression lift immediately instead of
    waiting for the next gxserver delta.
    */
    const cancel = normalizeGpuiWorkspaceFirstPromptTitleGenerationCancel(payload);
    if (!cancel || !this.client) {
      return;
    }
    const session = this.findLocalPresentationSession(cancel.projectId, cancel.sessionId);
    if (session?.isGeneratingFirstPromptTitle !== true) {
      return;
    }
    if (
      this.clearLocalPresentationSessionFirstPromptTitleGeneration(
        cancel.projectId,
        cancel.sessionId,
      )
    ) {
      this.publishPresentation("patch");
    }
    try {
      await this.client.rpc("/api/cancelFirstPromptAutoTitle", {
        projectId: cancel.projectId,
        reason: "escape",
        sessionId: cancel.sessionId,
      });
    } catch {
      /*
      gxserver owns the title job, so a rejected cancel is recovered by the
      next presentation delta: it republishes the generating flag and the
      overlay comes back if the job is still alive.
      */
    }
  },

  clearLocalPresentationSessionFirstPromptTitleGeneration(this: GpuiSidebarRuntime,
    projectId: string,
    sessionId: string,
  ): boolean {
    const presentation = this.presentation;
    if (!presentation) {
      return false;
    }
    let didChange = false;
    const sessions = presentation.sessions.map((session) => {
      if (
        session.projectId !== projectId ||
        session.sessionId !== sessionId ||
        session.isGeneratingFirstPromptTitle !== true
      ) {
        return session;
      }
      didChange = true;
      return {
        ...session,
        isGeneratingFirstPromptTitle: false,
      };
    });
    if (!didChange) {
      return false;
    }
    this.presentation = {
      ...presentation,
      sessions,
    };
    return true;
  },

  async handleGpuiWorkspaceTerminalBell(this: GpuiSidebarRuntime, payload: unknown): Promise<void> {
    /*
    Shells use BEL for routine feedback such as zsh Tab-completion misses, so
    the bell only becomes gxserver attention when the user opts in from
    Terminal settings — the same gate macOS applies to its terminalBell host
    event. Agent completion keeps its separate explicit attention path.
    */
    const bell = normalizeGpuiWorkspaceTerminalBell(payload);
    if (!bell || !this.client) {
      return;
    }
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    if (!settings.showNotificationOnTerminalBell) {
      return;
    }
    const agentName = normalizeNonEmptyString(
      this.presentation?.sessions.find(
        (session) => session.projectId === bell.projectId && session.sessionId === bell.sessionId,
      )?.agentName,
    );
    try {
      await this.client.rpc("/api/updateAgentActivity", {
        ...(agentName ? { agentName } : {}),
        event: "bell",
        projectId: bell.projectId,
        sessionId: bell.sessionId,
      });
    } catch {
      // gxserver attention sync is best-effort, matching macOS's log-only failure path.
    }
  },

  handleGpuiWorkspaceTerminalTitleChanged(this: GpuiSidebarRuntime, payload: unknown): void {
    const observation = normalizeGpuiWorkspaceTerminalTitleChanged(payload);
    if (!observation) {
      return;
    }
    const key = createGxserverPresentationProjectSessionId(
      observation.projectId,
      observation.sessionId,
    );
    this.workspaceTerminalTitleObservations.set(key, observation);
    const previousTimeout = this.workspaceTerminalTitleSettleTimeouts.get(key);
    if (previousTimeout !== undefined) {
      window.clearTimeout(previousTimeout);
    }
    const timeout = window.setTimeout(() => {
      this.workspaceTerminalTitleSettleTimeouts.delete(key);
      const settled = this.workspaceTerminalTitleObservations.get(key);
      this.workspaceTerminalTitleObservations.delete(key);
      if (settled) {
        void this.ingestGpuiWorkspaceTerminalTitle(settled);
      }
    }, GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_SETTLE_MS);
    this.workspaceTerminalTitleSettleTimeouts.set(key, timeout);
  },

  async ingestGpuiWorkspaceTerminalTitle(this: GpuiSidebarRuntime,
    observation: GpuiWorkspaceTerminalTitleChangedPayload,
  ): Promise<void> {
    if (!this.client) {
      return;
    }
    const visibleTitle = getVisibleTerminalTitle(observation.rawTitle)?.trim();
    if (!visibleTitle) {
      return;
    }
    const session = this.presentation?.sessions.find(
      (candidate) =>
        candidate.projectId === observation.projectId &&
        candidate.sessionId === observation.sessionId,
    );
    if (!session || (session.kind !== "terminal" && session.kind !== "agent")) {
      return;
    }
    const storedVisibleTitle = getVisibleTerminalTitle(session.terminalTitle)?.trim();
    if (
      storedVisibleTitle &&
      storedVisibleTitle.replace(/\s+/gu, " ") === visibleTitle.replace(/\s+/gu, " ")
    ) {
      return;
    }
    try {
      await this.client.rpc<GxserverTerminalTitleEventResult>("/api/ingestTerminalTitleEvent", {
        ...(session.agentName || session.agentId
          ? { agentName: session.agentName ?? session.agentId }
          : {}),
        projectId: observation.projectId,
        rawTitle: observation.rawTitle,
        sessionId: observation.sessionId,
        ...(session.sessionPersistenceProvider
          ? { sessionPersistenceProvider: session.sessionPersistenceProvider }
          : {}),
      });
    } catch {
      // A later terminal observation or presentation recovery retries without inventing local state.
    }
  },

  async handleGpuiWorkspaceTerminalRuntimeAction(this: GpuiSidebarRuntime, payload: unknown): Promise<void> {
    /*
    Rust-origin Fork/Reload for focused Agents terminals reuse the exact card
    action paths so gxserver ownership, focus follow-up, and reload semantics
    stay identical to sidebar-driven Fork/Full Reload.
    */
    const request = normalizeGpuiWorkspaceTerminalRuntimeAction(payload);
    if (!request) {
      return;
    }
    if (request.action === "sleepInactiveSessions") {
      await this.sleepInactiveSessionsFromTitlebar();
      return;
    }
    if (request.action === "sleepAllDaemonSessions") {
      await this.sleepAllLocalDaemonSessions();
      return;
    }
    const sessionId = createGxserverPresentationProjectSessionId(
      request.projectId,
      request.sessionId,
    );
    if (request.action === "forkSession") {
      await this.forkSession(sessionId);
      return;
    }
    if (request.action === "exportTranscript") {
      await this.exportSessionTranscript(sessionId);
      return;
    }
    await this.fullReloadSession(sessionId);
  },

  postLocalWorkspaceTerminalRenameCommand(this: GpuiSidebarRuntime,
    projectId: string,
    sessionId: string,
    title: string,
  ): void {
    /*
    CDXC:GxserverRendererCommands 2026-06-27-02:27:
    GPUI `renameCommand` is accepted when TypeScript resolves gxserver's raw sessionTarget to the local workspace session and posts one fixed fire-and-forget Rust bridge payload. Keep the result and errors id-only, and pass the normalized title only through `postWorkspaceTerminalRenameCommand` so logs/results do not expose user title text, command text, paths, URLs, tokens, or terminal output.

    CDXC:GPUISidebarRename 2026-07-28:
    Pi names its session with `/name <title>` and Hermes Agent uses
    `/title <title>` instead of `/rename <title>`, so the payload carries a
    fixed command selector resolved from the session's own agent identity.
    Rust still owns turning that selector into the actual terminal input.
    */
    const postRename = window.ghostexGpui?.postWorkspaceTerminalRenameCommand;
    if (typeof postRename !== "function") {
      throw new Error("Renderer command bridge unavailable.");
    }
    /*
    CDXC:GPUISidebarRename 2026-07-29:
    Rust may only type the rename command into a mounted Ghostty surface, and
    it accepts a sidebar-focus attach for this session only while gxserver
    presentation focus agrees. Activate the session exactly like a session-card
    click first so a rename of a background session mounts its terminal
    instead of being dropped at the surface-ownership check.
    */
    this.focusLocalWorkspaceSession(projectId, sessionId);
    this.publishPresentation("patch");
    const session = this.findLocalPresentationSession(projectId, sessionId);
    const agent = (session?.agentId ?? session?.agentName ?? "").trim().toLowerCase();
    const bridgeSent = postRename(
      JSON.stringify({
        version: GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_VERSION,
        type: GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_TYPE,
        projectId,
        sessionId,
        title,
        command: gpuiWorkspaceTerminalTitleCommandForAgent(agent),
      }),
    );
    if (!bridgeSent) {
      throw new Error("Renderer command bridge unavailable.");
    }
  },

  transitionWorkspaceTerminalLifecycleClose(this: GpuiSidebarRuntime,
    request: GpuiWorkspaceTerminalLifecycleRequest,
    fallbackReplacementSessionId: string | undefined,
  ): boolean {
    /*
    CDXC:GPUIWorkspaceLifecycle 2026-06-26-23:59:
    Rust-origin mapped Agents close matches macOS local-first behavior: hide/remove the SidebarApp row and focus the Rust-provided or project-list replacement locally, then attempt gxserver `/api/transitionSession` best-effort. Provider transition failure must not keep a retryable Ghostty close-confirm prompt or block the native tab close.
    */
    this.removePresentationSession(request.projectId, request.sessionId);
    const replacementProjectId = request.replacementProjectId ?? request.projectId;
    const replacementSessionId = request.replacementSessionId ?? fallbackReplacementSessionId;
    if (replacementSessionId) {
      this.focusLocalWorkspaceSession(replacementProjectId, replacementSessionId);
      this.publishPresentation("patch");
    }
    const client = this.client;
    if (client) {
      void client
        .rpc<GxserverSessionTransitionResult>("/api/transitionSession", {
          action: request.action,
          projectId: request.projectId,
          reason: "closeTerminal",
          sessionId: request.sessionId,
        })
        .catch(() => undefined);
    }
    return true;
  },

  workspaceTerminalLifecycleResultBridgeReady(this: GpuiSidebarRuntime): boolean {
    return typeof window.ghostexGpui?.postWorkspaceTerminalLifecycleResult === "function";
  },

  handleOrQueueWorkspaceTerminalLifecycleRequest(this: GpuiSidebarRuntime, payload: unknown): void {
    const request = normalizeGpuiWorkspaceTerminalLifecycleRequest(payload);
    if (!request) {
      return;
    }
    if (!this.workspaceTerminalLifecycleResultBridgeReady()) {
      this.queuePendingWorkspaceTerminalLifecycleRequest(request);
      return;
    }
    void this.handleNormalizedWorkspaceTerminalLifecycleRequest(request);
  },

  queuePendingWorkspaceTerminalLifecycleRequest(this: GpuiSidebarRuntime,
    request: GpuiWorkspaceTerminalLifecycleRequest,
  ): void {
    const gpuiBridge = (window.ghostexGpui = window.ghostexGpui ?? {});
    const pending = Array.isArray(gpuiBridge.pendingWorkspaceTerminalLifecycleRequests)
      ? gpuiBridge.pendingWorkspaceTerminalLifecycleRequests
      : [];
    pending.push(request);
    gpuiBridge.pendingWorkspaceTerminalLifecycleRequests = pending;
    this.scheduleWorkspaceTerminalLifecycleBridgeRetry();
  },

  scheduleWorkspaceTerminalLifecycleBridgeRetry(this: GpuiSidebarRuntime): void {
    if (this.workspaceTerminalLifecycleBridgeRetryId !== undefined) {
      return;
    }
    this.workspaceTerminalLifecycleBridgeRetryId = window.setTimeout(() => {
      this.workspaceTerminalLifecycleBridgeRetryId = undefined;
      this.drainPendingWorkspaceTerminalLifecycleRequests();
    }, GPUI_WORKSPACE_TERMINAL_LIFECYCLE_BRIDGE_RETRY_DELAY_MS);
  },

  drainPendingWorkspaceTerminalLifecycleRequests(this: GpuiSidebarRuntime,
    queuedRequests?: readonly unknown[],
  ): void {
    const gpuiBridge = (window.ghostexGpui = window.ghostexGpui ?? {});
    const pending = [
      ...(queuedRequests ?? []),
      ...(Array.isArray(gpuiBridge.pendingWorkspaceTerminalLifecycleRequests)
        ? gpuiBridge.pendingWorkspaceTerminalLifecycleRequests.splice(0)
        : []),
    ];
    if (pending.length === 0) {
      return;
    }
    if (!this.workspaceTerminalLifecycleResultBridgeReady()) {
      for (const payload of pending) {
        const request = normalizeQueuedGpuiWorkspaceTerminalLifecycleRequest(payload);
        if (request) {
          this.queuePendingWorkspaceTerminalLifecycleRequest(request);
        }
      }
      return;
    }
    for (const payload of pending) {
      const request = normalizeQueuedGpuiWorkspaceTerminalLifecycleRequest(payload);
      if (request) {
        void this.handleNormalizedWorkspaceTerminalLifecycleRequest(request);
      }
    }
  },

  async handleNormalizedWorkspaceTerminalLifecycleRequest(this: GpuiSidebarRuntime,
    request: GpuiWorkspaceTerminalLifecycleRequest,
  ): Promise<void> {
    let ok = false;
    try {
      ok = await this.applyWorkspaceTerminalLifecycleRequest(request);
    } catch {
      ok = false;
    }
    this.postWorkspaceTerminalLifecycleResult(request.requestId, ok);
  },

  async applyWorkspaceTerminalLifecycleRequest(this: GpuiSidebarRuntime,
    request: GpuiWorkspaceTerminalLifecycleRequest,
  ): Promise<boolean> {
    const remoteProject = parseGpuiRemotePresentationProjectId(request.projectId);
    if (remoteProject) {
      return this.applyRemoteWorkspaceTerminalLifecycleRequest(request, remoteProject);
    }
    const fallbackReplacementSessionId =
      request.replacementSessionId === undefined && !request.skipReplacementFallback
        ? this.resolveLocalProjectListTransitionFocusTarget(request.projectId, request.sessionId)
        : undefined;
    if (request.action === "close") {
      /*
      CDXC:GPUIWorkspaceLifecycle 2026-07-10:
      Close is local-first and must hide the sidebar row even when gxserver is
      disconnected. The provider transition is best-effort cleanup owned by
      transitionWorkspaceTerminalLifecycleClose; unlike Sleep and Wake, it is
      not a prerequisite for acknowledging the user's tab close.
      */
      return this.transitionWorkspaceTerminalLifecycleClose(request, fallbackReplacementSessionId);
    }
    if (!this.client) {
      return false;
    }
    if (request.action === "wake") {
      /*
      CDXC:GPUIWorkspaceLifecycle 2026-06-26-23:24:
      Rust-origin mapped sleeping placeholder activation must mirror macOS wake ownership: SidebarApp/gxserver commits `/api/wakeSession`, the sidebar marks the row running, and only the result ack lets Rust move the native tab into Mounting. Do not post WorkspaceTerminalFocus from this branch or the wake request would re-enter Rust before its pending lifecycle mutation applies.
      */
      await this.client.rpc("/api/wakeSession", {
        projectId: request.projectId,
        reason: "gpui-sidebar",
        sessionId: request.sessionId,
      });
      this.patchPresentationSession(request.projectId, request.sessionId, {
        lifecycleState: "running",
      });
      this.setLocalPresentationSessionFocus(request.projectId, request.sessionId);
      this.publishPresentation("patch");
      return true;
    }
    const result = await this.client.rpc<GxserverSessionTransitionResult>(
      "/api/transitionSession",
      {
        action: request.action,
        projectId: request.projectId,
        reason: "sleepSession",
        sessionId: request.sessionId,
      },
    );
    if (!shouldApplyGpuiLocalWorkspaceTransition(result, request.action)) {
      return false;
    }
    this.patchPresentationSession(request.projectId, request.sessionId, {
      lifecycleState: "sleeping",
    });
    const replacementProjectId = request.replacementProjectId ?? request.projectId;
    const replacementSessionId = request.replacementSessionId ?? fallbackReplacementSessionId;
    if (replacementSessionId) {
      this.focusLocalWorkspaceSession(replacementProjectId, replacementSessionId);
      this.publishPresentation("patch");
    }
    return true;
  },

  async applyRemoteWorkspaceTerminalLifecycleRequest(this: GpuiSidebarRuntime,
    request: GpuiWorkspaceTerminalLifecycleRequest,
    remoteProject: GpuiRemoteProjectReference,
  ): Promise<boolean> {
    const scopedSessionId = createGpuiRemotePresentationSessionId(
      remoteProject.machineId,
      remoteProject.projectId,
      request.sessionId,
    );
    const replacementProject = request.replacementProjectId
      ? parseGpuiRemotePresentationProjectId(request.replacementProjectId)
      : undefined;
    const focusReplacement = (): void => {
      if (
        replacementProject &&
        request.replacementSessionId &&
        replacementProject.machineId === remoteProject.machineId
      ) {
        const replacementReference = {
          machineId: replacementProject.machineId,
          projectId: replacementProject.projectId,
          sessionId: request.replacementSessionId,
        };
        /*
        CDXC:GPUIRemoteWorkspaceLifecycle 2026-08-08:
        A remote direct close must focus its surviving terminal through the
        same native open/focus bridge as a remote sidebar session click. A
        presentation-only focus update selects the replacement row but never
        transfers AppKit/GPUI keyboard ownership, leaving both the Agents pane
        and project-editor companion unable to type until clicked.
        */
        this.postRemoteSessionNativeAction(
          "openRemoteSessionTerminal",
          replacementReference,
          {
            sessionId: createGpuiRemotePresentationSessionId(
              replacementReference.machineId,
              replacementReference.projectId,
              replacementReference.sessionId,
            ),
            type: "focusSession",
          },
        );
        this.setRemotePresentationSessionFocus(replacementReference);
      }
    };

    if (request.action === "close") {
      const presentation = this.remotePresentations.get(remoteProject.machineId);
      if (presentation) {
        this.remotePresentations.set(remoteProject.machineId, {
          ...presentation,
          sessions: presentation.sessions.filter(
            (session) =>
              session.projectId !== remoteProject.projectId ||
              session.sessionId !== request.sessionId,
          ),
        });
      }
      if (this.focusedSessionId === scopedSessionId) {
        this.focusedSessionId = undefined;
        this.visibleSessionIds.delete(scopedSessionId);
      }
      focusReplacement();
      this.publishRemotePresentationPatch();
      void this.requestRemoteGxserver(remoteProject.machineId, "/api/killSession", {
        projectId: remoteProject.projectId,
        reason: "closeTerminal",
        sessionId: request.sessionId,
      })
        .then(() => this.refreshRemotePresentationFromGxserver(remoteProject.machineId))
        .catch(() => undefined);
      return true;
    }

    await this.requestRemoteGxserver(
      remoteProject.machineId,
      request.action === "wake" ? "/api/wakeSession" : "/api/sleepSession",
      {
        projectId: remoteProject.projectId,
        reason: "gpui-sidebar",
        sessionId: request.sessionId,
      },
    );
    await this.refreshRemotePresentationFromGxserver(remoteProject.machineId);
    if (request.action === "wake") {
      this.setRemotePresentationSessionFocus({
        machineId: remoteProject.machineId,
        projectId: remoteProject.projectId,
        sessionId: request.sessionId,
      });
    } else {
      focusReplacement();
    }
    return true;
  },

  postWorkspaceTerminalLifecycleResult(this: GpuiSidebarRuntime, requestId: number, ok: boolean): void {
    const postResult = window.ghostexGpui?.postWorkspaceTerminalLifecycleResult;
    if (typeof postResult !== "function") {
      return;
    }
    const payload = JSON.stringify({
      ok,
      requestId,
      type: GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_TYPE,
      version: GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_VERSION,
    });
    postResult(payload);
  },
};

const gpuiSidebarRuntimeTerminalLifecycleMethodsShapeCheck: GpuiSidebarRuntimeTerminalLifecycleMethods = gpuiSidebarRuntimeTerminalLifecycleMethods;
void gpuiSidebarRuntimeTerminalLifecycleMethodsShapeCheck;
