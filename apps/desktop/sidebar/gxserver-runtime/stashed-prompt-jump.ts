/*
CDXC:StashedPromptSessionAssociation 2026-08-24:
"Go to session" on a Saved Prompts row. A stash row remembers the raw gxserver
ids it was written from AND the provider conversation id (`agentSessionId`)
that outlives them, so the target is resolved in that order of durability:
the session that currently OWNS the conversation, then the exact session the
prompt was stashed from, then the daemon's restore/resume contract for a
session that is gone. Resolution deliberately runs here rather than in Rust:
session lifecycle state lives in this runtime's presentation snapshot.
*/
import type { GpuiSidebarRuntime } from './core';
import type { GpuiStashedPromptSessionJumpRequest } from './helpers/stashed-prompt-jump';
import {
  normalizeGpuiStashedPromptSessionJump,
  normalizeGpuiStashedPromptSessionJumpRequest,
} from './helpers/stashed-prompt-jump';
import { createGxserverPresentationProjectSessionId } from '@/packages/shared/gxserver-presentation-sidebar-projection';

const STASHED_PROMPT_JUMP_FAILURE_TITLE = "Couldn't open the session for this prompt";

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Hand-written interface plus the shape check at the bottom, like every other
runtime slice; see `core.ts` for how these are attached to the prototype.
*/
export interface GpuiSidebarRuntimeStashedPromptJumpMethods {
  handleGpuiStashedPromptSessionJump(payload: unknown): Promise<void>;
  jumpToStashedPromptSession(request: unknown): Promise<void>;
}

export const gpuiSidebarRuntimeStashedPromptJumpMethods = {
  /** Entry point for the Rust bridge payload (envelope-checked). */
  async handleGpuiStashedPromptSessionJump(this: GpuiSidebarRuntime, payload: unknown): Promise<void> {
    const request = normalizeGpuiStashedPromptSessionJump(payload);
    if (!request) {
      return;
    }
    await this.jumpToStashedPromptSession(request);
  },

  /** Entry point for the sidebar's own `jumpToStashedPromptSession` message. */
  async jumpToStashedPromptSession(this: GpuiSidebarRuntime, value: unknown): Promise<void> {
    const request: GpuiStashedPromptSessionJumpRequest | undefined =
      normalizeGpuiStashedPromptSessionJumpRequest(value);
    if (!request) {
      return;
    }
    const sessions = this.presentation?.sessions ?? [];
    /*
    A conversation can have been compacted, resumed, or forked into a different
    gxserver session since the prompt was stashed. Whichever live session now
    carries the conversation id is the one the user means, sleeping included —
    `focusSession` wakes those rather than focusing an empty shell.
    */
    const conversationOwner = request.agentSessionId
      ? sessions.find((session) => session.agentSessionId?.trim() === request.agentSessionId)
      : undefined;
    const target =
      conversationOwner ??
      (request.projectId && request.sessionId
        ? sessions.find((session) => session.projectId === request.projectId && session.sessionId === request.sessionId)
        : undefined);
    if (target) {
      await this.focusSession(createGxserverPresentationProjectSessionId(target.projectId, target.sessionId));
      return;
    }
    if (!request.projectId || !request.sessionId) {
      // Nothing is live and there is no session row to restore or resume from.
      this.postSidebarActionToast('warning', STASHED_PROMPT_JUMP_FAILURE_TITLE);
      return;
    }
    try {
      await this.openGpuiConversationSessionReference(
        { projectId: request.projectId, sessionId: request.sessionId },
        {
          restoreReason: 'stashedPromptJumpToSessionRestore',
          resumeReason: 'stashedPromptJumpToSessionResume',
        }
      );
    } catch {
      /*
      The daemon answers a removed session row or an unresumable agent with an
      error; that is a prompt whose session cannot be reopened, not a failure
      worth surfacing raw. Prompt text, paths, and ids stay out of the notice.
      */
      this.postSidebarActionToast('warning', STASHED_PROMPT_JUMP_FAILURE_TITLE);
    }
  },
};

const gpuiSidebarRuntimeStashedPromptJumpMethodsShapeCheck: GpuiSidebarRuntimeStashedPromptJumpMethods =
  gpuiSidebarRuntimeStashedPromptJumpMethods;
void gpuiSidebarRuntimeStashedPromptJumpMethodsShapeCheck;
