import type { GxserverPresentationSession, GxserverSessionDomainState } from '@/packages/shared/gxserver-protocol';
import type { WorkspacePresentationState, WorkspaceSession } from '../workspace/workspace-model';

export interface SessionReference {
  machineId: string;
  projectId: string;
  sessionId: string;
}

export function createWorkspaceSessionId(reference: SessionReference): string {
  return [reference.machineId, reference.projectId, reference.sessionId]
    .map((part) => encodeURIComponent(part))
    .join('/');
}

export function presentationSessionToWorkspaceSession(
  machineId: string,
  session: GxserverPresentationSession
): WorkspaceSession {
  const reference = { machineId, projectId: session.projectId, sessionId: session.sessionId };
  return {
    ...reference,
    activity: session.activity,
    ...(session.commandId ? { commandId: session.commandId } : {}),
    ...(session.agentIcon || session.agentName || session.agentId
      ? { agentIcon: session.agentIcon ?? session.agentName ?? session.agentId }
      : {}),
    ...(session.agentId ? { agentId: session.agentId } : {}),
    ...(session.agentSessionId ? { agentSessionId: session.agentSessionId } : {}),
    ...(session.delayedSendDeadlineAt ? { delayedSendDeadlineAt: session.delayedSendDeadlineAt } : {}),
    ...(session.delayedSendRemainingLabel ? { delayedSendRemainingLabel: session.delayedSendRemainingLabel } : {}),
    ...(session.sendWhenAllProjectSessionsStopActive === true ? { sendWhenAllProjectSessionsStopActive: true } : {}),
    ...(session.sendWhenAgentStopsActive === true ? { sendWhenAgentStopsActive: true } : {}),
    ...(typeof session.queuedPromptCount === 'number' && session.queuedPromptCount > 0
      ? { queuedPromptCount: session.queuedPromptCount }
      : {}),
    ...(typeof session.queuedPromptFailedCount === 'number' && session.queuedPromptFailedCount > 0
      ? { queuedPromptFailedCount: session.queuedPromptFailedCount }
      : {}),
    presentationState: presentationStateForSession(session),
    title: session.displayTitle ?? session.title,
    workspaceId: createWorkspaceSessionId(reference),
  };
}

export function domainSessionToWorkspaceSession(
  machineId: string,
  session: GxserverSessionDomainState,
  presentationState: WorkspacePresentationState,
  statusMessage?: string
): WorkspaceSession {
  const reference = { machineId, projectId: session.projectId, sessionId: session.sessionId };
  const agentSessionId =
    typeof session.runtimeSettings.agentSessionId === 'string' ? session.runtimeSettings.agentSessionId.trim() : '';
  const agentIcon = typeof session.launchSettings.icon === 'string' ? session.launchSettings.icon.trim() : '';
  return {
    ...reference,
    activity: 'idle',
    ...(session.commandId ? { commandId: session.commandId } : {}),
    ...(session.agentId ? { agentIcon: agentIcon || session.agentId, agentId: session.agentId } : {}),
    ...(agentSessionId ? { agentSessionId } : {}),
    presentationState,
    ...(statusMessage ? { statusMessage } : {}),
    title: session.title || 'Terminal',
    workspaceId: createWorkspaceSessionId(reference),
  };
}

function presentationStateForSession(session: GxserverPresentationSession): WorkspacePresentationState {
  if (session.lifecycleState === 'sleeping') {
    return 'sleeping';
  }
  if (session.lifecycleState === 'running' && session.providerSessionState === 'exists') {
    return 'running';
  }
  if (session.lifecycleState === 'running' && session.providerSessionState === 'missing') {
    return 'restored-unmounted';
  }
  if (session.lifecycleState === 'unknown' || session.providerSessionState === 'unknown') {
    return 'mounting';
  }
  return 'startup-failed';
}
