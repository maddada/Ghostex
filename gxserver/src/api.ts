import http from "node:http";
import {
  GXSERVER_PRODUCT,
  GXSERVER_PROTOCOL_VERSION,
  type GxserverApiPermission,
  type GxserverEndpointDescriptor,
  type GxserverEndpointPath,
  type GxserverListenerKind,
  type GxserverProtocolVersion,
  type GxserverRpcErrorCode,
  type GxserverRpcErrorResponse,
} from "../protocol/index.js";

export const GXSERVER_PROTOCOL_HEADER = "x-gxserver-protocol-version" as const;

export const GXSERVER_ENDPOINTS: readonly GxserverEndpointDescriptor[] = [
  descriptor("/api/health", "remoteAllowed", false, false, "http"),
  descriptor("/api/health/server", "remoteAllowed", true, true, "http"),
  descriptor("/api/events", "remoteAllowed", true, true, "webSocket"),
  descriptor("/api/control/stop", "remoteBlocked", true, true, "http"),
  descriptor("/api/control/stopAll", "remoteBlocked", true, true, "http"),
  descriptor("/api/readAgentSettings", "remoteAllowed", true, true, "http"),
  descriptor("/api/updateAgentSettings", "remoteAllowed", true, true, "http"),
  /*
  CDXC:AgentHooks 2026-06-03-20:28:
  Agent hook file probes and plugin writes inspect/mutate user-local dotfiles.
  Keep those setup endpoints local-only while shared agent policy remains
  remote-readable through the existing settings endpoints.
  */
  descriptor("/api/readAgentHookStatus", "fullLocal", true, true, "http"),
  descriptor("/api/installAgentHooks", "fullLocal", true, true, "http"),
  descriptor("/api/ingestAgentHookEvent", "remoteAllowed", true, true, "http"),
  descriptor("/api/createSession", "remoteAllowed", true, true, "http"),
  descriptor("/api/createAgentSession", "remoteAllowed", true, true, "http"),
  descriptor("/api/forkSession", "remoteAllowed", true, true, "http"),
  descriptor("/api/readAgentLaunchPlan", "remoteAllowed", true, true, "http"),
  descriptor("/api/readAgentResumePlan", "remoteAllowed", true, true, "http"),
  descriptor("/api/requestSessionRename", "remoteAllowed", true, true, "http"),
  descriptor("/api/cancelFirstPromptAutoTitle", "remoteAllowed", true, true, "http"),
  descriptor("/api/ingestSessionStateEvent", "remoteAllowed", true, true, "http"),
  descriptor("/api/ingestTerminalTitleEvent", "remoteAllowed", true, true, "http"),
  descriptor("/api/updateAgentActivity", "remoteAllowed", true, true, "http"),
  descriptor("/api/readPresentationSnapshot", "remoteAllowed", true, true, "http"),
  descriptor("/api/searchSessions", "remoteAllowed", true, true, "http"),
  descriptor("/api/listPreviousSessions", "remoteAllowed", true, true, "http"),
  descriptor("/api/transitionSession", "remoteAllowed", true, true, "http"),
  descriptor("/api/sleepSession", "remoteAllowed", true, true, "http"),
  descriptor("/api/wakeSession", "remoteAllowed", true, true, "http"),
  descriptor("/api/startSessionProvider", "remoteAllowed", true, true, "http"),
  descriptor("/api/killSession", "remoteAllowed", true, true, "http"),
  descriptor("/api/probeSessionProvider", "remoteAllowed", true, true, "http"),
  descriptor("/api/listSessions", "remoteAllowed", true, true, "http"),
  descriptor("/api/removeSession", "remoteAllowed", true, true, "http"),
  descriptor("/api/readSessionText", "remoteAllowed", true, true, "http"),
  descriptor("/api/sendSessionText", "remoteAllowed", true, true, "http"),
  descriptor("/api/sendSessionMessage", "remoteAllowed", true, true, "http"),
  descriptor("/api/sendSessionEnter", "remoteAllowed", true, true, "http"),
  descriptor("/api/focusSession", "remoteAllowed", true, true, "http"),
  descriptor("/api/dispatchRendererCommand", "remoteAllowed", true, true, "http"),
  descriptor("/api/attachSessionMetadata", "remoteAllowed", true, true, "http"),
  descriptor("/api/createProject", "remoteAllowed", true, true, "http"),
  descriptor("/api/updateProject", "remoteAllowed", true, true, "http"),
  descriptor("/api/listProjects", "remoteAllowed", true, true, "http"),
  descriptor("/api/readProjectStatus", "remoteAllowed", true, true, "http"),
  descriptor("/api/addProjectPath", "remoteAllowed", true, true, "http"),
  descriptor("/api/removeProject", "remoteAllowed", true, true, "http"),
  descriptor("/api/deleteWorktreeProject", "remoteAllowed", true, true, "http"),
  descriptor("/api/updateSession", "remoteAllowed", true, true, "http"),
  descriptor("/api/updateSessionOrder", "remoteAllowed", true, true, "http"),
  descriptor("/api/runGitAction", "remoteAllowed", true, true, "http"),
  descriptor("/api/runGitHubAction", "remoteAllowed", true, true, "http"),
  descriptor("/api/runWorktreeAction", "remoteAllowed", true, true, "http"),
  descriptor("/api/runProjectSetupCommand", "remoteAllowed", true, true, "http"),
  descriptor("/api/runBeadsAction", "remoteAllowed", true, true, "http"),
  descriptor("/api/previewRepositoryClone", "remoteAllowed", true, true, "http"),
  descriptor("/api/startRepositoryClone", "remoteAllowed", true, true, "http"),
  descriptor("/api/readRepositoryCloneJob", "remoteAllowed", true, true, "http"),
  descriptor("/api/cancelRepositoryCloneJob", "remoteAllowed", true, true, "http"),
  descriptor("/api/browseProjectDirectories", "remoteAllowed", true, true, "http"),
  descriptor("/api/resolveGitRootForPath", "remoteBlocked", true, true, "http"),
  descriptor("/api/queryLogs", "fullLocal", true, true, "http"),
  /*
  CDXC:GxserverTypedOperations 2026-06-02-15:33:
  gxserver must not expose a generic process RPC after the native/gxserver ownership split. Shared project operations use typed Git, worktree, Beads, clone, and lifecycle endpoints; macOS keeps its separate native runProcess bridge only for current-window OS-local hosting tasks.
  */
  descriptor("/api/updateAuth", "remoteBlocked", true, true, "http"),
  descriptor("/api/updateListenerConfig", "remoteBlocked", true, true, "http"),
  descriptor("/api/installTool", "remoteBlocked", true, true, "http"),
  descriptor("/api/browseFilesystem", "remoteBlocked", true, true, "http"),
  descriptor("/api/destructiveAdminAction", "remoteBlocked", true, true, "http"),
] as const;

export function getGxserverEndpoint(pathname: string): GxserverEndpointDescriptor | undefined {
  return GXSERVER_ENDPOINTS.find((endpoint) => endpoint.path === pathname);
}

export function isGxserverEndpointPath(pathname: string): pathname is GxserverEndpointPath {
  return Boolean(getGxserverEndpoint(pathname));
}

export function isRemoteEndpointAllowed(
  listenerKind: GxserverListenerKind,
  permission: GxserverApiPermission,
): boolean {
  return listenerKind === "local" || permission === "remoteAllowed";
}

export function readProtocolVersion(request: http.IncomingMessage, url: URL, body?: unknown): unknown {
  const header = request.headers[GXSERVER_PROTOCOL_HEADER];
  if (typeof header === "string" && header.trim()) {
    return parseProtocolVersion(header);
  }
  if (isObjectRecord(body) && "protocolVersion" in body) {
    return body.protocolVersion;
  }
  const queryValue = url.searchParams.get("protocolVersion");
  if (queryValue) {
    return parseProtocolVersion(queryValue);
  }
  return undefined;
}

export function isExpectedProtocolVersion(value: unknown): value is GxserverProtocolVersion {
  return value === GXSERVER_PROTOCOL_VERSION;
}

export function createRpcError(
  error: GxserverRpcErrorCode,
  message: string,
  requestId?: string,
  includeProtocolVersion = true,
): GxserverRpcErrorResponse {
  return {
    error,
    message,
    ok: false,
    product: GXSERVER_PRODUCT,
    ...(includeProtocolVersion ? { protocolVersion: GXSERVER_PROTOCOL_VERSION } : {}),
    ...(requestId ? { requestId } : {}),
  };
}

export function createProtocolMismatchError(actualProtocolVersion: unknown, requestId?: string): GxserverRpcErrorResponse {
  return createRpcError(
    "protocolMismatch",
    `gxserver protocol mismatch. Expected protocol ${GXSERVER_PROTOCOL_VERSION}, got ${String(
      actualProtocolVersion,
    )}. Update Ghostex and gxserver so their protocol versions match.`,
    requestId,
  );
}

/*
CDXC:GxserverApi 2026-05-30-14:26:
The HTTP API intentionally has one endpoint catalog for local and remote listeners. Remote/Tailscale clients may call typed session/project/Git/worktree/Beads operations, but generic process execution, auth/listener mutation, tool installation, broad filesystem browsing, destructive admin actions, and local-only log querying are blocked before domain handlers run.
*/
function descriptor(
  path: GxserverEndpointPath,
  permission: GxserverApiPermission,
  requiresAuth: boolean,
  requiresProtocolVersion: boolean,
  transport: "http" | "webSocket",
): GxserverEndpointDescriptor {
  return {
    path,
    permission,
    requiresAuth,
    requiresProtocolVersion,
    transport,
  };
}

function parseProtocolVersion(value: string): unknown {
  if (/^\d+$/.test(value)) {
    return Number(value);
  }
  return value;
}

function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
