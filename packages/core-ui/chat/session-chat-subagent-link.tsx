import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';
import type {
  SessionChatSubagentInfo,
  SessionChatToolCallBlock,
  SessionChatToolResultBlock,
} from '@/packages/shared/session-chat';
import { AppTooltip } from '../app-tooltip';

export interface SessionChatSubagentTarget {
  selector: string;
  name: string;
  agentType?: string;
  task?: string;
  model?: string;
  effort?: string;
}

export const SessionChatSubagentContext = createContext<{
  open: (target: SessionChatSubagentTarget) => void;
  readInfo?: (selector: string) => Promise<SessionChatSubagentInfo>;
  agentPath?: string;
} | null>(null);

/** CDXC:Tooltips 2026-09-10 DECISION: User: subagent transcript links use the same styled tooltip as chat skill references; show the agent type in the same regular font as "View subagent transcript", with no model name. */
export function SessionChatSubagentLink({
  selector,
  name,
  agentType,
  task,
  model,
  effort,
  children,
}: SessionChatSubagentTarget & { children?: ReactNode }) {
  const viewer = useContext(SessionChatSubagentContext);
  const [hovered, setHovered] = useState(false);
  const [info, setInfo] = useState<SessionChatSubagentInfo | null>(null);
  const readInfo = viewer?.readInfo;
  useEffect(() => {
    if (!hovered || !readInfo) return;
    let cancelled = false;
    setInfo(null);
    void readInfo(selector).then(
      (next) => {
        if (!cancelled) setInfo(next);
      },
      () => {
        if (!cancelled) setInfo(null);
      }
    );
    return () => {
      cancelled = true;
    };
  }, [hovered, readInfo, selector]);
  if (!viewer || selector === '/root' || selector === viewer.agentPath) return <>{children ?? name}</>;
  return (
    <AppTooltip
      content={
        <div className='space-y-2'>
          <div>{info?.agentType ?? agentType ?? name}</div>
          <div>View subagent transcript</div>
        </div>
      }
      onOpenChange={setHovered}
      side='top'
    >
      <button
        className='ghostex-chat-subagent-link'
        type='button'
        aria-haspopup='dialog'
        aria-label={`View ${name}'s transcript`}
        onClick={(event) => {
          event.stopPropagation();
          viewer.open({ name, selector, agentType, task, model, effort });
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
  return selector
    ? {
        selector,
        name: name ?? selector,
        agentType: text(input?.subagent_type) ?? text(input?.agent_type),
        task: text(input?.description),
      }
    : null;
}
