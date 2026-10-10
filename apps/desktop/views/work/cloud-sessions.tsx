import { IconCloud, IconTerminal2 } from "@tabler/icons-react";
import { Button } from "./components";
import { relativeTime } from "./format";
import type { CloudSessionRecord } from "./types";

/** Open in terminal and Open in Claude, for any cloud session's row (yours or the team's). */
export function CloudSessionButtons({
  sessionUrl,
  onOpenUrl,
  onOpenInTerminal,
}: {
  sessionUrl: string;
  onOpenUrl: (url: string) => void;
  onOpenInTerminal: (url: string) => void;
}) {
  return (
    <>
      <Button
        size="sm"
        className="open-in-terminal"
        title="Attach to it in a new terminal session (claude --cloud)"
        onClick={() => onOpenInTerminal(sessionUrl)}
      >
        <IconTerminal2 size={13} />
        Open in terminal
      </Button>
      <Button
        size="sm"
        className="open-in-claude"
        onClick={() => onOpenUrl(sessionUrl)}
      >
        Open in Claude
      </Button>
    </>
  );
}

/**
 * The cloud sessions you started on this ticket from this computer (server/src/work_mode/
 * cloud_work.rs). They are never in the sidebar, so the row says where they run instead.
 */
export function CloudSessionRows({
  sessions,
  now,
  onOpenUrl,
  onOpenInTerminal,
}: {
  sessions: CloudSessionRecord[];
  now: number;
  onOpenUrl: (url: string) => void;
  onOpenInTerminal: (url: string) => void;
}) {
  return (
    <>
      {sessions.map((session) => (
        <div key={session.sessionUrl} className="w-convo cloud-session">
          <span className="w-convo-icon">
            <IconCloud size={13} />
          </span>
          <div className="w-convo-main">
            <div className="w-convo-who">
              {session.providerName ?? "Claude Code"} in the cloud
              <span className="w-faint">started by you</span>
            </div>
            <div className="w-convo-sub">
              {session.branch ? (
                <>
                  <span className="w-mono">{session.branch}</span>
                  {" · "}
                </>
              ) : null}
              {relativeTime(new Date(session.startedAt).toISOString(), now)}
            </div>
          </div>
          <div className="w-convo-right">
            <CloudSessionButtons
              sessionUrl={session.sessionUrl}
              onOpenUrl={onOpenUrl}
              onOpenInTerminal={onOpenInTerminal}
            />
          </div>
        </div>
      ))}
    </>
  );
}
