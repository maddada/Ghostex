import {
  createGxserverPresentationProjectGroupId,
  createGxserverPresentationProjectSessionId,
  parseGxserverPresentationProjectGroupId,
  parseGxserverPresentationProjectSessionId,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';

export type SidebarProjectReference = {
  machineId: string;
  projectId: string;
};

export type SidebarSessionReference = SidebarProjectReference & {
  sessionId: string;
};

export function createSidebarGroupId(machineId: string, projectId: string): string {
  return machineId === 'local'
    ? createGxserverPresentationProjectGroupId(projectId)
    : `remote:${machineId}:group:${projectId}`;
}

export function createSidebarProjectId(machineId: string, projectId: string): string {
  return machineId === 'local' ? projectId : `remote:${machineId}:project:${projectId}`;
}

export function createSidebarSessionId(machineId: string, projectId: string, sessionId: string): string {
  return machineId === 'local'
    ? createGxserverPresentationProjectSessionId(projectId, sessionId)
    : `remote:${machineId}:session:${projectId}:${sessionId}`;
}

export function parseSidebarGroupId(groupId: string): SidebarProjectReference | undefined {
  const localProjectId = parseGxserverPresentationProjectGroupId(groupId);
  if (localProjectId) {
    return { machineId: 'local', projectId: localProjectId };
  }
  const remote = /^remote:([^:]+):group:(.+)$/u.exec(groupId);
  return remote ? { machineId: remote[1]!, projectId: remote[2]! } : undefined;
}

export function parseSidebarProjectId(projectId: string): SidebarProjectReference | undefined {
  const remote = /^remote:([^:]+):project:(.+)$/u.exec(projectId);
  return remote ? { machineId: remote[1]!, projectId: remote[2]! } : { machineId: 'local', projectId };
}

export function parseSidebarSessionId(sessionId: string): SidebarSessionReference | undefined {
  const local = parseGxserverPresentationProjectSessionId(sessionId);
  if (local) {
    return { machineId: 'local', ...local };
  }
  const remote = /^remote:([^:]+):session:([^:]+):(.+)$/u.exec(sessionId);
  return remote ? { machineId: remote[1]!, projectId: remote[2]!, sessionId: remote[3]! } : undefined;
}
