/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. See `core.ts` for
how the runtime's methods are re-attached.
*/
import type { GpuiSidebarRuntime } from './core';
import { normalizeNonEmptyString } from './helpers/records';
import { parseGpuiRemotePresentationSessionId } from './helpers/remote-presentation';
import { createExportedTranscriptMentionDraft } from './helpers/terminal-lifecycle';
import type { GpuiExportTranscriptRequestContext } from './types-and-protocol';
import { openAppModal, postAppModalHostMessage } from '@/packages/core-ui/app-modal-host-bridge';
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
  runExportSessionTranscriptForDialog(options: {
    includeCommands: boolean;
    includePatches: boolean;
    includeReasoning: boolean;
  }): Promise<void>;
  resolveExportTranscriptAgentId(agent: string | undefined): string | undefined;
  handleGpuiExportTranscriptModalCommand(payload: unknown): Promise<void>;
}

export const gpuiSidebarRuntimeExportTranscriptMethods = {
  /*
  CDXC:ExportTranscript 2026-08-20 / CDXC:ExportTranscriptOptions 2026-08-24:
  Export Transcript opens its dialog on the include-toggle options stage; the
  daemon call only runs when the user confirms it there
  (`runExportSessionTranscriptForDialog`). This method resolves which session
  the dialog is about — local or remote, exactly like Fork — and parks that
  context here, because the dialog is a separate child window with no gxserver
  client of its own.
  */
  async exportSessionTranscript(this: GpuiSidebarRuntime, sessionId: string): Promise<void> {
    const remoteSession = parseGpuiRemotePresentationSessionId(sessionId);
    if (remoteSession) {
      this.pendingExportedTranscript = undefined;
      this.pendingExportTranscriptRequest = {
        machineId: remoteSession.machineId,
        projectId: remoteSession.projectId,
        sessionId: remoteSession.sessionId,
      };
      openAppModal({
        // A remote export lives on the remote machine's disk, so this host has
        // nothing to reveal and the dialog hides the button instead of
        // offering a path the local file manager cannot open.
        canReveal: false,
        modal: 'exportTranscriptResult',
        type: 'open',
      });
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
    const agentId = normalizeNonEmptyString(sourceSession.agentId);
    this.pendingExportedTranscript = undefined;
    this.pendingExportTranscriptRequest = {
      ...(agentId ? { agentId } : {}),
      projectId: reference.projectId,
      sessionId: reference.sessionId,
    };
    openAppModal({
      ...(agentId ? { agentId } : {}),
      canReveal: true,
      modal: 'exportTranscriptResult',
      type: 'open',
    });
  },

  /*
  CDXC:ExportTranscriptOptions 2026-08-24:
  The dialog's Export button. The export is a daemon operation, not a local
  file read: the agent's raw transcript only exists on the machine that runs
  it, so the export lands next to the transcript and the returned path is
  absolute on THAT machine. Local sessions go through the local gxserver
  client and remote sessions through the machine-scoped tunnel. Failures
  surface the daemon's own message (including `unsupportedAgent`) inside the
  dialog instead of degrading into a partial export.
  */
  async runExportSessionTranscriptForDialog(
    this: GpuiSidebarRuntime,
    options: { includeCommands: boolean; includePatches: boolean; includeReasoning: boolean }
  ): Promise<void> {
    const request = this.pendingExportTranscriptRequest;
    if (!request) {
      return;
    }
    // Plain record: the runtime's ids are unbranded strings, matching how the
    // other daemon RPCs in this runtime build their params.
    const params: Record<string, unknown> = {
      includeCommands: options.includeCommands,
      includePatches: options.includePatches,
      includeReasoning: options.includeReasoning,
      projectId: request.projectId,
      sessionId: request.sessionId,
    };
    try {
      const result = request.machineId
        ? await this.requestRemoteGxserver<GxserverExportSessionTranscriptResult>(
            request.machineId,
            '/api/exportSessionTranscript',
            params,
            { timeoutMs: 60_000 }
          )
        : await (() => {
            if (!this.client) {
              throw new Error('The local gxserver connection is not available.');
            }
            return this.client.rpc<GxserverExportSessionTranscriptResult>('/api/exportSessionTranscript', params);
          })();
      const path = normalizeNonEmptyString(result?.path);
      if (!path) {
        throw new Error('gxserver did not return the exported file.');
      }
      const agentId = request.agentId ?? this.resolveExportTranscriptAgentId(result?.agent);
      this.pendingExportedTranscript = {
        ...(agentId ? { agentId } : {}),
        ...(request.machineId ? { machineId: request.machineId } : {}),
        path,
        projectId: request.projectId,
      };
      postAppModalHostMessage(
        {
          ...(agentId ? { agentId } : {}),
          canReveal: request.machineId === undefined,
          ok: true,
          path,
          type: 'exportSessionTranscriptResult',
        },
        'AppModals:exportTranscriptResult'
      );
    } catch (error) {
      postAppModalHostMessage(
        {
          error: error instanceof Error ? error.message : String(error),
          ok: false,
          type: 'exportSessionTranscriptResult',
        },
        'AppModals:exportTranscriptResult'
      );
    }
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

  /**
   * The export dialog's host side effects: run the export with the chosen
   * include-toggles, or start the follow-up conversation. The dialog owns only
   * the toggle and agent choices; the exported path and its project stay in
   * this runtime.
   */
  async handleGpuiExportTranscriptModalCommand(this: GpuiSidebarRuntime, payload: unknown): Promise<void> {
    if (!payload || typeof payload !== 'object' || Array.isArray(payload)) {
      return;
    }
    const record = payload as Record<string, unknown>;
    if (record.type === 'runExportSessionTranscript') {
      await this.runExportSessionTranscriptForDialog({
        includeCommands: record.includeCommands !== false,
        includePatches: record.includePatches !== false,
        includeReasoning: record.includeReasoning === true,
      });
      return;
    }
    if (record.type !== 'startExportedTranscriptConversation') {
      return;
    }
    const exported = this.pendingExportedTranscript;
    this.pendingExportedTranscript = undefined;
    this.pendingExportTranscriptRequest = undefined;
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
