/*
CDXC:StashedPromptSessionAssociation 2026-08-24:
Refactor-by-extraction of the Project Board's jump-to-linked-conversation core.
The bodies below moved out of `project-board.ts`
(`jumpToGpuiProjectBoardConversation` / `resumeGpuiProjectBoardConversation`)
unchanged; the only edits are the board-specific bits lifted into parameters:
the daemon `reason` strings each caller stamps, and an `onSessionReplaced`
callback that the board uses to rewrite its bead link. That leaves exactly one
implementation of "take me to this conversation" — present → restore the
recorded-but-closed session → resume the conversation into a fresh session —
shared by the board's bead links and the Saved Prompts jump.
*/
import type { GpuiSidebarRuntime } from './core';
import { gpuiProjectBoardPreviousSessionRowTitle } from './helpers/project-board';
import { normalizeNonEmptyString } from './helpers/records';
import { createGxserverPresentationProjectSessionId } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type { GxserverForkSessionResult } from '@/packages/shared/gxserver-protocol';

/** Raw gxserver ids of the session that owned (or last owned) a conversation. */
export type GpuiConversationSessionReference = {
  projectId: string;
  sessionId: string;
};

/**
 * The live session that took the reference's place after a restore or a
 * resume. Field names match the board's link-replacement arguments so the
 * callback can forward this object straight through.
 */
export type GpuiConversationSessionReplacement = {
  restoredProjectId: string;
  restoredSessionId: string;
  restoredSessionPersistenceName?: string;
};

export type GpuiConversationJumpOptions = {
  /**
   * Called after the replacement session exists and before it is focused, so a
   * caller that stores the old session id can rewrite it in the same order the
   * board always has.
   */
  onSessionReplaced?: (replacement: GpuiConversationSessionReplacement) => Promise<void> | void;
  /** Daemon `reason` stamped on the `/api/removeSession` of the restore path. */
  restoreReason: string;
  /** Daemon `reason` stamped on the `/api/forkSession` of the resume path. */
  resumeReason: string;
};

export type GpuiConversationJumpOutcome = 'focused' | 'restored' | 'resumed';

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Same declaration-merge contract as every other runtime slice: the interface is
written out by hand rather than derived from the method object, and
`gpuiSidebarRuntimeConversationJumpMethodsShapeCheck` at the bottom keeps the
two in step.
*/
export interface GpuiSidebarRuntimeConversationJumpMethods {
  openGpuiConversationSessionReference(
    reference: GpuiConversationSessionReference,
    options: GpuiConversationJumpOptions
  ): Promise<GpuiConversationJumpOutcome>;
  resumeGpuiConversationSessionReference(
    reference: GpuiConversationSessionReference,
    options: GpuiConversationJumpOptions
  ): Promise<GpuiConversationJumpOutcome>;
}

export const gpuiSidebarRuntimeConversationJumpMethods = {
  async openGpuiConversationSessionReference(
    this: GpuiSidebarRuntime,
    reference: GpuiConversationSessionReference,
    options: GpuiConversationJumpOptions
  ): Promise<GpuiConversationJumpOutcome> {
    const live = this.presentation?.sessions.some(
      (session) => session.projectId === reference.projectId && session.sessionId === reference.sessionId
    );
    if (live) {
      await this.focusSession(createGxserverPresentationProjectSessionId(reference.projectId, reference.sessionId));
      return 'focused';
    }
    /*
    macOS restores dead links through the previous-sessions owner and rewrites
    the link to the restored session; GPUI uses the same daemon restore
    contract as the Previous Sessions modal (`createSession` with
    `restoredFromSessionId`, then remove the stopped history row).
    */
    const row = await this.findGpuiProjectBoardPreviousSessionRow(reference);
    if (!row) {
      return await this.resumeGpuiConversationSessionReference(reference, options);
    }
    if (!this.client) {
      throw new Error('The linked Ghostex session is no longer available.');
    }
    const created = await this.client.rpc<{
      session?: { projectId?: string; sessionId?: string; zmxName?: string };
    }>('/api/createSession', {
      kind: 'terminal',
      lifecycleState: 'running',
      projectId: reference.projectId,
      restoredFromSessionId: reference.sessionId,
      ...(row.sessionTag ? { sessionTag: row.sessionTag } : {}),
      ...(row.sidebarOrder !== undefined ? { sidebarOrder: row.sidebarOrder } : {}),
      surface: 'workspace',
      title: gpuiProjectBoardPreviousSessionRowTitle(row),
    });
    const restoredSessionId = normalizeNonEmptyString(created.session?.sessionId);
    if (!restoredSessionId) {
      throw new Error('The linked Ghostex session could not be restored.');
    }
    const restoredProjectId = normalizeNonEmptyString(created.session?.projectId) ?? reference.projectId;
    await this.client
      .rpc('/api/removeSession', {
        projectId: reference.projectId,
        reason: options.restoreReason,
        sessionId: reference.sessionId,
      })
      .catch(() => undefined);
    this.projectBoardRestorableLinkChecks.delete(`${reference.projectId}:${reference.sessionId}`);
    await options.onSessionReplaced?.({
      restoredProjectId,
      restoredSessionId,
      restoredSessionPersistenceName: normalizeNonEmptyString(created.session?.zmxName),
    });
    this.focusLocalWorkspaceSession(restoredProjectId, restoredSessionId);
    return 'restored';
  },

  async resumeGpuiConversationSessionReference(
    this: GpuiSidebarRuntime,
    reference: GpuiConversationSessionReference,
    options: GpuiConversationJumpOptions
  ): Promise<GpuiConversationJumpOutcome> {
    /*
    CDXC:ProjectBoardBeads 2026-08-07:
    A bead's session usually closes without leaving a restorable history row,
    but the agent conversation it worked is still resumable from the session
    row's agent identity. `/api/forkSession` is the daemon-owned path for that:
    it plans the resume command in gxserver, starts the provider, and hands
    back a live session, which the bead then follows through the same link
    replacement the restore path uses.
    */
    if (!this.client) {
      throw new Error('The linked Ghostex session is no longer available.');
    }
    const { fork } = await this.client.rpc<{ fork?: GxserverForkSessionResult }>('/api/forkSession', {
      projectId: reference.projectId,
      reason: options.resumeReason,
      sessionId: reference.sessionId,
    });
    const resumedSessionId = normalizeNonEmptyString(fork?.session.sessionId);
    if (!resumedSessionId) {
      throw new Error('The linked conversation could not be resumed.');
    }
    const resumedProjectId = normalizeNonEmptyString(fork?.session.projectId) ?? reference.projectId;
    this.projectBoardRestorableLinkChecks.delete(`${reference.projectId}:${reference.sessionId}`);
    await options.onSessionReplaced?.({
      restoredProjectId: resumedProjectId,
      restoredSessionId: resumedSessionId,
      restoredSessionPersistenceName: normalizeNonEmptyString(fork?.session.zmxName),
    });
    this.focusLocalWorkspaceSession(resumedProjectId, resumedSessionId);
    return 'resumed';
  },
};

const gpuiSidebarRuntimeConversationJumpMethodsShapeCheck: GpuiSidebarRuntimeConversationJumpMethods =
  gpuiSidebarRuntimeConversationJumpMethods;
void gpuiSidebarRuntimeConversationJumpMethodsShapeCheck;
