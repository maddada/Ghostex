/**
 * CDXC:Daemon-lifecycle 2026-04-25-09:25
 * This protocol shape is retained for serialized terminal snapshots. The native
 * macOS host now owns terminal process creation and rendering.
 */
export const TERMINAL_HOST_PROTOCOL_VERSION = 29;

export type TerminalSessionStatus = "starting" | "running" | "exited" | "error" | "disconnected";

export type TerminalSessionRestoreState = "live" | "replayed";

export type TerminalAgentStatus = "idle" | "working" | "attention";

export type TerminalSessionSnapshot = {
  agentName?: string;
  agentStatus: TerminalAgentStatus;
  cols: number;
  cwd: string;
  exitCode?: number;
  frontendAttachmentGeneration?: number;
  firstUserMessage?: string;
  history?: string;
  isAttached: boolean;
  restoreState: TerminalSessionRestoreState;
  rows: number;
  sessionId: string;
  shell: string;
  startedAt: string;
  status: TerminalSessionStatus;
  title?: string;
  workspaceId: string;
  endedAt?: string;
  errorMessage?: string;
};

export type TerminalSessionsBySessionId = Record<string, TerminalSessionSnapshot>;

export type TerminalHostAuthenticateRequest = {
  type: "authenticate";
  token: string;
  version: typeof TERMINAL_HOST_PROTOCOL_VERSION;
};

export type TerminalHostCreateOrAttachRequest = {
  type: "createOrAttach";
  requestId: string;
  sessionId: string;
  workspaceId: string;
  workspaceRoot?: string;
  cols: number;
  cwd: string;
  rows: number;
  sessionStateFilePath: string;
  shellIntegrationBinDir?: string;
  shellIntegrationZdotDir?: string;
  shell: string;
  shellArgs?: string[];
};

export type TerminalHostWriteRequest = {
  type: "write";
  workspaceId: string;
  sessionId: string;
  data: string;
};

export type TerminalHostResizeRequest = {
  type: "resize";
  workspaceId: string;
  sessionId: string;
  cols: number;
  rows: number;
};

export type TerminalHostKillRequest = {
  type: "kill";
  workspaceId: string;
  sessionId: string;
};

export type TerminalHostAcknowledgeAttentionRequest = {
  type: "acknowledgeAttention";
  workspaceId: string;
  sessionId: string;
};

export type TerminalHostListSessionsRequest = {
  type: "listSessions";
  requestId: string;
  workspaceId?: string;
};

export type TerminalHostConfigureRequest = {
  type: "configure";
  requestId: string;
  idleShutdownTimeoutMs: number | null;
};

export type TerminalHostSyncSessionLeasesRequest = {
  type: "syncSessionLeases";
  requestId: string;
  workspaceId: string;
  sessionIds: string[];
  leaseDurationMs: number | null;
};

export type TerminalHostSyncResizeEligibleSessionsRequest = {
  type: "syncResizeEligibleSessions";
  requestId: string;
  workspaceId: string;
  sessionIds: string[];
};

export type TerminalHostHeartbeatOwnerRequest = {
  type: "heartbeatOwner";
  requestId: string;
  workspaceId: string;
  ownerId: string;
  ownerPid: number;
};

export type TerminalHostRequest =
  | TerminalHostAuthenticateRequest
  | TerminalHostCreateOrAttachRequest
  | TerminalHostWriteRequest
  | TerminalHostResizeRequest
  | TerminalHostKillRequest
  | TerminalHostAcknowledgeAttentionRequest
  | TerminalHostListSessionsRequest
  | TerminalHostConfigureRequest
  | TerminalHostSyncSessionLeasesRequest
  | TerminalHostSyncResizeEligibleSessionsRequest
  | TerminalHostHeartbeatOwnerRequest;

export type TerminalHostAuthenticatedEvent = {
  type: "authenticated";
};

export type TerminalHostResponse =
  | {
      type: "response";
      requestId: string;
      ok: true;
    }
  | {
      type: "response";
      requestId: string;
      ok: true;
      session: TerminalSessionSnapshot;
      didCreateSession: boolean;
    }
  | {
      type: "response";
      requestId: string;
      ok: true;
      sessions: TerminalSessionSnapshot[];
    }
  | {
      type: "response";
      requestId: string;
      ok: false;
      error: string;
    };

export type TerminalHostSessionOutputEvent = {
  type: "sessionOutput";
  sessionId: string;
  data: string;
};

export type TerminalHostSessionStateEvent = {
  type: "sessionState";
  session: TerminalSessionSnapshot;
};

export type TerminalHostEvent =
  | TerminalHostAuthenticatedEvent
  | TerminalHostResponse
  | TerminalHostSessionOutputEvent
  | TerminalHostSessionStateEvent;

export type TerminalInputMessage = {
  type: "terminalInput";
  sessionId: string;
  data: string;
};

export type TerminalResizeMessage = {
  type: "terminalResize";
  sessionId: string;
  cols: number;
  rows: number;
};

export type TerminalReadyMessage = {
  type: "terminalReady";
  sessionId: string;
  cols: number;
  rows: number;
};

export type TerminalStateMessage = {
  type: "terminalSessionState";
  session: TerminalSessionSnapshot;
};

export type TerminalOutputMessage = {
  type: "terminalOutput";
  sessionId: string;
  data: string;
};
