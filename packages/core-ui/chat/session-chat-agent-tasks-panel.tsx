/*
CDXC:SessionChatAgentTasks 2026-09-03:
Claude Code's task list, the block its TUI pins under the transcript:

      ◼ ⟳ MNS-40: dedupe start_adapter across four test suites
      ◻ ⌛ connection_audit_log flakes under full-suite load
      ✔ ✅ MNS-24: Close the unclassified-POST fail-open hole
        … +16 completed

gxserver reads the CLI's on-disk task store (server/src/session_chat_agent_tasks.rs)
and republishes it whenever a task appears or changes state, so this panel is
always the list the terminal is showing.

It stands directly above the composer and OUTSIDE it, next to the sub-agent
strip, for the same reason that strip does: this is the agent's plan, which
the user can read but not edit. It sits ABOVE the fleet strip because the plan
outlives any one sub-agent that is working on it.

Two folds, both the user's:
  - The whole panel collapses to its header (remembered across sessions in
    localStorage, because "I want the plan out of the way" is a preference,
    not a per-session mood). The header still names the running task, so a
    collapsed panel is a one-line status, not a blank bar.
  - Completed tasks fold behind a "+N completed" row exactly as the CLI folds
    them, keeping the most recent one visible as the "just did" marker. Open
    tasks are what the reader came for; the done pile is available on demand.

Rows are ordered running → waiting → done, then by the CLI's own numbering
inside each group, so the eye lands on what is happening now first.
*/

import { useEffect, useState } from 'react';
import { IconChevronDown, IconCircleCheckFilled, IconLoader2, IconListCheck } from '@tabler/icons-react';
import type { SessionChatAgentTask, SessionChatAgentTasks } from '../../shared/session-chat';
import { cn } from '@/packages/components/utils';

const COLLAPSED_STORAGE_KEY = 'ghostex.chat.agentTasks.collapsed';

export interface SessionChatAgentTasksPanelProps {
  /** Null or empty renders nothing: no tasks is not a state worth a box. */
  tasks: SessionChatAgentTasks | null;
}

type TaskGroup = 'in_progress' | 'pending' | 'completed';

function taskGroup(task: SessionChatAgentTask): TaskGroup {
  if (task.status === 'in_progress' || task.status === 'completed') {
    return task.status;
  }
  return 'pending';
}

function taskOrder(task: SessionChatAgentTask): number {
  const parsed = Number.parseInt(task.id, 10);
  return Number.isFinite(parsed) ? parsed : Number.MAX_SAFE_INTEGER;
}

function byCliOrder(left: SessionChatAgentTask, right: SessionChatAgentTask): number {
  return taskOrder(left) - taskOrder(right) || left.id.localeCompare(right.id);
}

function readCollapsed(): boolean {
  try {
    return window.localStorage.getItem(COLLAPSED_STORAGE_KEY) === '1';
  } catch {
    return false;
  }
}

function writeCollapsed(collapsed: boolean): void {
  try {
    if (collapsed) {
      window.localStorage.setItem(COLLAPSED_STORAGE_KEY, '1');
    } else {
      window.localStorage.removeItem(COLLAPSED_STORAGE_KEY);
    }
  } catch {
    // Storage may be unavailable (private mode); the fold still works for the session.
  }
}

export function SessionChatAgentTasksPanel({ tasks }: SessionChatAgentTasksPanelProps) {
  const [collapsed, setCollapsed] = useState(readCollapsed);
  const [showCompleted, setShowCompleted] = useState(false);
  const list = tasks?.tasks ?? [];
  // A fresh list means a fresh fold: a new plan's done pile starts closed.
  const listSignature = list.length;
  useEffect(() => {
    setShowCompleted(false);
  }, [listSignature]);

  if (list.length === 0) {
    return null;
  }

  const running = list.filter((task) => taskGroup(task) === 'in_progress').sort(byCliOrder);
  const waiting = list.filter((task) => taskGroup(task) === 'pending').sort(byCliOrder);
  const done = list.filter((task) => taskGroup(task) === 'completed').sort(byCliOrder);
  const doneCount = done.length;
  const total = list.length;
  // Only OPEN tasks can still block: a finished blocker is no longer a wait.
  const subjectById = new Map(
    list.filter((task) => taskGroup(task) !== 'completed').map((task) => [task.id, task.subject])
  );
  // The latest completed task stays visible as the "just did" marker, like the
  // CLI; the rest fold behind the count until asked for.
  const latestDone = done.length > 0 ? done[done.length - 1] : null;
  const foldedDone = done.slice(0, -1);
  const visibleDone = showCompleted ? done : latestDone ? [latestDone] : [];
  const headline = running[0] ?? waiting[0] ?? null;

  const toggleCollapsed = () => {
    const next = !collapsed;
    setCollapsed(next);
    writeCollapsed(next);
  };

  return (
    <section
      aria-label='Agent tasks'
      className={cn('ghostex-chat-agent-tasks', collapsed && 'ghostex-chat-agent-tasks-collapsed')}
      data-collapsed={collapsed ? 'true' : undefined}
    >
      <button
        aria-expanded={!collapsed}
        className='ghostex-chat-agent-tasks-header'
        onClick={toggleCollapsed}
        title={collapsed ? 'Show tasks' : 'Hide tasks'}
        type='button'
      >
        <IconListCheck aria-hidden='true' className='ghostex-chat-agent-tasks-icon' size={14} stroke={1.8} />
        <span className='ghostex-chat-agent-tasks-title'>Tasks</span>
        <span className='ghostex-chat-agent-tasks-count'>
          {doneCount}/{total}
        </span>
        <span aria-hidden='true' className='ghostex-chat-agent-tasks-bar'>
          <span
            className='ghostex-chat-agent-tasks-bar-fill'
            style={{ width: `${total === 0 ? 0 : Math.round((doneCount / total) * 100)}%` }}
          />
        </span>
        {/* Only the collapsed header carries the running task: expanded, the
            rows below say it, and saying it twice reads as a glitch. */}
        {collapsed && headline ? (
          <span className='ghostex-chat-agent-tasks-headline' title={headline.subject}>
            {headline.status === 'in_progress' ? (headline.activeForm ?? headline.subject) : headline.subject}
          </span>
        ) : (
          <span className='ghostex-chat-agent-tasks-headline' />
        )}
        <IconChevronDown aria-hidden='true' className='ghostex-chat-agent-tasks-chevron' size={14} stroke={2} />
      </button>
      {collapsed ? null : (
        <ul className='ghostex-chat-agent-tasks-rows'>
          {running.map((task) => (
            <TaskRow group='in_progress' key={task.id} subjectById={subjectById} task={task} />
          ))}
          {waiting.map((task) => (
            <TaskRow group='pending' key={task.id} subjectById={subjectById} task={task} />
          ))}
          {visibleDone.map((task) => (
            <TaskRow group='completed' key={task.id} subjectById={subjectById} task={task} />
          ))}
          {foldedDone.length > 0 ? (
            <li className='ghostex-chat-agent-tasks-fold'>
              <button
                aria-expanded={showCompleted}
                className='ghostex-chat-agent-tasks-fold-button'
                onClick={() => setShowCompleted((value) => !value)}
                type='button'
              >
                {showCompleted ? 'Hide completed' : `+${foldedDone.length} more completed`}
              </button>
            </li>
          ) : null}
        </ul>
      )}
    </section>
  );
}

function TaskRow({
  group,
  subjectById,
  task,
}: {
  group: TaskGroup;
  subjectById: Map<string, string>;
  task: SessionChatAgentTask;
}) {
  const blockers = (task.blockedBy ?? []).filter((id) => id !== task.id && subjectById.has(id));
  const blockedTitle =
    blockers.length > 0
      ? `Waits for ${blockers.map((id) => `#${id} ${subjectById.get(id) ?? ''}`.trim()).join(', ')}`
      : null;
  return (
    <li className='ghostex-chat-agent-tasks-row' data-status={group} title={blockedTitle ?? task.subject}>
      <span aria-hidden='true' className='ghostex-chat-agent-tasks-marker'>
        {group === 'in_progress' ? (
          <IconLoader2 className='ghostex-chat-agent-tasks-spinner' size={13} stroke={2} />
        ) : group === 'completed' ? (
          <IconCircleCheckFilled size={13} stroke={2} />
        ) : (
          <span className='ghostex-chat-agent-tasks-dot' />
        )}
      </span>
      <span className='ghostex-chat-agent-tasks-subject'>{task.subject}</span>
      {group === 'pending' && blockers.length > 0 ? (
        <span className='ghostex-chat-agent-tasks-blocked'>waits for #{blockers.join(', #')}</span>
      ) : null}
    </li>
  );
}
