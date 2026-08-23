/*
CDXC:SessionChatAgentFleet 2026-08-23:
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
starts at the same x no matter how long the names above it are. The name track
sizes to the widest name and stops; every cell is therefore rendered even when
empty, because a missing cell would shift that row's remaining columns out of
the shared alignment.

Every row's clock ticks LOCALLY from the fleet's `detectedAt`. The server holds
that timestamp still while the roster is unchanged and deliberately omits the
per-second token counter, so the strip must interpolate or it would read as
frozen between the probes that actually change something.
*/

import { useEffect, useState } from "react";
import type { SessionChatAgentFleet } from "../../shared/session-chat";
import {
  formatSessionChatActivityElapsed,
  sessionChatActivityElapsedSeconds,
} from "./session-chat-activity-row";

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

  return (
    <div aria-label="Sub-agents" className="ghostex-chat-agent-fleet" role="group">
      <div className="ghostex-chat-agent-fleet-header">
        <span className="ghostex-chat-agent-fleet-title">
          {agents.length === 1 ? "Sub-agent" : "Sub-agents"}
        </span>
        {agents.length > 1 ? (
          // The singular title already says "one"; a `1` beside it is noise.
          <span className="ghostex-chat-agent-fleet-count">{agents.length}</span>
        ) : null}
      </div>
      <div className="ghostex-chat-agent-fleet-rows" role="list">
        {agents.map((agent, index) => {
          const elapsed = sessionChatActivityElapsedSeconds(
            { detectedAt: fleet.detectedAt, ...(agent.elapsedSeconds === undefined ? {} : { elapsedSeconds: agent.elapsedSeconds }) },
            now,
          );
          return (
            <div
              className="ghostex-chat-agent-fleet-row"
              // Claude runs several agents of one type at once, so the name is
              // not a key. Position is: the block keeps screen order.
              key={`${index}:${agent.name}`}
              role="listitem"
            >
              <span aria-hidden="true" className="ghostex-chat-agent-fleet-pulse" />
              <span className="ghostex-chat-agent-fleet-name" title={agent.name}>
                {agent.name}
              </span>
              {/* Task and marker share one cell: `+2` reads as belonging to the
                  work on its left, and staying out of the clock's column keeps
                  a marked row aligned with every unmarked one. */}
              <span className="ghostex-chat-agent-fleet-work">
                <span className="ghostex-chat-agent-fleet-task" title={agent.task ?? ""}>
                  {agent.task ?? ""}
                </span>
                {agent.nested ? (
                  <span
                    className="ghostex-chat-agent-fleet-nested"
                    title={`${agent.nested} more agent${agent.nested === 1 ? "" : "s"} under this one`}
                  >
                    +{agent.nested}
                  </span>
                ) : null}
              </span>
              <span className="ghostex-chat-agent-fleet-clock">
                {elapsed === null ? "" : formatSessionChatActivityElapsed(elapsed)}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
