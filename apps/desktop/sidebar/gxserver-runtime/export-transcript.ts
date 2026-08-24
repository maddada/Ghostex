/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import type { GpuiSidebarRuntime } from './core';
import { normalizeNonEmptyString } from './helpers/records';
import { parseGpuiRemotePresentationSessionId } from './helpers/remote-presentation';
import { createExportedTranscriptMentionDraft } from './helpers/terminal-lifecycle';
import type { GpuiExportedTranscriptResult } from './types-and-protocol';
import { openAppModal } from '@/packages/core-ui/app-modal-host-bridge';
import { parseGxserverPresentationProjectSessionId } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type { GxserverExportSessionTranscriptResult } from '@/packages/shared/gxserver-protocol';
import { createAgentSessionDefaultTitle } from '@/packages/shared/session-grid-contract';
import type { SidebarAgentButton } from '@/packages/shared/sidebar-agents';

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeExportTranscriptMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeExportTranscriptMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeExportTranscriptMethods {
  exportSessionTranscript(sessionId: string): Promise<void>;
  resolveExportTranscriptAgentId(agent: string | undefined): string | undefined;
  openExportedTranscriptResultModal(result: GpuiExportedTranscriptResult): void;
  handleGpuiExportTranscriptModalCommand(payload: unknown): Promise<void>;
}

export const gpuiSidebarRuntimeExportTranscriptMethods = {
  /*
  CDXC:ExportTranscript 2026-08-20:
  Export Transcript is a daemon operation, not a local file read: the agent's
  raw transcript only exists on the machine that runs it, so the export lands
  next to the transcript and the returned path is absolute on THAT machine.
  Local sessions go through the local gxserver client and remote sessions
  through the machine-scoped tunnel, exactly like Fork. Failures surface the
  daemon's own message (including `unsupportedAgent`) instead of degrading into
  a partial export.
  */
  async exportSessionTranscript(this: GpuiSidebarRuntime, sessionId: string): Promise<void> {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      let exported: GpuiExportedTranscriptResult;
      try {
        const result = await this.requestRemoteGxserver<GxserverExportSessionTranscriptResult>(
          remoteSession.machineId,
          '/api/exportSessionTranscript',
          {
            projectId: remoteSession.projectId,
            sessionId: remoteSession.sessionId,
          },
          { timeoutMs: 60_000 }
        );
        const path = normalizeNonEmptyString(result?.path);
        if (!path) {
          throw new Error('The remote gxserver did not return the exported file.');
        }
        exported = {
          agentId: this.resolveExportTranscriptAgentId(result?.agent),
          machineId: remoteSession.machineId,
          path,
          projectId: remoteSession.projectId,
        };
      } catch (error) {
        this.postRemoteToast('error', 'Could not export transcript', {
          description: error instanceof Error ? error.message : String(error),
        });
        return;
      }
      // Outside the catch: the file is already written, so a failure to present
      // the result dialog must not be reported as a failed export.
      this.openExportedTranscriptResultModal(exported);
      return;
    }
    const reference = parseGxserverPresentationProjectSessionId(sessionId);
    if (!reference || !this.client) {
      return;
    }
    const sourceSession = this.presentation?.sessions.find(
      (session) => session.projectId === reference.projectId && session.sessionId === reference.sessionId
    );
    if (!sourceSession) {
      return;
    }
    let exported: GpuiExportedTranscriptResult;
    try {
      const result = await this.client.rpc<GxserverExportSessionTranscriptResult>('/api/exportSessionTranscript', {
        projectId: reference.projectId,
        sessionId: reference.sessionId,
      });
      const path = normalizeNonEmptyString(result?.path);
      if (!path) {
        throw new Error('gxserver did not return the exported file.');
      }
      exported = {
        agentId: normalizeNonEmptyString(sourceSession.agentId) ?? this.resolveExportTranscriptAgentId(result?.agent),
        path,
        projectId: reference.projectId,
      };
    } catch (error) {
      this.postSidebarActionToast('error', 'Could not export transcript', {
        description: error instanceof Error ? error.message : String(error),
      });
      return;
    }
    this.openExportedTranscriptResultModal(exported);
  },

  /**
   * Maps the daemon's transcript-format name (`claude`, `codex`, `grok`, `pi`)
   * onto one of the user's configured agents so the result dialog can preselect
   * the same agent the exported session used.
   */
  resolveExportTranscriptAgentId(this: GpuiSidebarRuntime, agent: string | undefined): string | undefined {
    const normalizedAgent = normalizeNonEmptyString(agent)?.toLowerCase();
    if (!normalizedAgent) {
      return undefined;
    }
    const agents = (this.sidebarHud?.agents ?? []) as readonly SidebarAgentButton[];
    return (
      agents.find((candidate) => candidate.agentId.toLowerCase() === normalizedAgent)?.agentId ??
      agents.find((candidate) => candidate.icon?.toLowerCase() === normalizedAgent)?.agentId
    );
  },

  openExportedTranscriptResultModal(this: GpuiSidebarRuntime, result: GpuiExportedTranscriptResult): void {
    this.pendingExportedTranscript = result;
    openAppModal({
      ...(result.agentId ? { agentId: result.agentId } : {}),
      // A remote export lives on the remote machine's disk, so this host has
      // nothing to reveal and the dialog hides the button instead of offering
      // a path the local file manager cannot open.
      canReveal: result.machineId === undefined,
      modal: 'exportTranscriptResult',
      path: result.path,
      type: 'open',
    });
  },

  /**
   * "Start new conversation" in the export result dialog. The dialog owns only
   * the agent choice; the exported path and its project stay in this runtime.
   */
  async handleGpuiExportTranscriptModalCommand(this: GpuiSidebarRuntime, payload: unknown): Promise<void> {
    if (!payload || typeof payload !== 'object' || Array.isArray(payload)) {
      return;
    }
    const record = payload as Record<string, unknown>;
    if (record.type !== 'startExportedTranscriptConversation') {
      return;
    }
    const exported = this.pendingExportedTranscript;
    this.pendingExportedTranscript = undefined;
    if (!exported) {
      return;
    }
    const agentId = normalizeNonEmptyString(record.agentId) ?? exported.agentId;
    const agent = agentId ? this.resolveSidebarAgent(agentId) : undefined;
    if (!agent) {
      this.postSidebarActionToast('warning', 'Could not start the conversation', {
        description: 'Choose a configured agent for the new session.',
      });
      return;
    }
    /*
    CDXC:ExportTranscript 2026-08-20:
    The new session is created with a staged input draft, never a first user
    message: gxserver types the mention into the agent's composer after the
    provider starts and stops there. Nothing on this side sends a prompt or an
    Enter, so the conversation only begins when the user writes their own
    prompt around the mention and submits it themselves.
    */
    const draft = createExportedTranscriptMentionDraft(exported.path);
    const title = createAgentSessionDefaultTitle(agent.name);
    if (exported.machineId) {
      await this.createRemoteAgentSessionForProject(
        { machineId: exported.machineId, projectId: exported.projectId },
        agent.agentId,
        '',
        title,
        { firstUserInputDraft: draft }
      ).catch((error: unknown) => {
        this.postRemoteToast('error', 'Could not start the conversation', {
          description: error instanceof Error ? error.message : String(error),
        });
      });
      return;
    }
    const project = this.domainProjects.find((candidate) => candidate.projectId === exported.projectId);
    if (!project) {
      return;
    }
    await this.createAgentSessionRecordForProject(project, agent, '', {
      errorMessage: 'Could not create the new agent session.',
      firstUserInputDraft: draft,
      title,
    }).catch((error: unknown) => {
      this.postSidebarActionToast('error', 'Could not start the conversation', {
        description: error instanceof Error ? error.message : String(error),
      });
    });
  },
};

const gpuiSidebarRuntimeExportTranscriptMethodsShapeCheck: GpuiSidebarRuntimeExportTranscriptMethods =
  gpuiSidebarRuntimeExportTranscriptMethods;
void gpuiSidebarRuntimeExportTranscriptMethodsShapeCheck;
