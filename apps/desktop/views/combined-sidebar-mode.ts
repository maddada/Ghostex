const COMBINED_PROJECT_GROUP_ID_PREFIX = "combined-project:";
const COMBINED_PROJECT_SESSION_ID_PREFIX = "combined-session:";
const COMBINED_ID_SEPARATOR = ":";

export type CombinedSidebarSessionReference = {
  projectId: string;
  sessionId: string;
};

/**
 * CDXC:SidebarMode 2026-05-03-10:42
 * Combined mode renders sessions from every native project in one React store.
 * Encode project identity into presentation IDs so common per-project session
 * IDs such as session-00 cannot collide, while native handlers can still route
 * actions back to the original project/session pair.
 */
export function createCombinedProjectGroupId(projectId: string): string {
  return `${COMBINED_PROJECT_GROUP_ID_PREFIX}${encodeCombinedIdPart(projectId)}`;
}

export function parseCombinedProjectGroupId(groupId: string): string | undefined {
  if (!groupId.startsWith(COMBINED_PROJECT_GROUP_ID_PREFIX)) {
    return undefined;
  }

  return decodeCombinedIdPart(groupId.slice(COMBINED_PROJECT_GROUP_ID_PREFIX.length));
}

export function createCombinedProjectSessionId(projectId: string, sessionId: string): string {
  return [
    COMBINED_PROJECT_SESSION_ID_PREFIX,
    encodeCombinedIdPart(projectId),
    COMBINED_ID_SEPARATOR,
    encodeCombinedIdPart(sessionId),
  ].join("");
}

export function parseCombinedProjectSessionId(
  sessionId: string,
): CombinedSidebarSessionReference | undefined {
  if (!sessionId.startsWith(COMBINED_PROJECT_SESSION_ID_PREFIX)) {
    return undefined;
  }

  const payload = sessionId.slice(COMBINED_PROJECT_SESSION_ID_PREFIX.length);
  const separatorIndex = payload.indexOf(COMBINED_ID_SEPARATOR);
  if (separatorIndex < 0) {
    return undefined;
  }

  const projectId = decodeCombinedIdPart(payload.slice(0, separatorIndex));
  const originalSessionId = decodeCombinedIdPart(payload.slice(separatorIndex + 1));
  if (!projectId || !originalSessionId) {
    return undefined;
  }

  return {
    projectId,
    sessionId: originalSessionId,
  };
}

function encodeCombinedIdPart(value: string): string {
  return encodeURIComponent(value);
}

function decodeCombinedIdPart(value: string): string | undefined {
  try {
    return decodeURIComponent(value);
  } catch {
    return undefined;
  }
}
