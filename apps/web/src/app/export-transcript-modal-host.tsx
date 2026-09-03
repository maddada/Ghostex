// The Export Transcript dialog for the web app. The Agent Actions row only
// publishes which session to export (`ghostex-web:exportTranscriptStatus`);
// this host — mounted once in the app shell next to the other web modal hosts
// — owns the whole flow through the shared ExportTranscriptModal: the
// include-toggle options stage, the daemon call, and the result stage.
//
// The exported markdown sits on the DAEMON's filesystem, so the browser can
// only offer the path itself (copy) or hand it to a new agent session on that
// same machine; there is no Reveal in Finder on web (`canReveal` stays false).

import { useCallback, useEffect, useRef, useState } from 'react';
import {
  ExportTranscriptModal,
  type ExportTranscriptIncludeOptions,
  type ExportTranscriptModalStage,
} from '@/packages/core-ui/export-transcript-result-modal';
import { useSidebarStore } from '@/packages/core-ui/sidebar-store';
import type { GxserverExportSessionTranscriptResult } from '@/packages/shared/gxserver-protocol';
import { rpcForMachine } from '../connections/connection-registry';
import type { GhostexWebFocusSessionDetail } from '../sidebar-runtime/sidebar-runtime';
import type { ExportTranscriptStatusDetail } from './action-events';

export function publishExportTranscriptStatus(detail: ExportTranscriptStatusDetail): void {
  window.dispatchEvent(new CustomEvent('ghostex-web:exportTranscriptStatus', { detail }));
}

/*
CDXC:Drafts 2026-08-20:
Plan 015 §7: the follow-up conversation is never given a prompt on the user's
behalf. gxserver types this draft into the new session's composer once and never
submits it, so all we stage is a mention of the exported markdown. The trailing
space is load-bearing — it separates the mention from the prompt the user types
after it — so the value must reach gxserver verbatim, untrimmed.
*/
function transcriptMentionDraft(path: string): string {
  return `@${path} `;
}

export function ExportTranscriptModalHost() {
  const [detail, setDetail] = useState<ExportTranscriptStatusDetail>();
  const [stage, setStage] = useState<ExportTranscriptModalStage>({ stage: 'options' });
  const [starting, setStarting] = useState(false);
  const [actionError, setActionError] = useState<string>();
  const requestSequence = useRef(0);
  // Hydrated by the web sidebar runtime's hydrate/sessionState messages, the
  // same store the shared sidebar UI reads. Used for the Continue-with picker.
  const agents = useSidebarStore((state) => state.hud.agents);

  const close = useCallback(() => {
    requestSequence.current += 1;
    setDetail(undefined);
  }, []);

  useEffect(() => {
    const onStatus = (event: WindowEventMap['ghostex-web:exportTranscriptStatus']) => {
      requestSequence.current += 1;
      setStage({ stage: 'options' });
      setStarting(false);
      setActionError(undefined);
      setDetail(event.detail);
    };
    window.addEventListener('ghostex-web:exportTranscriptStatus', onStatus);
    window.addEventListener('ghostex-web:closeAppModal', close);
    return () => {
      window.removeEventListener('ghostex-web:exportTranscriptStatus', onStatus);
      window.removeEventListener('ghostex-web:closeAppModal', close);
    };
  }, [close]);

  if (!detail) {
    return null;
  }

  const runExport = async (options: ExportTranscriptIncludeOptions) => {
    const requestId = ++requestSequence.current;
    setActionError(undefined);
    setStage({ stage: 'exporting' });
    /*
    The export can fail for reasons the user has to read (unsupported agent,
    transcript not found yet), so the structured gxserver message lands on the
    dialog's failed stage instead of the console-only path other actions take.
    */
    try {
      const result = await rpcForMachine<GxserverExportSessionTranscriptResult>(
        detail.machineId,
        '/api/exportSessionTranscript',
        {
          includeCommands: options.includeCommands,
          includePatches: options.includePatches,
          includeReasoning: options.includeReasoning,
          projectId: detail.projectId,
          sessionId: detail.sessionId,
        }
      );
      if (requestSequence.current !== requestId) {
        return;
      }
      setStage({
        ...(detail.agentId ? { agentId: detail.agentId } : {}),
        canReveal: false,
        path: result.path,
        stage: 'done',
      });
    } catch (error: unknown) {
      if (requestSequence.current !== requestId) {
        return;
      }
      setStage({ message: error instanceof Error ? error.message : String(error), stage: 'failed' });
    }
  };

  const startNewConversation = async (path: string, agentId: string) => {
    const requestId = ++requestSequence.current;
    setActionError(undefined);
    setStarting(true);
    try {
      const agentName = agents.find((agent) => agent.agentId === agentId)?.name ?? agentId;
      const { session } = await rpcForMachine<{
        session?: { projectId?: string; sessionId?: string };
      }>(detail.machineId, '/api/createAgentSession', {
        agentId,
        projectId: detail.projectId,
        requireLaunchCommand: true,
        runtimeSettings: { firstUserInputDraft: transcriptMentionDraft(path) },
        surface: 'workspace',
        title: `${agentName} Session`,
      });
      const sessionId = session?.sessionId;
      if (!sessionId) {
        throw new Error('gxserver created the session without reporting its id.');
      }
      if (requestSequence.current !== requestId) {
        return;
      }
      const focusDetail: GhostexWebFocusSessionDetail = {
        machineId: detail.machineId,
        placement: 'focusedPane',
        placementTargetSessionId: detail.sessionId,
        projectId: session?.projectId ?? detail.projectId,
        sessionId,
        source: 'sidebar',
      };
      window.dispatchEvent(new CustomEvent('ghostex-web:focusSession', { detail: focusDetail }));
      close();
    } catch (error: unknown) {
      if (requestSequence.current !== requestId) {
        return;
      }
      setStarting(false);
      setActionError(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <ExportTranscriptModal
      actionErrorMessage={actionError}
      agents={agents}
      defaultAgentId={detail.agentId}
      isOpen
      onClose={close}
      onExport={(options) => {
        void runExport(options);
      }}
      onStartNewConversation={(agentId) => {
        if (stage.stage === 'done') {
          void startNewConversation(stage.path, agentId);
        }
      }}
      stage={stage}
      startBusy={starting}
    />
  );
}
