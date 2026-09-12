import type {
  GxserverReadSessionChatResult,
  GxserverSessionChatEvent,
  SessionChatDetectedOptions,
} from '@/packages/shared/session-chat';
import { mergeSessionChatMessagesWith } from '@/packages/core-ui/chat/session-chat-merge';
import { sessionChatOptionEvidencePriority } from '@/packages/core-ui/chat/session-chat-session-options';
import { sessionChatTranscriptStatusAfterState } from '@/packages/core-ui/chat/session-chat-view-state';
import { mergeSessionChatDraftState } from '@/packages/core-ui/chat/session-chat-queue';

type StateCarrier = Exclude<GxserverSessionChatEvent, { type: 'sessionChatAppended' }> | GxserverReadSessionChatResult;

function mergeOptions(
  current: SessionChatDetectedOptions | undefined,
  incoming: SessionChatDetectedOptions | undefined
): SessionChatDetectedOptions | undefined {
  if (!incoming) return current;
  const stronger = (['model', 'effort', 'mode'] as const).some(
    (field) =>
      sessionChatOptionEvidencePriority(incoming[field]?.source) >
      sessionChatOptionEvidencePriority(current?.[field]?.source)
  );
  const chosen =
    current && !stronger && Date.parse(current.detectedAt) > Date.parse(incoming.detectedAt) ? current : incoming;
  return {
    ...chosen,
    codexStatus: chosen.codexStatus ?? incoming.codexStatus ?? current?.codexStatus,
    claudeStatus: chosen.claudeStatus ?? incoming.claudeStatus ?? current?.claudeStatus,
    contextUsage: chosen.contextUsage ?? incoming.contextUsage ?? current?.contextUsage,
  };
}

export function foldSessionChatState(
  previous: GxserverReadSessionChatResult | undefined,
  incoming: StateCarrier
): GxserverReadSessionChatResult {
  const stateOnly = 'type' in incoming && incoming.type === 'sessionChatState';
  const base = stateOnly ? previous! : (incoming as GxserverReadSessionChatResult);
  const agentChanged =
    'sessionAgentId' in incoming &&
    previous?.sessionAgentId &&
    incoming.sessionAgentId &&
    previous.sessionAgentId !== incoming.sessionAgentId;
  return {
    ...previous,
    ...base,
    messages: stateOnly
      ? base.messages
      : (mergeSessionChatMessagesWith([], base.messages) as GxserverReadSessionChatResult['messages']),
    epoch: incoming.epoch,
    seq: incoming.seq,
    status: stateOnly ? sessionChatTranscriptStatusAfterState(previous!.status, incoming.status) : incoming.status,
    lifecycle: incoming.lifecycle ?? (stateOnly ? previous?.lifecycle : undefined),
    working: incoming.working ?? previous?.working,
    prompt: incoming.prompt,
    terminalNotice: incoming.terminalNotice,
    terminalActivity: incoming.terminalActivity,
    agentFleet: incoming.agentFleet,
    agentTasks: incoming.agentTasks,
    agent: incoming.agent ?? previous?.agent,
    agentSessionId: incoming.agentSessionId ?? (stateOnly ? previous?.agentSessionId : undefined),
    selectedOptions: mergeOptions(agentChanged ? undefined : previous?.selectedOptions, incoming.selectedOptions),
    screenProbed: agentChanged ? incoming.screenProbed : incoming.screenProbed || previous?.screenProbed,
    appCommands: incoming.appCommands ?? previous?.appCommands,
    returnedPrompt:
      'returnedPrompt' in incoming ? (incoming.returnedPrompt ?? previous?.returnedPrompt) : previous?.returnedPrompt,
    accountSwitch: incoming.accountSwitch !== undefined ? incoming.accountSwitch : previous?.accountSwitch,
    pendingModelSelection:
      incoming.pendingModelSelection !== undefined ? incoming.pendingModelSelection : previous?.pendingModelSelection,
    queue: incoming.queue ?? previous?.queue,
    draft: incoming.draft
      ? (mergeSessionChatDraftState(previous?.draft ?? null, incoming.draft) ?? undefined)
      : previous?.draft,
  };
}

export function foldSessionChatAppend(
  previous: GxserverReadSessionChatResult,
  event: Extract<GxserverSessionChatEvent, { type: 'sessionChatAppended' }>
): GxserverReadSessionChatResult {
  const removed = new Set(event.supersededMessageIds ?? []);
  const messages = removed.size ? previous.messages.filter((message) => !removed.has(message.id)) : previous.messages;
  return {
    ...previous,
    epoch: event.epoch,
    seq: event.seq,
    messages: mergeSessionChatMessagesWith(messages, event.messages) as GxserverReadSessionChatResult['messages'],
    lifecycle: event.lifecycle ?? previous.lifecycle,
    status: event.messages.length ? 'ready' : previous.status,
  };
}
