import { createContext, useContext, type ReactNode } from 'react';
import type { SessionChatToolCallBlock, SessionChatToolResultBlock } from '@/packages/shared/session-chat';
import { AppTooltip } from '../app-tooltip';

export interface SessionChatSubagentTarget {
  selector: string;
  name: string;
}

export const SessionChatSubagentContext = createContext<{
  open: (target: SessionChatSubagentTarget) => void;
  agentPath?: string;
} | null>(null);

/** CDXC:Tooltips 2026-09-09 DECISION: User: subagent transcript links use the same styled tooltip as chat skill references. */
export function SessionChatSubagentLink({
  selector,
  name,
  children,
}: SessionChatSubagentTarget & { children?: ReactNode }) {
  const viewer = useContext(SessionChatSubagentContext);
  if (!viewer || selector === '/root' || selector === viewer.agentPath) return <>{children ?? name}</>;
  return (
    <AppTooltip content='View subagent transcript' side='top'>
      <button
        className='ghostex-chat-subagent-link'
        type='button'
        aria-haspopup='dialog'
        aria-label={`View ${name}'s transcript`}
        onClick={(event) => {
          event.stopPropagation();
          viewer.open({ name, selector });
        }}
      >
        {children ?? name}
      </button>
    </AppTooltip>
  );
}

function record(value: unknown): Record<string, unknown> | null {
  if (typeof value === 'string') {
    try {
      return record(JSON.parse(value));
    } catch {
      return null;
    }
  }
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function text(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() ? value.trim() : undefined;
}

export function sessionChatToolSubagent(
  call: SessionChatToolCallBlock | undefined,
  result: SessionChatToolResultBlock | undefined,
  agentPath = '/root'
): SessionChatSubagentTarget | null {
  const tool = call?.name.split(/[.:]/).at(-1)?.toLowerCase();
  if (!tool || !['spawn_agent', 'agent', 'task', 'send_message', 'followup_task'].includes(tool)) return null;
  const input = record(call?.input);
  const output = record(result?.output);
  if (tool === 'send_message' || tool === 'followup_task') {
    const target = text(input?.target) ?? text(input?.id);
    return target
      ? {
          name: target.split('/').at(-1) ?? target,
          selector: target.startsWith('/') || target === input?.id ? target : `${agentPath}/${target}`,
        }
      : null;
  }
  const task = text(input?.task_name);
  const name = task ?? text(input?.name) ?? text(input?.description) ?? text(output?.agent_nickname);
  const id =
    text(output?.agent_id) ?? text(output?.agentId) ?? /\bagentId:\s*([a-zA-Z0-9_-]+)/.exec(result?.output ?? '')?.[1];
  const selector =
    id ?? text(output?.task_name) ?? (task ? (task.startsWith('/') ? task : `${agentPath}/${task}`) : name);
  return selector ? { selector, name: name ?? selector } : null;
}
