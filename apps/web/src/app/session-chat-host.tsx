// Mounts the shared SessionChatView for a workspace session. The transport is
// memoized per (machineId, projectId, sessionId) so the chat hook's
// subscription survives unrelated re-renders. Chat styles (.ghostex-chat-*)
// live in packages/core-ui/styles/chat.css, pulled in through the shared sheet below
// (already loaded app-wide by WebSidebar; the duplicate import dedupes).
//
// The top-right Agent Actions row mirrors the gpui terminal overlay, limited
// to the actions the web app can actually execute against gxserver: Rename
// (/api/requestSessionRename via an inline input), Sleep, Fork (focuses the
// created session like the sidebar fork), Full Reload (sleep→wake, the same
// composition gpui uses), and Export Transcript (the daemon writes the
// markdown and ExportTranscriptModalHost shows the result). Delayed Actions
// is also exposed beside the chat composer through the web modal host. Prompt
// Editor, Stash Prompt, the Prompts modal, and Attach File or Folder need
// native pickers, terminal buffer access, or modal hosts the web app does not
// have.

import { useEffect, useMemo, useState } from 'react';
import type {
  GxserverForkSessionResult,
  GxserverSessionRenameRequestResult,
} from '@/packages/shared/gxserver-protocol';
import {
  resolveSessionChatDisplayAgent,
  resolveSessionChatTranscriptAgent,
} from '@/packages/shared/session-chat';
import { getSidebarAgentIconById } from '@/packages/shared/sidebar-agents';
import { openAppModal } from '@/packages/core-ui/app-modal-host-bridge';
import { SessionChatView, type SessionChatHostActions } from '@/packages/core-ui/chat/session-chat-view';
import '@/packages/core-ui/styles.css';
import { rpcForMachine } from '../connections/connection-registry';
import type { GhostexWebFocusSessionDetail } from '../sidebar-runtime/sidebar-runtime';
import { createSidebarSessionId } from '../sidebar-runtime/sidebar-ids';
import type { WorkspaceSession } from '../workspace/workspace-model';
import { createSessionChatTransport } from '../chat/session-chat-transport';
import type { ExportTranscriptSessionRef } from './action-events';
import { publishExportTranscriptStatus } from './export-transcript-modal-host';
import { readWebSettings, WEB_SETTINGS_CHANGED_EVENT } from './web-settings';

const CHAT_ACTION_REASON = 'ghostex-web-chat';

function openSessionDelayedActions(session: WorkspaceSession): void {
  const agentIcon = getSidebarAgentIconById(
    resolveSessionChatTranscriptAgent(session.agentId, session.agentIcon) ?? undefined
  );
  openAppModal({
    ...(agentIcon ? { agentIcon } : {}),
    ...(session.delayedSendDeadlineAt ? { delayedSendDeadlineAt: session.delayedSendDeadlineAt } : {}),
    ...(session.delayedSendRemainingLabel ? { delayedSendRemainingLabel: session.delayedSendRemainingLabel } : {}),
    modal: 'delayedSend',
    sendWhenAllProjectSessionsStopActive: session.sendWhenAllProjectSessionsStopActive === true,
    sendWhenAgentStopsActive: session.sendWhenAgentStopsActive === true,
    sessionId: createSidebarSessionId(session.machineId, session.projectId, session.sessionId),
    supportsSendWhenAgentStops: true,
    supportsSendWhenAllProjectSessionsStop: true,
    title: session.title,
    type: 'open',
  });
}

async function runChatAgentAction(session: WorkspaceSession, actionId: string, value?: string): Promise<void> {
  const lifecycleParams = {
    projectId: session.projectId,
    reason: CHAT_ACTION_REASON,
    sessionId: session.sessionId,
  };
  switch (actionId) {
    case 'rename': {
      const title = value?.trim() ?? '';
      if (title === '') {
        return;
      }
      const result = await rpcForMachine<GxserverSessionRenameRequestResult>(
        session.machineId,
        '/api/requestSessionRename',
        {
          ...(session.agentId ? { agentName: session.agentId } : {}),
          ...lifecycleParams,
          title,
          titleSource: 'user',
        }
      );
      /*
      CDXC:GPUISidebarRename 2026-07-28 (web chat variant):
      Agent-session renames stay pending until the Agent CLI itself renames,
      so the client must stage `/rename <title>` (Pi: `/name`) into the TUI.
      gpui types it into the mounted terminal; here the session-chat send
      endpoint delivers the same keystrokes server-side, which also works
      while the terminal is parked behind the chat surface.
      */
      if (result.shouldSendAgentRenameCommand) {
        const command = (session.agentId ?? '').trim().toLowerCase() === 'pi' ? 'name' : 'rename';
        await rpcForMachine(session.machineId, '/api/sendSessionChatMessage', {
          projectId: session.projectId,
          sessionId: session.sessionId,
          text: `/${command} ${title}`,
        });
      }
      return;
    }
    case 'sleep':
      await rpcForMachine(session.machineId, '/api/sleepSession', lifecycleParams);
      return;
    case 'fork': {
      const result = await rpcForMachine<GxserverForkSessionResult>(
        session.machineId,
        '/api/forkSession',
        lifecycleParams
      );
      const detail: GhostexWebFocusSessionDetail = {
        machineId: session.machineId,
        projectId: result.session.projectId,
        sessionId: result.session.sessionId,
        placement: 'focusedPane',
        placementTargetSessionId: session.sessionId,
        source: 'sidebar',
      };
      window.dispatchEvent(new CustomEvent('ghostex-web:focusSession', { detail }));
      return;
    }
    case 'fullReload':
      await rpcForMachine(session.machineId, '/api/sleepSession', lifecycleParams);
      await rpcForMachine(session.machineId, '/api/wakeSession', lifecycleParams);
      return;
    case 'exportTranscript': {
      /*
      CDXC:ExportTranscriptOptions 2026-08-24:
      The action only opens the Export Transcript dialog on its include-toggle
      options stage; ExportTranscriptModalHost runs the daemon call once the
      user confirms and renders the structured result or failure.
      */
      const target: ExportTranscriptSessionRef = {
        ...(session.agentId ? { agentId: session.agentId } : {}),
        machineId: session.machineId,
        projectId: session.projectId,
        sessionId: session.sessionId,
        sessionTitle: session.title,
      };
      publishExportTranscriptStatus({ ...target, status: 'requested' });
      return;
    }
    default:
      return;
  }
}

/**
 * The cluster contract for a web workspace session: the primary switch button
 * plus the gxserver-backed action row. Shared by the chat layer (switch =
 * back to terminal) and the terminal layer's floating overlay (switch = to
 * chat) so both surfaces offer the identical Agent Actions row.
 */
export function createWebSessionHostActions(
  session: WorkspaceSession,
  onSwitchSurface: () => void
): SessionChatHostActions {
  return {
    onSwitchToTerminal: onSwitchSurface,
    actions: [
      {
        id: 'rename',
        label: 'Rename',
        input: { initialValue: session.title, placeholder: 'Session name' },
      },
      { id: 'sleep', label: 'Sleep' },
      { id: 'fork', label: 'Fork' },
      { id: 'fullReload', label: 'Full Reload' },
      { id: 'exportTranscript', label: 'Export Transcript' },
    ],
    onAction: (id, value) => {
      runChatAgentAction(session, id, value).catch((error: unknown) => {
        console.error(`[ghostex-web chat] ${id} action failed`, error);
      });
    },
  };
}

export function SessionChatHost({
  onSwitchToTerminal,
  session,
}: {
  onSwitchToTerminal?: () => void;
  session: WorkspaceSession;
}) {
  const [chatSettings, setChatSettings] = useState(readWebSettings);
  useEffect(() => {
    const handleSettingsChanged = (event: Event) => {
      setChatSettings((event as CustomEvent<ReturnType<typeof readWebSettings>>).detail);
    };
    window.addEventListener(WEB_SETTINGS_CHANGED_EVENT, handleSettingsChanged);
    return () => window.removeEventListener(WEB_SETTINGS_CHANGED_EVENT, handleSettingsChanged);
  }, []);
  const transport = useMemo(
    () => createSessionChatTransport(session.machineId, session.projectId, session.sessionId),
    [session.machineId, session.projectId, session.sessionId]
  );
  const agentLabel = resolveSessionChatDisplayAgent(session.agentId, session.agentIcon) ?? session.agentId ?? null;
  const hostActions = useMemo<SessionChatHostActions | undefined>(
    () => (onSwitchToTerminal ? createWebSessionHostActions(session, onSwitchToTerminal) : undefined),
    [onSwitchToTerminal, session]
  );
  return (
    <SessionChatView
      agentLabel={agentLabel}
      canSend={session.presentationState === 'running'}
      className='workspace-session-chat'
      hostActions={hostActions}
      // Served from node_modules in dev and copied into dist by the vite
      // config's monaco plugin.
      monacoVsBaseUrl='/monaco/vs'
      onDelayedActions={() => openSessionDelayedActions(session)}
      sessionKey={`${session.machineId}:${session.projectId}:${session.sessionId}`}
      theme={chatSettings.sessionChatTheme}
      transport={transport}
      verboseMode={chatSettings.sessionChatVerboseMode}
      working={session.activity === 'working'}
    />
  );
}
