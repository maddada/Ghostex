import { IconCloud, IconDeviceDesktop } from "@tabler/icons-react";
import { CloudSessionButtons } from "./cloud-sessions";
import { LiveDot } from "./components";
import { relativeTime } from "./format";
import type { TeamSession } from "./types";

const STATUS_TEXT: Record<TeamSession["status"], string> = {
  starting: "Starting",
  running: "Running",
  failed: "Failed",
  cancelled: "Cancelled",
};

/**
 * The ticket's sessions the team's Convex project knows that are not in this sidebar: a
 * teammate's session, or one of yours in the cloud or on another computer.
 *
 * CDXC:WorkMode 2026-10-09 DECISION:
 * User: watching teammates' conversations is postponed ("we'll need to implement this well
 * later"), so a team session shows who runs it and where, with "Open in Claude" for a cloud
 * session's own page, and no transcript view.
 */
export function TeamSessionRows({
  sessions,
  now,
  onOpenUrl,
  onOpenInTerminal,
}: {
  sessions: TeamSession[];
  now: number;
  onOpenUrl: (url: string) => void;
  onOpenInTerminal: (url: string) => void;
}) {
  return (
    <>
      {sessions.map((session) => {
        const who = session.isMe ? "You" : (session.memberName ?? "A teammate");
        const where = session.isMe
          ? "another computer"
          : session.memberName
            ? `${session.memberName}’s computer`
            : "a teammate’s computer";
        const live =
          session.status === "running" || session.status === "starting";
        return (
          <div
            key={session.id}
            className={`w-convo team-session team-session-${session.runPlace}`}
          >
            <span className="w-convo-icon">
              {session.runPlace === "cloud" ? (
                <IconCloud size={13} />
              ) : (
                <IconDeviceDesktop size={13} />
              )}
            </span>
            <div className="w-convo-main">
              <div className="w-convo-who">
                {who}
                <span className="w-faint">
                  {session.runPlace === "cloud"
                    ? "Claude in the cloud"
                    : `on ${where}`}
                </span>
              </div>
              <div className="w-convo-sub">
                {STATUS_TEXT[session.status]}
                {session.branch ? (
                  <>
                    {" · "}
                    <span className="w-mono">{session.branch}</span>
                  </>
                ) : null}
                {` · ${relativeTime(new Date(session.updatedAt).toISOString(), now)}`}
                {session.status === "failed" && session.error
                  ? ` · ${session.error}`
                  : ""}
              </div>
            </div>
            <div className="w-convo-right">
              {live ? <LiveDot /> : null}
              {session.runPlace === "cloud" && session.sessionUrl ? (
                <CloudSessionButtons
                  sessionUrl={session.sessionUrl}
                  onOpenUrl={onOpenUrl}
                  onOpenInTerminal={onOpenInTerminal}
                />
              ) : null}
            </div>
          </div>
        );
      })}
    </>
  );
}
