/*
CDXC:RepoStructure 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import { GpuiGxserverClient } from './client';
import {
  GPUI_PRESENTATION_STREAM_HEALTHY_MS,
  GPUI_PRESENTATION_STREAM_RECOVERY_DELAYS_MS,
} from './constants';
import type { GpuiSidebarRuntime } from './core';
import { hasSameGpuiGxserverBootstrapTransport, validateGpuiGxserverBootstrap } from './helpers/bootstrap';
import { isCustomSessionTagsState } from './helpers/remote-presentation';
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
CDXC:RepoStructure 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimePresentationStreamMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimePresentationStreamMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimePresentationStreamMethods {
  applyGxserverBootstrapChanged(bootstrap: GpuiGxserverBootstrap): void;
  startFromBootstrap(bootstrap: GpuiGxserverBootstrap): void;
  openPresentationSubscription(clientId: string, lastRevision: number): void;
  recoverPresentationStream(clientId: string): void;
  reopenPresentationStream(clientId: string): void;
  notePresentationStreamAcknowledged(): void;
  applyPresentationSnapshot(snapshot: GxserverPresentationSnapshot, kind: GpuiSidebarRuntimeSnapshotKind): void;
  applyPresentationDelta(delta: GxserverPresentationDelta, gxserverRevision: number): void;
  findLocalPresentationSession(projectId: string, sessionId: string): GxserverPresentationSession | undefined;
  patchPresentationSession(
    projectId: string,
    sessionId: string,
    patch: Partial<GxserverPresentationSnapshot['sessions'][number]>
  ): void;
  removePresentationSession(projectId: string, sessionId: string): void;
  hideLocalPresentationSession(projectId: string, sessionId: string): void;
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
    CDXC:CefRuntime 2026-06-26-05:31:
    Post-start same-transport bootstrap refreshes are Rust's replay channel for the sidebar bridge, not a new macOS-style focus command. Store the refreshed transport/focus hint snapshot but do not reapply `initialActiveProjectId`, focused session, or visible ids over live React focus; otherwise the active project can bounce between stale and current sidebar snapshots after a local click.
    */
    this.gxserverBootstrap = validated;
  },

  startFromBootstrap(this: GpuiSidebarRuntime, bootstrap: GpuiGxserverBootstrap): void {

    const validated = validateGpuiGxserverBootstrap(bootstrap);
    if (!validated) {
      this.publishUnavailable('bootstrap-invalid');
      return;
    }

    this.subscription?.close();
    // A pending recovery belongs to the stream this bootstrap is replacing.
    if (this.presentationStreamRecoveryTimeoutId !== undefined) {
      window.clearTimeout(this.presentationStreamRecoveryTimeoutId);
      this.presentationStreamRecoveryTimeoutId = undefined;
    }
    this.presentationStreamRecoveryAttempt = 0;
    this.presentationStreamAcknowledgedAt = undefined;
    this.gxserverBootstrap = validated;
    this.client = new GpuiGxserverClient(validated);

    const client = this.client;
    void Promise.all([
      client.fetchPresentationSnapshot(),
      client.fetchAppUserData(),
      client.fetchProjectList().catch(() => undefined),
      client.fetchRecentProjects().catch(() => undefined),
    ])
      .then(([snapshot, appUserData, domainProjects, recentProjects]) => {
        if (this.client !== client) {
          return;
        }
        this.appUserData = appUserData;
        this.domainProjects = domainProjects ? [...domainProjects] : [];
        this.recentProjects = recentProjects ? [...recentProjects] : [];
        this.applyPresentationSnapshot(snapshot, 'hydrate');
        this.openPresentationSubscription(validated.clientId, snapshot.revision);
      })
      .catch(() => {
        this.publishUnavailable('snapshot-failed');
      });
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
      CDXC:Projects 2026-09-21 WHY:
      This computer's project collections and Spaces documents are NOT taken off this socket any
      more, exactly as the workspace session groups document stopped being in M5 piece 7c. The app
      owns both, and its pending-push guard is the only thing that knows whether a local edit is
      still on its way to the daemon; forwarding the daemon's echo straight into the page put a
      document the guard had refused back into the page's copy, which is the base its next edit is
      computed from. Nothing in this runtime holds either document now: the local relay and the
      hand-back that replaced it were both deleted on 2026-09-21. A REMOTE machine's copies still
      arrive through `forwardRemoteSidebar*FromGxserver`, because this runtime still owns those.
      SEE-ALSO: apps/desktop/src/app/gx_store/project_docs.rs,
      tooling/gx-core/sidebar-page-frozen/metadata.ts.
      */
      onCustomSessionTags: (state) => {
        this.forwardCustomSessionTagsFromGxserver(state);
      },
      onSnapshot: (snapshot) => {
        this.notePresentationStreamAcknowledged();
        this.applyPresentationSnapshot(snapshot, this.hasHydrated ? 'patch' : 'hydrate');
      },
      /*
      CDXC:StateSync 2026-09-01:
      The daemon answers a subscribe that already names its current revision
      with the revision alone, because there is nothing to send: this runtime
      applied that exact snapshot over HTTP moments earlier, right before it
      opened the socket. Keeping the presentation untouched is the whole point,
      so the only state this moves is the stream's own health — the same thing
      an unchanged full snapshot would have moved.
      */
      onSnapshotCurrent: () => {
        this.notePresentationStreamAcknowledged();
      },
    });
  },

  notePresentationStreamAcknowledged(this: GpuiSidebarRuntime): void {
    this.presentationStreamAcknowledgedAt = Date.now();
  },

  /*
  CDXC:StateSync 2026-09-01:
  A dropped socket fires `onClose` *and* `onError`, and a daemon that is down
  keeps dropping the replacement, so recovering inline meant a full presentation
  snapshot plus three more RPCs per drop with nothing between them. Schedule the
  recovery instead: the pending timer coalesces the duplicate notifications, and
  the delay escalates while the stream keeps failing.
  */
  recoverPresentationStream(this: GpuiSidebarRuntime, clientId: string): void {
    if (!this.client) {
      return;
    }
    this.subscription?.close();
    this.subscription = undefined;
    if (this.presentationStreamRecoveryTimeoutId !== undefined) {
      return;
    }
    const acknowledgedAt = this.presentationStreamAcknowledgedAt;
    this.presentationStreamAcknowledgedAt = undefined;
    if (acknowledgedAt !== undefined && Date.now() - acknowledgedAt >= GPUI_PRESENTATION_STREAM_HEALTHY_MS) {
      this.presentationStreamRecoveryAttempt = 0;
    }
    const delay =
      GPUI_PRESENTATION_STREAM_RECOVERY_DELAYS_MS[
        Math.min(this.presentationStreamRecoveryAttempt, GPUI_PRESENTATION_STREAM_RECOVERY_DELAYS_MS.length - 1)
      ] ?? GPUI_PRESENTATION_STREAM_RECOVERY_DELAYS_MS[0];
    this.presentationStreamRecoveryAttempt += 1;
    this.presentationStreamRecoveryTimeoutId = window.setTimeout(() => {
      this.presentationStreamRecoveryTimeoutId = undefined;
      this.reopenPresentationStream(clientId);
    }, delay);
  },

  reopenPresentationStream(this: GpuiSidebarRuntime, clientId: string): void {
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
    ])
      .then(([snapshot, domainProjects, recentProjects]) => {
        if (this.client !== client) {
          return;
        }
        if (domainProjects) {
          this.domainProjects = [...domainProjects];
        }
        if (recentProjects) {
          this.recentProjects = [...recentProjects];
        }
        this.applyPresentationSnapshot(snapshot, this.hasHydrated ? 'patch' : 'hydrate');
        this.openPresentationSubscription(clientId, snapshot.revision);
      })
      .catch(() => {
        if (this.client !== client) {
          return;
        }
        this.publishUnavailable('stream-recovery-failed');
        // A daemon that is still restarting fails the snapshot fetch itself.
        // Keep escalating instead of leaving the sidebar unavailable until an
        // unrelated bootstrap replay happens to revive it.
        this.recoverPresentationStream(clientId);
      });
  },

  applyPresentationSnapshot(
    this: GpuiSidebarRuntime,
    snapshot: GxserverPresentationSnapshot,
    kind: GpuiSidebarRuntimeSnapshotKind
  ): void {
    this.presentation = snapshot;
    // The snapshot's own copies of the two project documents go the same way as the socket's, and
    // for the same reason: the app judges them behind the guard and hands the result back.
    if (isCustomSessionTagsState(snapshot.customSessionTags)) {
      this.forwardCustomSessionTagsFromGxserver(snapshot.customSessionTags);
    }
    this.publishPresentation(kind);
    this.notifyNativeGxserverPresentationReady();
  },

  applyPresentationDelta(this: GpuiSidebarRuntime, delta: GxserverPresentationDelta, gxserverRevision: number): void {
    if (!this.presentation || gxserverRevision <= this.presentation.revision) {
      return;
    }
    this.applyDomainProjectDelta(delta);
    this.presentation = reduceGxserverPresentationDelta(this.presentation, delta, gxserverRevision);
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
    CDXC:Workarea 2026-06-26-23:59:
    GPUI native tab close must match macOS local-first sidebar removal. Keep a runtime-only hidden-session overlay so future gxserver hydrates cannot reinsert a locally closed mapped Agents row while the backend transition catches up or fails best-effort. Store only project/session ids.
    */
    this.localFirstHiddenPresentationSessionKeys.add(createGxserverPresentationSidebarSessionKey(projectId, sessionId));
  },

  removeLocalPresentationProject(this: GpuiSidebarRuntime, projectId: string): void {
    const presentation = this.presentation;
    if (!presentation) {
      return;
    }
    /*
    CDXC:Projects 2026-06-25-18:50:
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
