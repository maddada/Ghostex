import type { GxserverProjectId, GxserverSessionId } from "@/packages/shared/gxserver-protocol";
import { SessionTerminal } from "./session-terminal";

const DEV_PROJECT_ID = "P0dev" as GxserverProjectId;
const DEV_SESSION_ID = "G0dev" as GxserverSessionId;

export interface TerminalDevHarnessProps {
  authToken?: string;
  baseUrl?: string;
  projectId?: GxserverProjectId;
  sessionId?: GxserverSessionId;
}

export function TerminalDevHarness({
  authToken: providedAuthToken,
  baseUrl = window.location.origin,
  projectId = DEV_PROJECT_ID,
  sessionId = DEV_SESSION_ID,
}: TerminalDevHarnessProps = {}) {
  const authToken =
    providedAuthToken ?? new URLSearchParams(window.location.search).get("authToken");
  if (!authToken) {
    return <p>Pass the isolated gxserver token as ?authToken=… to use the terminal harness.</p>;
  }
  return (
    <SessionTerminal
      authToken={authToken}
      autoFocus
      baseUrl={baseUrl}
      projectId={projectId}
      sessionId={sessionId}
    />
  );
}
