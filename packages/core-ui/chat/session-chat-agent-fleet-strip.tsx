/*
CDXC:AgentScreenDetection 2026-08-23:
The sub-agents Claude is running, lifted off its terminal screen and put where
the chat can see them. gxserver parses the block the CLI pins below its
statusline (server/src/session_chat_agent_fleet.rs); nothing about it reaches
transcript JSONL, so without this the chat can only say "the agent is working"
while three agents are working.

It stands directly above the composer and OUTSIDE it, deliberately. The queue
strip lives inside the composer container because a queued prompt is part of
what the user is about to say; this is the opposite — work already underway,
owned by the agent, that the user cannot edit. Two strips, two containers, no
competition for the same row shape.

Rows lay out on ONE grid rather than each flexing on its own, so every task
starts at the same x no matter how long the names above it are, and every token
counter ends on the same edge. The name track sizes to the widest name and
stops; every cell is therefore rendered even when empty, because a missing cell
would shift that row's remaining columns out of the shared alignment.

A narrow pane drops whole columns instead of squeezing the task into nothing —
counter first, then clock, then name — because the task is the only part that
says what is actually happening. Those steps are container queries in chat.css;
the transcript link's accessible label still names the agent after the name
column is gone.

Every row's clock ticks LOCALLY from `detectedAt`, which gxserver mints with the
seconds it belongs to. It republishes a fleet only when the roster or a token
counter moves, never for a clock, so without interpolating here the times would
sit frozen between the samples that actually changed something.
*/

import { useEffect, useState } from 'react';
import type { SessionChatAgentFleet } from '../../shared/session-chat';
import { formatSessionChatActivityElapsed, sessionChatActivityElapsedSeconds } from './session-chat-activity-row';
import { SessionChatSubagentLink } from './session-chat-subagent-link';

/** How often the local clocks re-render between server samples. */
const FLEET_CLOCK_TICK_MS = 1_000;

export interface SessionChatAgentFleetStripProps {
  /** Null or empty renders nothing: no sub-agents is not a state worth a box. */
  fleet: SessionChatAgentFleet | null;
}

export function SessionChatAgentFleetStrip({ fleet }: SessionChatAgentFleetStripProps) {
  const [now, setNow] = useState(() => Date.now());
  const detectedAt = fleet?.detectedAt ?? null;
  // Only run a timer while there is a roster whose clocks can advance.
  useEffect(() => {
    if (detectedAt === null) {
      return;
    }
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), FLEET_CLOCK_TICK_MS);
    return () => clearInterval(timer);
  }, [detectedAt]);

  const agents = fleet?.agents ?? [];
  if (!fleet || agents.length === 0) {
    return null;
  }

  // Carry the captured roster, not the ticking display clock, so identical
  // agent types can be resolved against the provider's ordered launch records.
  const roster = agents.map((agent) => ({
    name: agent.name,
    startedAt: agent.elapsedSeconds === undefined ? null : Date.parse(fleet.detectedAt) - agent.elapsedSeconds * 1000,
  }));

  return (
    <div aria-label='Subagents' className='ghostex-chat-prompt-card ghostex-chat-agent-fleet' role='group'>
      <div className='ghostex-chat-agent-fleet-header'>
        {/* CDXC:SessionChat 2026-09-07 DECISION: User: the title is "Subagents", without a hyphen or all caps. */}
        <span className='ghostex-chat-card-title ghostex-chat-agent-fleet-title'>Subagents</span>
        {agents.length > 1 ? (
          <span className='ghostex-chat-card-hint [--chat-card-hint-base:0.625rem] ghostex-chat-agent-fleet-count'>
            {agents.length}
          </span>
        ) : null}
      </div>
      <div className='ghostex-chat-agent-fleet-rows' role='list'>
        {agents.map((agent, index) => {
          const selector = `fleet:${JSON.stringify({ agents: roster, index })}`;
          const transcriptName = agent.task ? `${agent.name}: ${agent.task}` : agent.name;
          const elapsed = sessionChatActivityElapsedSeconds(
            {
              detectedAt: fleet.detectedAt,
              ...(agent.elapsedSeconds === undefined ? {} : { elapsedSeconds: agent.elapsedSeconds }),
            },
            now
          );
          return (
            <div
              className='ghostex-chat-agent-fleet-row'
              // Claude runs several agents of one type at once, so the name is
              // not a key. Position is: the block keeps screen order.
              key={`${index}:${agent.name}`}
              role='listitem'
            >
              <span aria-hidden='true' className='ghostex-chat-agent-fleet-pulse' />
              <span className='ghostex-chat-card-content ghostex-chat-agent-fleet-name'>
                <SessionChatSubagentLink name={transcriptName} selector={selector}>
                  {agent.name}
                </SessionChatSubagentLink>
              </span>
              {/* Task and marker share one cell: `+2` reads as belonging to the
                  work on its left, and staying out of the clock's column keeps
                  a marked row aligned with every unmarked one. */}
              <span className='ghostex-chat-agent-fleet-work'>
                <span className='ghostex-chat-card-content ghostex-chat-agent-fleet-task'>
                  {agent.task ? (
                    <SessionChatSubagentLink name={transcriptName} selector={selector}>
                      {agent.task}
                    </SessionChatSubagentLink>
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
