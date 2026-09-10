/**
 * CDXC:SessionStatus 2026-09-10 WHY:
 * A retained roster is not evidence of continued work. Pause both providers' clocks and pulses when verification fails or fresh observations stop arriving.
 * Provider IDs keep repeated names and resumed turns linked to the exact child transcript.
 */

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { SessionChatAgentFleet } from '../../shared/session-chat';
import { formatSessionChatActivityElapsed, sessionChatActivityElapsedSeconds } from './session-chat-activity-row';
import { SessionChatSubagentLink } from './session-chat-subagent-link';
import { SessionChatSubagentModel } from './session-chat-subagent-model';

/** How often the local clocks re-render between server samples. */
const FLEET_CLOCK_TICK_MS = 1_000;

export interface SessionChatAgentFleetStripProps {
  /** Null or empty renders nothing: no sub-agents is not a state worth a box. */
  fleet: SessionChatAgentFleet | null;
}

export function SessionChatAgentFleetStrip({ fleet }: SessionChatAgentFleetStripProps) {
  const rowsRef = useRef<HTMLDivElement>(null);
  const [scrollable, setScrollable] = useState(false);
  const agentCount = fleet?.agents.length ?? 0;
  // CDXC:SessionChat 2026-09-10 WHY: Scroll animations can retain their last fade after the roster shrinks to fit; remove the mask when there is no overflow.
  useLayoutEffect(() => {
    const rows = rowsRef.current;
    if (!rows) return;
    const measure = () => setScrollable(rows.scrollHeight > rows.clientHeight);
    const observer = new ResizeObserver(measure);
    observer.observe(rows);
    measure();
    return () => observer.disconnect();
  }, [agentCount]);
  const [now, setNow] = useState(() => Date.now());
  const detectedAt = fleet?.detectedAt ?? null;
  const validUntil = fleet?.validUntil ? Date.parse(fleet.validUntil) : null;
  const stale = fleet?.stale === true || (validUntil !== null && (!Number.isFinite(validUntil) || now >= validUntil));
  // Keep checking the observation lease even when every row's elapsed clock is idle.
  useEffect(() => {
    if (detectedAt === null || stale) {
      return;
    }
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), FLEET_CLOCK_TICK_MS);
    return () => clearInterval(timer);
  }, [detectedAt, stale]);

  const agents = fleet?.agents ?? [];
  if (!fleet || agents.length === 0) {
    return null;
  }

  // Carry the captured roster, not the ticking display clock, so identical
  // agent types can be resolved against the provider's ordered launch records.
  const roster = agents.map((agent) => ({
    name: agent.name,
    startedAt:
      agent.startedAt ??
      (agent.elapsedSeconds === undefined ? null : Date.parse(fleet.detectedAt) - agent.elapsedSeconds * 1000),
  }));

  return (
    <div
      aria-label='Subagents'
      className='ghostex-chat-prompt-card ghostex-chat-agent-fleet'
      role='group'
      data-stale={stale || undefined}
    >
      <div className='ghostex-chat-agent-fleet-header'>
        {/* CDXC:SessionChat 2026-09-07 DECISION: User: the title is "Subagents", without a hyphen or all caps. */}
        <span className='ghostex-chat-card-title ghostex-chat-agent-fleet-title'>Subagents</span>
        {agents.length > 1 ? (
          <span className='ghostex-chat-card-hint [--chat-card-hint-base:0.625rem] ghostex-chat-agent-fleet-count'>
            {agents.length}
          </span>
        ) : null}
      </div>
      {stale ? (
        <div className='ghostex-chat-card-hint' role='status'>
          Subagent status unavailable
        </div>
      ) : null}
      <div ref={rowsRef} className={`ghostex-chat-agent-fleet-rows${scrollable ? ' scroll-fade-y' : ''}`} role='list'>
        {agents.map((agent, index) => {
          const idle = agent.status === 'idle';
          const working = !stale && !idle;
          const selector = agent.id ?? `fleet:${JSON.stringify({ agents: roster, index })}`;
          const transcriptTarget = {
            name: agent.task ?? agent.name,
            selector,
            agentType: agent.name,
            task: agent.task,
            model: agent.model,
            effort: agent.effort,
          };
          const elapsed = sessionChatActivityElapsedSeconds(
            {
              detectedAt: fleet.detectedAt,
              ...(agent.elapsedSeconds === undefined ? {} : { elapsedSeconds: agent.elapsedSeconds }),
            },
            working ? now : Date.parse(fleet.detectedAt)
          );
          return (
            <div
              className='ghostex-chat-agent-fleet-row'
              key={agent.id ?? `${index}:${agent.name}`}
              role='listitem'
              data-status={stale ? 'unavailable' : idle ? 'idle' : 'working'}
            >
              <span
                aria-hidden='true'
                className='ghostex-chat-agent-fleet-pulse'
                style={
                  !working ? { animation: 'none', backgroundColor: 'var(--muted-foreground)', opacity: 0.5 } : undefined
                }
              />
              <span className='ghostex-chat-card-content ghostex-chat-agent-fleet-name'>
                <SessionChatSubagentLink {...transcriptTarget}>
                  <SessionChatSubagentModel info={agent} />
                </SessionChatSubagentLink>
              </span>
              {/* Task and marker share one cell: `+2` reads as belonging to the
                  work on its left, and staying out of the clock's column keeps
                  a marked row aligned with every unmarked one. */}
              <span className='ghostex-chat-agent-fleet-work'>
                {/* CDXC:SessionChat 2026-09-10 DECISION: User: put the ‣ separator at the start of the status cell so it aligns across subagent rows regardless of model label width. */}
                {agent.task || (idle && !stale) || agent.nested ? (
                  <span aria-hidden='true' className='ghostex-chat-card-content shrink-0'>
                    ‣
                  </span>
                ) : null}
                {idle && !stale ? <span className='ghostex-chat-card-hint'>Idle</span> : null}
                <span className='ghostex-chat-card-content ghostex-chat-agent-fleet-task'>
                  {agent.task ? (
                    <SessionChatSubagentLink {...transcriptTarget}>{agent.task}</SessionChatSubagentLink>
                  ) : (
                    ''
                  )}
                </span>
                {agent.nested ? (
                  <span
                    className='ghostex-chat-card-hint [--chat-card-hint-base:0.625rem] ghostex-chat-agent-fleet-nested'
                    title={`${agent.nested} more agent${agent.nested === 1 ? '' : 's'} under this one`}
                  >
                    +{agent.nested}
                  </span>
                ) : null}
              </span>
              {/* Counter, separator and clock are three tracks, not one cell:
                  that is what right-aligns every counter on the same edge no
                  matter how long the one above it was. The separator only
                  appears when it has something on both sides of it. */}
              <span className='ghostex-chat-card-hint [--chat-card-hint-base:0.6875rem] ghostex-chat-agent-fleet-tokens'>
                {agent.tokens ?? ''}
              </span>
              <span aria-hidden='true' className='ghostex-chat-agent-fleet-separator'>
                {agent.tokens && elapsed !== null ? '•' : ''}
              </span>
              <span className='ghostex-chat-card-hint [--chat-card-hint-base:0.6875rem] ghostex-chat-agent-fleet-clock'>
                {elapsed === null ? '' : formatSessionChatActivityElapsed(elapsed)}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
