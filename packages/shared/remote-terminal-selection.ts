export type RemoteProjectReference = {
  machineId: string;
  projectId: string;
};

export type RemoteTerminalSessionReference = RemoteProjectReference & {
  sessionId: string;
};

const REMOTE_PROJECT_ID_PATTERN = /^remote:([^:]+):project:(.+)$/u;
const REMOTE_SESSION_ID_PATTERN = /^remote:([^:]+):session:([^:]+):(.+)$/u;

export function createRemoteProjectId(reference: RemoteProjectReference): string {
  return `remote:${reference.machineId}:project:${reference.projectId}`;
}

export function createRemoteTerminalSessionId(reference: RemoteTerminalSessionReference): string {
  return `remote:${reference.machineId}:session:${reference.projectId}:${reference.sessionId}`;
}

export function parseRemoteProjectId(projectId: string): RemoteProjectReference | undefined {
  const match = REMOTE_PROJECT_ID_PATTERN.exec(projectId);
  if (!match) {
    return undefined;
  }
  return { machineId: match[1]!, projectId: match[2]! };
}

export function parseRemoteTerminalSessionId(sessionId: string): RemoteTerminalSessionReference | undefined {
  const match = REMOTE_SESSION_ID_PATTERN.exec(sessionId);
  if (!match) {
    return undefined;
  }
  return { machineId: match[1]!, projectId: match[2]!, sessionId: match[3]! };
}

export type ActiveTerminalSelection =
  ({ remote: false } & { projectId: string; sessionId: string }) | ({ remote: true } & RemoteTerminalSessionReference);

/**
 * Resolves the active terminal identity carried by the GPUI presentation-focus
 * contract. Remote ids must agree on machine and project; mixed or stale focus
 * state is rejected instead of selecting a terminal from another project.
 * This contract is host-neutral so the browser workspace can consume the same
 * selection when it gains a Companion surface.
 */
export function resolveActiveTerminalSelection(input: {
  activeProjectId?: string;
  focusedSessionId?: string;
}): ActiveTerminalSelection | undefined {
  const { activeProjectId, focusedSessionId } = input;
  if (!activeProjectId || !focusedSessionId) {
    return undefined;
  }
  const remoteProject = parseRemoteProjectId(activeProjectId);
  const remoteSession = parseRemoteTerminalSessionId(focusedSessionId);
  if (remoteProject || remoteSession) {
    if (
      !remoteProject ||
      !remoteSession ||
      remoteProject.machineId !== remoteSession.machineId ||
      remoteProject.projectId !== remoteSession.projectId
    ) {
      return undefined;
    }
    return { ...remoteSession, remote: true };
  }
  return { projectId: activeProjectId, remote: false, sessionId: focusedSessionId };
}
