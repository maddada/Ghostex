/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import { GpuiGxserverClient } from './client';
import { GPUI_SIDEBAR_BOOTSTRAP_MAX_ATTEMPTS, GPUI_SIDEBAR_BOOTSTRAP_RETRY_DELAY_MS } from './constants';
import type { GpuiSidebarRuntime } from './core';
import {
  activeGroupIdForGpuiGxserverBootstrapPresentationState,
  hasSameGpuiGxserverBootstrapTransport,
  validateGpuiGxserverBootstrap,
} from './helpers/bootstrap';
import { sameStringSet } from './helpers/records';
import {
  isSidebarProjectCollectionsState,
  isSidebarSpacesState,
  parseGpuiRemotePresentationProjectId,
} from './helpers/remote-presentation';
import type {
  GpuiGxserverBootstrap,
  GpuiSidebarRuntimeSnapshotKind,
  GpuiValidatedGxserverBootstrap,
} from './types-and-protocol';
import { reduceGxserverPresentationDelta } from '@/packages/shared/gxserver-presentation-cache';
import { createGxserverPresentationSidebarSessionKey } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type {
  GxserverPresentationDelta,
  GxserverPresentationSession,
  GxserverPresentationSnapshot,
  GxserverProjectId,
  GxserverSessionId,
} from '@/packages/shared/gxserver-protocol';

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimePresentationStreamMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimePresentationStreamMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimePresentationStreamMethods {
  applyGxserverBootstrapChanged(bootstrap: GpuiGxserverBootstrap): void;
  tryStartFromInstalledBootstrap(attempt: number): void;
  startFromBootstrap(bootstrap: GpuiGxserverBootstrap): void;
  applyGxserverBootstrapPresentationState(bootstrap: GpuiValidatedGxserverBootstrap): boolean;
  openPresentationSubscription(clientId: string, lastRevision: number): void;
  recoverPresentationStream(clientId: string): void;
  applyPresentationSnapshot(snapshot: GxserverPresentationSnapshot, kind: GpuiSidebarRuntimeSnapshotKind): void;
  autoMaterializeStartupFocusedSession(): void;
  applyPresentationDelta(delta: GxserverPresentationDelta, gxserverRevision: number): void;
  findLocalPresentationSession(projectId: string, sessionId: string): GxserverPresentationSession | undefined;
  patchPresentationSession(
    projectId: string,
    sessionId: string,
    patch: Partial<GxserverPresentationSnapshot['sessions'][number]>
  ): void;
  removePresentationSession(projectId: string, sessionId: string): void;
  hideLocalPresentationSession(projectId: string, sessionId: string): void;
  unhideLocalPresentationSession(projectId: string, sessionId: string): void;
  removeLocalPresentationProject(projectId: string): void;
}

export const gpuiSidebarRuntimePresentationStreamMethods = {
  applyGxserverBootstrapChanged(this: GpuiSidebarRuntime, bootstrap: GpuiGxserverBootstrap): void {
    const validated = validateGpuiGxserverBootstrap(bootstrap);
    if (!validated) {
      this.startFromBootstrap(bootstrap);
      return;
    }
    if (
      !this.gxserverBootstrap ||
      !hasSameGpuiGxserverBootstrapTransport(this.gxserverBootstrap, validated) ||
      !this.presentation
    ) {
      this.startFromBootstrap(bootstrap);
      return;
    }
    /*
    CDXC:GPUISidebarBootstrapReplay 2026-06-26-05:31:
    Post-start same-transport bootstrap refreshes are Rust's replay channel for the sidebar bridge, not a new macOS-style focus command. Store the refreshed transport/focus hint snapshot but do not reapply `initialActiveProjectId`, focused session, or visible ids over live React focus; otherwise the active project can bounce between stale and current sidebar snapshots after a local click.
    */
    this.gxserverBootstrap = validated;
  },

  tryStartFromInstalledBootstrap(this: GpuiSidebarRuntime, attempt: number): void {
    const bootstrap = window.ghostexGpui?.gxserverBootstrap;
    if (bootstrap) {
      this.startFromBootstrap(bootstrap);
      return;
    }
    if (attempt >= GPUI_SIDEBAR_BOOTSTRAP_MAX_ATTEMPTS) {
      return;
    }
    this.bootstrapPollTimeoutId = window.setTimeout(() => {
      this.tryStartFromInstalledBootstrap(attempt + 1);
    }, GPUI_SIDEBAR_BOOTSTRAP_RETRY_DELAY_MS);
  },

  startFromBootstrap(this: GpuiSidebarRuntime, bootstrap: GpuiGxserverBootstrap): void {
    if (this.bootstrapPollTimeoutId !== undefined) {
      window.clearTimeout(this.bootstrapPollTimeoutId);
      this.bootstrapPollTimeoutId = undefined;
    }

    const validated = validateGpuiGxserverBootstrap(bootstrap);
    if (!validated) {
      this.publishUnavailable('bootstrap-invalid');
      return;
    }

    this.subscription?.close();
    this.gxserverBootstrap = validated;
    this.client = new GpuiGxserverClient(validated);
    this.applyGxserverBootstrapPresentationState(validated);
    // Adopt whatever trail this scope already has on the daemon so Back keeps
    // working across an app restart instead of starting from an empty stack.
    void this.navigationHistory.refresh();
    // Heal the shared composer draft cache from the daemon's durable copy —
    // an app kill can drop localStorage batches the daemon still holds.
    this.reconcileSessionChatDraftCache();

    const client = this.client;
    void Promise.all([
      client.fetchPresentationSnapshot(),
      client.fetchAppUserData(),
      client.fetchProjectList().catch(() => undefined),
      client.fetchRecentProjects().catch(() => undefined),
      client.fetchSidebarHud(validated.initialActiveProjectId),
      client.fetchWorkspaceSessionGroups().catch(() => undefined),
    ])
      .then(([snapshot, appUserData, domainProjects, recentProjects, sidebarHud, workspaceGroups]) => {
        if (this.client !== client) {
          return;
        }
        this.appUserData = appUserData;
        this.domainProjects = domainProjects ? [...domainProjects] : [];
        this.recentProjects = recentProjects ? [...recentProjects] : [];
        this.sidebarHud = sidebarHud;
        this.adoptWorkspaceGroupsFromGxserver(workspaceGroups);
        this.applyPresentationSnapshot(snapshot, 'hydrate');
        this.openPresentationSubscription(validated.clientId, snapshot.revision);
      })
      .catch(() => {
        this.publishUnavailable('snapshot-failed');
      });
  },

  applyGxserverBootstrapPresentationState(
    this: GpuiSidebarRuntime,
    bootstrap: GpuiValidatedGxserverBootstrap
  ): boolean {
    const nextFocusedSessionId = bootstrap.focusedSessionId;
    const nextVisibleSessionIds = new Set(bootstrap.visibleSessionIds ?? []);
    /*
    CDXC:GPUIRemoteWorkspaceProjectKey 2026-07-30:
    A bootstrap can replay a machine-scoped remote project id after a remote
    session owned focus at shutdown. `this.activeProjectId` is a local-only
    gxserver key (HUD fetches, domain-project lookups), so the scoped id may
    only select the remote group; it must never become the local active
    project id.
    */
    const nextActiveProjectId =
      bootstrap.initialActiveProjectId && parseGpuiRemotePresentationProjectId(bootstrap.initialActiveProjectId)
        ? this.activeProjectId
        : bootstrap.initialActiveProjectId;
    const nextActiveGroupId = activeGroupIdForGpuiGxserverBootstrapPresentationState({
      focusedSessionId: nextFocusedSessionId,
      initialActiveProjectId: bootstrap.initialActiveProjectId,
    });
    const didChange =
      this.activeProjectId !== nextActiveProjectId ||
      this.activeGroupId !== nextActiveGroupId ||
      this.focusedSessionId !== nextFocusedSessionId ||
      !sameStringSet(this.visibleSessionIds, nextVisibleSessionIds);
    this.activeProjectId = nextActiveProjectId;
    this.activeGroupId = nextActiveGroupId;
    this.focusedSessionId = nextFocusedSessionId;
    this.visibleSessionIds = nextVisibleSessionIds;
    return didChange;
  },

  openPresentationSubscription(this: GpuiSidebarRuntime, clientId: string, lastRevision: number): void {
    if (!this.client) {
      return;
    }
    this.subscription = this.client.subscribePresentation({
      clientId,
      lastRevision,
      onClose: () => {
        this.recoverPresentationStream(clientId);
      },
      onDelta: (delta, revision) => {
        this.applyPresentationDelta(delta, revision);
      },
      onError: () => {
        this.recoverPresentationStream(clientId);
      },
      /*
      CDXC:GlobalActions 2026-08-07:
      Global Action writes reach this surface only as this announcement. They
      are not project writes, so they produce no projectUpdated delta, and the
      Settings window that made the write is a different surface whose response
      never lands here. Refetch the HUD the same way a project Action edit
      already does, so a Global Action flagged for the project row appears and
      disappears with the toggle instead of on the next unrelated delta.
      */
      onGlobalSidebarCommands: () => {
        this.refreshSidebarHudFromClient();
      },
      onRendererCommand: (command) => this.handleGxserverRendererCommand(command),
      onSidebarProjectCollections: (state) => {
        this.forwardSidebarProjectCollectionsFromGxserver(state);
      },
      onSidebarSpaces: (state) => {
        this.forwardSidebarSpacesFromGxserver(state);
      },
      onSnapshot: (snapshot) => {
        this.applyPresentationSnapshot(snapshot, this.hasHydrated ? 'patch' : 'hydrate');
      },
      onWorkspaceGroups: (state) => {
        const previous = this.workspaceGroups;
        this.adoptWorkspaceGroupsFromGxserver(state);
        if (this.workspaceGroups !== previous) {
          this.publishPresentation('patch');
        }
      },
    });
  },

  recoverPresentationStream(this: GpuiSidebarRuntime, clientId: string): void {
    if (!this.client) {
      return;
    }
    const client = this.client;
    this.subscription?.close();
    this.subscription = undefined;
    void Promise.all([
      client.fetchPresentationSnapshot(),
      client.fetchProjectList().catch(() => undefined),
      client.fetchRecentProjects().catch(() => undefined),
      client.fetchSidebarHud(this.activeProjectId),
    ])
      .then(([snapshot, domainProjects, recentProjects, sidebarHud]) => {
        if (this.client !== client) {
          return;
        }
        if (domainProjects) {
          this.domainProjects = [...domainProjects];
        }
        if (recentProjects) {
          this.recentProjects = [...recentProjects];
        }
        this.sidebarHud = sidebarHud;
        this.applyPresentationSnapshot(snapshot, this.hasHydrated ? 'patch' : 'hydrate');
        this.openPresentationSubscription(clientId, snapshot.revision);
      })
      .catch(() => {
        this.publishUnavailable('stream-recovery-failed');
      });
  },

  applyPresentationSnapshot(
    this: GpuiSidebarRuntime,
    snapshot: GxserverPresentationSnapshot,
    kind: GpuiSidebarRuntimeSnapshotKind
  ): void {
    const previousSessions = this.presentation?.sessions ?? [];
    const projectedSnapshot = this.projectLocalPresentationAttentionAcknowledgementGuards(snapshot);
    this.presentation = projectedSnapshot;
    this.syncLocalPresentationAttentionTracking(previousSessions, projectedSnapshot.sessions);
    if (isSidebarProjectCollectionsState(snapshot.sidebarProjectCollections)) {
      this.forwardSidebarProjectCollectionsFromGxserver(snapshot.sidebarProjectCollections);
    }
    if (isSidebarSpacesState(snapshot.sidebarSpaces)) {
      this.forwardSidebarSpacesFromGxserver(snapshot.sidebarSpaces);
    }
    this.adoptWorkspaceGroupsFromGxserver(snapshot.workspaceGroups);
    this.publishPresentation(kind);
    this.notifyNativeGxserverPresentationReady();
    if (kind === 'hydrate') {
      void this.runGpuiAutoSleepMonitor('startup');
      this.autoMaterializeStartupFocusedSession();
    }
  },

  autoMaterializeStartupFocusedSession(this: GpuiSidebarRuntime): void {
    /*
    Restore eagerness (Decision #3, 2026-07-02, revised 2026-08-07): the
    session the user was looking at when the app quit re-materializes
    automatically on relaunch. Rust persists the presentation focus state
    across restarts and replays it through the bootstrap; once the first
    presentation hydrate confirms that focused session is still a running local
    session, re-attach it through the normal workspace focus bridge. This
    covers the focused session only. Every other surfaced session — the other
    panes of a split, remote attach tabs, and sessions whose provider went to
    sleep while the app was closed — is now restored by Rust from the workspace
    model it already owns, so nothing further is needed here.
    */
    if (this.didAutoMaterializeStartupSession) {
      return;
    }
    this.didAutoMaterializeStartupSession = true;
    const focusedSessionId = this.focusedSessionId;
    if (!focusedSessionId || !this.visibleSessionIds.has(focusedSessionId)) {
      return;
    }
    const session = this.presentation?.sessions.find(
      (presentationSession) => presentationSession.sessionId === focusedSessionId
    );
    if (!session || session.lifecycleState !== 'running') {
      return;
    }
    this.postLocalWorkspaceTerminalFocus(session.projectId, focusedSessionId);
  },

  applyPresentationDelta(this: GpuiSidebarRuntime, delta: GxserverPresentationDelta, gxserverRevision: number): void {
    if (!this.presentation || gxserverRevision <= this.presentation.revision) {
      return;
    }
    this.applyDomainProjectDelta(delta);
    const previousSessions = this.presentation.sessions;
    const projectedSnapshot = this.projectLocalPresentationAttentionAcknowledgementGuards(
      reduceGxserverPresentationDelta(this.presentation, delta, gxserverRevision)
    );
    this.presentation = projectedSnapshot;
    this.syncLocalPresentationAttentionTracking(previousSessions, projectedSnapshot.sessions);
    this.detectSessionAttentionCompletionSounds(previousSessions, projectedSnapshot.sessions);
    this.publishPresentation('patch');
  },

  findLocalPresentationSession(
    this: GpuiSidebarRuntime,
    projectId: string,
    sessionId: string
  ): GxserverPresentationSession | undefined {
    return this.presentation?.sessions.find(
      (session) => session.projectId === projectId && session.sessionId === sessionId
    );
  },

  patchPresentationSession(
    this: GpuiSidebarRuntime,
    projectId: string,
    sessionId: string,
    patch: Partial<GxserverPresentationSnapshot['sessions'][number]>
  ): void {
    const presentation = this.presentation;
    const session = presentation?.sessions.find(
      (candidate) => candidate.projectId === projectId && candidate.sessionId === sessionId
    );
    if (!presentation || !session) {
      return;
    }
    /*
    Local presentation patches are overlays on the last daemon snapshot, not
    gxserver events. Preserve the daemon revision so the next real delta is not
    discarded by `applyPresentationDelta` as stale when it receives the same
    revision number a client-only `+ 1` previously invented. This matters most
    for provider metadata changes such as `/rename`: the title delta can be the
    next daemon event, while a later unrelated tag delta only appeared to fix
    the stale title because it advanced the revision again.
    */
    this.presentation = reduceGxserverPresentationDelta(
      presentation,
      {
        session: {
          ...session,
          ...patch,
        },
        type: 'sessionUpdated',
      },
      presentation.revision
    );
    this.publishPresentation('patch');
  },

  removePresentationSession(this: GpuiSidebarRuntime, projectId: string, sessionId: string): void {
    this.hideLocalPresentationSession(projectId, sessionId);
    const presentation = this.presentation;
    if (!presentation) {
      return;
    }
    this.presentation = reduceGxserverPresentationDelta(
      presentation,
      {
        projectId: projectId as GxserverProjectId,
        sessionId: sessionId as GxserverSessionId,
        type: 'sessionRemoved',
      },
      presentation.revision
    );
    this.publishPresentation('patch');
  },

  hideLocalPresentationSession(this: GpuiSidebarRuntime, projectId: string, sessionId: string): void {
    /*
    CDXC:GPUIWorkspaceLifecycle 2026-06-26-23:59:
    GPUI native tab close must match macOS local-first sidebar removal. Keep a runtime-only hidden-session overlay so future gxserver hydrates cannot reinsert a locally closed mapped Agents row while the backend transition catches up or fails best-effort. Store only project/session ids.
    */
    this.localFirstHiddenPresentationSessionKeys.add(createGxserverPresentationSidebarSessionKey(projectId, sessionId));
  },

  unhideLocalPresentationSession(this: GpuiSidebarRuntime, projectId: string, sessionId: string): void {
    /*
    CDXC:DraftSessions 2026-08-28:
    The inverse of `hideLocalPresentationSession`, for the one caller whose
    local-first removal can be REFUSED by the daemon. The empty-draft discard
    hides the row before its `/api/removeSession`, and gxserver re-derives the
    predicate from its own state and may decline (the session was promoted, or
    gained draft text, since the snapshot the client decided from). Dropping the
    key here is what lets the next hydrate show the row again — the overlay is
    consulted when groups are built, so a key left behind would keep a live
    session invisible on this client forever.

    Deliberately NOT wired into the ordinary close path: that removal is an
    instruction, and its overlay entry is exactly what stops a hydrate still in
    flight from resurrecting a row the user closed.
    */
    this.localFirstHiddenPresentationSessionKeys.delete(
      createGxserverPresentationSidebarSessionKey(projectId, sessionId)
    );
  },

  removeLocalPresentationProject(this: GpuiSidebarRuntime, projectId: string): void {
    const presentation = this.presentation;
    if (!presentation) {
      return;
    }
    /*
    CDXC:GPUIRecentProjects 2026-06-25-18:50:
    Local close-to-recent must immediately mirror macOS by removing the parked project from normal GPUI sidebar groups while using gxserver's `/api/closeProjectToRecent` recent-project response as the only drawer source.
    */
    this.presentation = reduceGxserverPresentationDelta(
      presentation,
      {
        projectId: projectId as GxserverProjectId,
        type: 'projectRemoved',
      },
      presentation.revision
    );
  },
};

const gpuiSidebarRuntimePresentationStreamMethodsShapeCheck: GpuiSidebarRuntimePresentationStreamMethods =
  gpuiSidebarRuntimePresentationStreamMethods;
void gpuiSidebarRuntimePresentationStreamMethodsShapeCheck;
