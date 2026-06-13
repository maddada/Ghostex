import http from "node:http";
import { execFile } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { readFile, stat } from "node:fs/promises";
import path from "node:path";
import type { Duplex } from "node:stream";
import { promisify } from "node:util";
import {
  createProtocolMismatchError,
  createRpcError,
  getGxserverEndpoint,
  isExpectedProtocolVersion,
  isRemoteEndpointAllowed,
  readProtocolVersion,
} from "./api.js";
import { installGxserverAgentHooks, normalizeGxserverProcessPath, readGxserverAgentHookStatus } from "./agent-hooks.js";
import {
  GXSERVER_CONTROL_PLANE_CAPABILITIES,
  GXSERVER_LOCAL_API_HOST,
  GXSERVER_LOCAL_API_PORT,
  GXSERVER_PRODUCT,
  GXSERVER_PROTOCOL_VERSION,
} from "./constants.js";
import {
  ensureGxserverAuthToken,
  isAuthorizedGxserverRequest,
  isExpectedGxserverAuthToken,
  readGxserverAuthToken,
} from "./auth.js";
import { GxserverDomainRepository, GxserverDomainStateError } from "./domain-state.js";
import {
  GxserverEventHub,
  GxserverRendererCommandTimeoutError,
  GxserverRendererCommandUnavailableError,
} from "./events.js";
import { resolveGitRootForExistingDirectory } from "./git-root.js";
import { detectRegisteredGitWorktreeMetadata } from "./git-worktrees.js";
import { isGxserverProjectId, isGxserverSessionId } from "./ids.js";
import { fetchServerHealth, requestServerStop } from "./http-client.js";
import { ensureGxserverIdentity } from "./identity.js";
import { migrateLegacyMacosStateIntoGxserver } from "./legacy-macos-state-migration.js";
import { createGxserverLogger, logLevelFromStatus, type GxserverLogger } from "./logger.js";
import { GxserverLogQueryInputError, queryGxserverLogs } from "./logs.js";
import { assertSupportedNodeVersion } from "./node-version.js";
import {
  buildAgentForkPlan,
  buildAgentResumePlan,
  buildProjectAgentLaunchPlan,
  createAgentForkSessionParams,
  createAgentSessionParams,
  getAgentLaunchStartupTextForSession,
  getAgentStartupTextForSession,
  getAgentActivityStaleProjectionDelayMs,
  updateSessionActivitySettings,
} from "./agents/lifecycle.js";
import { GxserverAgentSettingsRepository } from "./agents/settings.js";
import { applyTerminalTitleEvent, getVisibleTerminalTitle } from "./session-title/index.js";
import { projectSessionTitle } from "./session-title/projection.js";
import { applySessionTransition, normalizeSessionTransitionParams } from "./session-transition/index.js";
import {
  applySessionRenameRequest,
  applySessionStateEvent,
  createAgentTitleDebouncer,
  defaultGroupId,
  GxserverPresentationDeltaCoalescer,
  incrementPresentationRevision,
  listGxserverPreviousSessions,
  projectPresentationProject,
  projectPresentationSession,
  readGxserverPresentationSnapshot,
  searchGxserverPresentation,
  shouldIncludePresentationSession,
  reconcileAgentMetadataTitle,
  resolveSessionIdentity,
  resolveSessionLaunchAgentMismatch,
  shouldCheckAgentMetadataTitle,
  type GxserverAgentTitleDebounceDecision,
  type GxserverLaunchAgentMismatch,
  type GxserverSessionIdentityConflict,
} from "./session-presentation/index.js";
import { classifyTerminalTitleStatus, normalizeAgentActivityState } from "./session-status/index.js";
import { type GxserverPaths, getGxserverPaths } from "./paths.js";
import { browseProjectDirectories } from "./project-directory-browser.js";
import { GxserverProjectPathError, normalizeExistingDirectoryPath, resolveProjectOperationDirectory } from "./project-paths.js";
import { GxserverRepositoryCloneError, GxserverRepositoryCloneJobManager } from "./repository-clone/index.js";
import { removeRuntimeMetadata, writeRuntimeMetadata } from "./runtime.js";
import { deleteGxserverWorktreeProject } from "./worktree-delete.js";
import {
  createGxserverMigrationStatus,
  initializeGxserverStorage,
  openGxserverDatabase,
  readGxserverConfig,
  type GxserverConfig,
} from "./storage.js";
import { getGxserverToolStatuses, requireBundledZmx, type GxserverResolvedTool } from "./toolchain.js";
import {
  GxserverTypedOperationError,
  runBeadsAction,
  runGitAction,
  runGitHubAction,
  runProjectSetupCommand,
  runWorktreeAction,
} from "./typed-operations.js";
import {
  buildZmxAttachCommand,
  buildZmxHistoryCommand,
  buildZmxRunCommand,
  buildZmxShellProviderCommand,
  buildZmxSendCommand,
  decideStartupTextDisposition,
  defaultCwdExists,
  failedKillProviderStatePatch,
  killZmxSession,
  missingProviderStatePatch,
  normalizeLifecycleReason,
  probeZmxSession,
  providerStatePatch,
  providerZmxSessionName,
  readZmxSessionProcessIdentities,
  summarizeZmxChildEnvironmentSanitization,
  GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES,
  GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES,
  runZshScript,
  type GxserverCwdExists,
  type GxserverZmxCommandResult,
  type GxserverZmxCommandOptions,
  type GxserverZmxCommandRunner,
  type GxserverZmxProcessIdentityReader,
} from "./zmx-lifecycle.js";
import { GxserverZmxTitleObserver, type GxserverZmxTitleObservationStateChange } from "./zmx-title-observer.js";
import { GXSERVER_RENDERER_COMMAND_ACTIONS } from "../protocol/index.js";
import type {
  GxserverAttachSessionMetadataParams,
  GxserverAttachSessionMetadataResult,
  GxserverAgentSettings,
  GxserverAuthToken,
  GxserverCreateProjectParams,
  GxserverCreateSessionParams,
  GxserverEndpointPath,
  GxserverForkSessionResult,
  GxserverIngestAgentHookEventParams,
  GxserverIngestAgentHookEventResult,
  GxserverInstallAgentHooksParams,
  GxserverListenerConfig,
  GxserverListenerKind,
  GxserverMinimalHealthResponse,
  GxserverMigrationStatus,
  GxserverProjectDomainState,
  GxserverProjectDirectoryBrowseParams,
  GxserverProjectId,
  GxserverProviderKillResult,
  GxserverProviderProbeResult,
  GxserverPresentationSearchParams,
  GxserverPresentationDelta,
  GxserverCancelFirstPromptAutoTitleParams,
  GxserverCancelFirstPromptAutoTitleResult,
  GxserverDeleteWorktreeProjectParams,
  GxserverResolveGitRootForPathResult,
  GxserverRemoveSessionParams,
  GxserverRunBeadsActionParams,
  GxserverRepositoryCloneJobRpcResult,
  GxserverRepositoryClonePreviewRpcResult,
  GxserverRendererCommandAction,
  GxserverRunGitActionParams,
  GxserverRunGitHubActionParams,
  GxserverRunProjectSetupCommandParams,
  GxserverRunWorktreeActionParams,
  GxserverRpcSuccessResponse,
  GxserverRuntimeMetadata,
  GxserverServerId,
  GxserverSessionDomainState,
  GxserverSessionId,
  GxserverSessionLifecycleParams,
  GxserverStartSessionProviderParams,
  GxserverStartSessionProviderResult,
  GxserverSessionTransitionParams,
  GxserverSessionTransitionResult,
  GxserverSessionRenameRequestParams,
  GxserverSessionRenameRequestResult,
  GxserverSessionStateEventParams,
  GxserverSessionStateEventResult,
  GxserverTerminalTitleEventParams,
  GxserverTerminalTitleEventResult,
  GxserverUpdateAgentSettingsParams,
  GxserverReadAgentHookStatusParams,
  GxserverUpdateProjectParams,
  GxserverUpdateAgentActivityParams,
  GxserverUpdateAgentActivityResult,
  GxserverUpdateSessionOrderParams,
  GxserverUpdateSessionParams,
  GxserverFirstPromptTitleGenerationAgent,
  GxserverAgentActivityState,
  GxserverZmxSessionName,
} from "../protocol/index.js";
import { createSourceGxserverBuildIdentity, isGxserverBuildIdentityReusable } from "./build-identity.js";

export interface GxserverForegroundOptions {
  buildIdentity?: string;
  homeDir?: string;
  version: string;
}

export interface GxserverForegroundResult {
  reused: boolean;
}

export interface GxserverApiRuntime {
  authToken: GxserverAuthToken;
  buildIdentity: string;
  config: GxserverConfig;
  eventHub: GxserverEventHub;
  logger: GxserverLogger;
  metadata: GxserverRuntimeMetadata;
  migration: GxserverMigrationStatus;
  paths: GxserverPaths;
  shutdown: () => void;
  version: string;
  firstPromptTitleGeneration?: {
    generateTitle?: (input: { cwd?: string; prompt: string }) => Promise<string>;
  };
  repositoryCloneJobs?: GxserverRepositoryCloneJobManager;
  presentationDeltaCoalescer?: GxserverPresentationDeltaCoalescer;
  presentationLastDeltaJsonBySessionKey?: Map<string, string>;
  presentationStaleActivityTimers?: Map<string, ReturnType<typeof setTimeout>>;
  zmxTitleObserver?: GxserverZmxTitleObserver;
  zmxLifecycle?: {
    cwdExists?: GxserverCwdExists;
    readSessionProcessIdentities?: GxserverZmxProcessIdentityReader;
    requireZmx?: () => Promise<GxserverResolvedTool>;
    runZsh?: GxserverZmxCommandRunner;
  };
}
type GxserverCreateSessionDomainParams = GxserverCreateSessionParams & { projectId: GxserverProjectId };

/*
CDXC:GxserverApi 2026-05-30-20:04:
Authenticated HTTP RPC clients must not be able to grow gxserver memory by streaming unbounded JSON bodies. Keep POST body parsing capped at 1 MiB, reject larger Content-Length values before reading, and stop retaining chunks as soon as chunked transfer input crosses the same limit.
*/
export const GXSERVER_JSON_BODY_LIMIT_BYTES = 1024 * 1024;
const GXSERVER_AGENT_TITLE_METADATA_DEBOUNCE_MS = 3_000;
const GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH = 250;
const GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH = 39;
const GXSERVER_FIRST_PROMPT_TITLE_GENERATION_TIMEOUT_MS = 30_000;
const GXSERVER_SESSION_STATE_SIDECAR_MAX_BYTES = 1024 * 1024;
const GXSERVER_COMMAND_COLOR_DISABLING_ENVIRONMENT_KEYS = [
  "ANSI_COLORS_DISABLED",
  "NO_COLOR",
  "NODE_DISABLE_COLORS",
] as const;
const GXSERVER_INTERNAL_PROMPT_GENERATION_ENVIRONMENT_KEYS = [
  "GHOSTEX_GLOBAL_SESSION_REF",
  "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE",
  "GHOSTEX_GXSERVER_BASE_URL",
  "GHOSTEX_GXSERVER_PROTOCOL_VERSION",
  "GHOSTEX_SESSION_ID",
  "GHOSTEX_SESSION_STATE_FILE",
  "GHOSTEX_WORKSPACE_ID",
  "GHOSTEX_WORKSPACE_ROOT",
  "VSMUX_SESSION_ID",
  "VSMUX_SESSION_STATE_FILE",
  "VSMUX_WORKSPACE_ID",
  "VSMUX_WORKSPACE_ROOT",
  "ghostex_SESSION_STATE_FILE",
  "ghostex_WORKSPACE_ID",
  "ghostex_WORKSPACE_ROOT",
] as const;
const GXSERVER_AGENT_HOOK_WORKING_EVENTS = new Set([
  "BeforeAgent",
  "PreInvocation",
  "PreToolUse",
  "UserPromptSubmit",
  "agent.start",
  "agent_start",
  "agentSpawn",
  "beforeShellExecution",
  "beforeSubmitPrompt",
  "on_session_reset",
  "on_session_start",
  "on_tool_permission",
  "post_approval_response",
  "postToolUse",
  "pre_llm_call",
  "pre_tool_call",
  "preToolUse",
  "userPromptSubmit",
]);
const GXSERVER_AGENT_HOOK_ATTENTION_EVENTS = new Set([
  "Notification",
  "PermissionRequest",
  "message.updated",
  "permission.updated",
  "pre_approval_request",
  "session.updated",
]);
const GXSERVER_AGENT_HOOK_IDLE_EVENTS = new Set([
  "AfterAgent",
  "SessionEnd",
  "Stop",
  "afterAgentResponse",
  "agent.end",
  "agent_end",
  "on_complete",
  "on_error",
  "on_session_end",
  "on_session_finalize",
  "release",
  "session.end",
  "session_shutdown",
  "stop",
  "turn-completion",
]);
const agentTitleMetadataDebouncer = createAgentTitleDebouncer({
  delayMs: GXSERVER_AGENT_TITLE_METADATA_DEBOUNCE_MS,
});
const execFileAsync = promisify(execFile);

export async function runGxserverForeground(options: GxserverForegroundOptions): Promise<GxserverForegroundResult> {
  assertSupportedNodeVersion();
  const buildIdentity = options.buildIdentity ?? createSourceGxserverBuildIdentity(options.version);

  const paths = getGxserverPaths(options.homeDir);
  const existingAuth = await readGxserverAuthToken(paths);
  const existing = await fetchServerHealth({ token: existingAuth?.token });
  if (existing) {
    if (isGxserverBuildIdentityReusable(existing.buildIdentity, buildIdentity)) {
      return { reused: true };
    }
    /*
    CDXC:GxserverLifecycle 2026-06-07-13:32:
    Foreground launch is the macOS app's daemon entry point. If an older same-protocol gxserver is still bound to the fixed port after a Ghostex update, stop only that control plane and continue with the current package so legacy session migration and repair code actually runs.
    */
    await requestServerStop({ token: existingAuth?.token, timeoutMs: 2000 });
    const stoppedState = await waitForMismatchedGxserverToStop(existingAuth?.token, buildIdentity);
    if (stoppedState === "reusable") {
      return { reused: true };
    }
    if (stoppedState !== "stopped") {
      throw new Error(
        "gxserver build identity changed, but the old control plane did not stop. Stop gxserver and launch Ghostex again so the current migration code can run.",
      );
    }
  }

  const storage = await initializeGxserverStorage(paths);
  const config = await readGxserverConfig(paths);
  const identity = await ensureGxserverIdentity(paths);
  const auth = await ensureGxserverAuthToken(paths);
  const logger = createGxserverLogger(paths);
  const legacyStateImport = await migrateLegacyMacosStateIntoGxserver({
    logger,
    paths,
    serverId: identity.serverId,
  });
  const startedAt = new Date().toISOString();
  const metadata: GxserverRuntimeMetadata = {
    buildIdentity,
    pid: process.pid,
    port: GXSERVER_LOCAL_API_PORT,
    protocolVersion: GXSERVER_PROTOCOL_VERSION,
    serverId: identity.serverId,
    startedAt,
    version: options.version,
  };
  const eventHub = new GxserverEventHub(metadata.serverId);
  const servers: http.Server[] = [];
  let shuttingDown = false;
  let resolveShutdown: () => void = () => {};
  let runtime: GxserverApiRuntime;
  const zmxTitleObserver = new GxserverZmxTitleObserver({
    authToken: auth.token,
    logger,
    metadata,
    onObservationStateChange: (change) => recordZmxTitleObservationState(runtime, change),
    requireZmx: () => (runtime.zmxLifecycle?.requireZmx ?? requireBundledZmx)(),
  });

  const shutdown = (): void => {
    if (shuttingDown) {
      return;
    }
    shuttingDown = true;
    zmxTitleObserver.close();
    eventHub.broadcast({
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      serverId: metadata.serverId,
      type: "serverStopping",
    });
    void eventHub
      .close()
      .then(() => Promise.allSettled(servers.map(closeServer)))
      .then(() => removeRuntimeMetadata(paths))
      .finally(resolveShutdown);
  };

  runtime = {
    authToken: auth.token,
    buildIdentity,
    config,
    eventHub,
    logger,
    metadata,
    migration: createGxserverMigrationStatus(storage, undefined, {
      legacyMacosState: legacyStateImport.status,
    }),
    paths,
    shutdown,
    version: options.version,
    zmxTitleObserver,
  };

  eventHub.setPresentationSnapshotProvider(async ({ clientId, lastRevision }) => {
    const db = openGxserverDatabase(paths);
    try {
      const repository = new GxserverDomainRepository(db, metadata.serverId);
      await syncSessionStateSidecars(runtime, repository, {
        reason: "presentation-snapshot-provider",
      });
      const snapshot = readGxserverPresentationSnapshot(db, metadata.serverId);
      await logger.log({
        client: clientId,
        details: {
          groupCount: snapshot.groups.length,
          lastRevision,
          projectCount: snapshot.projects.length,
          revision: snapshot.revision,
          sessionCount: snapshot.sessions.length,
        },
        event: "session-presentation.snapshot",
        level: "debug",
        serverId: metadata.serverId,
      });
      return snapshot;
    } finally {
      db.close();
    }
  });

  /*
  CDXC:GxserverLifecycle 2026-05-30-20:09:
  Foreground startup is all-or-nothing across configured listeners. If the remote listener fails after the local API is already bound, gxserver must close the local listener before throwing so status never observes a half-started daemon without runtime metadata.
  */
  try {
    const localServer = createGxserverHttpServer(runtime, "local");
    await listen(localServer, config.listeners.local);
    servers.push(localServer);

    if (config.listeners.remote.enabled) {
      const remoteServer = createGxserverHttpServer(runtime, "remote");
      await listen(remoteServer, config.listeners.remote);
      servers.push(remoteServer);
    }
  } catch (error) {
    await eventHub.close();
    await Promise.allSettled(servers.map(closeServer));
    await removeRuntimeMetadata(paths);
    throw error;
  }

  await writeRuntimeMetadata(paths, metadata);
  await logger.log({
    event: "serverStarted",
    level: "info",
    serverId: metadata.serverId,
  });
  await syncZmxTitleObserversFromStorage(runtime, "server-start");
  eventHub.broadcast({
    protocolVersion: GXSERVER_PROTOCOL_VERSION,
    serverId: metadata.serverId,
    type: "serverStarted",
  });

  await new Promise<void>((resolve) => {
    resolveShutdown = resolve;
    process.once("SIGINT", shutdown);
    process.once("SIGTERM", shutdown);
  });

  return { reused: false };
}

async function waitForMismatchedGxserverToStop(
  token: GxserverAuthToken | undefined,
  expectedBuildIdentity: string,
): Promise<"running" | "reusable" | "stopped"> {
  const deadline = Date.now() + 5000;
  while (Date.now() < deadline) {
    await delay(100);
    const health = await fetchServerHealth({ token, timeoutMs: 500 });
    if (!health) {
      return "stopped";
    }
    if (isGxserverBuildIdentityReusable(health.buildIdentity, expectedBuildIdentity)) {
      return "reusable";
    }
  }
  const health = await fetchServerHealth({ token, timeoutMs: 500 });
  if (!health) {
    return "stopped";
  }
  return isGxserverBuildIdentityReusable(health.buildIdentity, expectedBuildIdentity) ? "reusable" : "running";
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export function createGxserverHttpServer(runtime: GxserverApiRuntime, listenerKind: GxserverListenerKind): http.Server {
  const server = http.createServer((request, response) => {
    void handleRequest({
      listenerKind,
      request,
      response,
      runtime,
    });
  });

  server.on("upgrade", (request, socket, head) => {
    void handleUpgrade({
      head,
      listenerKind,
      request,
      runtime,
      socket,
    });
  });

  return server;
}

interface HandleRequestOptions {
  listenerKind: GxserverListenerKind;
  request: http.IncomingMessage;
  response: http.ServerResponse;
  runtime: GxserverApiRuntime;
}

async function handleRequest(options: HandleRequestOptions): Promise<void> {
  const { request, response, runtime } = options;
  const requestId = getRequestId(request);
  const startedAtMs = Date.now();
  const clientAddress = request.socket.remoteAddress;
  let error: string | undefined;
  let endpointPath: GxserverEndpointPath | undefined;
  try {
    endpointPath = await routeRequest(options, requestId);
  } catch (caught) {
    error = caught instanceof Error ? caught.message : String(caught);
    if (!response.headersSent) {
      sendJson(response, 500, createRpcError("internalError", error, requestId));
    } else {
      response.destroy(caught instanceof Error ? caught : new Error(error));
    }
  } finally {
    await runtime.logger.log({
      client: clientAddress,
      durationMs: Date.now() - startedAtMs,
      error,
      event: "apiRequest",
      level: logLevelFromStatus(response.statusCode),
      requestId,
      serverId: runtime.metadata.serverId,
      details: {
        method: request.method,
        path: endpointPath ?? request.url,
        statusCode: response.statusCode,
      },
    });
    if (endpointPath) {
      runtime.eventHub.broadcast({
        path: endpointPath,
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
        requestId,
        serverId: runtime.metadata.serverId,
        type: "apiRequestHandled",
      });
    }
  }
}

async function routeRequest(options: HandleRequestOptions, requestId: string): Promise<GxserverEndpointPath | undefined> {
  const { listenerKind, request, response, runtime } = options;
  const url = new URL(request.url ?? "/", `http://${GXSERVER_LOCAL_API_HOST}:${GXSERVER_LOCAL_API_PORT}`);
  const endpoint = getGxserverEndpoint(url.pathname);
  applyCorsHeaders(request, response, runtime.config);

  /*
  CDXC:GxserverApi 2026-05-30-18:04:
  The macOS React sidebar calls gxserver from a WebKit page with `Authorization`, `content-type`, and the protocol header, so WebKit sends an unauthenticated CORS preflight before zmx attach/list RPCs. Answer `OPTIONS` before auth, method, and protocol gates so the browser can reach the real authenticated endpoint instead of surfacing a generic `Load failed` while gxserver is healthy.
  */
  if (request.method === "OPTIONS") {
    if (!endpoint || endpoint.transport !== "http") {
      sendJson(response, 404, createRpcError("notFound", `${url.pathname} is not a gxserver HTTP endpoint.`, requestId));
      return endpoint?.path;
    }
    if (!isRemoteEndpointAllowed(listenerKind, endpoint.permission)) {
      sendJson(
        response,
        403,
        createRpcError(
          "forbidden",
          `${endpoint.path} is not available on the remote gxserver listener.`,
          requestId,
        ),
      );
      return endpoint.path;
    }
    response.writeHead(204);
    response.end();
    return endpoint.path;
  }

  if (request.method === "GET" && url.pathname === "/api/health") {
    sendJson(response, 200, createMinimalHealth(runtime.version));
    return "/api/health";
  }

  if (!endpoint || endpoint.transport !== "http") {
    sendJson(
      response,
      404,
      createRpcError("notFound", `No gxserver endpoint for ${request.method ?? "UNKNOWN"} ${url.pathname}.`, requestId),
    );
    return undefined;
  }

  if (endpoint.path !== "/api/health/server" && request.method !== "POST") {
    sendJson(response, 405, createRpcError("methodNotAllowed", `${endpoint.path} requires POST.`, requestId));
    return endpoint.path;
  }

  if (endpoint.path === "/api/health/server" && request.method !== "GET") {
    sendJson(response, 405, createRpcError("methodNotAllowed", `${endpoint.path} requires GET.`, requestId));
    return endpoint.path;
  }

  if (endpoint.requiresAuth && !isAuthorizedGxserverRequest(request, runtime.authToken)) {
    sendJson(
      response,
      401,
      createRpcError("unauthorized", "gxserver auth token is required for this endpoint.", requestId),
    );
    return endpoint.path;
  }

  let body: unknown;
  if (request.method === "POST") {
    try {
      body = await readJsonBody(request);
    } catch (caught) {
      if (caught instanceof GxserverRequestBodyTooLargeError) {
        sendJson(
          response,
          413,
          createRpcError(
            "badRequest",
            `Request body exceeds the gxserver JSON RPC limit of ${GXSERVER_JSON_BODY_LIMIT_BYTES} bytes.`,
            requestId,
          ),
        );
      } else {
        sendJson(response, 400, createRpcError("badRequest", "Request body must be valid JSON.", requestId));
      }
      return endpoint.path;
    }
  }

  if (endpoint.requiresProtocolVersion) {
    const protocolVersion = readProtocolVersion(request, url, body);
    if (!isExpectedProtocolVersion(protocolVersion)) {
      sendJson(response, 426, createProtocolMismatchError(protocolVersion, requestId));
      return endpoint.path;
    }
  }

  if (!isRemoteEndpointAllowed(listenerKind, endpoint.permission)) {
    sendJson(
      response,
      403,
      createRpcError(
        "forbidden",
        `${endpoint.path} is not available on the remote gxserver listener.`,
        requestId,
      ),
    );
    return endpoint.path;
  }

  if (endpoint.path === "/api/health/server") {
    sendJson(response, 200, await createAuthenticatedHealth(runtime));
    return endpoint.path;
  }

  /*
  CDXC:GxserverApi 2026-05-30-17:08:
  `/api/queryLogs` now implements the gxserver-16 read-only local log API while preserving gxserver-5 auth, method, protocol, and listener gates. The endpoint stays local-only until remote log redaction/scope rules are defined, so remote clients must use a local/tunneled authenticated daemon connection rather than scraping `~/.ghostex/logs/gxserver.jsonl`.
  */
  if (endpoint.path === "/api/queryLogs") {
    try {
      const params = readRpcParams(body);
      const result = await queryGxserverLogs(runtime.paths, params);
      sendJson(response, 200, {
        ok: true,
        product: GXSERVER_PRODUCT,
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
        requestId,
        result,
      });
    } catch (caught) {
      if (caught instanceof GxserverLogQueryInputError) {
        sendJson(response, 400, createRpcError("badRequest", caught.message, requestId));
      } else {
        throw caught;
      }
    }
    return endpoint.path;
  }

  if (endpoint.path === "/api/dispatchRendererCommand") {
    try {
      sendJson(response, 200, await handleRendererCommandEndpoint(runtime, body, requestId));
    } catch (caught) {
      if (caught instanceof GxserverTypedOperationError) {
        sendJson(response, statusForOperationError(caught.code), createRpcError(caught.code, caught.message, requestId));
      } else if (isRendererCommandDispatchError(caught)) {
        sendJson(response, 503, createRpcError("dependencyUnavailable", caught.message, requestId));
      } else {
        throw caught;
      }
    }
    return endpoint.path;
  }

  if (isAgentHookEndpoint(endpoint.path)) {
    const result = await handleAgentHookEndpoint(runtime, endpoint.path, body, requestId);
    sendJson(response, 200, result);
    return endpoint.path;
  }

  if (isTypedOperationEndpoint(endpoint.path)) {
    const abortController = new AbortController();
    const abortTypedOperation = (): void => {
      if (!response.writableEnded) {
        abortController.abort();
      }
    };
    request.once("aborted", abortTypedOperation);
    response.once("close", abortTypedOperation);
    try {
      const result = await handleTypedOperationEndpoint(runtime, endpoint.path, body, requestId, abortController.signal);
      if (!response.destroyed) {
        sendJson(response, 200, result);
      }
    } catch (caught) {
      if (caught instanceof GxserverDomainStateError) {
        sendJson(response, statusForDomainStateError(caught.code), createRpcError(caught.code, caught.message, requestId));
      } else if (caught instanceof GxserverTypedOperationError || caught instanceof GxserverProjectPathError) {
        sendJson(response, statusForOperationError(caught.code), createRpcError(caught.code, caught.message, requestId));
      } else {
        throw caught;
      }
    } finally {
      request.removeListener("aborted", abortTypedOperation);
      response.removeListener("close", abortTypedOperation);
    }
    return endpoint.path;
  }

  if (isZmxLifecycleEndpoint(endpoint.path)) {
    try {
      const result = await handleZmxLifecycleEndpoint(runtime, endpoint.path, body, requestId);
      sendJson(response, 200, result);
    } catch (caught) {
      if (caught instanceof GxserverDomainStateError) {
        sendJson(response, statusForDomainStateError(caught.code), createRpcError(caught.code, caught.message, requestId));
      } else if (caught instanceof GxserverProjectPathError) {
        sendJson(response, statusForOperationError(caught.code), createRpcError(caught.code, caught.message, requestId));
      } else {
        throw caught;
      }
    }
    return endpoint.path;
  }

  /*
  CDXC:GxserverSessionInteraction 2026-05-30-20:53:
  gx/ghostex session interaction commands must terminate at gxserver after the hard cutover. zmx-backed read/send/send-enter/session-targeted send-message use the bundled zmx binary directly; renderer focus goes through gxserver's authenticated event channel so the macOS app executes only the visible-pane UI effect instead of receiving direct CLI bridge traffic.
  */
  if (isZmxSessionInteractionEndpoint(endpoint.path)) {
    try {
      const result = await handleZmxSessionInteractionEndpoint(runtime, endpoint.path, body, requestId);
      sendJson(response, 200, result);
    } catch (caught) {
      if (caught instanceof GxserverDomainStateError) {
        sendJson(response, statusForDomainStateError(caught.code), createRpcError(caught.code, caught.message, requestId));
      } else if (caught instanceof GxserverTypedOperationError) {
        sendJson(response, statusForOperationError(caught.code), createRpcError(caught.code, caught.message, requestId));
      } else if (isRendererCommandDispatchError(caught)) {
        sendJson(response, 503, createRpcError("dependencyUnavailable", caught.message, requestId));
      } else {
        throw caught;
      }
    }
    return endpoint.path;
  }

  if (isRepositoryCloneEndpoint(endpoint.path)) {
    try {
      const result = await handleRepositoryCloneEndpoint(runtime, endpoint.path, body, requestId);
      sendJson(response, 200, result);
    } catch (caught) {
      if (caught instanceof GxserverRepositoryCloneError) {
        sendJson(response, statusForRepositoryCloneError(caught.code), createRpcError(caught.code, caught.message, requestId));
      } else if (caught instanceof GxserverDomainStateError) {
        sendJson(response, statusForDomainStateError(caught.code), createRpcError(caught.code, caught.message, requestId));
      } else if (caught instanceof GxserverProjectPathError) {
        sendJson(response, statusForOperationError(caught.code), createRpcError(caught.code, caught.message, requestId));
      } else {
        throw caught;
      }
    }
    return endpoint.path;
  }

  if (endpoint.path === "/api/browseProjectDirectories") {
    try {
      const result = await handleBrowseProjectDirectoriesEndpoint(body);
      sendJson(response, 200, {
        ok: true,
        product: GXSERVER_PRODUCT,
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
        requestId,
        result,
      });
    } catch (caught) {
      if (caught instanceof GxserverProjectPathError) {
        sendJson(response, statusForOperationError(caught.code), createRpcError(caught.code, caught.message, requestId));
      } else {
        throw caught;
      }
    }
    return endpoint.path;
  }

  /*
  CDXC:OSIntegration 2026-06-02-12:14:
  Native owns local open-file/open-folder routing, but gxserver owns repository fact lookup. Keep this arbitrary-path Git-root probe local-only so remote clients cannot enumerate or test paths outside registered projects.
  */
  if (endpoint.path === "/api/resolveGitRootForPath") {
    try {
      const result = handleResolveGitRootForPathEndpoint(body);
      sendJson(response, 200, {
        ok: true,
        product: GXSERVER_PRODUCT,
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
        requestId,
        result,
      });
    } catch (caught) {
      if (caught instanceof GxserverProjectPathError) {
        sendJson(response, statusForOperationError(caught.code), createRpcError(caught.code, caught.message, requestId));
      } else {
        throw caught;
      }
    }
    return endpoint.path;
  }

  /*
  CDXC:GxserverApi 2026-05-30-17:30:
  Domain-state endpoints run after gxserver-5 auth, protocol-version, listener-permission, logging, and event semantics. zmx lifecycle and typed Git/worktree/Beads operations have their own handlers so project/session persistence cannot accidentally accept process-execution payloads or lifecycle commands.
  */
  if (isDomainStateEndpoint(endpoint.path)) {
    try {
      sendJson(response, 200, await handleDomainStateEndpoint(runtime, endpoint.path, body, requestId));
    } catch (caught) {
      if (caught instanceof GxserverDomainStateError) {
        sendJson(response, statusForDomainStateError(caught.code), createRpcError(caught.code, caught.message, requestId));
      } else if (caught instanceof GxserverProjectPathError) {
        sendJson(response, statusForOperationError(caught.code), createRpcError(caught.code, caught.message, requestId));
      } else {
        throw caught;
      }
    }
    return endpoint.path;
  }

  if (endpoint.path === "/api/control/stopAll") {
    const result = await stopAllTrackedZmxSessions(runtime);
    sendJson(response, 200, {
      ok: true,
      product: GXSERVER_PRODUCT,
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      requestId,
      result,
    });
    setTimeout(() => {
      runtime.shutdown();
    }, 25).unref();
    return endpoint.path;
  }

  if (endpoint.path === "/api/control/stop") {
    sendJson(response, 200, {
      ok: true,
      product: GXSERVER_PRODUCT,
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      requestId,
      result: {},
    });
    setTimeout(() => {
      runtime.shutdown();
    }, 25).unref();
    return endpoint.path;
  }

  sendJson(
    response,
    501,
    createRpcError("notImplemented", `${endpoint.path} is defined but not implemented in this milestone.`, requestId),
  );
  return endpoint.path;
}

async function stopAllTrackedZmxSessions(runtime: GxserverApiRuntime): Promise<Record<string, unknown>> {
  /*
  CDXC:GxserverCli 2026-06-02-18:45:
  `stop-all` is intentionally separate from `stop`: it is the explicit
  destructive path that kills gxserver-tracked zmx sessions before shutting down
  the control plane. Keep this local-only control API on existing zmx lifecycle
  helpers so provider state, failed-kill handling, and logging match ordinary
  session kill requests.
  */
  const db = openGxserverDatabase(runtime.paths);
  try {
    const repository = new GxserverDomainRepository(db, runtime.metadata.serverId);
    const sessions = repository.listSessions().filter(shouldStopAllKillSession);
    if (sessions.length === 0) {
      return {
        attemptedSessions: 0,
        failedSessions: 0,
        killedSessions: 0,
        skippedSessions: repository.listSessions().length,
      };
    }

    const zmx = await (runtime.zmxLifecycle?.requireZmx ?? requireBundledZmx)();
    const killByZmxName = new Map<string, Promise<GxserverProviderKillResult>>();
    for (const session of sessions) {
      const zmxName = providerZmxSessionName(session);
      if (!killByZmxName.has(zmxName)) {
        killByZmxName.set(
          zmxName,
          killZmxSession({
            runZsh: runtime.zmxLifecycle?.runZsh,
            sessionName: zmxName,
            zmxExecutablePath: zmx.executablePath,
          }),
        );
      }
    }

    let failedSessions = 0;
    let killedSessions = 0;
    for (const session of sessions) {
      const zmxName = providerZmxSessionName(session);
      const kill = await killByZmxName.get(zmxName)!;
      const timestamp = new Date().toISOString();
      const updated = repository.updateSession({
        lifecycleState: kill.killed ? "stopped" : "unknown",
        projectId: session.projectId,
        providerState: kill.killed
          ? missingProviderStatePatch(session, timestamp)
          : failedKillProviderStatePatch(session, kill, timestamp),
        sessionId: session.sessionId,
      });
      if (kill.killed) {
        killedSessions += 1;
      } else {
        failedSessions += 1;
      }
      await runtime.logger.log({
        event: kill.killed ? "zmx.stopAll.kill.completed" : "zmx.stopAll.kill.failed",
        level: kill.killed ? "info" : "warn",
        projectId: session.projectId,
        serverId: runtime.metadata.serverId,
        sessionId: session.sessionId,
        details: {
          exitCode: kill.exitCode,
          zmxName,
        },
        ...(kill.error ? { error: kill.error } : {}),
      });
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: updated.projectId,
        reason: "stop-all",
        sessionId: updated.sessionId,
      });
    }
    scheduleZmxTitleObserverSync(runtime, repository, "stop-all");
    return {
      attemptedSessions: sessions.length,
      failedSessions,
      killedSessions,
      skippedSessions: repository.listSessions().length - sessions.length,
      uniqueZmxSessions: killByZmxName.size,
    };
  } finally {
    db.close();
  }
}

function shouldStopAllKillSession(session: GxserverSessionDomainState): boolean {
  return session.lifecycleState !== "stopped" && session.providerState.lifecycleState !== "missing";
}

async function handleRendererCommandEndpoint(
  runtime: GxserverApiRuntime,
  body: unknown,
  requestId: string,
): Promise<GxserverRpcSuccessResponse> {
  const params = readRendererCommandRpcParams(body);
  /*
  CDXC:GxserverRendererCommands 2026-06-13-02:24:
  Keep the command contract in gxserver even when execution still belongs to the macOS renderer. This lets the daemon own auth, remote access, action allow-listing, timeouts, and HTTP result shape while native code only handles UI/AppKit/CEF/sidebar-local effects.
  */
  const result = await runtime.eventHub.dispatchRendererCommand(params);
  return {
    ok: true,
    product: GXSERVER_PRODUCT,
    protocolVersion: GXSERVER_PROTOCOL_VERSION,
    requestId,
    result,
  };
}

function readRendererCommandRpcParams(body: unknown): {
  action: GxserverRendererCommandAction;
  payload: Record<string, unknown>;
  timeoutMs?: number;
} {
  const params = readOperationRpcParams(body);
  const action = typeof params.action === "string" ? params.action : "";
  if (!isGxserverRendererCommandAction(action)) {
    throw new GxserverTypedOperationError("badRequest", `Unsupported renderer command action: ${String(params.action)}`);
  }
  const payload = params.payload === undefined ? {} : params.payload;
  if (!isRecord(payload)) {
    throw new GxserverTypedOperationError("badRequest", "Renderer command payload must be an object.");
  }
  const timeoutMs = params.timeoutMs === undefined ? undefined : Number(params.timeoutMs);
  if (timeoutMs !== undefined && (!Number.isFinite(timeoutMs) || timeoutMs <= 0)) {
    throw new GxserverTypedOperationError("badRequest", "Renderer command timeoutMs must be a positive number.");
  }
  return {
    action,
    payload,
    ...(timeoutMs !== undefined ? { timeoutMs } : {}),
  };
}

function isGxserverRendererCommandAction(value: string): value is GxserverRendererCommandAction {
  return (GXSERVER_RENDERER_COMMAND_ACTIONS as readonly string[]).includes(value);
}

async function handleDomainStateEndpoint(
  runtime: GxserverApiRuntime,
  endpointPath: GxserverEndpointPath,
  body: unknown,
  requestId: string,
): Promise<GxserverRpcSuccessResponse> {
  const db = openGxserverDatabase(runtime.paths);
  try {
    const repository = new GxserverDomainRepository(db, runtime.metadata.serverId);
    const result = await dispatchDomainStateEndpoint(runtime, db, repository, endpointPath, readDomainRpcParams(body), requestId);
    return {
      ok: true,
      product: GXSERVER_PRODUCT,
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      requestId,
      result,
    };
  } finally {
    db.close();
  }
}

async function dispatchDomainStateEndpoint(
  runtime: GxserverApiRuntime,
  db: ReturnType<typeof openGxserverDatabase>,
  repository: GxserverDomainRepository,
  endpointPath: GxserverEndpointPath,
  params: Record<string, unknown>,
  requestId: string,
): Promise<Record<string, unknown>> {
  const agentSettingsRepository = new GxserverAgentSettingsRepository(db);
  switch (endpointPath) {
    case "/api/readAgentSettings": {
      return agentSettingsRepository.readWithMetadata() as unknown as Record<string, unknown>;
    }
    case "/api/updateAgentSettings": {
      return {
        settings: agentSettingsRepository.update(params as unknown as GxserverUpdateAgentSettingsParams),
      };
    }
    case "/api/createProject": {
      const project = repository.createProject(params as unknown as GxserverCreateProjectParams);
      schedulePresentationProjectDelta(runtime, db, repository, {
        projectId: project.projectId,
        reason: "create-project",
        type: "projectAdded",
      });
      return { project };
    }
    case "/api/updateProject": {
      const project = repository.updateProject(params as unknown as GxserverUpdateProjectParams);
      schedulePresentationProjectDelta(runtime, db, repository, {
        projectId: project.projectId,
        reason: "update-project",
        type: "projectUpdated",
      });
      return { project };
    }
    case "/api/listProjects":
      return { projects: repository.listProjects() };
    case "/api/readProjectStatus": {
      const projectId = readProjectId(params);
      const project = repository.getProject(projectId);
      if (!project) {
        throw new GxserverDomainStateError("notFound", `Project ${projectId} does not exist.`);
      }
      await syncLiveZmxProcessIdentities(runtime, repository, {
        projectId,
        reason: "read-project-status",
        requestId,
      });
      scheduleAgentTitleMetadataChecksForSessions(runtime, repository, repository.listSessions(projectId), "read-project-status");
      return { project, sessions: repository.listSessions(projectId) };
    }
    case "/api/addProjectPath": {
      const project = addProjectPath(repository, params);
      schedulePresentationProjectDelta(runtime, db, repository, {
        projectId: project.projectId,
        reason: "add-project-path",
        type: "projectAdded",
      });
      return { project };
    }
    case "/api/removeProject": {
      const projectId = readProjectId(params);
      const project = repository.removeProject(projectId);
      schedulePresentationProjectDelta(runtime, db, repository, {
        projectId,
        reason: "remove-project",
        type: "projectUpdated",
      });
      return { project };
    }
    case "/api/deleteWorktreeProject": {
      const result = await deleteGxserverWorktreeProject({
        context: {
          abortSignal: undefined,
          envPath: process.env.PATH,
        },
        params: params as unknown as GxserverDeleteWorktreeProjectParams,
        repository,
      });
      /*
      CDXC:WorktreeDelete 2026-06-10-22:56:
      The shared delete endpoint owns presentation refresh for every client.
      After the helper removes the project row, this projectUpdated request
      builds a projectRemoved delta and advances the gxserver presentation
      revision so clients do not need local pruning fallbacks.
      */
      schedulePresentationProjectDelta(runtime, db, repository, {
        projectId: result.project.projectId,
        reason: "delete-worktree-project",
        type: "projectUpdated",
      });
      if (result.warnings.length > 0) {
        void runtime.logger.log({
          details: {
            forced: result.checkoutRemoval.forced,
            retriedForSubmodules: result.checkoutRemoval.retriedForSubmodules,
            warningCount: result.warnings.length,
          },
          event: "worktree.deleteProject.warning",
          level: "warn",
          projectId: result.project.projectId,
          requestId,
          serverId: runtime.metadata.serverId,
        });
      }
      return result as unknown as Record<string, unknown>;
    }
    case "/api/createSession":
    case "/api/createAgentSession": {
      const createParams =
        endpointPath === "/api/createAgentSession"
          ? normalizeCreateAgentSessionParams(repository, params, agentSettingsRepository.read())
          : normalizeCreateSessionParams(repository, params);
      const createdSession = repository.createSession(createParams);
      const presentation = applySessionStateEvent(repository, {
        agentName: createParams.agentId,
        agentSessionId: readRuntimeText(createParams.runtimeSettings, "agentSessionId"),
        agentSessionPath: readRuntimeText(createParams.runtimeSettings, "agentSessionPath"),
        identityUpdateSource: "lifecycle",
        onIdentityConflict: logSessionIdentityConflict(runtime, createdSession.projectId, createdSession.sessionId),
        projectId: createdSession.projectId,
        sessionId: createdSession.sessionId,
        startupText: readRuntimeText(createParams.runtimeSettings, "startupText") ?? readRuntimeText(createParams.launchSettings, "startupText"),
        title: createParams.title,
        titleSource: readRuntimeText(createParams.runtimeSettings, "titleSource") as GxserverSessionStateEventParams["titleSource"],
      });
      const suppressedSession = applyCreatedSessionActivitySuppression(runtime, repository, presentation.session, requestId);
      const reconciled = scheduleAgentTitleMetadataCheck(runtime, repository, {
        force: true,
        projectId: suppressedSession.projectId,
        reason: "create-session",
        sessionId: suppressedSession.sessionId,
      });
      const session = reconciled?.session ?? repository.getSession(suppressedSession.projectId, suppressedSession.sessionId) ?? suppressedSession;
      observeZmxTitleForSession(runtime, session, "create-session");
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: session.projectId,
        reason: "create-session",
        sessionId: session.sessionId,
      });
      return {
        session,
      };
    }
    case "/api/forkSession":
      return {
        fork: await forkSession(
          runtime,
          repository,
          params as unknown as GxserverSessionLifecycleParams,
          requestId,
          agentSettingsRepository.read(),
        ),
      };
    case "/api/readAgentLaunchPlan": {
      const projectId = readProjectId(params);
      const project = repository.getProject(projectId);
      if (!project) {
        throw new GxserverDomainStateError("notFound", `Project ${projectId} does not exist.`);
      }
      const agentId = readRequiredText(params.agentId, "agentId");
      return {
        plan: buildProjectAgentLaunchPlan(project, {
          agentId,
          agentSessionId: readOptionalText(params.agentSessionId),
        }, agentSettingsRepository.read()),
      };
    }
    case "/api/readAgentResumePlan": {
      const lifecycle = readSessionLifecycleParams(params);
      const project = repository.getProject(lifecycle.projectId);
      const session = repository.getSession(lifecycle.projectId, lifecycle.sessionId);
      if (!project) {
        throw new GxserverDomainStateError("notFound", `Project ${lifecycle.projectId} does not exist.`);
      }
      if (!session) {
        throw new GxserverDomainStateError(
          "notFound",
          `Session ${lifecycle.projectId}/${lifecycle.sessionId} does not exist.`,
        );
      }
      return {
        plan: buildAgentResumePlan(project, session, agentSettingsRepository.read()),
        session,
      };
    }
    case "/api/requestSessionRename": {
      const renameParams = params as unknown as GxserverSessionRenameRequestParams;
      const lifecycle = readSessionLifecycleParams(params);
      const result = applySessionRenameRequest(repository, {
        ...renameParams,
        projectId: lifecycle.projectId,
        sessionId: lifecycle.sessionId,
      });
      const reconciled = result.pendingAgentMetadata
        ? scheduleAgentTitleMetadataCheck(runtime, repository, {
            force: true,
            projectId: lifecycle.projectId,
            reason: "rename-request",
            sessionId: lifecycle.sessionId,
          })
        : undefined;
      const session = reconciled?.session ?? repository.getSession(lifecycle.projectId, lifecycle.sessionId) ?? result.session;
      observeZmxTitleForSession(runtime, session, "rename-request");
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: lifecycle.projectId,
        reason: "rename-request",
        sessionId: lifecycle.sessionId,
      });
      return {
        ...result,
        changed: result.changed || reconciled?.changed === true,
        projection: projectSessionTitle(session),
        reason: reconciled?.changed === true ? reconciled.reason : result.reason,
        session,
      } satisfies Record<string, unknown> & GxserverSessionRenameRequestResult;
    }
    case "/api/cancelFirstPromptAutoTitle": {
      const lifecycle = readSessionLifecycleParams(params);
      const result = cancelGxserverFirstPromptAutoTitle(repository, {
        ...(params as unknown as GxserverCancelFirstPromptAutoTitleParams),
        projectId: lifecycle.projectId,
        sessionId: lifecycle.sessionId,
      });
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: lifecycle.projectId,
        reason: "first-prompt-auto-title-cancelled",
        sessionId: lifecycle.sessionId,
      });
      return {
        ...result,
      } satisfies Record<string, unknown> & GxserverCancelFirstPromptAutoTitleResult;
    }
    case "/api/ingestAgentHookEvent": {
      const hookEvent = params as unknown as GxserverIngestAgentHookEventParams;
      const lifecycle = readSessionLifecycleParams(params);
      const current = repository.getSession(lifecycle.projectId, lifecycle.sessionId);
      if (!current) {
        throw new GxserverDomainStateError(
          "notFound",
          `Session ${lifecycle.projectId}/${lifecycle.sessionId} does not exist.`,
        );
      }
      /*
      CDXC:AgentHooks 2026-06-07-08:51:
      Hook activity is a gxserver-owned state transition, not a macOS sidebar fallback. Ingest provider metadata and explicit hook status together so every client receives the same working/attention/idle projection from gxserver presentation state.

      CDXC:AgentHooks 2026-06-09-21:59:
      Hook events may come from globally installed agent integrations. Reject cross-agent hook identity before applying passive metadata or explicit activity so a Cursor hook cannot relabel or pin working state on a gxserver-launched Codex fork.
      */
      const hookActivity = normalizeAgentHookActivity(
        hookEvent.status,
        hookEvent.eventName ?? hookEvent.rawEventName,
        hookEvent.agentName,
      );
      const hookObservedIdentity = resolveSessionIdentity({
        agentName: hookEvent.agentName,
        agentSessionId: hookEvent.agentSessionId,
        agentSessionPath: hookEvent.agentSessionPath,
      });
      const hookAgentMismatch = resolveSessionLaunchAgentMismatch(current, hookObservedIdentity.agentId);
      if (hookAgentMismatch) {
        logSessionLaunchAgentMismatch(runtime, lifecycle, hookAgentMismatch, "agent-hook-event", {
          activity: hookActivity,
          hasAgentSessionId: hookEvent.agentSessionId !== undefined,
          hasAgentSessionPath: hookEvent.agentSessionPath !== undefined,
          hasExplicitStatus: hookEvent.status !== undefined,
          hasFirstUserMessage: hookEvent.firstUserMessage !== undefined,
          hasHookEventName: hookEvent.eventName !== undefined || hookEvent.rawEventName !== undefined,
        });
        const previous = normalizeAgentActivityState(current.runtimeSettings.agentActivity, { activity: "idle" });
        return {
          activity: previous,
          changed: false,
          enteredAttention: false,
          previousActivity: previous.activity,
          projection: projectSessionTitle(current),
          reason: "agent-hook-agent-mismatch",
          session: current,
        } satisfies Record<string, unknown> & GxserverIngestAgentHookEventResult;
      }
      const metadata = applySessionStateEvent(repository, {
        ...hookEvent,
        identityUpdateSource: "passive",
        onIdentityConflict: logSessionIdentityConflict(runtime, lifecycle.projectId, lifecycle.sessionId),
        projectId: lifecycle.projectId,
        sessionId: lifecycle.sessionId,
      });
      if (isRejectedPassiveSessionIdentityConflict(metadata.identityConflict)) {
        logPassiveSessionIdentityEventRejected(runtime, lifecycle, metadata.identityConflict, "agent-hook-event", {
          activity: hookActivity,
          hasAgentSessionId: hookEvent.agentSessionId !== undefined,
          hasAgentSessionPath: hookEvent.agentSessionPath !== undefined,
          hasExplicitStatus: hookEvent.status !== undefined,
          hasFirstUserMessage: hookEvent.firstUserMessage !== undefined,
          hasHookEventName: hookEvent.eventName !== undefined || hookEvent.rawEventName !== undefined,
          hasTitle: hookEvent.title !== undefined,
        });
        const previous = normalizeAgentActivityState(current.runtimeSettings.agentActivity, { activity: "idle" });
        return {
          activity: previous,
          changed: false,
          enteredAttention: false,
          previousActivity: previous.activity,
          projection: projectSessionTitle(current),
          reason: "passive-session-identity-conflict",
          session: current,
        } satisfies Record<string, unknown> & GxserverIngestAgentHookEventResult;
      }
      let session = metadata.session;
      const hookActivityNowMs = parseAgentHookActivityTimestamp(hookEvent.statusUpdatedAt) ?? Date.now();
      const previousHookActivityState = normalizeAgentActivityState(session.runtimeSettings.agentActivity, { activity: "idle" });
      const previousHookAttentionSuppressedUntilMs = previousHookActivityState.attentionSuppressedUntil
        ? Date.parse(previousHookActivityState.attentionSuppressedUntil)
        : Number.NaN;
      const hookAttentionSuppressedByWindow =
        hookActivity === "attention" &&
        Number.isFinite(previousHookAttentionSuppressedUntilMs) &&
        hookActivityNowMs < previousHookAttentionSuppressedUntilMs;
      let activityUpdate: ReturnType<typeof updateSessionActivitySettings> | undefined;
      let activityChanged = false;
      let activityReason = hookActivity ? "activity-unchanged" : "metadata-only";
      if (hookActivity) {
        if (isStaleAgentHookActivityEvent(session.runtimeSettings.agentActivity, hookActivityNowMs)) {
          activityReason = "stale-activity-event";
        } else {
          activityUpdate = updateSessionActivitySettings(session, {
            activity: hookActivity,
            agentName: hookEvent.agentName,
            nowMs: hookActivityNowMs,
            projectId: lifecycle.projectId,
            sessionId: lifecycle.sessionId,
          });
          activityChanged = shouldPersistSessionStatusUpdate(session, activityUpdate);
          if (activityChanged) {
            session = repository.updateSession({
              lastActiveAt: activityUpdate.lastActiveAt,
              projectId: session.projectId,
              runtimeSettings: activityUpdate.runtimeSettings,
              sessionId: session.sessionId,
            });
            scheduleStaleActivityPresentationRefresh(runtime, session, "agent-hook-stale-activity");
          }
          activityReason = activityChanged ? "activity-updated" : "activity-unchanged";
          if (hookAttentionSuppressedByWindow) {
            void runtime.logger.log({
              details: {
                agentName: hookEvent.agentName,
                changed: activityChanged,
                hasExplicitStatus: hookEvent.status !== undefined,
                hasHookEventName: hookEvent.eventName !== undefined || hookEvent.rawEventName !== undefined,
                incomingActivity: hookActivity,
                previousActivity: previousHookActivityState.activity,
                resultActivity: activityUpdate.activity.activity,
                source: "agent-hook",
                suppressionRemainingMs: Math.max(0, previousHookAttentionSuppressedUntilMs - hookActivityNowMs),
                suppressedUntil: previousHookActivityState.attentionSuppressedUntil,
              },
              event: "sessionActivity.attentionSuppressed",
              level: "info",
              projectId: lifecycle.projectId,
              requestId,
              serverId: runtime.metadata.serverId,
              sessionId: lifecycle.sessionId,
            });
          }
        }
      }
      const reconciled = scheduleAgentTitleMetadataCheck(runtime, repository, {
        force: true,
        projectId: lifecycle.projectId,
        reason: "agent-hook-event",
        sessionId: lifecycle.sessionId,
      });
      session = reconciled?.session ?? repository.getSession(lifecycle.projectId, lifecycle.sessionId) ?? session;
      const autoTitleClaim = claimGxserverFirstPromptAutoTitle(repository, session, hookEvent.firstUserMessage);
      const responseSession = autoTitleClaim.session ?? session;
      if (autoTitleClaim.claimed) {
        scheduleGxserverFirstPromptAutoTitleJob(runtime, {
          projectId: lifecycle.projectId,
          requestId,
          sessionId: lifecycle.sessionId,
        });
      }
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: lifecycle.projectId,
        reason: "agent-hook-event",
        sessionId: lifecycle.sessionId,
      });
      return {
        ...(activityUpdate ? { activity: activityUpdate.activity } : {}),
        changed: metadata.changed || activityChanged || reconciled?.changed === true || autoTitleClaim.claimed,
        enteredAttention: activityUpdate?.enteredAttention ?? false,
        ...(activityUpdate ? { previousActivity: activityUpdate.previousActivity } : {}),
        projection: projectSessionTitle(responseSession),
        reason: reconciled?.changed === true
          ? reconciled.reason
          : autoTitleClaim.claimed
            ? "first-prompt-auto-title-claimed"
            : activityReason !== "metadata-only"
              ? activityReason
              : metadata.reason,
        session: responseSession,
      } satisfies Record<string, unknown> & GxserverIngestAgentHookEventResult;
    }
    case "/api/ingestSessionStateEvent": {
      const stateEvent = params as unknown as GxserverSessionStateEventParams;
      const lifecycle = readSessionLifecycleParams(params);
      const current = repository.getSession(lifecycle.projectId, lifecycle.sessionId);
      if (!current) {
        throw new GxserverDomainStateError(
          "notFound",
          `Session ${lifecycle.projectId}/${lifecycle.sessionId} does not exist.`,
        );
      }
      const stateObservedIdentity = resolveSessionIdentity({
        agentName: stateEvent.agentName,
        agentSessionId: stateEvent.agentSessionId,
        agentSessionPath: stateEvent.agentSessionPath,
        startupText: stateEvent.startupText,
      });
      const stateAgentMismatch = resolveSessionLaunchAgentMismatch(current, stateObservedIdentity.agentId);
      if (stateAgentMismatch) {
        logSessionLaunchAgentMismatch(runtime, lifecycle, stateAgentMismatch, "session-state-event", {
          hasAgentSessionId: stateEvent.agentSessionId !== undefined,
          hasAgentSessionPath: stateEvent.agentSessionPath !== undefined,
          hasFirstUserMessage: stateEvent.firstUserMessage !== undefined,
          hasStartupText: stateEvent.startupText !== undefined,
          hasTitle: stateEvent.title !== undefined,
        });
        return {
          changed: false,
          projection: projectSessionTitle(current),
          reason: "session-state-agent-mismatch",
          session: current,
        } satisfies Record<string, unknown> & GxserverSessionStateEventResult;
      }
      const result = applySessionStateEvent(repository, {
        ...stateEvent,
        identityUpdateSource: "passive",
        onIdentityConflict: logSessionIdentityConflict(runtime, lifecycle.projectId, lifecycle.sessionId),
        projectId: lifecycle.projectId,
        sessionId: lifecycle.sessionId,
      });
      if (isRejectedPassiveSessionIdentityConflict(result.identityConflict)) {
        logPassiveSessionIdentityEventRejected(runtime, lifecycle, result.identityConflict, "session-state-event", {
          hasAgentSessionId: stateEvent.agentSessionId !== undefined,
          hasAgentSessionPath: stateEvent.agentSessionPath !== undefined,
          hasFirstUserMessage: stateEvent.firstUserMessage !== undefined,
          hasStartupText: stateEvent.startupText !== undefined,
          hasTitle: stateEvent.title !== undefined,
        });
        return {
          ...result,
        } satisfies Record<string, unknown> & GxserverSessionStateEventResult;
      }
      const reconciled = scheduleAgentTitleMetadataCheck(runtime, repository, {
        force: true,
        projectId: lifecycle.projectId,
        reason: "session-state-event",
        sessionId: lifecycle.sessionId,
      });
      const session = reconciled?.session ?? repository.getSession(lifecycle.projectId, lifecycle.sessionId) ?? result.session;
      const autoTitleClaim = claimGxserverFirstPromptAutoTitle(repository, session, stateEvent.firstUserMessage);
      const responseSession = autoTitleClaim.session ?? session;
      if (autoTitleClaim.claimed) {
        scheduleGxserverFirstPromptAutoTitleJob(runtime, {
          projectId: lifecycle.projectId,
          requestId,
          sessionId: lifecycle.sessionId,
        });
      }
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: lifecycle.projectId,
        reason: "session-state-event",
        sessionId: lifecycle.sessionId,
      });
      return {
        ...result,
        changed: result.changed || reconciled?.changed === true || autoTitleClaim.claimed,
        projection: projectSessionTitle(responseSession),
        reason: reconciled?.changed === true ? reconciled.reason : result.reason,
        session: responseSession,
      } satisfies Record<string, unknown> & GxserverSessionStateEventResult;
    }
    case "/api/ingestTerminalTitleEvent": {
      const titleEvent = params as unknown as GxserverTerminalTitleEventParams;
      const lifecycle = readSessionLifecycleParams(params);
      const current = repository.getSession(lifecycle.projectId, lifecycle.sessionId);
      if (!current) {
        throw new GxserverDomainStateError(
          "notFound",
          `Session ${lifecycle.projectId}/${lifecycle.sessionId} does not exist.`,
        );
      }
      const presentationDeltaBefore = stringifyImmediateTerminalTitlePresentationDelta(
        repository,
        lifecycle.projectId,
        lifecycle.sessionId,
      );
      const decision = applyTerminalTitleEvent(repository, {
        ...titleEvent,
        projectId: lifecycle.projectId,
        sessionId: lifecycle.sessionId,
      });
      const titledSession = decision.session ?? repository.getSession(lifecycle.projectId, lifecycle.sessionId) ?? current;
      const titleDetectedAgentName = getTerminalTitleDetectedAgentName(titleEvent.rawTitle);
      const presentation =
        decision.agentSessionId || titleDetectedAgentName
          ? applySessionStateEvent(repository, {
              /*
              CDXC:ClaudeSessionIdentity 2026-06-11-21:43:
              A Claude Code terminal can expose only the program title before any hook metadata arrives. Promote that recognized title observation through gxserver's normal session-identity reducer so every client receives the same Claude agent row instead of macOS keeping a local-only terminal classification.
              */
              agentName: titleEvent.agentName ?? titleDetectedAgentName,
              ...(decision.agentSessionId ? { agentSessionId: decision.agentSessionId } : {}),
              identityUpdateSource: "terminal-title",
              onIdentityConflict: logSessionIdentityConflict(runtime, lifecycle.projectId, lifecycle.sessionId),
              projectId: lifecycle.projectId,
              sessionId: lifecycle.sessionId,
            })
          : undefined;
      const presentedSession = presentation?.session ?? titledSession;
      const previousActivityState = normalizeAgentActivityState(presentedSession.runtimeSettings.agentActivity, { activity: "idle" });
      const titleActivityNowMs = Date.now();
      const previousSuppressedUntilMs = previousActivityState.suppressedUntil
        ? Date.parse(previousActivityState.suppressedUntil)
        : Number.NaN;
      const previousAttentionSuppressedUntilMs = previousActivityState.attentionSuppressedUntil
        ? Date.parse(previousActivityState.attentionSuppressedUntil)
        : Number.NaN;
      const statusUpdate = updateSessionActivitySettings(presentedSession, {
        agentName: titleEvent.agentName ?? titleDetectedAgentName,
        event: "title",
        nowMs: titleActivityNowMs,
        projectId: lifecycle.projectId,
        sessionId: lifecycle.sessionId,
        settledTitle: readAgentMetadataSettledTitle(presentedSession),
        title: titleEvent.rawTitle,
      });
      const activitySuppressedByWindow =
        Number.isFinite(previousSuppressedUntilMs) && titleActivityNowMs < previousSuppressedUntilMs;
      const attentionSuppressedByWindow =
        Number.isFinite(previousAttentionSuppressedUntilMs) && titleActivityNowMs < previousAttentionSuppressedUntilMs;
      /*
      CDXC:GxserverSessionTitles 2026-06-06-07:09:
      zmx title streams can produce many repeated spinner frames. Preserve title-derived working detection by storing real activity-window changes, but do not rewrite the session row, force metadata checks, or broadcast sidebar presentation deltas when a terminal-title event leaves both stored status and projected session chrome unchanged.
      */
      const shouldStoreStatusUpdate = shouldPersistSessionStatusUpdate(presentedSession, statusUpdate);
      const session = shouldStoreStatusUpdate
        ? repository.updateSession({
            lastActiveAt: statusUpdate.lastActiveAt,
            projectId: presentedSession.projectId,
            runtimeSettings: statusUpdate.runtimeSettings,
            sessionId: presentedSession.sessionId,
          })
        : presentedSession;
      if (shouldStoreStatusUpdate) {
        scheduleStaleActivityPresentationRefresh(runtime, session, "terminal-title-stale-activity");
      }
      const shouldCheckAgentTitleMetadata = decision.changed || presentation?.changed === true;
      const reconciled = shouldCheckAgentTitleMetadata
        ? scheduleAgentTitleMetadataCheck(runtime, repository, {
            force: true,
            projectId: lifecycle.projectId,
            reason: "terminal-title-event",
            sessionId: lifecycle.sessionId,
          })
        : undefined;
      const responseSession = reconciled?.session ?? repository.getSession(lifecycle.projectId, lifecycle.sessionId) ?? session;
      /*
      CDXC:GxserverSessionTitles 2026-06-08-09:30:
      Same-title terminal observations can update status bookkeeping such as `updatedAt`, `lastActiveAt`, or title-derived activity timestamps without changing the sidebar row. Compare only immediate visible presentation chrome before broadcasting a title-event delta so passive sidebar hydrates do not take macOS keyboard focus from the terminal while users type.
      */
      const presentationDeltaAfter = stringifyImmediateTerminalTitlePresentationDelta(
        repository,
        lifecycle.projectId,
        lifecycle.sessionId,
      );
      const presentationDeltaScheduled = presentationDeltaBefore !== presentationDeltaAfter;
      if (presentationDeltaScheduled) {
        schedulePresentationSessionDelta(runtime, repository, {
          projectId: lifecycle.projectId,
          reason: "terminal-title-event",
          sessionId: lifecycle.sessionId,
        });
      }
      const response = {
        agentSessionId: decision.agentSessionId,
        activity: statusUpdate.activity,
        changed: decision.changed || presentation?.changed === true || reconciled?.changed === true,
        enteredAttention: statusUpdate.enteredAttention,
        previousActivity: statusUpdate.previousActivity,
        projection: projectSessionTitle(responseSession),
        reason: reconciled?.changed === true ? reconciled.reason : decision.reason,
        session: responseSession,
        visibleTitle: decision.visibleTitle,
      } satisfies GxserverTerminalTitleEventResult;
      /*
      CDXC:GxserverSessionTitles 2026-06-01-20:59:
      Title ownership lives in gxserver for every client. Log terminal-title ingest decisions at the shared API boundary so noisy settled-title streams, rejected titles, metadata reconciliation, and final presentation title source can be diagnosed without duplicating title logic in macOS, TUI, CLI, or mobile clients.
      */
      const rawTitleForLog = titleEvent.rawTitle ?? "";
      void runtime.logger.log({
        details: {
          activity: statusUpdate.activity,
          agentName: titleEvent.agentName ?? titleDetectedAgentName,
          agentSessionIdCaptured: decision.agentSessionId !== undefined,
          agentSessionIdCapturedHash: hashLogIdentity(decision.agentSessionId),
          agentSessionIdUpdateSource: decision.agentSessionId !== undefined ? "terminal-title" : undefined,
          agentDetectedFromTitle: titleDetectedAgentName,
          activitySuppressedByWindow,
          activitySuppressedUntil: previousActivityState.suppressedUntil,
          activitySuppressionRemainingMs: activitySuppressedByWindow
            ? Math.max(0, previousSuppressedUntilMs - titleActivityNowMs)
            : undefined,
          attentionSuppressedByWindow,
          attentionSuppressedUntil: previousActivityState.attentionSuppressedUntil,
          attentionSuppressionRemainingMs: attentionSuppressedByWindow
            ? Math.max(0, previousAttentionSuppressedUntilMs - titleActivityNowMs)
            : undefined,
          changed: response.changed,
          decisionChanged: decision.changed,
          decisionReason: decision.reason,
          metadataChanged: reconciled?.changed === true,
          metadataCheckQueued: shouldCheckAgentTitleMetadata,
          metadataReason: reconciled?.reason,
          metadataTitleFound: reconciled?.metadataTitleFound,
          presentationChanged: presentation?.changed === true,
          presentationDeltaScheduled,
          provider: titleEvent.sessionPersistenceProvider,
          rawTitleLength: rawTitleForLog.length,
          responseReason: response.reason,
          responseTitleSource: responseSession.runtimeSettings.titleSource,
          statusUpdateStored: shouldStoreStatusUpdate,
          sessionTitleBeforeLength: current.title.length,
          sessionTitleDecisionLength: titledSession.title.length,
          visibleTitleLength: decision.visibleTitle?.length,
        },
        event: "sessionTitle.terminalTitleEvent",
        level: response.changed ? "info" : "debug",
        projectId: lifecycle.projectId,
        serverId: runtime.metadata.serverId,
        sessionId: lifecycle.sessionId,
      });
      return response;
    }
    case "/api/updateAgentActivity": {
      const activity = params as unknown as GxserverUpdateAgentActivityParams;
      const lifecycle = readSessionLifecycleParams(params);
      const current = repository.getSession(lifecycle.projectId, lifecycle.sessionId);
      if (!current) {
        throw new GxserverDomainStateError(
          "notFound",
          `Session ${lifecycle.projectId}/${lifecycle.sessionId} does not exist.`,
        );
      }
      const activityAgentMismatch = resolveSessionLaunchAgentMismatch(current, activity.agentName);
      if (activityAgentMismatch) {
        logSessionLaunchAgentMismatch(runtime, lifecycle, activityAgentMismatch, "update-agent-activity", {
          activity: activity.activity,
          event: activity.event,
          hasTitle: activity.title !== undefined,
        });
        const previous = normalizeAgentActivityState(current.runtimeSettings.agentActivity, { activity: "idle" });
        return {
          activity: previous,
          enteredAttention: false,
          previousActivity: previous.activity,
          session: current,
        } satisfies Record<string, unknown> & GxserverUpdateAgentActivityResult;
      }
      const previousActivityState = normalizeAgentActivityState(current.runtimeSettings.agentActivity, { activity: "idle" });
      const update = updateSessionActivitySettings(current, activity);
      const session = repository.updateSession({
        lastActiveAt: update.lastActiveAt,
        projectId: current.projectId,
        runtimeSettings: update.runtimeSettings,
        sessionId: current.sessionId,
      });
      scheduleStaleActivityPresentationRefresh(runtime, session, "agent-activity-stale-activity");
      scheduleAgentTitleMetadataCheck(runtime, repository, {
        force: true,
        projectId: lifecycle.projectId,
        reason: "agent-activity",
        sessionId: lifecycle.sessionId,
      });
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: lifecycle.projectId,
        reason: "agent-activity",
        sessionId: lifecycle.sessionId,
      });
      if (activity.event === "escape") {
        const suppressedUntilMs = update.activity.attentionSuppressedUntil
          ? Date.parse(update.activity.attentionSuppressedUntil)
          : Number.NaN;
        const changed = shouldPersistSessionStatusUpdate(current, update);
        void runtime.logger.log({
          details: {
            agentName: activity.agentName,
            changed,
            previousActivity: previousActivityState.activity,
            previousHadAttentionSuppression: previousActivityState.attentionSuppressedUntil !== undefined,
            resultActivity: update.activity.activity,
            source: "terminal-escape",
            suppressedUntil: update.activity.attentionSuppressedUntil,
            suppressionMs: Number.isFinite(suppressedUntilMs) ? Math.max(0, suppressedUntilMs - Date.now()) : undefined,
          },
          event: "sessionActivity.escapeSuppressionStarted",
          level: "info",
          projectId: lifecycle.projectId,
          requestId,
          serverId: runtime.metadata.serverId,
          sessionId: lifecycle.sessionId,
        });
      }
      return {
        activity: update.activity,
        enteredAttention: update.enteredAttention,
        previousActivity: update.previousActivity,
        session,
      };
    }
    case "/api/readPresentationSnapshot":
      await syncSessionStateSidecars(runtime, repository, {
        reason: "read-presentation-snapshot",
        requestId,
      });
      return {
        snapshot: readGxserverPresentationSnapshot(db, runtime.metadata.serverId),
      };
    case "/api/searchSessions":
      return searchGxserverPresentation(
        db,
        runtime.metadata.serverId,
        params as unknown as GxserverPresentationSearchParams,
      ) as unknown as Record<string, unknown>;
    case "/api/listPreviousSessions":
      return listGxserverPreviousSessions(db, runtime.metadata.serverId, {
        ...(params as unknown as GxserverPresentationSearchParams),
        includeActive: false,
        includePrevious: true,
      }) as unknown as Record<string, unknown>;
    case "/api/updateSession":
    case "/api/attachSessionMetadata": {
      const updateParams = normalizeMetadataSessionUpdateParams(
        repository,
        params as unknown as GxserverUpdateSessionParams,
        endpointPath === "/api/updateSession" ? "update-session" : "attach-session-metadata",
      );
      const session = repository.updateSession(updateParams);
      scheduleZmxTitleObserverSync(runtime, repository, endpointPath === "/api/updateSession" ? "update-session" : "attach-session-metadata");
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: session.projectId,
        reason: endpointPath === "/api/updateSession" ? "update-session" : "attach-session-metadata",
        sessionId: session.sessionId,
      });
      return { session };
    }
    case "/api/updateSessionOrder": {
      const sessions = repository.updateSessionOrder(params as unknown as GxserverUpdateSessionOrderParams);
      for (const session of sessions) {
        schedulePresentationSessionDelta(runtime, repository, {
          projectId: session.projectId,
          reason: "update-session-order",
          sessionId: session.sessionId,
        });
      }
      return { sessions };
    }
    case "/api/listSessions": {
      const projectId = readOptionalProjectId(params);
      await syncSessionStateSidecars(runtime, repository, {
        projectId,
        reason: "list-sessions",
        requestId,
      });
      const sessions = repository.listSessions(projectId);
      scheduleAgentTitleMetadataChecksForSessions(runtime, repository, sessions, "list-sessions");
      return { sessions };
    }
    case "/api/removeSession": {
      const removeParams = params as unknown as GxserverRemoveSessionParams;
      const session = repository.removeSession(removeParams);
      /*
      CDXC:PreviousSessions 2026-06-02-11:24:
      Previous-session delete/restore cleanup must mutate gxserver session history and notify subscribed clients. Reuse the normal sessionRemoved presentation delta so native local-first deletion reconciles through the shared websocket feed instead of staying hidden only in WK memory.
      */
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: session.projectId,
        reason: removeParams.reason ?? "remove-session",
        sessionId: session.sessionId,
      });
      return { session };
    }
    default:
      throw new GxserverDomainStateError("notFound", `${endpointPath} is not a gxserver domain-state endpoint.`);
  }
}

interface GxserverSessionStateSidecar {
  agentName?: string;
  agentSessionId?: string;
  agentSessionPath?: string;
  firstUserMessage?: string;
  lastActivityAt?: string;
  pendingFirstPromptAutoRenamePrompt?: string;
  status?: GxserverAgentActivityState["activity"];
  statusUpdatedAt?: string;
  title?: string;
}

async function syncSessionStateSidecars(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  input: {
    projectId?: GxserverProjectId;
    reason: string;
    requestId?: string;
  },
): Promise<void> {
  /*
  CDXC:MobileSessionStatus 2026-06-11-23:52:
  Android and iOS session cards must receive agent working/attention/idle state from gxserver alone when they connect over SSH. Read the legacy hook sidecar files at the daemon boundary before list/snapshot responses so the macOS app can be closed without becoming the status-forwarding service.
  */
  for (const session of repository.listSessions(input.projectId)) {
    if (!shouldSyncSessionStateSidecar(session)) {
      continue;
    }
    const sidecar = await readSessionStateSidecar(runtime.paths, session);
    if (!sidecar || !hasSessionStateSidecarPayload(sidecar)) {
      continue;
    }
    applySessionStateSidecar(runtime, repository, session, sidecar, input);
  }
  await syncLiveZmxProcessIdentities(runtime, repository, input);
}

function shouldSyncSessionStateSidecar(session: GxserverSessionDomainState): boolean {
  return session.lifecycleState === "running" && session.surface !== "commands";
}

async function syncLiveZmxProcessIdentities(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  input: {
    projectId?: GxserverProjectId;
    reason: string;
    requestId?: string;
  },
): Promise<void> {
  const sessions = repository.listSessions(input.projectId).filter(shouldSyncLiveZmxProcessIdentity);
  const sessionNames = sessions.map((session) => providerZmxSessionName(session));
  if (sessionNames.length === 0) {
    return;
  }
  let zmx: GxserverResolvedTool;
  try {
    zmx = await (runtime.zmxLifecycle?.requireZmx ?? requireBundledZmx)();
  } catch (error) {
    await runtime.logger.log({
      details: {
        reason: input.reason,
        sessionCount: sessionNames.length,
      },
      error: error instanceof Error ? error.message : String(error),
      event: "sessionIdentity.liveProcessSync.skipped",
      level: "debug",
      requestId: input.requestId,
      serverId: runtime.metadata.serverId,
    });
    return;
  }
  let identities: ReadonlyMap<GxserverZmxSessionName, { agentId?: string; agentSessionId?: string; agentSessionPath?: string }>;
  try {
    identities = await (runtime.zmxLifecycle?.readSessionProcessIdentities ?? readZmxSessionProcessIdentities)({
      runZsh: runtime.zmxLifecycle?.runZsh,
      sessionNames,
      zmxExecutablePath: zmx.executablePath,
    });
  } catch (error) {
    await runtime.logger.log({
      details: {
        reason: input.reason,
        sessionCount: sessionNames.length,
      },
      error: error instanceof Error ? error.message : String(error),
      event: "sessionIdentity.liveProcessSync.failed",
      level: "debug",
      requestId: input.requestId,
      serverId: runtime.metadata.serverId,
    });
    return;
  }
  for (const originalSession of sessions) {
    const current = repository.getSession(originalSession.projectId, originalSession.sessionId);
    if (!current || !shouldSyncLiveZmxProcessIdentity(current)) {
      continue;
    }
    const identity = identities.get(providerZmxSessionName(current));
    if (!identity?.agentId) {
      continue;
    }
    const launchAgentMismatch = resolveSessionLaunchAgentMismatch(current, identity.agentId);
    if (launchAgentMismatch) {
      logSessionLaunchAgentMismatch(runtime, {
        projectId: current.projectId,
        sessionId: current.sessionId,
      }, launchAgentMismatch, "live-process-identity", {
        hasAgentSessionId: identity.agentSessionId !== undefined,
        hasAgentSessionPath: identity.agentSessionPath !== undefined,
      });
      continue;
    }
    const result = applySessionStateEvent(repository, {
      agentName: identity.agentId,
      agentSessionId: identity.agentSessionId,
      agentSessionPath: identity.agentSessionPath,
      identityUpdateSource: "live-process",
      onIdentityConflict: logSessionIdentityConflict(runtime, current.projectId, current.sessionId),
      projectId: current.projectId,
      sessionId: current.sessionId,
    });
    const reconciled = result.changed
      ? scheduleAgentTitleMetadataCheck(runtime, repository, {
          force: true,
          projectId: current.projectId,
          reason: "live-process-identity",
          sessionId: current.sessionId,
        })
      : undefined;
    if (result.changed || reconciled?.changed === true) {
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: current.projectId,
        reason: input.reason,
        sessionId: current.sessionId,
      });
    }
  }
}

function shouldSyncLiveZmxProcessIdentity(session: GxserverSessionDomainState): boolean {
  /*
  CDXC:GxserverSessionIdentity 2026-06-12-04:41:
  Hook metadata can be stale when a generic terminal helper starts a different live agent, e.g. a Claude-labeled row whose active zmx process tree is Codex. Let gxserver repair only running zmx-backed workspace sessions from live process evidence so all clients get the current agent/session id while stopped history and command panes keep their stored metadata.
  */
  return (
    session.lifecycleState === "running" &&
    session.surface !== "commands" &&
    readRuntimeText(session.runtimeSettings, "sessionPersistenceProvider") === "zmx"
  );
}

async function readSessionStateSidecar(
  paths: GxserverPaths,
  session: Pick<GxserverSessionDomainState, "projectId" | "sessionId">,
): Promise<GxserverSessionStateSidecar | undefined> {
  const statePath = buildSessionStateSidecarPath(paths, session.projectId, session.sessionId);
  const fileStats = await stat(statePath).catch(() => undefined);
  if (!fileStats?.isFile() || fileStats.size > GXSERVER_SESSION_STATE_SIDECAR_MAX_BYTES) {
    return undefined;
  }
  const rawState = await readFile(statePath, "utf8").catch(() => undefined);
  return rawState?.trim() ? parseSessionStateSidecar(rawState) : undefined;
}

function buildSessionStateSidecarPath(
  paths: GxserverPaths,
  projectId: GxserverProjectId,
  sessionId: GxserverSessionId,
): string {
  return path.join(
    path.dirname(paths.rootDir),
    "session-state",
    sanitizeSessionStateSidecarPathPart(projectId),
    `${sanitizeSessionStateSidecarPathPart(sessionId)}.env`,
  );
}

function sanitizeSessionStateSidecarPathPart(value: string): string {
  return value.replace(/[^a-z0-9._-]+/giu, "-").replace(/^-+|-+$/gu, "") || "session";
}

function parseSessionStateSidecar(rawState: string): GxserverSessionStateSidecar {
  const state: GxserverSessionStateSidecar = {};
  for (const line of rawState.split(/\r?\n/gu)) {
    const separator = line.indexOf("=");
    if (separator < 0) {
      continue;
    }
    const key = line.slice(0, separator);
    const rawValue = line.slice(separator + 1).trim();
    const value =
      key === "firstUserMessageBase64" || key === "agentSessionPath"
        ? rawValue
        : rawValue.replace(/\s+/gu, " ");
    if (!value) {
      continue;
    }
    if (key === "agent") {
      state.agentName = value;
    } else if (key === "agentSessionId") {
      state.agentSessionId = value;
    } else if (key === "agentSessionPath") {
      state.agentSessionPath = value;
    } else if (key === "firstUserMessageBase64") {
      state.firstUserMessage = decodeSessionStateSidecarBase64(value);
    } else if (key === "lastActivityAt") {
      state.lastActivityAt = normalizeSidecarIsoTimestamp(value);
    } else if (key === "pendingFirstPromptAutoRenamePrompt") {
      state.pendingFirstPromptAutoRenamePrompt = value;
    } else if (key === "status" && (value === "attention" || value === "idle" || value === "working")) {
      state.status = value;
    } else if (key === "statusUpdatedAt") {
      state.statusUpdatedAt = normalizeSidecarIsoTimestamp(value);
    } else if (key === "title") {
      state.title = getVisibleTerminalTitle(value);
    }
  }
  state.firstUserMessage = state.firstUserMessage ?? state.pendingFirstPromptAutoRenamePrompt;
  return state;
}

function hasSessionStateSidecarPayload(state: GxserverSessionStateSidecar): boolean {
  return Boolean(
    state.agentName ||
      state.agentSessionId ||
      state.agentSessionPath ||
      state.firstUserMessage ||
      state.status ||
      state.title,
  );
}

function decodeSessionStateSidecarBase64(value: string): string | undefined {
  try {
    const decoded = Buffer.from(value, "base64").toString("utf8").trim();
    return decoded || undefined;
  } catch {
    return undefined;
  }
}

function normalizeSidecarIsoTimestamp(value: string): string | undefined {
  const timestamp = Date.parse(value);
  return Number.isFinite(timestamp) ? new Date(timestamp).toISOString() : undefined;
}

function applySessionStateSidecar(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  originalSession: GxserverSessionDomainState,
  sidecar: GxserverSessionStateSidecar,
  input: {
    reason: string;
    requestId?: string;
  },
): void {
  const current = repository.getSession(originalSession.projectId, originalSession.sessionId);
  if (!current) {
    return;
  }
  const params: GxserverIngestAgentHookEventParams = {
    agentName: sidecar.agentName,
    agentSessionId: sidecar.agentSessionId,
    agentSessionPath: sidecar.agentSessionPath,
    eventName: "legacy-session-state",
    firstUserMessage: sidecar.firstUserMessage,
    projectId: current.projectId,
    sessionId: current.sessionId,
    status: sidecar.status,
    statusUpdatedAt: sidecar.statusUpdatedAt ?? sidecar.lastActivityAt,
    title: sidecar.title,
  };
  const sidecarActivity = normalizeAgentHookActivity(params.status, params.eventName, params.agentName);
  const observedIdentity = resolveSessionIdentity({
    agentName: params.agentName,
    agentSessionId: params.agentSessionId,
    agentSessionPath: params.agentSessionPath,
  });
  const launchAgentMismatch = resolveSessionLaunchAgentMismatch(current, observedIdentity.agentId);
  if (launchAgentMismatch) {
    logSessionLaunchAgentMismatch(runtime, {
      projectId: current.projectId,
      sessionId: current.sessionId,
    }, launchAgentMismatch, "agent-hook-event", {
      activity: sidecarActivity,
      hasAgentSessionId: params.agentSessionId !== undefined,
      hasAgentSessionPath: params.agentSessionPath !== undefined,
      hasExplicitStatus: params.status !== undefined,
      hasFirstUserMessage: params.firstUserMessage !== undefined,
      hasHookEventName: false,
    });
    return;
  }

  const metadata = applySessionStateEvent(repository, {
    ...params,
    identityUpdateSource: "passive",
    onIdentityConflict: logSessionIdentityConflict(runtime, current.projectId, current.sessionId),
  });
  if (isRejectedPassiveSessionIdentityConflict(metadata.identityConflict)) {
    logPassiveSessionIdentityEventRejected(runtime, {
      projectId: current.projectId,
      sessionId: current.sessionId,
    }, metadata.identityConflict, "agent-hook-event", {
      activity: sidecarActivity,
      hasAgentSessionId: params.agentSessionId !== undefined,
      hasAgentSessionPath: params.agentSessionPath !== undefined,
      hasExplicitStatus: params.status !== undefined,
      hasFirstUserMessage: params.firstUserMessage !== undefined,
      hasHookEventName: false,
      hasTitle: params.title !== undefined,
    });
    return;
  }

  let session = metadata.session;
  let activityChanged = false;
  if (sidecarActivity) {
    const sidecarActivityNowMs = parseAgentHookActivityTimestamp(params.statusUpdatedAt) ?? Date.now();
    if (!isStaleAgentHookActivityEvent(session.runtimeSettings.agentActivity, sidecarActivityNowMs)) {
      const activityUpdate = updateSessionActivitySettings(session, {
        activity: sidecarActivity,
        agentName: params.agentName,
        nowMs: sidecarActivityNowMs,
        projectId: current.projectId,
        sessionId: current.sessionId,
      });
      activityChanged = shouldPersistSessionStatusUpdate(session, activityUpdate);
      if (activityChanged) {
        session = repository.updateSession({
          lastActiveAt: activityUpdate.lastActiveAt,
          projectId: session.projectId,
          runtimeSettings: activityUpdate.runtimeSettings,
          sessionId: session.sessionId,
        });
        scheduleStaleActivityPresentationRefresh(runtime, session, "session-state-sidecar-stale-activity");
      }
    }
  }

  const reconciled = scheduleAgentTitleMetadataCheck(runtime, repository, {
    force: true,
    projectId: current.projectId,
    reason: "session-state-sidecar",
    sessionId: current.sessionId,
  });
  session = reconciled?.session ?? repository.getSession(current.projectId, current.sessionId) ?? session;
  const autoTitleClaim = claimGxserverFirstPromptAutoTitle(repository, session, params.firstUserMessage);
  if (autoTitleClaim.claimed) {
    scheduleGxserverFirstPromptAutoTitleJob(runtime, {
      projectId: current.projectId,
      requestId: input.requestId ?? "session-state-sidecar",
      sessionId: current.sessionId,
    });
  }
  if (metadata.changed || activityChanged || reconciled?.changed === true || autoTitleClaim.claimed) {
    schedulePresentationSessionDelta(runtime, repository, {
      projectId: current.projectId,
      reason: input.reason,
      sessionId: current.sessionId,
    });
  }
}

async function handleRepositoryCloneEndpoint(
  runtime: GxserverApiRuntime,
  endpointPath: GxserverEndpointPath,
  body: unknown,
  requestId: string,
): Promise<GxserverRpcSuccessResponse<Record<string, unknown>>> {
  /*
  CDXC:RepositoryClone 2026-06-01-11:18:
  Clone Repository is a gxserver API, not a macOS implementation detail. Keep preview, start, polling, cancellation, and project registration behind these RPCs so all clients share the same existing-folder warning and clone job lifecycle.
  */
  const params = readRepositoryCloneRpcParams(body);
  const manager = getRepositoryCloneJobManager(runtime);
  let result: GxserverRepositoryClonePreviewRpcResult | GxserverRepositoryCloneJobRpcResult;
  switch (endpointPath) {
    case "/api/previewRepositoryClone":
      result = { preview: await manager.preview(params) };
      break;
    case "/api/startRepositoryClone":
      result = {
        job: await manager.start(
          {
            logger: runtime.logger,
            paths: runtime.paths,
            serverId: runtime.metadata.serverId,
          },
          params,
        ),
      };
      break;
    case "/api/readRepositoryCloneJob":
      result = { job: manager.read(params.jobId) };
      break;
    case "/api/cancelRepositoryCloneJob":
      result = { job: manager.cancel(params.jobId) };
      break;
    default:
      throw new GxserverRepositoryCloneError("notFound", `${endpointPath} is not a gxserver repository clone endpoint.`);
  }
  return {
    ok: true,
    product: GXSERVER_PRODUCT,
    protocolVersion: GXSERVER_PROTOCOL_VERSION,
    requestId,
    result: result as unknown as Record<string, unknown>,
  };
}

function getRepositoryCloneJobManager(runtime: GxserverApiRuntime): GxserverRepositoryCloneJobManager {
  runtime.repositoryCloneJobs ??= new GxserverRepositoryCloneJobManager();
  return runtime.repositoryCloneJobs;
}

function scheduleAgentTitleMetadataChecksForSessions(
  runtime: GxserverApiRuntime,
  leadingRepository: GxserverDomainRepository | undefined,
  sessions: readonly GxserverSessionDomainState[],
  reason: string,
): void {
  for (const session of sessions) {
    if (!shouldCheckAgentMetadataTitle(session)) {
      continue;
    }
    scheduleAgentTitleMetadataCheck(runtime, leadingRepository, {
      projectId: session.projectId,
      reason,
      sessionId: session.sessionId,
    });
  }
}

async function syncZmxTitleObserversFromStorage(runtime: GxserverApiRuntime, reason: string): Promise<void> {
  const db = openGxserverDatabase(runtime.paths);
  try {
    const repository = new GxserverDomainRepository(db, runtime.metadata.serverId);
    await syncZmxTitleObserversFromRepository(runtime, repository, reason);
  } catch (error) {
    await runtime.logger.log({
      details: { message: error instanceof Error ? error.message : String(error), reason },
      event: "zmxTitleObserver.syncFailed",
      level: "warn",
      serverId: runtime.metadata.serverId,
    });
  } finally {
    db.close();
  }
}

async function syncZmxTitleObserversFromRepository(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  reason: string,
): Promise<void> {
  if (!runtime.zmxTitleObserver) {
    return;
  }
  await runtime.zmxTitleObserver.syncSessions(repository.listSessions(), reason);
}

function scheduleZmxTitleObserverSync(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  reason: string,
): void {
  void syncZmxTitleObserversFromRepository(runtime, repository, reason).catch((error) =>
    runtime.logger.log({
      details: { message: error instanceof Error ? error.message : String(error), reason },
      event: "zmxTitleObserver.syncFailed",
      level: "warn",
      serverId: runtime.metadata.serverId,
    }),
  );
}

function observeZmxTitleForSession(runtime: GxserverApiRuntime, session: GxserverSessionDomainState, reason: string): void {
  if (!runtime.zmxTitleObserver) {
    return;
  }
  void runtime.zmxTitleObserver.observeSession(session, reason).catch((error) =>
    runtime.logger.log({
      details: { message: error instanceof Error ? error.message : String(error), reason },
      event: "zmxTitleObserver.observeFailed",
      level: "warn",
      projectId: session.projectId,
      serverId: runtime.metadata.serverId,
      sessionId: session.sessionId,
    }),
  );
}

function recordZmxTitleObservationState(
  runtime: GxserverApiRuntime,
  change: GxserverZmxTitleObservationStateChange,
): void {
  const db = openGxserverDatabase(runtime.paths);
  try {
    const repository = new GxserverDomainRepository(db, runtime.metadata.serverId);
    const session = repository.getSession(change.projectId, change.sessionId);
    if (!session) {
      return;
    }
    const runtimeSettings = {
      ...session.runtimeSettings,
      zmxTitleObservation: change.state,
    };
    if (JSON.stringify(session.runtimeSettings.zmxTitleObservation) === JSON.stringify(change.state)) {
      return;
    }
    /*
    CDXC:SessionStatus 2026-06-07-00:30:
    The zmx title watcher is the source of terminal-title working detection. Store only the coarse watcher health in session runtime settings and immediately publish a presentation delta so native Auto Sleep can defer decisions while detection is starting or retrying.
    */
    repository.updateSession({
      projectId: change.projectId,
      runtimeSettings,
      sessionId: change.sessionId,
    });
    schedulePresentationSessionDelta(runtime, repository, {
      projectId: change.projectId,
      reason: `zmx-title-observer-${change.state.status}`,
      sessionId: change.sessionId,
    });
  } catch (error) {
    void runtime.logger.log({
      details: {
        message: error instanceof Error ? error.message : String(error),
        reason: change.reason,
        state: change.state.status,
      },
      event: "zmxTitleObserver.statePersistFailed",
      level: "warn",
      projectId: change.projectId,
      serverId: runtime.metadata.serverId,
      sessionId: change.sessionId,
    });
  } finally {
    db.close();
  }
}

function schedulePresentationProjectDelta(
  runtime: GxserverApiRuntime,
  db: ReturnType<typeof openGxserverDatabase>,
  repository: GxserverDomainRepository,
  input: {
    projectId: GxserverProjectId;
    reason: string;
    type: "projectAdded" | "projectUpdated";
  },
): void {
  const delta = buildPresentationProjectDelta(repository, input.projectId, input.type);
  if (!delta) {
    return;
  }
  /*
  CDXC:GxserverPresentationProjects 2026-06-01-21:14:
  Add Project is user-visible sidebar state, not only database state. Publish a project presentation delta immediately when a path is registered or updated so connected clients can render empty project rows before any session delta exists.

  CDXC:GxserverPresentationProjects 2026-06-02-15:04:
  Project mutations must advance presentation revision even when no client is currently connected. WebSocket broadcast is already a no-op without clients, while the stored revision lets reconnecting clients detect that project presentation changed without native refetch fallbacks.
  */
  const revision = incrementPresentationRevision(db);
  runtime.eventHub.broadcast({
    delta,
    protocolVersion: GXSERVER_PROTOCOL_VERSION,
    revision,
    serverId: runtime.metadata.serverId,
    type: "presentationDelta",
  });
  void runtime.logger.log({
    details: {
      deltaType: delta.type,
      reason: input.reason,
      revision,
    },
    event: "session-presentation.projectDelta",
    level: "debug",
    projectId: input.projectId,
    serverId: runtime.metadata.serverId,
  });
}

function schedulePresentationSessionDelta(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  input: {
    projectId: GxserverProjectId;
    reason: string;
    sessionId: GxserverSessionId;
  },
): void {
  const delta = buildPresentationSessionDelta(repository, input.projectId, input.sessionId);
  if (!delta) {
    return;
  }
  if (runtime.eventHub.server.clients.size === 0) {
    recordPresentationSessionRevisionWithoutClients(runtime, input, delta);
    return;
  }
  getPresentationDeltaCoalescer(runtime).schedule(
    { projectId: input.projectId, sessionId: input.sessionId },
    input.reason,
    delta,
    (decision) => {
      const db = openGxserverDatabase(runtime.paths);
      try {
        const flushRepository = new GxserverDomainRepository(db, runtime.metadata.serverId);
        const latestDelta = buildPresentationSessionDelta(flushRepository, input.projectId, input.sessionId);
        if (!latestDelta) {
          return;
        }
        const deltaJson = stringifyPresentationDelta(latestDelta);
        const previousDeltaJson = getPresentationLastDeltaJsonBySessionKey(runtime).get(decision.key);
        if (previousDeltaJson === deltaJson) {
          void runtime.logger.log({
            details: {
              coalescedCount: decision.coalescedCount,
              reason: decision.reason,
              skipped: "unchanged-projection",
            },
            event: "session-presentation.deltaSkipped",
            level: "debug",
            projectId: input.projectId,
            serverId: runtime.metadata.serverId,
            sessionId: input.sessionId,
          });
          return;
        }
        getPresentationLastDeltaJsonBySessionKey(runtime).set(decision.key, deltaJson);
        const revision = incrementPresentationRevision(db);
        runtime.eventHub.broadcast({
          delta: latestDelta,
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
          revision,
          serverId: runtime.metadata.serverId,
          type: "presentationDelta",
        });
        void runtime.logger.log({
          details: {
            coalescedCount: decision.coalescedCount,
            deltaType: latestDelta.type,
            reason: decision.reason,
            revision,
          },
          event: "session-presentation.delta",
          level: "debug",
          projectId: input.projectId,
          serverId: runtime.metadata.serverId,
          sessionId: input.sessionId,
        });
      } finally {
        db.close();
      }
    },
  );
}

function recordPresentationSessionRevisionWithoutClients(
  runtime: GxserverApiRuntime,
  input: {
    projectId: GxserverProjectId;
    reason: string;
    sessionId: GxserverSessionId;
  },
  delta: GxserverPresentationDelta,
): void {
  const deltaJson = stringifyPresentationDelta(delta);
  const key = `${input.projectId}/${input.sessionId}`;
  const previousDeltaJson = getPresentationLastDeltaJsonBySessionKey(runtime).get(key);
  if (previousDeltaJson === deltaJson) {
    return;
  }
  getPresentationLastDeltaJsonBySessionKey(runtime).set(key, deltaJson);
  const db = openGxserverDatabase(runtime.paths);
  try {
    /*
    CDXC:GxserverPresentationSessions 2026-06-02-19:31:
    Session mutations are shared presentation state even when no macOS sidebar is currently subscribed. Advance the durable presentation revision for the latest projected session delta so reconnecting clients can observe that session state changed through gxserver snapshots instead of relying on native refetch or stale WK project storage.
    */
    const revision = incrementPresentationRevision(db);
    void runtime.logger.log({
      details: {
        deltaType: delta.type,
        reason: input.reason,
        revision,
        subscribers: 0,
      },
      event: "session-presentation.deltaRecorded",
      level: "debug",
      projectId: input.projectId,
      serverId: runtime.metadata.serverId,
      sessionId: input.sessionId,
    });
  } finally {
    db.close();
  }
}

function buildPresentationProjectDelta(
  repository: GxserverDomainRepository,
  projectId: GxserverProjectId,
  type: "projectAdded" | "projectUpdated",
): GxserverPresentationDelta | undefined {
  const project = repository.getProject(projectId);
  if (!project) {
    return {
      projectId,
      type: "projectRemoved",
    };
  }
  return {
    domainProject: project,
    project: projectPresentationProject(project),
    type,
  };
}

function scheduleStaleActivityPresentationRefresh(
  runtime: GxserverApiRuntime,
  session: GxserverSessionDomainState,
  reason: string,
): void {
  const key = `${session.projectId}/${session.sessionId}`;
  const timers = getPresentationStaleActivityTimers(runtime);
  const existingTimer = timers.get(key);
  if (existingTimer) {
    clearTimeout(existingTimer);
    timers.delete(key);
  }
  const delayMs = getAgentActivityStaleProjectionDelayMs(session.runtimeSettings.agentActivity);
  if (delayMs === undefined) {
    return;
  }
  /*
  CDXC:SessionStatus 2026-06-01-20:26:
  zmx emits title observations only when the terminal title changes. Schedule one presentation refresh at the old macOS spinner window boundary so connected clients clear a frozen Codex/Claude/Cursor/Pi working glyph even when no further title event arrives.
  */
  const timer = setTimeout(() => {
    timers.delete(key);
    const db = openGxserverDatabase(runtime.paths);
    try {
      schedulePresentationSessionDelta(runtime, new GxserverDomainRepository(db, runtime.metadata.serverId), {
        projectId: session.projectId,
        reason,
        sessionId: session.sessionId,
      });
    } finally {
      db.close();
    }
  }, delayMs + 25);
  timer.unref();
  timers.set(key, timer);
}

function getPresentationStaleActivityTimers(
  runtime: GxserverApiRuntime,
): Map<string, ReturnType<typeof setTimeout>> {
  runtime.presentationStaleActivityTimers ??= new Map();
  return runtime.presentationStaleActivityTimers;
}

function buildPresentationSessionDelta(
  repository: GxserverDomainRepository,
  projectId: GxserverProjectId,
  sessionId: GxserverSessionId,
): GxserverPresentationDelta | undefined {
  const project = repository.getProject(projectId);
  const session = repository.getSession(projectId, sessionId);
  if (!project || !session) {
    return {
      projectId,
      sessionId,
      type: "sessionRemoved",
    };
  }
  if (!shouldIncludePresentationSession(session)) {
    return {
      projectId,
      sessionId,
      type: "sessionRemoved",
    };
  }
  return {
    session: projectPresentationSession(project, defaultGroupId(projectId), session),
    type: "sessionPresentationChanged",
  };
}

function stringifyPresentationSessionDelta(
  repository: GxserverDomainRepository,
  projectId: GxserverProjectId,
  sessionId: GxserverSessionId,
): string | undefined {
  const delta = buildPresentationSessionDelta(repository, projectId, sessionId);
  return delta ? stringifyPresentationDelta(delta) : undefined;
}

function stringifyImmediateTerminalTitlePresentationDelta(
  repository: GxserverDomainRepository,
  projectId: GxserverProjectId,
  sessionId: GxserverSessionId,
): string | undefined {
  const delta = buildPresentationSessionDelta(repository, projectId, sessionId);
  if (!delta) {
    return undefined;
  }
  if (delta.type !== "sessionPresentationChanged") {
    return stringifyPresentationDelta(delta);
  }
  const session = delta.session;
  /*
  CDXC:GxserverSessionTitles 2026-06-08-09:30:
  Terminal-title events are high-volume and often mutate only freshness fields. Include fields that should immediately repaint or reorder visible sidebar/session chrome, and intentionally omit volatile timestamps so routine title/status bookkeeping waits for the stale-activity refresh instead of rehydrating the macOS sidebar on every observation.
  */
  return JSON.stringify({
    session: {
      actions: session.actions,
      activity: session.activity,
      agentIcon: session.agentIcon,
      agentId: session.agentId,
      agentName: session.agentName,
      attention: session.attention,
      displayTitle: session.displayTitle,
      displayTitleTooltip: session.displayTitleTooltip,
      groupId: session.groupId,
      isFavorite: session.isFavorite,
      isGeneratingFirstPromptTitle: session.isGeneratingFirstPromptTitle,
      isPinned: session.isPinned,
      isPrimaryTitleTerminalTitle: session.isPrimaryTitleTerminalTitle,
      isTemporaryTitle: session.isTemporaryTitle,
      kind: session.kind,
      lifecycleState: session.lifecycleState,
      primaryTitle: session.primaryTitle,
      projectId: session.projectId,
      sessionId: session.sessionId,
      sessionTag: session.sessionTag,
      sidebarOrder: session.sidebarOrder,
      subtitle: session.subtitle,
      surface: session.surface,
      terminalTitle: session.terminalTitle,
      title: session.title,
      titleObservation: session.titleObservation,
      titleSource: session.titleSource,
      tooltip: session.tooltip,
      trustedResumeTitle: session.trustedResumeTitle,
      visibleInSidebarByDefault: session.visibleInSidebarByDefault,
      zmxName: session.zmxName,
    },
    type: delta.type,
  });
}

function stringifyPresentationDelta(delta: GxserverPresentationDelta): string {
  return JSON.stringify(delta);
}

function shouldPersistSessionStatusUpdate(
  session: GxserverSessionDomainState,
  update: { activity: unknown; lastActiveAt?: string },
): boolean {
  if ((session.lastActiveAt ?? undefined) !== (update.lastActiveAt ?? undefined)) {
    return true;
  }
  return (
    JSON.stringify(persistableAgentActivitySnapshot(session.runtimeSettings.agentActivity)) !==
    JSON.stringify(persistableAgentActivitySnapshot(update.activity))
  );
}

function getTerminalTitleDetectedAgentName(rawTitle: unknown): string | undefined {
  const title = typeof rawTitle === "string" ? rawTitle.trim().replace(/\s+/gu, " ") : "";
  if (!title) {
    return undefined;
  }
  const signal = classifyTerminalTitleStatus(title);
  if (!signal?.agentName) {
    return undefined;
  }
  return isExplicitAgentProgramTerminalTitle(title, signal.agentName) ? signal.agentName : undefined;
}

function isExplicitAgentProgramTerminalTitle(title: string, agentName: string): boolean {
  /*
  CDXC:AgentDetection 2026-06-11-22:44:
  Terminal-title promotion is an identity bootstrap for explicit agent program titles only. Real task titles can mention provider names, so gxserver must wait for hook/session metadata instead of relabeling those terminal rows from classifier-only word matches.
  */
  switch (agentName) {
    case "antigravity":
      return /^(?:🔔\s*)?agy$/iu.test(title);
    case "claude":
      return /^(?:[✳*⠐⠂·✶✻✽✸✹✺✷✴]\s*)?Claude Code$/iu.test(title);
    case "codex":
      return /^(?:[\u2800-\u28ff·•⋅◦]\s*)?Codex(?: CLI)?$/iu.test(title);
    case "copilot":
      return /^(?:[🤖🔔]\s*)?(?:GitHub\s+)?Copilot(?: CLI)?$/iu.test(title);
    case "cursor":
      return /^Cursor Agent(?:\s*-\s*✅ Ready)?$/iu.test(title);
    case "gemini":
      return /^(?:[✦◇]\s*)?Gemini(?: CLI)?$/iu.test(title);
    case "pi":
      return /^[\s\u2800-\u28ff·•⋅◦✳*✦◇🤖🔔]*π\s*-/u.test(title);
    default:
      return false;
  }
}

function normalizeAgentHookActivity(
  status: unknown,
  eventName: unknown,
  agentName?: unknown,
): GxserverAgentActivityState["activity"] | undefined {
  const normalizedAgentName = normalizeAgentHookAgentName(agentName);
  const normalizedEventName = typeof eventName === "string" ? eventName.trim() : "";
  const lowerEventName = normalizedEventName.toLowerCase();
  /*
  CDXC:AgentHookStatus 2026-06-11-22:19:
  gxserver must normalize hook status by provider event semantics before trusting sidecar status text. Older installed hooks may still write `attention` for Stop/agent-response events; these are settled turn boundaries for the supported hook agents and should not produce a false Done/needs-attention state in any client.
  */
  if (normalizedAgentName === "claude") {
    if (lowerEventName === "stop" || lowerEventName === "idle") {
      return "idle";
    }
    if (lowerEventName === "notification" || lowerEventName === "notify" || lowerEventName === "permissionrequest") {
      return "attention";
    }
    if (
      lowerEventName === "userpromptsubmit" ||
      lowerEventName === "prompt-submit" ||
      lowerEventName === "pretooluse" ||
      lowerEventName === "pre-tool-use"
    ) {
      return "working";
    }
    if (lowerEventName === "sessionend" || lowerEventName === "session-end") {
      return "idle";
    }
  }
  if (
    normalizedAgentName === "copilot" ||
    normalizedAgentName === "codebuddy" ||
    normalizedAgentName === "droid" ||
    normalizedAgentName === "qoder"
  ) {
    if (
      lowerEventName === "stop" ||
      lowerEventName === "notification" ||
      lowerEventName === "sessionend" ||
      lowerEventName === "session-end"
    ) {
      return "idle";
    }
    if (lowerEventName === "pretooluse" || lowerEventName === "pre-tool-use") {
      return "working";
    }
  }
  if (normalizedAgentName === "antigravity") {
    if (
      lowerEventName === "stop" ||
      lowerEventName === "turn-completion" ||
      lowerEventName === "sessionend" ||
      lowerEventName === "session-end"
    ) {
      return "idle";
    }
    if (lowerEventName === "preinvocation" || lowerEventName === "pretooluse" || lowerEventName === "posttooluse") {
      return "working";
    }
  }
  if (
    lowerEventName === "stop" ||
    lowerEventName === "agent-response" ||
    lowerEventName === "afteragent" ||
    lowerEventName === "afteragentresponse" ||
    lowerEventName === "agent.end" ||
    lowerEventName === "agent_end" ||
    lowerEventName === "on_complete" ||
    lowerEventName === "on_error" ||
    lowerEventName === "post_llm_call" ||
    lowerEventName === "turn-completion"
  ) {
    return "idle";
  }
  if (
    lowerEventName === "on_tool_permission" ||
    lowerEventName === "post_approval_response" ||
    lowerEventName === "pretooluse" ||
    lowerEventName === "posttooluse" ||
    lowerEventName === "pre_tool_call" ||
    lowerEventName === "beforeagent" ||
    lowerEventName === "preinvocation" ||
    lowerEventName === "userpromptsubmit" ||
    lowerEventName === "userpromptsubmit" ||
    lowerEventName === "agent.start" ||
    lowerEventName === "agent_start" ||
    lowerEventName === "beforeshellexecution" ||
    lowerEventName === "beforesubmitprompt"
  ) {
    return "working";
  }
  if (status === "attention" || status === "idle" || status === "working") {
    return status;
  }
  if (!normalizedEventName) {
    return undefined;
  }
  if (
    GXSERVER_AGENT_HOOK_WORKING_EVENTS.has(normalizedEventName) ||
    GXSERVER_AGENT_HOOK_WORKING_EVENTS.has(lowerEventName)
  ) {
    return "working";
  }
  if (
    GXSERVER_AGENT_HOOK_ATTENTION_EVENTS.has(normalizedEventName) ||
    GXSERVER_AGENT_HOOK_ATTENTION_EVENTS.has(lowerEventName)
  ) {
    return "attention";
  }
  if (
    GXSERVER_AGENT_HOOK_IDLE_EVENTS.has(normalizedEventName) ||
    GXSERVER_AGENT_HOOK_IDLE_EVENTS.has(lowerEventName)
  ) {
    return "idle";
  }
  return undefined;
}

function normalizeAgentHookAgentName(value: unknown): string | undefined {
  const normalized = typeof value === "string" ? value.trim().toLowerCase().replace(/\s+/gu, " ") : "";
  if (!normalized) {
    return undefined;
  }
  const aliases: Record<string, string> = {
    "agy": "antigravity",
    "amp cli": "amp",
    "antigravity cli": "antigravity",
    "claude code": "claude",
    "code buddy": "codebuddy",
    "codex cli": "codex",
    "cursor agent": "cursor",
    "cursor cli": "cursor",
    "cursor-agent": "cursor",
    "factory": "droid",
    "factory droid": "droid",
    "gemini cli": "gemini",
    "github copilot": "copilot",
    "hermes": "hermes-agent",
    "hermes agent": "hermes-agent",
    "kiro cli": "kiro",
    "kiro-cli": "kiro",
    "open code": "opencode",
    "qodercli": "qoder",
    "rovo": "rovodev",
    "rovo dev": "rovodev",
    "π": "pi",
  };
  return aliases[normalized] ?? normalized;
}

function parseAgentHookActivityTimestamp(value: unknown): number | undefined {
  if (typeof value !== "string" || !value.trim()) {
    return undefined;
  }
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : undefined;
}

function isStaleAgentHookActivityEvent(currentActivity: unknown, incomingNowMs: number): boolean {
  const currentChangedAt = isObjectRecord(currentActivity) && typeof currentActivity.lastChangedAt === "string"
    ? Date.parse(currentActivity.lastChangedAt)
    : Number.NaN;
  return Number.isFinite(currentChangedAt) && Number.isFinite(incomingNowMs) && incomingNowMs < currentChangedAt;
}

function persistableAgentActivitySnapshot(value: unknown): Record<string, unknown> {
  if (!isObjectRecord(value)) {
    return {};
  }
  const activity = value.activity;
  const snapshot: Record<string, unknown> = {
    activity,
    agentName: value.agentName,
    attentionEventId: value.attentionEventId,
    attentionSuppressedUntil: value.attentionSuppressedUntil,
    hasSeenWorking: value.hasSeenWorking,
    isAcknowledged: value.isAcknowledged,
    lastTitle: value.lastTitle,
    lastTitleChangeAt: value.lastTitleChangeAt,
    suppressedUntil: value.suppressedUntil,
    workingSource: value.workingSource,
    workingStartedAt: value.workingStartedAt,
  };
  if (activity !== "idle") {
    snapshot.lastChangedAt = value.lastChangedAt;
  }
  return snapshot;
}

function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function getPresentationLastDeltaJsonBySessionKey(runtime: GxserverApiRuntime): Map<string, string> {
  runtime.presentationLastDeltaJsonBySessionKey ??= new Map();
  return runtime.presentationLastDeltaJsonBySessionKey;
}

function getPresentationDeltaCoalescer(runtime: GxserverApiRuntime): GxserverPresentationDeltaCoalescer {
  runtime.presentationDeltaCoalescer ??= new GxserverPresentationDeltaCoalescer();
  return runtime.presentationDeltaCoalescer;
}

function logSessionLaunchAgentMismatch(
  runtime: GxserverApiRuntime,
  lifecycle: GxserverSessionLifecycleParams,
  mismatch: GxserverLaunchAgentMismatch,
  source: "agent-hook-event" | "live-process-identity" | "session-state-event" | "update-agent-activity",
  extraDetails: Record<string, unknown>,
): void {
  /*
  CDXC:GxserverSessionIdentity 2026-06-09-21:59:
  Cross-agent hook rejections need support-bundle evidence without exposing user content. Log enum-like agent ids, source, status/event classes, and shape booleans only; never persist command text, paths, titles, prompts, or raw hook payload fields.
  */
  void runtime.logger.log({
    details: {
      ...extraDetails,
      currentAgentId: mismatch.currentAgentId,
      hasForkSource: mismatch.hasForkSource,
      hasLaunchAgentId: mismatch.hasLaunchAgentId,
      hasLaunchPlan: mismatch.hasLaunchPlan,
      incomingAgentId: mismatch.incomingAgentId,
      lockedAgentId: mismatch.lockedAgentId,
      source,
    },
    event: "sessionIdentity.launchAgentMismatch",
    level: "warn",
    projectId: lifecycle.projectId,
    serverId: runtime.metadata.serverId,
    sessionId: lifecycle.sessionId,
  });
}

function logSessionIdentityConflict(
  runtime: GxserverApiRuntime,
  projectId: GxserverProjectId,
  sessionId: GxserverSessionLifecycleParams["sessionId"],
): (conflict: GxserverSessionIdentityConflict) => void {
  return (conflict) => {
    /*
    CDXC:GxserverSessionIdentity 2026-06-09-08:55:
    Debugging Mode needs evidence when gxserver rejects a passive Codex identity swap, but support logs must not persist raw Codex thread ids, transcript paths, terminal titles, or user prompts. Log stable gxserver ids, enum reasons, booleans, and short hashes only.
    */
    void runtime.logger.log({
      details: {
        agentId: conflict.agentId,
        currentAgentSessionIdHash: hashLogIdentity(conflict.currentAgentSessionId),
        currentAgentSessionIdPresent: conflict.currentAgentSessionId !== undefined,
        incomingAgentSessionIdHash: hashLogIdentity(conflict.incomingAgentSessionId),
        ownerProjectId: conflict.ownerProjectId,
        ownerSessionId: conflict.ownerSessionId,
        reason: conflict.reason,
        source: conflict.source,
      },
      event: "sessionIdentity.updateBlocked",
      level: "debug",
      projectId,
      serverId: runtime.metadata.serverId,
      sessionId,
    });
  };
}

function isRejectedPassiveSessionIdentityConflict(
  conflict: GxserverSessionIdentityConflict | undefined,
): conflict is GxserverSessionIdentityConflict {
  return conflict?.source === "passive";
}

function logPassiveSessionIdentityEventRejected(
  runtime: GxserverApiRuntime,
  lifecycle: GxserverSessionLifecycleParams,
  conflict: GxserverSessionIdentityConflict,
  source: "agent-hook-event" | "session-state-event",
  extraDetails: Record<string, unknown>,
): void {
  /*
  CDXC:GxserverSessionIdentity 2026-06-09-22:30:
  Support needs to distinguish "blocked id update" from "entire passive event rejected" when a clicked/surfaced Codex terminal would otherwise inherit another row's done status. Persist only stable IDs, enum fields, booleans, and short hashes; never write thread ids, paths, titles, prompts, or command text.
  */
  void runtime.logger.log({
    details: {
      ...extraDetails,
      agentId: conflict.agentId,
      currentAgentSessionIdHash: hashLogIdentity(conflict.currentAgentSessionId),
      currentAgentSessionIdPresent: conflict.currentAgentSessionId !== undefined,
      incomingAgentSessionIdHash: hashLogIdentity(conflict.incomingAgentSessionId),
      incomingAgentSessionIdPresent: conflict.incomingAgentSessionId !== undefined,
      identitySource: conflict.source,
      ownerProjectId: conflict.ownerProjectId,
      ownerSessionId: conflict.ownerSessionId,
      reason: conflict.reason,
      source,
    },
    event: "sessionIdentity.passiveEventRejected",
    level: "warn",
    projectId: lifecycle.projectId,
    serverId: runtime.metadata.serverId,
    sessionId: lifecycle.sessionId,
  });
}

function hashLogIdentity(value: string | undefined): string | undefined {
  if (!value) {
    return undefined;
  }
  return createHash("sha256").update(value).digest("hex").slice(0, 16);
}

function scheduleAgentTitleMetadataCheck(
  runtime: GxserverApiRuntime,
  leadingRepository: GxserverDomainRepository | undefined,
  input: {
    force?: boolean;
    projectId: GxserverProjectId;
    reason: string;
    sessionId: GxserverSessionLifecycleParams["sessionId"];
  },
):
  | {
      changed: boolean;
      metadataTitleFound: boolean;
      reason: string;
      session?: GxserverSessionDomainState;
    }
  | undefined {
  let latestResult:
    | {
        changed: boolean;
        metadataTitleFound: boolean;
        reason: string;
        session?: GxserverSessionDomainState;
      }
    | undefined;
  agentTitleMetadataDebouncer.schedule({
    key: `${runtime.paths.rootDir}:${runtime.metadata.serverId}:${input.projectId}:${input.sessionId}`,
    run: (decision) => {
      latestResult = runAgentTitleMetadataCheck(runtime, leadingRepository, input, decision);
    },
  });
  return latestResult;
}

function runAgentTitleMetadataCheck(
  runtime: GxserverApiRuntime,
  leadingRepository: GxserverDomainRepository | undefined,
  input: {
    force?: boolean;
    projectId: GxserverProjectId;
    reason: string;
    sessionId: GxserverSessionLifecycleParams["sessionId"];
  },
  decision: GxserverAgentTitleDebounceDecision,
):
  | {
      changed: boolean;
      metadataTitleFound: boolean;
      reason: string;
      session?: GxserverSessionDomainState;
    }
  | undefined {
  const useLeadingRepository = decision.edge === "leading" && leadingRepository !== undefined;
  let db: ReturnType<typeof openGxserverDatabase> | undefined;
  try {
    db = useLeadingRepository ? undefined : openGxserverDatabase(runtime.paths);
    const repository = useLeadingRepository ? leadingRepository! : new GxserverDomainRepository(db!, runtime.metadata.serverId);
    const session = repository.getSession(input.projectId, input.sessionId);
    if (!session || (input.force !== true && !shouldCheckAgentMetadataTitle(session))) {
      const skippedReason = session ? "not-needed" : "session-missing";
      if (shouldLogAgentTitleMetadataCheckSkipped(input, decision, skippedReason)) {
        void runtime.logger.log({
          details: {
            edge: decision.edge,
            reason: input.reason,
            skippedReason,
            suppressedCount: decision.suppressedCount,
          },
          event: "agentTitleMetadata.checkSkipped",
          level: "debug",
          projectId: input.projectId,
          serverId: runtime.metadata.serverId,
          sessionId: input.sessionId,
        });
      }
      return session
        ? { changed: false, metadataTitleFound: false, reason: "metadata-check-not-needed", session }
        : undefined;
    }
    /*
    CDXC:GxserverAgentTitles 2026-06-01-09:03:
    Rename and title-observation triggers enter one gxserver metadata-check path with structured logging under `agentTitleMetadata.*`. The log label lets clients verify whether rename checks are noisy while the debouncer keeps Codex session-index reads to a leading check plus one trailing check per burst.
    */
    const result = reconcileAgentMetadataTitle(repository, {
      homeDir: runtime.paths.homeDir,
      pendingMismatchStatus: decision.edge === "leading" ? "pending" : "metadata-mismatch",
      projectId: input.projectId,
      sessionId: input.sessionId,
    });
    if (shouldLogAgentTitleMetadataCheck(input, decision, result)) {
      void runtime.logger.log({
        details: {
          changed: result.changed,
          edge: decision.edge,
          metadataTitleFound: result.metadataTitleFound,
          reason: input.reason,
          reconcileReason: result.reason,
          suppressedCount: decision.suppressedCount,
        },
        event: "agentTitleMetadata.check",
        level: "debug",
        projectId: input.projectId,
        serverId: runtime.metadata.serverId,
        sessionId: input.sessionId,
      });
    }
    return result;
  } catch (error) {
    void runtime.logger.log({
      details: {
        edge: decision.edge,
        reason: input.reason,
        suppressedCount: decision.suppressedCount,
      },
      error: error instanceof Error ? error : String(error),
      event: "agentTitleMetadata.checkFailed",
      level: "warn",
      projectId: input.projectId,
      serverId: runtime.metadata.serverId,
      sessionId: input.sessionId,
    });
    return undefined;
  } finally {
    db?.close();
  }
}

function shouldLogAgentTitleMetadataCheckSkipped(
  input: { force?: boolean; reason: string },
  decision: GxserverAgentTitleDebounceDecision,
  skippedReason: "not-needed" | "session-missing",
): boolean {
  if (skippedReason === "session-missing" || input.force === true) {
    return true;
  }
  if (!isPollingAgentTitleMetadataReason(input.reason)) {
    return true;
  }
  return decision.suppressedCount >= 10;
}

function shouldLogAgentTitleMetadataCheck(
  input: { force?: boolean; reason: string },
  decision: GxserverAgentTitleDebounceDecision,
  result: { changed: boolean; metadataTitleFound: boolean },
): boolean {
  /*
  CDXC:GxserverLogs 2026-06-06-23:18:
  Debugging Mode should keep actionable agent-title evidence without turning list-session polling into one JSONL line per session every few seconds. Persist metadata checks when they change title state, find metadata, are forced by a user/session mutation, or represent a large suppressed burst; skip unchanged polling checks.
  */
  if (result.changed || result.metadataTitleFound || input.force === true) {
    return true;
  }
  if (!isPollingAgentTitleMetadataReason(input.reason)) {
    return true;
  }
  return decision.suppressedCount >= 10;
}

function isPollingAgentTitleMetadataReason(reason: string): boolean {
  return reason === "list-sessions" || reason === "read-project-status";
}

async function handleTypedOperationEndpoint(
  runtime: GxserverApiRuntime,
  endpointPath: GxserverEndpointPath,
  body: unknown,
  requestId: string,
  abortSignal?: AbortSignal,
): Promise<GxserverRpcSuccessResponse> {
  const params = readOperationRpcParams(body);
  const db = openGxserverDatabase(runtime.paths);
  try {
    const repository = new GxserverDomainRepository(db, runtime.metadata.serverId);
    const projects = repository.listProjects();
    let scope: ReturnType<typeof resolveProjectOperationDirectory>;
    try {
      scope = resolveProjectOperationDirectory(projects, params);
    } catch (error) {
      /*
       * CDXC:ProjectBoardRouting 2026-06-04-23:51:
       * Typed operation scope failures are the signal for stale Project board URLs and mismatched project ids. Log only enum/count/identity facts at the gxserver boundary so support can diagnose the mismatch without recording raw paths, URLs, command text, or user content.
       */
      await runtime.logger.log({
        details: {
          action: typeof params.action === "string" ? params.action : undefined,
          endpoint: endpointPath.replace(/^\/api\//u, ""),
          errorCode:
            error instanceof GxserverProjectPathError || error instanceof GxserverTypedOperationError
              ? error.code
              : undefined,
          errorType: error instanceof Error ? error.name : typeof error,
          hasProjectId: typeof params.projectId === "string" && params.projectId.trim().length > 0,
          hasProjectPath: typeof params.projectPath === "string" && params.projectPath.trim().length > 0,
        },
        event: "typedOperation.scopeRejected",
        level: "warn",
        requestId,
        serverId: runtime.metadata.serverId,
      });
      throw error;
    }
    const { cwd, project } = scope;
    const context = { abortSignal, cwd, envPath: process.env.PATH, projects };
    const result =
      endpointPath === "/api/runGitAction"
        ? await runGitAction(params as unknown as GxserverRunGitActionParams, context)
        : endpointPath === "/api/runGitHubAction"
          ? await runGitHubAction(params as unknown as GxserverRunGitHubActionParams, context)
          : endpointPath === "/api/runWorktreeAction"
            ? await runWorktreeAction(params as unknown as GxserverRunWorktreeActionParams, context)
            : endpointPath === "/api/runProjectSetupCommand"
              ? await runProjectSetupCommand(params as unknown as GxserverRunProjectSetupCommandParams, context)
              : await runBeadsAction(params as unknown as GxserverRunBeadsActionParams, context);
    await runtime.logger.log({
      details: {
        action: result.action,
        argumentCount: result.command?.args.length,
        commandBuilt: result.command !== undefined,
        executable: result.command?.executable,
        exitCode: result.exitCode,
        operationError: result.error,
      },
      event: "typedOperation",
      level: result.exitCode === 0 ? "info" : "warn",
      projectId: project.projectId,
      requestId,
      serverId: runtime.metadata.serverId,
    });
    return {
      ok: true,
      product: GXSERVER_PRODUCT,
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      requestId,
      result: result as unknown as Record<string, unknown>,
    };
  } finally {
    db.close();
  }
}

async function handleZmxLifecycleEndpoint(
  runtime: GxserverApiRuntime,
  endpointPath: GxserverEndpointPath,
  body: unknown,
  requestId: string,
): Promise<GxserverRpcSuccessResponse> {
  const params = readDomainRpcParams(body);
  const db = openGxserverDatabase(runtime.paths);
  try {
    const repository = new GxserverDomainRepository(db, runtime.metadata.serverId);
    const agentSettings = new GxserverAgentSettingsRepository(db).read();
    const result = await dispatchZmxLifecycleEndpoint(runtime, repository, endpointPath, params, requestId, agentSettings);
    return {
      ok: true,
      product: GXSERVER_PRODUCT,
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      requestId,
      result,
    };
  } finally {
    db.close();
  }
}

function normalizeMetadataSessionUpdateParams(
  repository: GxserverDomainRepository,
  params: GxserverUpdateSessionParams,
  reason: string,
): GxserverUpdateSessionParams {
  const current = repository.getSession(params.projectId, params.sessionId);
  if (!current) {
    throw new GxserverDomainStateError(
      "notFound",
      `Session ${params.projectId}/${params.sessionId} does not exist.`,
    );
  }
  if (current.lifecycleState !== "stopped") {
    return params;
  }
  const requestedLifecycle = params.lifecycleState;
  const requestedProviderLifecycle = params.providerState?.lifecycleState;
  /*
  CDXC:GxserverSessionLifecycle 2026-06-05-12:25:
  Closed sessions must not reappear because a stale renderer, Electron window, zmx title observer, or native AppKit surface sends late metadata. Only explicit lifecycle endpoints may revive a stopped row; generic metadata updates can still persist flags, titles, and runtime details, but cannot change domain lifecycle or mark the provider live.
  */
  if (requestedLifecycle && requestedLifecycle !== "stopped") {
    throw new GxserverDomainStateError(
      "badRequest",
      `${reason} cannot change a stopped session to ${requestedLifecycle}; use a lifecycle endpoint to wake or start it.`,
    );
  }
  if (requestedProviderLifecycle === "exists") {
    throw new GxserverDomainStateError(
      "badRequest",
      `${reason} cannot mark a stopped session provider as exists; use a lifecycle endpoint to wake or start it.`,
    );
  }
  return params;
}

async function dispatchZmxLifecycleEndpoint(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  endpointPath: GxserverEndpointPath,
  params: Record<string, unknown>,
  requestId: string,
  agentSettings: GxserverAgentSettings,
): Promise<Record<string, unknown>> {
  switch (endpointPath) {
    case "/api/probeSessionProvider": {
      const { session, probe } = await probeAndCacheSessionProvider(runtime, repository, readSessionLifecycleParams(params));
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: session.projectId,
        reason: "probe-session-provider",
        sessionId: session.sessionId,
      });
      return { provider: "zmx", providerState: probe, session };
    }
    case "/api/attachSessionMetadata":
    case "/api/wakeSession": {
      const attach = await createAttachSessionMetadata(
        runtime,
        repository,
        params as unknown as GxserverAttachSessionMetadataParams,
        requestId,
        agentSettings,
      );
      const lifecycleSession =
        endpointPath === "/api/wakeSession" && !attach.restoreBlocked
          ? repository.updateSession({
              lifecycleState: "running",
              projectId: attach.session.projectId,
              providerState: attach.session.providerState,
              sessionId: attach.session.sessionId,
            })
          : attach.session;
      const session =
        endpointPath === "/api/wakeSession" && !attach.restoreBlocked
          ? applyWakeSessionActivitySuppression(runtime, repository, lifecycleSession, requestId)
          : lifecycleSession;
      const normalizedAttach: GxserverAttachSessionMetadataResult =
        session === attach.session ? attach : { ...attach, session };
      observeZmxTitleForSession(runtime, normalizedAttach.session, endpointPath === "/api/wakeSession" ? "wake-session" : "attach-session-metadata");
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: normalizedAttach.session.projectId,
        reason: endpointPath === "/api/wakeSession" ? "wake-session" : "attach-session-metadata",
        sessionId: normalizedAttach.session.sessionId,
      });
      return endpointPath === "/api/wakeSession"
        ? { attach: normalizedAttach, session }
        : { attach: normalizedAttach };
    }
    case "/api/startSessionProvider": {
      const result = await startSessionProvider(
        runtime,
        repository,
        params as unknown as GxserverStartSessionProviderParams,
        requestId,
        agentSettings,
      );
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: result.session.projectId,
        reason: "start-session-provider",
        sessionId: result.session.sessionId,
      });
      return result as unknown as Record<string, unknown>;
    }
    case "/api/transitionSession": {
      const result = await dispatchSessionTransitionEndpoint(runtime, repository, normalizeSessionTransitionParams(params));
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: result.session.projectId,
        reason: "transition-session",
        sessionId: result.session.sessionId,
      });
      return result as unknown as Record<string, unknown>;
    }
    case "/api/sleepSession": {
      const lifecycle = readSessionLifecycleParams(params);
      const { kill, session } = await killAndCacheSessionProvider(
        runtime,
        repository,
        lifecycle,
        normalizeLifecycleReason(lifecycle.reason, "sleepSession"),
        "sleeping",
      );
      scheduleZmxTitleObserverSync(runtime, repository, "sleep-session");
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: session.projectId,
        reason: "sleep-session",
        sessionId: session.sessionId,
      });
      return { kill, session };
    }
    case "/api/killSession": {
      const lifecycle = readSessionLifecycleParams(params);
      const { kill, session } = await killAndCacheSessionProvider(
        runtime,
        repository,
        lifecycle,
        normalizeLifecycleReason(lifecycle.reason, "killSession"),
        "stopped",
      );
      scheduleZmxTitleObserverSync(runtime, repository, "kill-session");
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: session.projectId,
        reason: "kill-session",
        sessionId: session.sessionId,
      });
      return { kill, session };
    }
    default:
      throw new GxserverDomainStateError("notFound", `${endpointPath} is not a gxserver zmx lifecycle endpoint.`);
  }
}

function applyCreatedSessionActivitySuppression(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  session: GxserverSessionDomainState,
  requestId: string,
): GxserverSessionDomainState {
  if ((session.kind !== "terminal" && session.kind !== "agent") || session.lifecycleState !== "running") {
    return session;
  }
  const previous = normalizeAgentActivityState(session.runtimeSettings.agentActivity, { activity: "idle" });
  const update = updateSessionActivitySettings(session, {
    event: "launch",
    projectId: session.projectId,
    sessionId: session.sessionId,
  });
  const changed = shouldPersistSessionStatusUpdate(session, update);
  const suppressedUntilMs = update.activity.suppressedUntil ? Date.parse(update.activity.suppressedUntil) : Number.NaN;
  const nowMs = update.activity.lastChangedAt ? Date.parse(update.activity.lastChangedAt) : Date.now();
  const updatedSession = changed
    ? repository.updateSession({
        lastActiveAt: update.lastActiveAt,
        projectId: session.projectId,
        runtimeSettings: update.runtimeSettings,
        sessionId: session.sessionId,
      })
    : session;
  /*
  CDXC:GxserverSessionStatus 2026-06-11-08:32:
  Freshly-created running sessions can emit stale zmx terminal-title status before the shell or foreground process has produced new activity. Apply the shared gxserver launch suppression before title observation starts so macOS, remote, CLI/TUI, and mobile clients do not play false done status from create-time title replay.
  */
  void runtime.logger.log({
    details: {
      changed,
      lifecycleState: session.lifecycleState,
      previousActivity: update.previousActivity,
      previousHadSuppression: previous.suppressedUntil !== undefined,
      sessionKind: session.kind,
      suppressedUntil: update.activity.suppressedUntil,
      suppressionMs: Number.isFinite(suppressedUntilMs) ? Math.max(0, suppressedUntilMs - nowMs) : undefined,
    },
    event: "sessionActivity.createSuppressionStarted",
    level: "info",
    projectId: session.projectId,
    requestId,
    serverId: runtime.metadata.serverId,
    sessionId: session.sessionId,
  });
  return updatedSession;
}

function applyWakeSessionActivitySuppression(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  session: GxserverSessionDomainState,
  requestId: string,
): GxserverSessionDomainState {
  const previous = normalizeAgentActivityState(session.runtimeSettings.agentActivity, { activity: "idle" });
  const update = updateSessionActivitySettings(session, {
    event: "wake",
    projectId: session.projectId,
    sessionId: session.sessionId,
  });
  const changed = shouldPersistSessionStatusUpdate(session, update);
  const suppressedUntilMs = update.activity.suppressedUntil ? Date.parse(update.activity.suppressedUntil) : Number.NaN;
  const nowMs = update.activity.lastChangedAt ? Date.parse(update.activity.lastChangedAt) : Date.now();
  const updatedSession = changed
    ? repository.updateSession({
        lastActiveAt: update.lastActiveAt,
        projectId: session.projectId,
        runtimeSettings: update.runtimeSettings,
        sessionId: session.sessionId,
      })
    : session;
  /*
  CDXC:GxserverZmxLifecycle 2026-06-10-11:27:
  Wake must leave an auditable server-side breadcrumb before the zmx title observer starts. Log only IDs, booleans, enum states, and suppression timing so support can prove stale wake-time done status was suppressed without persisting titles, commands, paths, prompts, or terminal content.
  */
  void runtime.logger.log({
    details: {
      changed,
      previousActivity: update.previousActivity,
      previousHadSuppression: previous.suppressedUntil !== undefined,
      provider: "zmx",
      suppressedUntil: update.activity.suppressedUntil,
      suppressionMs: Number.isFinite(suppressedUntilMs) ? Math.max(0, suppressedUntilMs - nowMs) : undefined,
    },
    event: "sessionActivity.wakeSuppressionStarted",
    level: "info",
    projectId: session.projectId,
    requestId,
    serverId: runtime.metadata.serverId,
    sessionId: session.sessionId,
  });
  return updatedSession;
}

async function dispatchSessionTransitionEndpoint(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  params: GxserverSessionTransitionParams,
): Promise<GxserverSessionTransitionResult> {
  return applySessionTransition({
    params,
    transitionSession: async (transitionParams) => {
      const { kill, session } = await killAndCacheSessionProvider(
        runtime,
        repository,
        transitionParams,
        normalizeLifecycleReason(
          transitionParams.reason,
          transitionParams.action === "sleep" ? "transitionSessionSleep" : "transitionSessionClose",
        ),
        transitionParams.action === "sleep" ? "sleeping" : "stopped",
      );
      return { kill, session };
    },
  });
}

async function handleZmxSessionInteractionEndpoint(
  runtime: GxserverApiRuntime,
  endpointPath: GxserverEndpointPath,
  body: unknown,
  requestId: string,
): Promise<GxserverRpcSuccessResponse> {
  const params = readDomainRpcParams(body);
  const db = openGxserverDatabase(runtime.paths);
  try {
    const repository = new GxserverDomainRepository(db, runtime.metadata.serverId);
    const result = await dispatchZmxSessionInteractionEndpoint(runtime, repository, endpointPath, params, requestId);
    return {
      ok: true,
      product: GXSERVER_PRODUCT,
      protocolVersion: GXSERVER_PROTOCOL_VERSION,
      requestId,
      result,
    };
  } finally {
    db.close();
  }
}

async function dispatchZmxSessionInteractionEndpoint(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  endpointPath: GxserverEndpointPath,
  params: Record<string, unknown>,
  requestId: string,
): Promise<Record<string, unknown>> {
  /*
  CDXC:GxserverSessionIO 2026-05-30-23:32:
  readSessionText must never return unbounded `zmx history` output, and sendSessionText/sendSessionMessage must never embed user text in the shell script passed to `/bin/zsh -lc`. Cap history responses with truncation metadata and pass send payload bytes through zmx stdin after an explicit request-size check.
  */
  if (endpointPath === "/api/focusSession") {
    const lifecycle = readSessionLifecycleParams(params);
    const session = requireSession(repository, lifecycle);
    /*
    CDXC:GxserverRendererCommands 2026-06-13-02:24:
    Focus is a daemon-owned command with renderer-owned execution. gxserver validates the target session and sends only the canonical project/session ids to the connected macOS renderer instead of letting the CLI reach the old app bridge directly.
    */
    const result = await runtime.eventHub.dispatchRendererCommand({
      action: "focusSession",
      payload: {
        ...params,
        projectId: session.projectId,
        sessionId: session.sessionId,
      },
    });
    return { session, ...result };
  }
  if (endpointPath === "/api/sendSessionMessage" && params.sessionId === undefined) {
    /*
    CDXC:GxserverRendererCommands 2026-06-13-02:24:
    Agent-targeted send-message may need to create and focus a visible macOS agent pane before staging the prompt. Keep the public API at gxserver and use the renderer channel only for that native workspace effect.
    */
    return runtime.eventHub.dispatchRendererCommand({
      action: "sendMessage",
      payload: params,
    });
  }

  const zmx = await (runtime.zmxLifecycle?.requireZmx ?? requireBundledZmx)();
  const lifecycle = readSessionLifecycleParams(params);
  const session = requireSession(repository, lifecycle);
  const zmxSessionName = providerZmxSessionName(session);
  switch (endpointPath) {
    case "/api/readSessionText": {
      const result = await runZmxInteractionCommand(
        runtime,
        buildZmxHistoryCommand({
          sessionName: zmxSessionName,
          zmxExecutablePath: zmx.executablePath,
        }),
        {
          allowStdoutTruncation: true,
          stdoutLimitBytes: GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES,
        },
      );
      return {
        capturedBytes: Buffer.byteLength(result.stdout, "utf8"),
        limitBytes: GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES,
        provider: "zmx",
        session,
        source: "history",
        text: result.stdout,
        truncated: result.stdoutTruncated === true,
        ...(result.stdoutTruncated ? { truncatedReason: "historyOutputLimitExceeded" } : {}),
        zmxName: zmxSessionName,
      };
    }
    case "/api/sendSessionText": {
      const text = readInteractionText(params.text, "sendSessionText");
      const result = await runZmxInteractionCommand(
        runtime,
        buildZmxSendCommand({
          sessionName: zmxSessionName,
          zmxExecutablePath: zmx.executablePath,
        }),
        { stdin: text },
      );
      return {
        exitCode: result.exitCode,
        provider: "zmx",
        session,
        textBytes: Buffer.byteLength(text, "utf8"),
        textLength: text.length,
        zmxName: zmxSessionName,
      };
    }
    case "/api/sendSessionEnter": {
      const result = await runZmxInteractionCommand(runtime, buildZmxSendCommand({
        sessionName: zmxSessionName,
        zmxExecutablePath: zmx.executablePath,
      }), { stdin: "\r" });
      return {
        exitCode: result.exitCode,
        provider: "zmx",
        session,
        textBytes: 1,
        textLength: 1,
        zmxName: zmxSessionName,
      };
    }
    case "/api/sendSessionMessage": {
      const text = readInteractionText(params.text, "sendSessionMessage");
      const submit = params.submit !== false;
      const payload = submit ? `${text}\r` : text;
      const result = await runZmxInteractionCommand(
        runtime,
        buildZmxSendCommand({
          sessionName: zmxSessionName,
          zmxExecutablePath: zmx.executablePath,
        }),
        { stdin: payload },
      );
      return {
        exitCode: result.exitCode,
        provider: "zmx",
        session,
        submit,
        textBytes: Buffer.byteLength(text, "utf8"),
        textLength: text.length,
        zmxName: zmxSessionName,
      };
    }
    default:
      throw new GxserverDomainStateError("notFound", `${endpointPath} is not a gxserver zmx session interaction endpoint.`);
  }
}

async function runZmxInteractionCommand(
  runtime: GxserverApiRuntime,
  script: string,
  options: GxserverZmxCommandOptions & { allowStdoutTruncation?: boolean } = {},
): Promise<GxserverZmxCommandResult> {
  const result = await (runtime.zmxLifecycle?.runZsh ?? runZshScript)(script, options);
  if (result.exitCode !== 0 && !(options.allowStdoutTruncation === true && result.stdoutTruncated === true)) {
    throw new GxserverTypedOperationError(
      "dependencyUnavailable",
      result.stderr || result.stdout || `zmx session interaction command exited ${result.exitCode}`,
    );
  }
  return result;
}

function claimGxserverFirstPromptAutoTitle(
  repository: GxserverDomainRepository,
  session: GxserverSessionDomainState,
  prompt: string | undefined,
): { claimed: boolean; session?: GxserverSessionDomainState } {
  const decision = decideGxserverFirstPromptAutoTitle(session, prompt);
  if (!decision.shouldRun || !decision.normalizedPrompt) {
    return { claimed: false };
  }
  const runtimeSettings = {
    ...clearGxserverFirstPromptAutoTitleCancellation(session.runtimeSettings),
    firstUserMessage: prompt,
    gxserverFirstPromptAutoTitleStatus: "running",
    gxserverFirstPromptAutoTitleStartedAt: new Date().toISOString(),
  };
  /*
  CDXC:GxserverSessionTitle 2026-06-04-04:05:
  First-prompt auto-title is a gxserver responsibility. Hooks and clients report observed agent identity plus first-user-message metadata; gxserver alone claims, generates, persists, and submits the rename command so macOS, Android, iOS, CLI, and future clients do not implement separate session-title flows.

  CDXC:GxserverSessionTitle 2026-06-07-18:16:
  Escape cancellation is scoped to the prompt that was cancelled. Clear cancellation metadata when a later prompt claims a fresh job so retries can run without preserving stale cancelled state.
  */
  const updated = repository.updateSession({
    projectId: session.projectId,
    runtimeSettings,
    sessionId: session.sessionId,
  });
  return { claimed: true, session: updated };
}

function cancelGxserverFirstPromptAutoTitle(
  repository: GxserverDomainRepository,
  params: GxserverCancelFirstPromptAutoTitleParams,
): GxserverCancelFirstPromptAutoTitleResult {
  const session = repository.getSession(params.projectId, params.sessionId);
  if (!session) {
    throw new GxserverDomainStateError(
      "notFound",
      `Session ${params.projectId}/${params.sessionId} does not exist.`,
    );
  }
  const previousStatus = readRuntimeText(session.runtimeSettings, "gxserverFirstPromptAutoTitleStatus");
  if (previousStatus !== "running") {
    return {
      changed: false,
      ...(previousStatus ? { previousStatus } : {}),
      reason: previousStatus ? `already-${previousStatus}` : "not-running",
      session,
    };
  }
  const cancelledPrompt = readRuntimeText(session.runtimeSettings, "firstUserMessage");
  /*
  CDXC:GxserverSessionTitle 2026-06-04-07:43:
  Escape cancellation is a gxserver state transition, not a local sidebar suppression. Mark the first-prompt title job as cancelled so presentation clears the terminal overlay and the background job can observe the terminal status before sending `/rename`.

  CDXC:GxserverSessionTitle 2026-06-07-18:16:
  Store the prompt associated with the cancelled job separately from `firstUserMessage`. Later session-state events may replace `firstUserMessage`, and gxserver must distinguish a stale retry of the same prompt from a new prompt that should start a new title job.
  */
  const updated = repository.updateSession({
    projectId: session.projectId,
    runtimeSettings: {
      ...session.runtimeSettings,
      gxserverFirstPromptAutoTitleCancelledAt: new Date().toISOString(),
      ...(cancelledPrompt ? { gxserverFirstPromptAutoTitleCancelledPrompt: cancelledPrompt } : {}),
      gxserverFirstPromptAutoTitleReason: params.reason ?? "userCancelled",
      gxserverFirstPromptAutoTitleStatus: "cancelled",
    },
    sessionId: session.sessionId,
  });
  return {
    changed: true,
    previousStatus,
    reason: "cancelled",
    session: updated,
  };
}

function scheduleGxserverFirstPromptAutoTitleJob(
  runtime: GxserverApiRuntime,
  input: {
    projectId: GxserverProjectId;
    requestId: string;
    sessionId: GxserverSessionId;
  },
): void {
  const timeout = setTimeout(() => {
    void runGxserverFirstPromptAutoTitleJob(runtime, input);
  }, 0);
  timeout.unref();
}

/*
CDXC:GxserverSessionTitle 2026-06-13-00:00:
Codex's first-prompt path stays in `running` long enough for the 250ms presentation coalescer (#GxserverPresentationEvents) to flush an observable `isGeneratingFirstPromptTitle: true` delta before it applies, so native macOS records the running→applied transition that gates its real Enter submit (`first-prompt-title-submit.ts`). Claude's bare `/rename` does no title generation and would otherwise apply within the same coalescer window, collapsing running and applied into a single applied delta — native never sees the transition and never submits Enter. Hold the bare-rename job past one presentation cadence so the staged command is submitted exactly like a generated Codex title.
*/
const GXSERVER_BARE_RENAME_GENERATING_WINDOW_MS = 400;

function delayGxserverBareRenameGeneratingWindow(): Promise<void> {
  return new Promise((resolve) => {
    const timeout = setTimeout(resolve, GXSERVER_BARE_RENAME_GENERATING_WINDOW_MS);
    timeout.unref();
  });
}

async function runGxserverFirstPromptAutoTitleJob(
  runtime: GxserverApiRuntime,
  input: {
    projectId: GxserverProjectId;
    requestId: string;
    sessionId: GxserverSessionId;
  },
): Promise<void> {
  const db = openGxserverDatabase(runtime.paths);
  try {
    const repository = new GxserverDomainRepository(db, runtime.metadata.serverId);
    const session = repository.getSession(input.projectId, input.sessionId);
    const project = repository.getProject(input.projectId);
    if (!session || !project) {
      return;
    }
    const prompt = readRuntimeText(session.runtimeSettings, "firstUserMessage");
    const decision = decideGxserverFirstPromptAutoTitle(session, prompt, { allowRunning: true });
    if (!decision.shouldRun || !decision.normalizedPrompt || !decision.strategy) {
      await markGxserverFirstPromptAutoTitleSkipped(runtime, repository, session, decision.reason, input.requestId);
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: input.projectId,
        reason: "first-prompt-auto-title-skipped",
        sessionId: input.sessionId,
      });
      return;
    }
    if (decision.strategy === "sendBareRenameCommand") {
      /*
      CDXC:GxserverSessionTitle 2026-06-13-00:00:
      Keep the bare-rename job in `running` for one presentation cadence so native observes the generating→applied transition that gates its Enter submit. Re-reading `latestSession` below still runs after this wait, so an Escape cancellation during the window is honored before the staged `/rename` is submitted.
      */
      await delayGxserverBareRenameGeneratingWindow();
    }
    const title = decision.strategy === "sendBareRenameCommand"
      ? undefined
      : await generateGxserverFirstPromptSessionTitle(
          runtime,
          project.path,
          decision.normalizedPrompt,
          session.runtimeSettings,
        );
    const latestSession = repository.getSession(input.projectId, input.sessionId);
    if (!latestSession) {
      return;
    }
    const latestStatus = readRuntimeText(latestSession.runtimeSettings, "gxserverFirstPromptAutoTitleStatus");
    if (latestStatus === "cancelled") {
      /*
      CDXC:GxserverSessionTitle 2026-06-04-07:43:
      Title generation may finish after the user presses Escape. Re-read gxserver state after generation and before provider interaction so a cancelled job never submits `/rename` even though the title model already returned text.
      */
      schedulePresentationSessionDelta(runtime, repository, {
        projectId: latestSession.projectId,
        reason: "first-prompt-auto-title-cancelled",
        sessionId: latestSession.sessionId,
      });
      await runtime.logger.log({
        details: {
          agentName: getGxserverFirstPromptAgentName(latestSession),
          status: "cancelled",
        },
        event: "session-title.firstPromptAutoTitle",
        level: "info",
        projectId: latestSession.projectId,
        requestId: input.requestId,
        serverId: runtime.metadata.serverId,
        sessionId: latestSession.sessionId,
      });
      return;
    }
    if (
      latestStatus !== "running" ||
      normalizeGxserverFirstPromptTitlePrompt(readRuntimeText(latestSession.runtimeSettings, "firstUserMessage")) !==
        decision.normalizedPrompt
    ) {
      /*
      CDXC:GxserverSessionTitle 2026-06-07-01:19:
      Escape cancels only the prompt that was being titled. A later prompt may claim a new title job while the cancelled generator is still returning; compare the stored prompt before submitting `/rename` so stale jobs cannot apply their old title over the retry.
      */
      await runtime.logger.log({
        details: {
          agentName: getGxserverFirstPromptAgentName(latestSession),
          status: "stale",
        },
        event: "session-title.firstPromptAutoTitle",
        level: "debug",
        projectId: latestSession.projectId,
        requestId: input.requestId,
        serverId: runtime.metadata.serverId,
        sessionId: latestSession.sessionId,
      });
      return;
    }
    const commandText =
      decision.strategy === "sendBareRenameCommand"
        ? "/rename"
        : decision.strategy === "generateTitleAndName"
          ? `/name ${title}`
          : `/rename ${title}`;
    const zmx = await (runtime.zmxLifecycle?.requireZmx ?? requireBundledZmx)();
    /*
    CDXC:GxserverSessionTitle 2026-06-05-12:43:
    Generated first-prompt titles must be staged as terminal text only. The macOS client observes the applied title transition and sends the real programmatic Enter key event, because appending a carriage return to zmx text input can insert a newline in agent prompt editors instead of submitting the staged rename command.

    CDXC:GxserverSessionTitle 2026-06-12-07:08:
    Claude's first-prompt naming path stages only `/rename` and relies on Claude to choose the title after native submits the staged command. Keep the same staged-text plus native-Enter bridge as generated Codex titles so gxserver does not write carriage returns into agent prompt editors.
    */
    await runZmxInteractionCommand(
      runtime,
      buildZmxSendCommand({
        sessionName: providerZmxSessionName(latestSession),
        zmxExecutablePath: zmx.executablePath,
      }),
      { stdin: commandText },
    );
    const updated = repository.updateSession({
      projectId: latestSession.projectId,
      runtimeSettings: {
        ...latestSession.runtimeSettings,
        autoTitleFromFirstPrompt: true,
        gxserverFirstPromptAutoTitleAppliedAt: new Date().toISOString(),
        gxserverFirstPromptAutoTitleReason: decision.reason,
        gxserverFirstPromptAutoTitleShouldSubmitStagedCommand: true,
        gxserverFirstPromptAutoTitleStatus: "applied",
        ...(title ? { titleSource: "generated" } : {}),
      },
      sessionId: latestSession.sessionId,
      ...(title ? { title } : {}),
    });
    observeZmxTitleForSession(runtime, updated, "first-prompt-auto-title");
    schedulePresentationSessionDelta(runtime, repository, {
      projectId: updated.projectId,
      reason: "first-prompt-auto-title",
      sessionId: updated.sessionId,
    });
    await runtime.logger.log({
      details: {
        agentName: getGxserverFirstPromptAgentName(updated),
        status: "applied",
        strategy: decision.strategy,
        ...(title ? { titleLength: title.length } : {}),
      },
      event: "session-title.firstPromptAutoTitle",
      level: "info",
      projectId: updated.projectId,
      requestId: input.requestId,
      serverId: runtime.metadata.serverId,
      sessionId: updated.sessionId,
    });
  } catch (error) {
    await markGxserverFirstPromptAutoTitleFailed(runtime, input, error);
  } finally {
    db.close();
  }
}

async function markGxserverFirstPromptAutoTitleSkipped(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  session: GxserverSessionDomainState,
  reason: string,
  requestId: string,
): Promise<void> {
  const updated = repository.updateSession({
    projectId: session.projectId,
    runtimeSettings: {
      ...session.runtimeSettings,
      gxserverFirstPromptAutoTitleReason: reason,
      gxserverFirstPromptAutoTitleStatus: "skipped",
    },
    sessionId: session.sessionId,
  });
  await runtime.logger.log({
    details: {
      agentName: getGxserverFirstPromptAgentName(updated),
      reason,
      status: "skipped",
    },
    event: "session-title.firstPromptAutoTitle",
    level: "debug",
    projectId: updated.projectId,
    requestId,
    serverId: runtime.metadata.serverId,
    sessionId: updated.sessionId,
  });
}

async function markGxserverFirstPromptAutoTitleFailed(
  runtime: GxserverApiRuntime,
  input: {
    projectId: GxserverProjectId;
    requestId: string;
    sessionId: GxserverSessionId;
  },
  error: unknown,
): Promise<void> {
  const db = openGxserverDatabase(runtime.paths);
  try {
    const repository = new GxserverDomainRepository(db, runtime.metadata.serverId);
    const session = repository.getSession(input.projectId, input.sessionId);
    if (!session) {
      return;
    }
    const updated = repository.updateSession({
      projectId: session.projectId,
      runtimeSettings: {
        ...session.runtimeSettings,
        gxserverFirstPromptAutoTitleFailedAt: new Date().toISOString(),
        gxserverFirstPromptAutoTitleStatus: "failed",
      },
      sessionId: session.sessionId,
    });
    schedulePresentationSessionDelta(runtime, repository, {
      projectId: updated.projectId,
      reason: "first-prompt-auto-title-failed",
      sessionId: updated.sessionId,
    });
    await runtime.logger.log({
      details: {
        agentName: getGxserverFirstPromptAgentName(updated),
        status: "failed",
      },
      error: error instanceof GxserverTypedOperationError ? error.code : error instanceof Error ? error.name : "unknown",
      event: "session-title.firstPromptAutoTitle",
      level: "warn",
      projectId: updated.projectId,
      requestId: input.requestId,
      serverId: runtime.metadata.serverId,
      sessionId: updated.sessionId,
    });
  } finally {
    db.close();
  }
}

function decideGxserverFirstPromptAutoTitle(
  session: GxserverSessionDomainState,
  prompt: string | undefined,
  options: { allowRunning?: boolean } = {},
): {
  normalizedPrompt?: string;
  reason: string;
  shouldRun: boolean;
  strategy?: "sendBareRenameCommand" | "generateTitleAndRename" | "generateTitleAndName";
} {
  const status = readRuntimeText(session.runtimeSettings, "gxserverFirstPromptAutoTitleStatus");
  const normalizedPrompt = normalizeGxserverFirstPromptTitlePrompt(prompt);
  const cancelledPrompt =
    normalizeGxserverFirstPromptTitlePrompt(
      readRuntimeText(session.runtimeSettings, "gxserverFirstPromptAutoTitleCancelledPrompt"),
    ) ??
    normalizeGxserverFirstPromptTitlePrompt(
      readRuntimeText(session.runtimeSettings, "firstUserMessage"),
    );
  const isCancelledRetryPrompt =
    status === "cancelled" &&
    normalizedPrompt !== undefined &&
    normalizedPrompt !== cancelledPrompt;
  if (
    (status === "running" && !options.allowRunning) ||
    status === "applied" ||
    status === "failed" ||
    status === "skipped" ||
    (status === "cancelled" && !isCancelledRetryPrompt)
  ) {
    return { reason: `already-${status}`, shouldRun: false };
  }
  if (session.runtimeSettings.autoTitleFromFirstPrompt === true) {
    return { reason: "alreadyAutoNamed", shouldRun: false };
  }
  const agentName = getGxserverFirstPromptAgentName(session);
  const strategy = resolveGxserverFirstPromptAutoTitleStrategy(agentName);
  if (!strategy) {
    return { reason: "unsupportedAgent", shouldRun: false };
  }
  if (!normalizedPrompt) {
    return { reason: "emptyPrompt", shouldRun: false, strategy };
  }
  if (isGxserverFirstPromptMetaPrompt(normalizedPrompt)) {
    return { normalizedPrompt, reason: "metaPrompt", shouldRun: false, strategy };
  }
  if (
    normalizedPrompt.length <= 50 &&
    /(?:^|\r?\n)[ \t]*\/[a-z][\w-]*(?=\s|$|[).,:;!?'"`])/iu.test(prompt ?? "")
  ) {
    return { normalizedPrompt, reason: "slashCommand", shouldRun: false, strategy };
  }
  if (!isGxserverGenericAgentSessionTitle(agentName, session.title)) {
    return { normalizedPrompt, reason: "nonGenericCurrentTitle", shouldRun: false, strategy };
  }
  return { normalizedPrompt, reason: "eligible", shouldRun: true, strategy };
}

function clearGxserverFirstPromptAutoTitleCancellation(
  runtimeSettings: Record<string, unknown>,
): Record<string, unknown> {
  const {
    gxserverFirstPromptAutoTitleCancelledAt: _cancelledAt,
    gxserverFirstPromptAutoTitleCancelledPrompt: _cancelledPrompt,
    gxserverFirstPromptAutoTitleReason: _cancelledReason,
    ...nextRuntimeSettings
  } = runtimeSettings;
  return nextRuntimeSettings;
}

function resolveGxserverFirstPromptAutoTitleStrategy(
  agentName: string | undefined,
): "sendBareRenameCommand" | "generateTitleAndRename" | "generateTitleAndName" | undefined {
  const normalized = normalizeGxserverAgentName(agentName);
  if (normalized === "claude") {
    /*
    CDXC:GxserverSessionTitle 2026-06-12-07:08:
    Claude Code hooks expose the first user prompt when the session starts working, but unrenamed Claude rows can remain at the generic `Claude Code` title. Stage a bare `/rename` command for Claude instead of generating a title in gxserver because Claude can generate its own title from the active conversation after the command is submitted.
    */
    return "sendBareRenameCommand";
  }
  if (normalized === "codex") {
    /*
    CDXC:GxserverSessionTitle 2026-06-12-07:08:
    Codex still needs gxserver to generate the first-prompt title before staging `/rename <generated title>`, while Claude receives only a bare `/rename` so the provider names itself.
    */
    return "generateTitleAndRename";
  }
  if (normalized === "pi") {
    return "generateTitleAndName";
  }
  return undefined;
}

function getGxserverFirstPromptAgentName(session: GxserverSessionDomainState): string | undefined {
  return session.agentId ?? readRuntimeText(session.runtimeSettings, "agentName");
}

function normalizeGxserverAgentName(value: string | undefined): string | undefined {
  const normalized = value?.trim().toLowerCase();
  if (!normalized) {
    return undefined;
  }
  if (normalized === "openai codex" || normalized === "codex cli") {
    return "codex";
  }
  if (normalized === "claude code") {
    return "claude";
  }
  if (normalized === "π") {
    return "pi";
  }
  return normalized;
}

function isGxserverGenericAgentSessionTitle(agentName: string | undefined, title: string | undefined): boolean {
  const normalizedTitle = title?.trim().replace(/\s+/gu, " ").toLowerCase();
  if (!normalizedTitle) {
    return true;
  }
  const normalizedAgent = normalizeGxserverAgentName(agentName);
  const generic = new Set([
    "terminal",
    "terminal session",
    "agent",
    "agent session",
    "claude",
    "claude code",
    "claude session",
    "codex",
    "codex cli",
    "codex session",
    "openai codex",
    "openai codex session",
    "pi",
    "π",
    "pi session",
  ]);
  return generic.has(normalizedTitle) || (normalizedAgent !== undefined && normalizedTitle === normalizedAgent);
}

function normalizeGxserverFirstPromptTitlePrompt(prompt: string | undefined): string | undefined {
  const normalized = prompt?.replace(/\s+/gu, " ").trim();
  if (!normalized) {
    return undefined;
  }
  const stripped = normalized
    .replace(
      /^(?:(?:please|kindly|hey|hi|hello)\s+|(?:can|could|would|will)\s+you\s+|(?:can|could|would)\s+we\s+|help\s+me\s+|i\s+need(?:\s+you)?\s+to\s+|i\s+need\s+|how\s+do\s+i\s+|how\s+does\s+|is\s+there\s+(?:any\s+)?way\s+to\s+)+/iu,
      "",
    )
    .trim();
  const cleaned = (stripped || normalized).replace(/[.?!:;,]+$/gu, "").trim();
  return cleaned || undefined;
}

function isGxserverFirstPromptMetaPrompt(prompt: string): boolean {
  if (prompt.startsWith("# AGENTS") || prompt.includes("tool_use_id")) {
    return true;
  }
  return [
    "<command",
    "<environment_context",
    "<permissions instructions>",
    "<user_instructions>",
    "<INSTRUCTIONS>",
    "<collaboration_mode>",
    "<app-context>",
    "<turn_aborted>",
    "<ide_opened_file>",
    "<local-",
    "[Tool Result]",
    "Caveat:",
  ].some((prefix) => prompt.startsWith(prefix));
}

async function generateGxserverFirstPromptSessionTitle(
  runtime: GxserverApiRuntime,
  cwd: string | undefined,
  prompt: string,
  runtimeSettings: Record<string, unknown>,
): Promise<string> {
  const injectedGenerator = runtime.firstPromptTitleGeneration?.generateTitle;
  if (injectedGenerator) {
    return parseGxserverGeneratedSessionTitleText(await injectedGenerator({ cwd, prompt }));
  }
  const sourceText = prompt.slice(0, GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH);
  const generationPrompt = buildGxserverFirstPromptTitleGenerationPrompt(sourceText);
  const delimiter = `ghostex_GXSERVER_SESSION_TITLE_${Date.now().toString(36)}`;
  const agent = normalizeGxserverFirstPromptTitleGenerationAgent(
    runtimeSettings.firstPromptTitleGenerationAgent,
  );
  const command = buildGxserverFirstPromptTitleGenerationCommand({
    agent,
    command: readGxserverFirstPromptTitleGenerationCommand(runtimeSettings, agent),
    delimiter,
    prompt: generationPrompt,
  });
  const commandEnvironment = gxserverInternalPromptGenerationEnvironment(gxserverCommandEnvironment({
    ...process.env,
    HOME: runtime.paths.homeDir,
  }));
  const pathValue = await normalizeGxserverProcessPath(process.env.PATH, commandEnvironment);
  const result = await execFileAsync("/bin/zsh", ["-lic", command], {
    cwd: cwd || runtime.paths.homeDir,
    env: {
      ...commandEnvironment,
      PATH: pathValue,
    },
    maxBuffer: 64 * 1024,
    timeout: GXSERVER_FIRST_PROMPT_TITLE_GENERATION_TIMEOUT_MS,
  });
  return parseGxserverGeneratedSessionTitleText(String(result.stdout ?? ""));
}

function gxserverCommandEnvironment(environment: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  /*
  CDXC:GxserverCommandColorEnv 2026-06-07-00:38:
  gxserver title-generation commands can launch agent CLIs outside an attached terminal. Strip NO_COLOR-style keys from daemon command env snapshots so generated-title helpers do not run in color-disabled mode.
  */
  const sanitized = { ...environment };
  for (const key of GXSERVER_COMMAND_COLOR_DISABLING_ENVIRONMENT_KEYS) {
    delete sanitized[key];
  }
  return sanitized;
}

function gxserverInternalPromptGenerationEnvironment(environment: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  /*
  CDXC:GxserverSessionTitle 2026-06-07-01:57:
  Internal Codex title generation must never attach itself to the user-facing terminal session being named. Strip Ghostex session-binding variables before launching the prompt job and mark the process so installed hooks ignore it instead of saving a title-generator transcript as restorable session identity.
  */
  const sanitized = { ...environment };
  for (const key of GXSERVER_INTERNAL_PROMPT_GENERATION_ENVIRONMENT_KEYS) {
    delete sanitized[key];
  }
  sanitized.GHOSTEX_INTERNAL_PROMPT_GENERATION = "1";
  sanitized.GHOSTEX_INTERNAL_TITLE_GENERATION = "1";
  return sanitized;
}

function normalizeGxserverFirstPromptTitleGenerationAgent(
  value: unknown,
): GxserverFirstPromptTitleGenerationAgent {
  return value === "cursor" || value === "claude" || value === "grok" || value === "custom"
    ? value
    : "codex";
}

function readGxserverFirstPromptTitleGenerationCommand(
  runtimeSettings: Record<string, unknown>,
  agent: GxserverFirstPromptTitleGenerationAgent,
): string {
  const configuredCommand =
    typeof runtimeSettings.firstPromptTitleGenerationCommand === "string"
      ? runtimeSettings.firstPromptTitleGenerationCommand.trim()
      : "";
  if (configuredCommand) {
    return configuredCommand;
  }
  switch (agent) {
    case "codex":
      return "codex";
    case "cursor":
      return "cursor-agent";
    case "claude":
      return "claude";
    case "grok":
      return "grok";
    case "custom":
      throw new GxserverTypedOperationError(
        "badRequest",
        "Custom title generation command is not configured.",
      );
  }
}

function buildGxserverFirstPromptTitleGenerationCommand(input: {
  agent: GxserverFirstPromptTitleGenerationAgent;
  command: string;
  delimiter: string;
  prompt: string;
}): string {
  /*
  CDXC:GxserverSessionTitle 2026-06-04-08:24:
  Settings can select Codex, Cursor, Claude, Grok Build, or a custom title generator for gxserver-owned first-prompt titles. Keep Codex on the existing low-reasoning `gpt-5.4-mini` command, use Claude's Haiku alias for this short summarization task, pass the prompt over stdin for heredoc-friendly CLIs, and let Cursor use print plus YOLO mode so the background job receives title text on stdout without opening an interactive pane.

  CDXC:GxserverSessionTitle 2026-06-04-22:44:
  Grok Build is available as a first-prompt title generator. The local `grok --help` contract exposes headless `-p`, `--model`, plain output, no alternate screen, no plan, no subagents, web-search disabling, and max-turn limits; `grok models` exposes Composer 2.5 as `grok-composer-2.5-fast`, so use that exact model id for the background summarization command.

  CDXC:GxserverSessionTitle 2026-06-07-01:57:
  Codex first-prompt title generation is internal summarization, not a user-restorable conversation. Run Codex with `--ephemeral` so the title prompt cannot create a persistent transcript that sleep/wake restore may later resolve by id or title.
  */
  switch (input.agent) {
    case "codex":
      return createGxserverHereDocCommand(
        `${input.command} exec --ephemeral --skip-git-repo-check -m gpt-5.4-mini -c 'model_reasoning_effort="low"'`,
        input.delimiter,
        input.prompt,
      );
    case "cursor":
      return [
        input.command,
        "--print --yolo --trust --output-format text",
        quoteGxserverShellArg(input.prompt),
      ].join(" ");
    case "claude":
      return createGxserverHereDocCommand(
        `${input.command} -p --model haiku`,
        input.delimiter,
        input.prompt,
      );
    case "grok":
      return [
        input.command,
        "-p --model grok-composer-2.5-fast --output-format plain --no-alt-screen --no-plan --no-subagents --disable-web-search --max-turns 1",
        quoteGxserverShellArg(input.prompt),
      ].join(" ");
    case "custom":
      return createGxserverHereDocCommand(input.command, input.delimiter, input.prompt);
  }
}

function createGxserverHereDocCommand(command: string, delimiter: string, body: string): string {
  return `${command} <<'${delimiter}'\n${body}\n${delimiter}`;
}

function quoteGxserverShellArg(value: string): string {
  return `'${value.replace(/'/gu, "'\\''")}'`;
}

function buildGxserverFirstPromptTitleGenerationPrompt(sourceText: string): string {
  return [
    "Write a concise session title that summarizes the user's text.",
    "Return plain text only.",
    "Rules:",
    "- keep it specific and scannable",
    "- prefer 2 to 4 words when possible",
    `- must be fewer than ${GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH + 1} characters`,
    "- do not abbreviate with ellipses",
    "- do not use quotes, markdown, or commentary",
    "- do not end with punctuation",
    "- focus on the task, bug, feature, or topic",
    "",
    "User text:",
    sourceText,
    "",
    "Output handling:",
    "- Produce only the final session title.",
    "- Do not wrap the result in backticks.",
    "- Print only the final result to stdout.",
  ].join("\n");
}

function parseGxserverGeneratedSessionTitleText(value: string): string {
  const normalized = normalizeGxserverGeneratedText(value);
  const titleLine = normalized.split(/\r?\n/gu).find((line) => line.trim().length > 0);
  if (!titleLine) {
    throw new Error("Title generation returned an empty session title.");
  }
  const sanitized = titleLine
    .replace(/^["'`]+|["'`]+$/gu, "")
    .replace(/\s+/gu, " ")
    .trim()
    .replace(/[.…]+$/gu, "");
  if (!sanitized) {
    throw new Error("Title generation returned an empty session title.");
  }
  return clampGxserverGeneratedSessionTitleLength(sanitized);
}

function normalizeGxserverGeneratedText(value: string): string {
  const trimmed = value.trim();
  const fencedMatch = /^```(?:[a-z0-9_-]+)?\n([\s\S]*?)\n```$/iu.exec(trimmed);
  return fencedMatch ? fencedMatch[1].trim() : trimmed;
}

function clampGxserverGeneratedSessionTitleLength(value: string): string {
  if (value.length <= GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH) {
    return value;
  }
  const words = value.split(" ").filter(Boolean);
  let candidate = "";
  for (const word of words) {
    const nextCandidate = candidate ? `${candidate} ${word}` : word;
    if (nextCandidate.length > GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH) {
      break;
    }
    candidate = nextCandidate;
  }
  return candidate || value.slice(0, GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH).trim();
}

function readInteractionText(value: unknown, commandName: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new GxserverDomainStateError("badRequest", `${commandName} requires non-empty text.`);
  }
  const textBytes = Buffer.byteLength(value, "utf8");
  if (textBytes > GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES) {
    throw new GxserverDomainStateError(
      "badRequest",
      `${commandName} text exceeds the ${GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES}-byte zmx send limit.`,
    );
  }
  return value;
}

async function createAttachSessionMetadata(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  params: GxserverAttachSessionMetadataParams,
  requestId: string,
  agentSettings: GxserverAgentSettings,
): Promise<GxserverAttachSessionMetadataResult> {
  const lifecycle = readSessionLifecycleParams(params as unknown as Record<string, unknown>);
  const project = repository.getProject(lifecycle.projectId);
  const session = repository.getSession(lifecycle.projectId, lifecycle.sessionId);
  if (!project) {
    throw new GxserverDomainStateError("notFound", `Project ${lifecycle.projectId} does not exist.`);
  }
  if (!session) {
    throw new GxserverDomainStateError(
      "notFound",
      `Session ${lifecycle.projectId}/${lifecycle.sessionId} does not exist.`,
    );
  }

  const cwd = session.cwd ?? project.path;
  const { probe, session: probedSession, zmx, zmxSessionName } = await probeAndCacheSessionProvider(
    runtime,
    repository,
    lifecycle,
  );
  /*
  CDXC:GxserverTerminalWake 2026-06-01-12:07:
  Sleeping-session wake should let gxserver rebuild the real agent resume command from shared session metadata. A macOS renderer can legitimately pass an empty string when its legacy local resume builder has no text; treat blank startupText as absent instead of suppressing the server-owned resume plan.
  */
  const startupText =
    normalizeOptionalStartupText(params.startupText) ??
    getAgentStartupTextForSession(project, probedSession, agentSettings);
  const startupTextDisposition = decideStartupTextDisposition({
    providerState: probe.lifecycleState,
    startupText,
  });

  if (probe.lifecycleState === "missing" && !(cwd && (await (runtime.zmxLifecycle?.cwdExists ?? defaultCwdExists)(cwd)))) {
    await runtime.logger.log({
      event: "zmx.restoreBlocked",
      level: "warn",
      projectId: lifecycle.projectId,
      requestId,
      serverId: runtime.metadata.serverId,
      sessionId: lifecycle.sessionId,
      details: {
        cwdConfigured: cwd !== undefined,
        reason: "missingCwd",
        zmxName: zmxSessionName,
      },
    });
    return {
      provider: "zmx",
      providerState: probe,
      restoreBlocked: {
        ...(cwd ? { cwd } : {}),
        reason: "missingCwd",
      },
      session: probedSession,
      ...(startupTextDisposition === "queueAfterTerminalReady" && startupText ? { startupText } : {}),
      startupTextDisposition,
      zmxName: zmxSessionName,
    };
  }

  return {
    attachCommand: buildZmxAttachCommand({
      cwd: cwd ?? "",
      globalSessionRef: probedSession.globalRef,
      gxserverAuthTokenFile: runtime.paths.authTokenFile,
      gxserverBaseUrl: `http://${GXSERVER_LOCAL_API_HOST}:${GXSERVER_LOCAL_API_PORT}`,
      gxserverProtocolVersion: GXSERVER_PROTOCOL_VERSION,
      promptEditor: normalizePromptEditorAttachMode(params.promptEditor),
      sessionName: zmxSessionName,
      title: probedSession.title,
      zmxExecutablePath: zmx.executablePath,
    }),
    cwd,
    persistenceSessionCreated: probe.lifecycleState === "missing",
    provider: "zmx",
    providerState: probe,
    session: probedSession,
    ...(startupTextDisposition === "queueAfterTerminalReady" && startupText ? { startupText } : {}),
    startupTextDisposition,
    zmxName: zmxSessionName,
  };
}

function normalizeOptionalStartupText(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value : undefined;
}

function normalizePromptEditorAttachMode(value: unknown): "monaco" | undefined {
  return value === "monaco" ? "monaco" : undefined;
}

async function probeAndCacheSessionProvider(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  lifecycle: GxserverSessionLifecycleParams,
): Promise<{
  probe: GxserverProviderProbeResult;
  session: GxserverSessionDomainState;
  zmx: GxserverResolvedTool;
  zmxSessionName: GxserverProviderProbeResult["zmxName"];
}> {
  const session = requireSession(repository, lifecycle);
  const zmx = await (runtime.zmxLifecycle?.requireZmx ?? requireBundledZmx)();
  const zmxSessionName = providerZmxSessionName(session);
  const probe = await probeZmxSession({
    runZsh: runtime.zmxLifecycle?.runZsh,
    sessionName: zmxSessionName,
    zmxExecutablePath: zmx.executablePath,
  });
  const updated = repository.updateSession({
    lifecycleState: reconcileDomainLifecycleFromProviderProbe(session.lifecycleState, probe.lifecycleState),
    projectId: session.projectId,
    providerState: providerStatePatch(session, probe),
    sessionId: session.sessionId,
  });
  return { probe, session: updated, zmx, zmxSessionName };
}

function reconcileDomainLifecycleFromProviderProbe(
  currentLifecycleState: GxserverSessionDomainState["lifecycleState"],
  providerLifecycleState: GxserverProviderProbeResult["lifecycleState"],
): GxserverSessionDomainState["lifecycleState"] {
  /*
  CDXC:GxserverSessionLifecycle 2026-06-07-17:08:
  TUI, Android, and iOS create and attach sessions through gxserver, while macOS renders only presentation-active domain lifecycle states. A successful shared zmx probe is the daemon-owned signal that a non-stopped client-created row is running, so promote it here instead of making each client duplicate macOS session-visibility logic.
  */
  if (providerLifecycleState === "exists" && currentLifecycleState !== "stopped") {
    return "running";
  }
  return currentLifecycleState;
}

async function killAndCacheSessionProvider(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  lifecycle: GxserverSessionLifecycleParams,
  reason: string,
  lifecycleState: "sleeping" | "stopped",
): Promise<{ kill: GxserverProviderKillResult; session: GxserverSessionDomainState }> {
  const session = requireSession(repository, lifecycle);
  const zmx = await (runtime.zmxLifecycle?.requireZmx ?? requireBundledZmx)();
  const zmxSessionName = providerZmxSessionName(session);
  const kill = await killZmxSession({
    runZsh: runtime.zmxLifecycle?.runZsh,
    sessionName: zmxSessionName,
    zmxExecutablePath: zmx.executablePath,
  });
  const timestamp = new Date().toISOString();
  /*
  CDXC:GxserverZmxLifecycle 2026-05-30-19:41:
  Sleep and kill requests may fail after zmx has kept the provider session alive. A failed kill must leave the stored session and provider lifecycle as `unknown` with the kill error instead of persisting terminal UI state (`sleeping`/`stopped`) or provider `missing`, so clients retain the route to the possible live zmx session.
  */
  const updated = repository.updateSession({
    lifecycleState: kill.killed ? lifecycleState : "unknown",
    projectId: session.projectId,
    providerState: kill.killed
      ? missingProviderStatePatch(session, timestamp)
      : failedKillProviderStatePatch(session, kill, timestamp),
    sessionId: session.sessionId,
  });
  await runtime.logger.log({
    event: kill.killed ? "zmx.kill.completed" : "zmx.kill.failed",
    level: kill.killed ? "info" : "warn",
    projectId: session.projectId,
    serverId: runtime.metadata.serverId,
    sessionId: session.sessionId,
    details: {
      exitCode: kill.exitCode,
      reason,
      zmxName: zmxSessionName,
    },
    ...(kill.error ? { error: kill.error } : {}),
  });
  return { kill, session: updated };
}

async function startSessionProvider(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  params: GxserverStartSessionProviderParams,
  requestId: string,
  agentSettings: GxserverAgentSettings,
): Promise<GxserverStartSessionProviderResult> {
  const lifecycle = readSessionLifecycleParams(params as unknown as Record<string, unknown>);
  const project = repository.getProject(lifecycle.projectId);
  if (!project) {
    throw new GxserverDomainStateError("notFound", `Project ${lifecycle.projectId} does not exist.`);
  }
  const { probe, session: probedSession, zmx, zmxSessionName } = await probeAndCacheSessionProvider(
    runtime,
    repository,
    lifecycle,
  );
  const startupText =
    normalizeOptionalStartupText(params.startupText) ??
    getAgentLaunchStartupTextForSession(probedSession) ??
    getAgentStartupTextForSession(project, probedSession, agentSettings);
  const startupTextDisposition = decideStartupTextDisposition({
    providerState: probe.lifecycleState,
    startupText,
  });
  const shouldStartProviderWithStartupText = startupTextDisposition === "queueAfterTerminalReady" && Boolean(startupText?.trim());
  /*
  CDXC:GxserverZmxLifecycle 2026-06-09-09:53:
  Plain terminal sessions created by TUI/CLI clients have no startup text, but they still need gxserver to materialize the zmx provider before interactive attach. Starting the shell provider here prevents a live zmx session from existing while gxserver presentation still caches providerState=missing and hides the row from macOS.
  */
  const shouldStartPlainTerminalProvider =
    probe.lifecycleState === "missing" &&
    startupTextDisposition === "none" &&
    probedSession.kind === "terminal";
  if (!shouldStartProviderWithStartupText && !shouldStartPlainTerminalProvider) {
    return {
      provider: "zmx",
      providerState: probe,
      session: probedSession,
      started: false,
      startupTextDisposition,
      zmxName: zmxSessionName,
    };
  }
  const cwd = probedSession.cwd ?? project.path;
  if (!(cwd && (await (runtime.zmxLifecycle?.cwdExists ?? defaultCwdExists)(cwd)))) {
    throw new GxserverTypedOperationError(
      "dependencyUnavailable",
      "Cannot start session provider because the project directory is missing.",
    );
  }
  /*
  CDXC:GxserverRemoteAgents 2026-06-03-01:47:
  Remote agent workflows need a real provider launch path without a macOS
  renderer. Start only missing zmx providers through detached `zmx run`, using
  the gxserver-owned launch/resume startup text, cwd, bundled zmx path, and
  global session identity. Existing providers must not receive replayed startup
  text because that would type a second agent command into a live session.

  CDXC:GxserverZmxLifecycle 2026-06-09-09:53:
  The same detached provider start path now covers plain terminal shells, with no startup text, so CLI/TUI attach no longer creates a zmx session outside gxserver's persisted lifecycle state.
  */
  const providerCommand = shouldStartProviderWithStartupText
    ? buildZmxRunCommand({
        cwd,
        globalSessionRef: probedSession.globalRef,
        gxserverAuthTokenFile: runtime.paths.authTokenFile,
        gxserverBaseUrl: `http://${GXSERVER_LOCAL_API_HOST}:${GXSERVER_LOCAL_API_PORT}`,
        gxserverProtocolVersion: GXSERVER_PROTOCOL_VERSION,
        sessionName: zmxSessionName,
        startupText: startupText!,
        zmxExecutablePath: zmx.executablePath,
      })
    : buildZmxShellProviderCommand({
        cwd,
        globalSessionRef: probedSession.globalRef,
        gxserverAuthTokenFile: runtime.paths.authTokenFile,
        gxserverBaseUrl: `http://${GXSERVER_LOCAL_API_HOST}:${GXSERVER_LOCAL_API_PORT}`,
        gxserverProtocolVersion: GXSERVER_PROTOCOL_VERSION,
        sessionName: zmxSessionName,
        zmxExecutablePath: zmx.executablePath,
      });
  const result = await runZmxInteractionCommand(runtime, providerCommand);
  const providerState: GxserverProviderProbeResult = {
    lifecycleState: "exists",
    probedAt: new Date().toISOString(),
    zmxName: zmxSessionName,
  };
  const session = repository.updateSession({
    lifecycleState: "running",
    projectId: probedSession.projectId,
    providerState: providerStatePatch(probedSession, providerState),
    sessionId: probedSession.sessionId,
  });
  await runtime.logger.log({
    event: "zmx.startProvider.completed",
    level: "info",
    projectId: session.projectId,
    requestId,
    serverId: runtime.metadata.serverId,
    sessionId: session.sessionId,
    details: {
      exitCode: result.exitCode,
      startupTextBytes: startupText ? Buffer.byteLength(startupText, "utf8") : 0,
      zmxChildEnvironment: summarizeZmxChildEnvironmentSanitization(),
      zmxName: zmxSessionName,
    },
  });
  return {
    exitCode: result.exitCode,
    provider: "zmx",
    providerState,
    session,
    started: true,
    startupTextDisposition,
    zmxName: zmxSessionName,
  };
}

function requireSession(
  repository: GxserverDomainRepository,
  lifecycle: GxserverSessionLifecycleParams,
): GxserverSessionDomainState {
  const session = repository.getSession(lifecycle.projectId, lifecycle.sessionId);
  if (!session) {
    throw new GxserverDomainStateError(
      "notFound",
      `Session ${lifecycle.projectId}/${lifecycle.sessionId} does not exist.`,
    );
  }
  return session;
}

async function forkSession(
  runtime: GxserverApiRuntime,
  repository: GxserverDomainRepository,
  params: GxserverSessionLifecycleParams,
  requestId: string,
  agentSettings: GxserverAgentSettings,
): Promise<GxserverForkSessionResult> {
  const lifecycle = readSessionLifecycleParams(params as unknown as Record<string, unknown>);
  const project = repository.getProject(lifecycle.projectId);
  if (!project) {
    throw new GxserverDomainStateError("notFound", `Project ${lifecycle.projectId} does not exist.`);
  }
  const sourceSession = requireSession(repository, lifecycle);
  const plan = buildAgentForkPlan(project, sourceSession, agentSettings);
  if (!plan.startupText?.trim() || !plan.primaryCommand?.trim()) {
    throw new GxserverDomainStateError(
      "badRequest",
      "Fork is only available for Codex, Claude, and Pi sessions with a restorable identity.",
    );
  }
  /*
  CDXC:GxserverForkSession 2026-06-04-07:42:
  Fork creates the canonical G-session and provider startup plan in gxserver before any client materializes UI. This lets macOS, Electron, CLI, Android, and iOS share the same Codex/Claude/Pi fork behavior and avoids using stale local sidebar fields as the availability check.
  */
  const createdSession = repository.createSession(
    createAgentForkSessionParams(project, sourceSession, plan) as GxserverCreateSessionDomainParams,
  );
  const provider = await startSessionProvider(
    runtime,
    repository,
    {
      projectId: createdSession.projectId,
      sessionId: createdSession.sessionId,
    },
    requestId,
    agentSettings,
  );
  const session = provider.session;
  observeZmxTitleForSession(runtime, session, "fork-session");
  schedulePresentationSessionDelta(runtime, repository, {
    projectId: session.projectId,
    reason: "fork-session",
    sessionId: session.sessionId,
  });
  return {
    plan,
    provider,
    session,
    sourceSession,
  };
}

function isDomainStateEndpoint(path: GxserverEndpointPath): boolean {
  return (
    path === "/api/createProject" ||
    path === "/api/readAgentSettings" ||
    path === "/api/updateAgentSettings" ||
    path === "/api/updateProject" ||
    path === "/api/listProjects" ||
    path === "/api/readProjectStatus" ||
    path === "/api/addProjectPath" ||
    path === "/api/removeProject" ||
    path === "/api/deleteWorktreeProject" ||
    path === "/api/createSession" ||
    path === "/api/createAgentSession" ||
    path === "/api/forkSession" ||
    path === "/api/readAgentLaunchPlan" ||
    path === "/api/readAgentResumePlan" ||
    path === "/api/requestSessionRename" ||
    path === "/api/cancelFirstPromptAutoTitle" ||
    path === "/api/ingestAgentHookEvent" ||
    path === "/api/ingestSessionStateEvent" ||
    path === "/api/ingestTerminalTitleEvent" ||
    path === "/api/updateAgentActivity" ||
    path === "/api/readPresentationSnapshot" ||
    path === "/api/searchSessions" ||
    path === "/api/listPreviousSessions" ||
    path === "/api/updateSession" ||
    path === "/api/updateSessionOrder" ||
    path === "/api/listSessions" ||
    path === "/api/removeSession"
  );
}

function isAgentHookEndpoint(path: GxserverEndpointPath): boolean {
  return path === "/api/readAgentHookStatus" || path === "/api/installAgentHooks";
}

async function handleAgentHookEndpoint(
  runtime: GxserverApiRuntime,
  path: GxserverEndpointPath,
  body: unknown,
  requestId: string,
): Promise<GxserverRpcSuccessResponse> {
  /*
  CDXC:AgentHooks 2026-06-03-20:28:
  OpenCode hook setup moved out of the macOS sidebar after nightly's gxserver
  split. Keep plugin status/install behind local authenticated daemon APIs so
  settings surfaces render results without owning plugin scripts or PATH probes.
  */
  const params = readRpcParams(body);
  const result = path === "/api/installAgentHooks"
    ? await installGxserverAgentHooks(runtime.paths, params as GxserverInstallAgentHooksParams)
    : await readGxserverAgentHookStatus(runtime.paths, params as GxserverReadAgentHookStatusParams);
  return {
    ok: true,
    product: GXSERVER_PRODUCT,
    protocolVersion: GXSERVER_PROTOCOL_VERSION,
    requestId,
    result: result as unknown as Record<string, unknown>,
  };
}

function isTypedOperationEndpoint(path: GxserverEndpointPath): boolean {
  return (
    path === "/api/runGitAction" ||
    path === "/api/runGitHubAction" ||
    path === "/api/runWorktreeAction" ||
    path === "/api/runProjectSetupCommand" ||
    path === "/api/runBeadsAction"
  );
}

function isRepositoryCloneEndpoint(path: GxserverEndpointPath): boolean {
  return (
    path === "/api/previewRepositoryClone" ||
    path === "/api/startRepositoryClone" ||
    path === "/api/readRepositoryCloneJob" ||
    path === "/api/cancelRepositoryCloneJob"
  );
}

function handleResolveGitRootForPathEndpoint(body: unknown): GxserverResolveGitRootForPathResult {
  const params = readDomainRpcParams(body);
  return { gitRoot: resolveGitRootForExistingDirectory(params.path) };
}

function isZmxLifecycleEndpoint(path: GxserverEndpointPath): boolean {
  return (
    path === "/api/attachSessionMetadata" ||
    path === "/api/probeSessionProvider" ||
    path === "/api/transitionSession" ||
    path === "/api/sleepSession" ||
    path === "/api/wakeSession" ||
    path === "/api/startSessionProvider" ||
    path === "/api/killSession"
  );
}

function isZmxSessionInteractionEndpoint(path: GxserverEndpointPath): boolean {
  return (
    path === "/api/readSessionText" ||
    path === "/api/sendSessionText" ||
    path === "/api/sendSessionMessage" ||
    path === "/api/sendSessionEnter" ||
    path === "/api/focusSession"
  );
}

function normalizeAddProjectPathParams(params: Record<string, unknown>): GxserverCreateProjectParams {
  const normalizedPath = normalizeExistingDirectoryPath(params.path ?? params.projectPath, "path");
  return {
    ...(params as unknown as GxserverCreateProjectParams),
    name:
      typeof params.name === "string" && params.name.trim()
        ? params.name.trim()
        : normalizedPath.split(/[\\/]/).filter(Boolean).at(-1) ?? normalizedPath,
    path: normalizedPath,
  };
}

function addProjectPath(
  repository: GxserverDomainRepository,
  params: Record<string, unknown>,
): GxserverProjectDomainState {
  const createParams = normalizeAddProjectPathParams(params);
  const projects = repository.listProjects();
  const worktree = detectRegisteredGitWorktreeMetadata(
    projects,
    createParams.path ?? "",
    createParams.name,
  );
  const existingProject = findProjectByPath(projects, createParams.path);
  if (existingProject) {
    if (worktree && !areProjectWorktreeMetadataEqual(existingProject.worktree, worktree)) {
      return repository.updateProject({
        projectId: existingProject.projectId,
        worktree,
      });
    }
    return existingProject;
  }
  return repository.createProject({
    ...createParams,
    ...(worktree ? { worktree } : {}),
  });
}

function areProjectWorktreeMetadataEqual(left: unknown, right: Record<string, unknown>): boolean {
  if (!left || typeof left !== "object" || Array.isArray(left)) {
    return false;
  }
  const current = left as Record<string, unknown>;
  return (
    current.branch === right.branch &&
    current.name === right.name &&
    current.parentProjectId === right.parentProjectId &&
    current.parentProjectName === right.parentProjectName &&
    current.parentProjectPath === right.parentProjectPath
  );
}

function normalizeCreateSessionParams(
  repository: GxserverDomainRepository,
  params: Record<string, unknown>,
): GxserverCreateSessionDomainParams {
  const project = resolveCreateSessionProject(repository, params);
  return {
    ...(params as unknown as GxserverCreateSessionParams),
    projectId: project.projectId,
  };
}

function readRuntimeText(value: unknown, key: string): string | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const candidate = value[key];
  return typeof candidate === "string" && candidate.trim() ? candidate.trim() : undefined;
}

function readAgentMetadataSettledTitle(session: GxserverSessionDomainState): string | undefined {
  /*
  CDXC:SessionStatus 2026-06-12-04:06:
  Only agent-metadata titles can act as settled-title evidence for clearing missed Codex Stop hooks. User, placeholder, and generic terminal titles are useful display text, but they are not enough proof to downgrade explicit hook-owned working after arbitrary title observations.
  */
  if (readRuntimeText(session.runtimeSettings, "titleMetadataSource") !== "agent-metadata") {
    return undefined;
  }
  return getVisibleTerminalTitle(session.title)?.trim();
}

function readOptionalText(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function readRequiredText(value: unknown, field: string): string {
  const text = readOptionalText(value);
  if (!text) {
    throw new GxserverDomainStateError("badRequest", `${field} is required.`);
  }
  return text;
}

function normalizeCreateAgentSessionParams(
  repository: GxserverDomainRepository,
  params: Record<string, unknown>,
  agentSettings: GxserverAgentSettings,
): GxserverCreateSessionDomainParams {
  const project = resolveCreateSessionProject(repository, params);
  return createAgentSessionParams(project, {
    ...(params as unknown as GxserverCreateSessionParams),
    kind: "agent",
    projectId: project.projectId,
  }, agentSettings) as GxserverCreateSessionDomainParams;
}

function resolveCreateSessionProject(
  repository: GxserverDomainRepository,
  params: Record<string, unknown>,
): GxserverProjectDomainState {
  const projectId = typeof params.projectId === "string" ? params.projectId.trim() : "";
  if (isGxserverProjectId(projectId)) {
    const project = repository.getProject(projectId);
    if (project) {
      return project;
    }
  }

  /*
  CDXC:GxserverProjectIdentity 2026-05-31-17:47:
  Session creation is a shared API used by macOS, CLI/TUI, mobile, and future desktop clients. Resolve `projectPath` or `cwd` to the canonical gxserver project before creating sessions so clients do not need to mint daemon-facing IDs, and so stale legacy `project-*` sidebar IDs cannot create orphaned or missing-project requests.
  */
  const projectPath = params.projectPath ?? params.cwd;
  if (typeof projectPath === "string" && projectPath.trim()) {
    const normalizedPath = normalizeExistingDirectoryPath(projectPath, params.projectPath ? "projectPath" : "cwd");
    const existingProject = findProjectByPath(repository.listProjects(), normalizedPath);
    if (existingProject) {
      return existingProject;
    }
    return repository.createProject({
      name:
        typeof params.projectName === "string" && params.projectName.trim()
          ? params.projectName.trim()
          : normalizedPath.split(/[\\/]/).filter(Boolean).at(-1) ?? normalizedPath,
      path: normalizedPath,
    });
  }

  if (projectId && !isGxserverProjectId(projectId)) {
    throw new GxserverDomainStateError("badRequest", `Invalid gxserver project ID: ${projectId}.`);
  }
  if (projectId) {
    throw new GxserverDomainStateError("notFound", `Project ${projectId} does not exist.`);
  }
  throw new GxserverDomainStateError("badRequest", "createSession requires projectId, projectPath, or cwd.");
}

function findProjectByPath(
  projects: readonly GxserverProjectDomainState[],
  normalizedPath: string | undefined,
): GxserverProjectDomainState | undefined {
  if (!normalizedPath) {
    return undefined;
  }
  return projects.find((project) => project.path === normalizedPath);
}

function readOperationRpcParams(body: unknown): Record<string, unknown> {
  if (!isRecord(body)) {
    throw new GxserverTypedOperationError("badRequest", "RPC request body must be an object.");
  }
  if (body.params === undefined) {
    return {};
  }
  if (!isRecord(body.params)) {
    throw new GxserverTypedOperationError("badRequest", "RPC params must be an object.");
  }
  return body.params;
}

function readProjectDirectoryBrowseParams(body: unknown): GxserverProjectDirectoryBrowseParams {
  if (!isRecord(body)) {
    throw new GxserverProjectPathError("badRequest", "RPC request body must be an object.");
  }
  if (!isRecord(body.params)) {
    throw new GxserverProjectPathError("badRequest", "RPC params must be an object.");
  }
  return {
    ...(typeof body.params.cwd === "string" ? { cwd: body.params.cwd } : {}),
    ...(body.params.limit !== undefined ? { limit: body.params.limit as number } : {}),
    partialPath: body.params.partialPath as string,
  };
}

async function handleBrowseProjectDirectoriesEndpoint(body: unknown) {
  return await browseProjectDirectories(readProjectDirectoryBrowseParams(body));
}

function readRepositoryCloneRpcParams(body: unknown): Record<string, unknown> {
  if (!isRecord(body)) {
    throw new GxserverRepositoryCloneError("badRequest", "RPC request body must be an object.");
  }
  if (body.params === undefined) {
    return {};
  }
  if (!isRecord(body.params)) {
    throw new GxserverRepositoryCloneError("badRequest", "RPC params must be an object.");
  }
  return body.params;
}

function statusForOperationError(code: GxserverTypedOperationError["code"] | GxserverProjectPathError["code"]): number {
  switch (code) {
    case "dependencyUnavailable":
      return 503;
    case "forbidden":
      return 403;
    case "notFound":
      return 404;
    default:
      return 400;
  }
}

function statusForRepositoryCloneError(code: GxserverRepositoryCloneError["code"]): number {
  switch (code) {
    case "dependencyUnavailable":
      return 503;
    case "forbidden":
      return 403;
    case "notFound":
      return 404;
    default:
      return 400;
  }
}

function isRendererCommandDispatchError(
  error: unknown,
): error is GxserverRendererCommandUnavailableError | GxserverRendererCommandTimeoutError {
  return error instanceof GxserverRendererCommandUnavailableError || error instanceof GxserverRendererCommandTimeoutError;
}

function statusForDomainStateError(code: GxserverDomainStateError["code"]): number {
  switch (code) {
    case "corruptState":
      return 409;
    case "notFound":
      return 404;
    default:
      return 400;
  }
}

function readDomainRpcParams(body: unknown): Record<string, unknown> {
  if (!isRecord(body)) {
    throw new GxserverDomainStateError("badRequest", "RPC request body must be an object.");
  }
  if (body.params === undefined) {
    return {};
  }
  if (!isRecord(body.params)) {
    throw new GxserverDomainStateError("badRequest", "RPC params must be an object.");
  }
  return body.params;
}

function readProjectId(params: Record<string, unknown>): GxserverProjectId {
  if (!isGxserverProjectId(params.projectId)) {
    throw new GxserverDomainStateError("badRequest", `Invalid gxserver project ID: ${String(params.projectId)}.`);
  }
  return params.projectId;
}

function readOptionalProjectId(params: Record<string, unknown>): GxserverProjectId | undefined {
  if (params.projectId === undefined || params.projectId === null || params.projectId === "") {
    return undefined;
  }
  return readProjectId(params);
}

function readSessionLifecycleParams(params: Record<string, unknown>): GxserverSessionLifecycleParams {
  const projectId = readProjectId(params);
  if (!isGxserverSessionId(params.sessionId)) {
    throw new GxserverDomainStateError("badRequest", `Invalid gxserver session ID: ${String(params.sessionId)}.`);
  }
  return {
    projectId,
    reason: typeof params.reason === "string" ? params.reason : undefined,
    sessionId: params.sessionId,
  };
}

interface HandleUpgradeOptions {
  head: Buffer;
  listenerKind: GxserverListenerKind;
  request: http.IncomingMessage;
  runtime: GxserverApiRuntime;
  socket: Duplex;
}

async function handleUpgrade(options: HandleUpgradeOptions): Promise<void> {
  const { head, listenerKind, request, runtime, socket } = options;
  const requestId = getRequestId(request);
  const url = new URL(request.url ?? "/", `http://${GXSERVER_LOCAL_API_HOST}:${GXSERVER_LOCAL_API_PORT}`);
  const endpoint = getGxserverEndpoint(url.pathname);

  const reject = (statusCode: number, body: unknown): void => {
    const payload = `${JSON.stringify(body)}\n`;
    socket.write(
      `HTTP/1.1 ${statusCode} ${http.STATUS_CODES[statusCode] ?? "Rejected"}\r\n` +
        "content-type: application/json; charset=utf-8\r\n" +
        `content-length: ${Buffer.byteLength(payload)}\r\n` +
        "connection: close\r\n\r\n" +
        payload,
    );
    socket.destroy();
  };

  if (!endpoint || endpoint.transport !== "webSocket") {
    reject(404, createRpcError("notFound", `No gxserver WebSocket endpoint for ${url.pathname}.`, requestId));
    return;
  }
  /*
  CDXC:GxserverPresentationEvents 2026-06-01-15:08:
  Browser WebSocket constructors cannot set Authorization headers. Allow the same bearer token as an `authToken` query value on the authenticated event-stream upgrade so native WebKit clients can subscribe to presentation deltas without adding a polling fallback.
  */
  const isAuthorizedUpgrade =
    isAuthorizedGxserverRequest(request, runtime.authToken) ||
    isExpectedGxserverAuthToken(url.searchParams.get("authToken") ?? "", runtime.authToken);
  if (!isAuthorizedUpgrade) {
    reject(401, createRpcError("unauthorized", "gxserver auth token is required for this endpoint.", requestId));
    return;
  }
  const protocolVersion = readProtocolVersion(request, url);
  if (!isExpectedProtocolVersion(protocolVersion)) {
    reject(426, createProtocolMismatchError(protocolVersion, requestId));
    return;
  }
  if (!isRemoteEndpointAllowed(listenerKind, endpoint.permission)) {
    reject(403, createRpcError("forbidden", `${endpoint.path} is not available on the remote gxserver listener.`, requestId));
    return;
  }

  runtime.eventHub.server.handleUpgrade(request, socket, head, (webSocket) => {
    runtime.eventHub.server.emit("connection", webSocket, request);
  });
}

function createMinimalHealth(version: string): GxserverMinimalHealthResponse {
  return {
    ok: true,
    product: GXSERVER_PRODUCT,
    protocolVersion: GXSERVER_PROTOCOL_VERSION,
    version,
  };
}

async function createAuthenticatedHealth(runtime: GxserverApiRuntime): Promise<Record<string, unknown>> {
  return {
    ...createMinimalHealth(runtime.version),
    buildIdentity: runtime.buildIdentity,
    capabilities: GXSERVER_CONTROL_PLANE_CAPABILITIES,
    listeners: runtime.config.listeners,
    migration: runtime.migration,
    pid: runtime.metadata.pid,
    port: runtime.metadata.port,
    serverId: runtime.metadata.serverId,
    startedAt: runtime.metadata.startedAt,
    tools: await getGxserverToolStatuses(),
  };
}

async function readJsonBody(request: http.IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];
  const contentLength = readContentLength(request);
  if (contentLength !== undefined && contentLength > GXSERVER_JSON_BODY_LIMIT_BYTES) {
    throw new GxserverRequestBodyTooLargeError(contentLength);
  }
  let receivedBytes = 0;
  for await (const chunk of request) {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    receivedBytes += buffer.byteLength;
    if (receivedBytes > GXSERVER_JSON_BODY_LIMIT_BYTES) {
      throw new GxserverRequestBodyTooLargeError(receivedBytes);
    }
    chunks.push(buffer);
  }
  if (chunks.length === 0) {
    return {};
  }
  const text = Buffer.concat(chunks).toString("utf8").trim();
  if (!text) {
    return {};
  }
  return JSON.parse(text);
}

function readContentLength(request: http.IncomingMessage): number | undefined {
  const header = request.headers["content-length"];
  if (typeof header !== "string" || !/^\d+$/.test(header)) {
    return undefined;
  }
  return Number(header);
}

class GxserverRequestBodyTooLargeError extends Error {
  constructor(readonly receivedBytes: number) {
    super(`Request body exceeds ${GXSERVER_JSON_BODY_LIMIT_BYTES} bytes: ${receivedBytes}`);
  }
}

function sendJson(response: http.ServerResponse, statusCode: number, body: unknown): void {
  response.writeHead(statusCode, {
    "content-type": "application/json; charset=utf-8",
  });
  response.end(`${JSON.stringify(body)}\n`);
}

function applyCorsHeaders(
  request: http.IncomingMessage,
  response: http.ServerResponse,
  config: GxserverConfig,
): void {
  const origin = readSingleHeader(request.headers.origin);
  if (!origin) {
    return;
  }
  appendVaryHeader(response, "Origin");
  appendVaryHeader(response, "Access-Control-Request-Private-Network");
  if (!isAllowedCorsOrigin(origin, config)) {
    return;
  }
  response.setHeader("access-control-allow-origin", origin);
  response.setHeader(
    "access-control-allow-headers",
    "authorization, content-type, x-gxserver-protocol-version, x-request-id",
  );
  response.setHeader("access-control-allow-methods", "GET, POST, OPTIONS");
  if (readSingleHeader(request.headers["access-control-request-private-network"]) === "true") {
    response.setHeader("access-control-allow-private-network", "true");
  }
  response.setHeader("access-control-max-age", "600");
}

function isAllowedCorsOrigin(origin: string, config: GxserverConfig): boolean {
  return config.cors.allowedOrigins.includes(origin);
}

function readSingleHeader(value: string | string[] | undefined): string | undefined {
  return Array.isArray(value) ? undefined : value;
}

function appendVaryHeader(response: http.ServerResponse, value: string): void {
  const existing = response.getHeader("vary");
  const values = new Set(
    (Array.isArray(existing) ? existing.join(",") : String(existing ?? ""))
      .split(",")
      .map((part) => part.trim())
      .filter(Boolean),
  );
  values.add(value);
  response.setHeader("vary", [...values].join(", "));
}

function readRpcParams(body: unknown): Record<string, unknown> {
  if (!isRecord(body)) {
    throw new GxserverLogQueryInputError("RPC request body must be an object.");
  }
  if (body.params === undefined) {
    return {};
  }
  if (!isRecord(body.params)) {
    throw new GxserverLogQueryInputError("RPC params must be an object.");
  }
  return body.params;
}

function getRequestId(request: http.IncomingMessage): string {
  const header = request.headers["x-request-id"];
  if (typeof header === "string" && header.trim()) {
    return header;
  }
  return randomUUID();
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function listen(server: http.Server, config: GxserverListenerConfig): Promise<void> {
  return new Promise((resolve, reject) => {
    const cleanup = (): void => {
      server.off("error", onError);
      server.off("listening", onListening);
    };
    const onError = (error: Error): void => {
      cleanup();
      if (isAddressInUseError(error)) {
        reject(
          new Error(
            `Port ${config.port} is already in use and did not respond as a compatible gxserver. Stop the conflicting process or update Ghostex/gxserver so their protocol versions match.`,
          ),
        );
        return;
      }
      reject(error);
    };
    const onListening = (): void => {
      cleanup();
      resolve();
    };
    server.once("error", onError);
    server.once("listening", onListening);
    server.listen(config.port, config.host);
  });
}

function closeServer(server: http.Server): Promise<void> {
  return new Promise((resolve, reject) => {
    server.close((error) => {
      if (error) {
        reject(error);
        return;
      }
      resolve();
    });
  });
}

function isAddressInUseError(error: unknown): boolean {
  return typeof error === "object" && error !== null && "code" in error && error.code === "EADDRINUSE";
}
