import {
  GXSERVER_PRODUCT,
  GXSERVER_PROTOCOL_VERSION,
  type GxserverEndpointPath,
  type GxserverAgentSettings,
  type GxserverReadAgentSettingsResult,
  type GxserverInstallAgentHooksResult,
  type GxserverAgentLaunchPlan,
  type GxserverAgentLaunchPlanParams,
  type GxserverAgentResumePlan,
  type GxserverAgentResumePlanParams,
  type GxserverAttachSessionMetadataParams,
  type GxserverAttachSessionMetadataResult,
  type GxserverCreateSessionParams,
  type GxserverDeleteWorktreeProjectParams,
  type GxserverDeleteWorktreeProjectResult,
  type GxserverEvent,
  type GxserverForkSessionParams,
  type GxserverForkSessionResult,
  type GxserverPresentationDelta,
  type GxserverPresentationSearchParams,
  type GxserverPresentationSearchResponse,
  type GxserverPresentationSnapshot,
  type GxserverReadAgentHookStatusResult,
  type GxserverResolveGitRootForPathParams,
  type GxserverResolveGitRootForPathResult,
  type GxserverRunBeadsActionParams,
  type GxserverRunGitActionParams,
  type GxserverRunGitHubActionParams,
  type GxserverRunWorktreeActionParams,
  type GxserverSessionProviderProbeResponse,
  type GxserverProjectDomainState,
  type GxserverRemoveSessionParams,
  type GxserverRendererCommand,
  type GxserverRpcErrorResponse,
  type GxserverRpcSuccessResponse,
  type GxserverServerHealthResponse,
  type GxserverSessionDomainState,
  type GxserverSessionTransitionParams,
  type GxserverSessionTransitionResult,
  type GxserverStartSessionProviderParams,
  type GxserverStartSessionProviderResult,
  type GxserverTypedOperationResult,
} from "../../shared/gxserver-protocol";

export type NativeSidebarGxserverBootstrap = {
  authToken?: string;
  baseUrl?: string;
  protocolVersion?: number;
  tokenFile?: string;
};

export type NativeSidebarGxserverStatus = NativeSidebarGxserverBootstrap & {
  alwaysStart?: boolean;
  health?: GxserverServerHealthResponse;
  message?: string;
  nodeModuleVersion?: string;
  nodePath?: string;
  nodeVersion?: string;
  ok?: boolean;
  expectedNodeMajor?: number;
  expectedNodeModuleVersion?: string;
  state?: string;
};

export type NativeSidebarGxserverStartupSnapshot = {
  agentSettings: GxserverAgentSettings;
  agentSettingsIsPersisted: boolean;
  health: GxserverServerHealthResponse;
  presentation?: GxserverPresentationSnapshot;
  projects: GxserverProjectDomainState[];
};

export type NativeGxserverHttpMethod = "GET" | "POST";

export type NativeGxserverRequestCommand = {
  method: NativeGxserverHttpMethod;
  paramsJson?: string;
  path: GxserverEndpointPath;
  requestId: string;
  type: "gxserverRequest";
};

export type NativeGxserverResponseEvent = {
  bodyJson?: string;
  error?: string;
  ok: boolean;
  path: GxserverEndpointPath;
  requestId: string;
  statusCode?: number;
  type: "gxserverResponse";
};

export type NativeGxserverRequestOptions = {
  method?: NativeGxserverHttpMethod;
  params?: Record<string, unknown>;
  requestId?: string;
};

type GxserverRequestContext = {
  method: NativeGxserverHttpMethod;
  path: GxserverEndpointPath;
};

export type NativeSidebarPresentationSubscription = {
  close: () => void;
};

export type NativeSidebarPresentationSubscriptionHandlers = {
  onClose?: (event: CloseEvent) => void;
  onDelta?: (delta: GxserverPresentationDelta, revision: number) => void;
  onError?: (error: Event) => void;
  onRendererCommand?: (
    command: GxserverRendererCommand,
  ) => Promise<Record<string, unknown> | void> | Record<string, unknown> | void;
  onSnapshot?: (snapshot: GxserverPresentationSnapshot) => void;
};

export class NativeGxserverClientError extends Error {
  readonly response: NativeGxserverResponseEvent;

  constructor(response: NativeGxserverResponseEvent) {
    const message =
      parseGxserverErrorMessage(response.bodyJson) ??
      createNativeGxserverClientErrorMessage(response);
    super(message);
    this.name = "NativeGxserverClientError";
    this.response = response;
  }
}

const DEFAULT_BASE_URL = "http://127.0.0.1:58744";
const NETWORK_RETRY_DELAYS_MS = [120, 300, 700] as const;

/*
CDXC:GxserverSidebarClient 2026-05-30-15:39:
The native React sidebar is no longer allowed to invent a second backend transport for shared project/session/agent/zmx/Git/log state. Keep gxserver HTTP auth, protocol headers, RPC envelope creation, and response validation in this wrapper so UI code consumes one hard-cutover client instead of mixing direct daemon ownership with compatibility paths.
*/
export function createNativeSidebarGxserverClient(
  bootstrap: NativeSidebarGxserverStatus | undefined,
) {
  let config: Required<Pick<NativeSidebarGxserverBootstrap, "baseUrl" | "protocolVersion">> &
    Omit<NativeSidebarGxserverBootstrap, "baseUrl" | "protocolVersion"> = {
    authToken: bootstrap?.authToken,
    baseUrl: bootstrap?.baseUrl || DEFAULT_BASE_URL,
    protocolVersion: bootstrap?.protocolVersion ?? GXSERVER_PROTOCOL_VERSION,
    tokenFile: bootstrap?.tokenFile,
  };
  let currentStatus: NativeSidebarGxserverStatus = {
    ...bootstrap,
    ...config,
    /*
    CDXC:GxserverBootstrap 2026-06-07-12:02:
    Native injects the first daemon status into bootstrap so startup gating does not depend on receiving a separate host event before the WebKit listener is ready. Preserve those status fields instead of resetting the sidebar client to unknown.
    */
    alwaysStart: bootstrap?.alwaysStart ?? true,
    state: bootstrap?.state ?? "unknown",
  };

  function applyNativeStatus(payloadJson: string): NativeSidebarGxserverStatus | undefined {
    const parsed = parseObject(payloadJson) as NativeSidebarGxserverStatus | undefined;
    if (!parsed) {
      return undefined;
    }
    config = {
      ...config,
      authToken: parsed.authToken ?? config.authToken,
      baseUrl: parsed.baseUrl || config.baseUrl,
      protocolVersion: parsed.protocolVersion ?? config.protocolVersion,
      tokenFile: parsed.tokenFile ?? config.tokenFile,
    };
    currentStatus = {
      ...currentStatus,
      ...parsed,
      ...config,
    };
    return parsed;
  }

  function getCurrentStatus(): NativeSidebarGxserverStatus {
    return currentStatus;
  }

  async function fetchHealth(): Promise<GxserverServerHealthResponse> {
    const requestContext: GxserverRequestContext = { method: "GET", path: "/api/health/server" };
    const response = await fetchWithRetry(`${config.baseUrl}/api/health/server`, {
      headers: createHeaders(),
      method: "GET",
    }, requestContext);
    const body = await readJson(response);
    if (!response.ok) {
      throw createGxserverError(body, response.status, requestContext);
    }
    return parseHealth(body);
  }

  async function rpc<TResult>(
    path: GxserverEndpointPath,
    params: Record<string, unknown> = {},
  ): Promise<TResult> {
    const requestContext: GxserverRequestContext = { method: "POST", path };
    const response = await fetchWithRetry(`${config.baseUrl}${path}`, {
      body: JSON.stringify({
        params,
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      }),
      headers: {
        ...createHeaders(),
        "content-type": "application/json",
      },
      method: "POST",
    }, requestContext);
    const body = await readJson(response);
    if (!response.ok || !isRpcSuccess(body)) {
      throw createGxserverError(body, response.status, requestContext);
    }
    return parseRpcResponse<TResult>(body, response.status, requestContext);
  }

  function rpcSync<TResult>(
    path: GxserverEndpointPath,
    params: Record<string, unknown> = {},
  ): TResult {
    const requestContext: GxserverRequestContext = { method: "POST", path };
    const Xhr = globalThis.XMLHttpRequest;
    if (typeof Xhr !== "function") {
      throw new Error("gxserver synchronous RPC requires XMLHttpRequest in the native sidebar runtime.");
    }
    const xhr = new Xhr();
    try {
      xhr.open("POST", `${config.baseUrl}${path}`, false);
      const headers = {
        ...createHeaders(),
        "content-type": "application/json",
      };
      for (const [name, value] of Object.entries(headers)) {
        xhr.setRequestHeader(name, value);
      }
      xhr.send(
        JSON.stringify({
          params,
          protocolVersion: GXSERVER_PROTOCOL_VERSION,
        }),
      );
    } catch (error) {
      throw createGxserverTransportError(requestContext, error);
    }
    const body = xhr.responseText.trim() ? JSON.parse(xhr.responseText) as unknown : undefined;
    return parseRpcResponse<TResult>(body, xhr.status, requestContext);
  }

  async function fetchStartupSnapshot(): Promise<NativeSidebarGxserverStartupSnapshot> {
    const health = await fetchHealth();
    const [agentSettingsResult, { projects }, { snapshot }] = await Promise.all([
      rpc<GxserverReadAgentSettingsResult>("/api/readAgentSettings"),
      rpc<{ projects: GxserverProjectDomainState[] }>("/api/listProjects"),
      rpc<{ snapshot: GxserverPresentationSnapshot }>("/api/readPresentationSnapshot"),
    ]);
    /*
    CDXC:GxserverPresentation 2026-06-01-15:08:
    Startup must no longer hydrate all gxserver session history into the macOS sidebar. The startup snapshot carries projects plus the bounded active-focused presentation snapshot only; raw session inventory stays behind gxserver APIs.
    */
    return {
      agentSettings: agentSettingsResult.settings,
      agentSettingsIsPersisted: agentSettingsResult.isPersisted,
      health,
      presentation: snapshot,
      projects,
    };
  }

  async function updateAgentSettings(
    settings: Partial<GxserverAgentSettings>,
  ): Promise<GxserverAgentSettings> {
    /*
    CDXC:GxserverAgentSettings 2026-06-02-22:23:
    The macOS settings UI edits global agent policy through gxserver. The sidebar may keep a local render cache, but inherited Accept All command behavior must come from the daemon API used by every client.
    */
    const result = await rpc<{ settings: GxserverAgentSettings }>(
      "/api/updateAgentSettings",
      settings as Record<string, unknown>,
    );
    return result.settings;
  }

  async function readAgentHookStatus(agentIds?: readonly string[]): Promise<GxserverReadAgentHookStatusResult> {
    /*
    CDXC:AgentHooks 2026-06-07-08:51:
    Hook status is gxserver-owned for every supported agent. Native clients ask
    for the shared daemon status instead of inspecting Codex/Claude/Pi/OpenCode
    files or merging provider-specific rows locally.
    */
    return rpc<GxserverReadAgentHookStatusResult>(
      "/api/readAgentHookStatus",
      agentIds ? { agentIds } : {},
    );
  }

  async function installAgentHooks(agentIds?: readonly string[]): Promise<GxserverInstallAgentHooksResult> {
    /*
    CDXC:AgentHooks 2026-06-07-08:51:
    Settings and first-launch remain the only user-facing install triggers, but
    gxserver writes every supported hook so macOS, TUI, CLI, mobile, and future
    desktop clients do not carry duplicate installation logic.
    */
    return rpc<GxserverInstallAgentHooksResult>(
      "/api/installAgentHooks",
      agentIds ? { agentIds } : {},
    );
  }

  async function fetchPresentationSnapshot(): Promise<GxserverPresentationSnapshot> {
    /*
    CDXC:GxserverPresentation 2026-06-01-15:08:
    Native sidebar startup is moving to gxserver's active-focused presentation feed. Keep a dedicated client method for the hard cutover path so UI code can consume snapshot/delta rows without calling raw listSessions or hydrating all previous sessions.
    */
    const { snapshot } = await rpc<{ snapshot: GxserverPresentationSnapshot }>("/api/readPresentationSnapshot");
    return snapshot;
  }

  async function searchSessions(
    params: GxserverPresentationSearchParams,
  ): Promise<GxserverPresentationSearchResponse> {
    return rpc<GxserverPresentationSearchResponse>("/api/searchSessions", params as unknown as Record<string, unknown>);
  }

  async function listPreviousSessions(
    params: GxserverPresentationSearchParams = {},
  ): Promise<GxserverPresentationSearchResponse> {
    return rpc<GxserverPresentationSearchResponse>("/api/listPreviousSessions", params as unknown as Record<string, unknown>);
  }

  async function removeSession(params: GxserverRemoveSessionParams): Promise<GxserverSessionDomainState> {
    const { session } = await rpc<{ session: GxserverSessionDomainState }>(
      "/api/removeSession",
      params as unknown as Record<string, unknown>,
    );
    return session;
  }

  async function runGitAction(params: GxserverRunGitActionParams): Promise<GxserverTypedOperationResult> {
    return rpc<GxserverTypedOperationResult>("/api/runGitAction", params as unknown as Record<string, unknown>);
  }

  async function runGitHubAction(params: GxserverRunGitHubActionParams): Promise<GxserverTypedOperationResult> {
    return rpc<GxserverTypedOperationResult>("/api/runGitHubAction", params as unknown as Record<string, unknown>);
  }

  async function runBeadsAction(params: GxserverRunBeadsActionParams): Promise<GxserverTypedOperationResult> {
    return rpc<GxserverTypedOperationResult>("/api/runBeadsAction", params as unknown as Record<string, unknown>);
  }

  async function runWorktreeAction(params: GxserverRunWorktreeActionParams): Promise<GxserverTypedOperationResult> {
    return rpc<GxserverTypedOperationResult>("/api/runWorktreeAction", params as unknown as Record<string, unknown>);
  }

  async function resolveGitRootForPath(
    params: GxserverResolveGitRootForPathParams,
  ): Promise<GxserverResolveGitRootForPathResult> {
    /*
    CDXC:OSIntegration 2026-06-02-12:14:
    Native open-file/open-folder routing stays local UI behavior, but repository root detection is gxserver-owned after the split. This endpoint is intentionally local-only because it accepts arbitrary paths that may not be registered projects yet.
    */
    return rpc<GxserverResolveGitRootForPathResult>(
      "/api/resolveGitRootForPath",
      params as unknown as Record<string, unknown>,
    );
  }

  function subscribePresentation(
    clientId: string,
    handlers: NativeSidebarPresentationSubscriptionHandlers,
    lastRevision?: number,
  ): NativeSidebarPresentationSubscription {
    /*
    CDXC:GxserverPresentationEvents 2026-06-01-15:08:
    The native sidebar consumes gxserver presentation as snapshot plus WebSocket deltas. WebKit cannot attach bearer headers to WebSocket, so the server accepts the same token in the event-stream query string and this client sends the subscription message immediately after open.

    CDXC:GxserverRendererCommands 2026-06-13-02:24:
    Renderer-only CLI actions share this authenticated gxserver event stream. The sidebar opts in explicitly and returns one structured result per command so gxserver stays the command owner while macOS executes only native UI/sidebar effects.
    */
    const url = new URL(`${config.baseUrl}/api/events`);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    url.searchParams.set("protocolVersion", String(GXSERVER_PROTOCOL_VERSION));
    url.searchParams.set("authToken", config.authToken ?? "");
    const socket = new WebSocket(url.toString());
    let closedByClient = false;
    socket.addEventListener("open", () => {
      socket.send(JSON.stringify({
        clientId,
        ...(lastRevision !== undefined ? { lastRevision } : {}),
        ...(handlers.onRendererCommand ? { rendererCommands: true } : {}),
        type: "subscribePresentation",
      }));
    });
    socket.addEventListener("message", (event) => {
      const parsed = parseGxserverEvent(event.data);
      if (!parsed) {
        return;
      }
      if (parsed.type === "presentationSnapshot") {
        handlers.onSnapshot?.(parsed.snapshot);
      } else if (parsed.type === "presentationDelta") {
        handlers.onDelta?.(parsed.delta, parsed.revision);
      } else if (parsed.type === "rendererCommand" && handlers.onRendererCommand) {
        void handleRendererCommand(socket, parsed.command, handlers.onRendererCommand);
      }
    });
    socket.addEventListener("error", (event) => {
      handlers.onError?.(event);
    });
    socket.addEventListener("close", (event) => {
      /*
      CDXC:GxserverPresentationEvents 2026-06-03-19:56:
      All gxserver presentation clients must treat unexpected WebSocket closure as a lost delta cursor. Surface close separately from deliberate teardown so UI adapters can refresh the authoritative snapshot before subscribing again instead of rendering stale session titles indefinitely.
      */
      if (!closedByClient) {
        handlers.onClose?.(event);
      }
    });
    return {
      close: () => {
        closedByClient = true;
        socket.close();
      },
    };
  }

  async function fetchAttachSessionMetadata(
    params: GxserverAttachSessionMetadataParams,
  ): Promise<GxserverAttachSessionMetadataResult> {
    /*
    CDXC:GxserverTerminalAttach 2026-05-30-15:50:
    Native macOS terminal panes are renderers in the hard cutover. Fetch zmx attach metadata through gxserver so provider existence, missing-cwd restore blocks, and startup-text replay/discard decisions stay server-owned before React asks Swift to render a Ghostty command.
    */
    const { attach } = await rpc<{ attach: GxserverAttachSessionMetadataResult }>(
      "/api/attachSessionMetadata",
      params as unknown as Record<string, unknown>,
    );
    return attach;
  }

  async function fetchWakeSessionMetadata(
    params: GxserverAttachSessionMetadataParams,
  ): Promise<GxserverAttachSessionMetadataResult> {
    /*
    CDXC:GxserverTerminalWake 2026-06-01-12:07:
    Sleeping-session selection is a wake intent, not a plain attach. Call gxserver's wake endpoint so the daemon marks the session running and returns the server-built resume startup text for newly recreated zmx sessions.
    */
    const { attach } = await rpc<{ attach: GxserverAttachSessionMetadataResult }>(
      "/api/wakeSession",
      params as unknown as Record<string, unknown>,
    );
    return attach;
  }

  async function startSessionProvider(
    params: GxserverStartSessionProviderParams,
  ): Promise<GxserverStartSessionProviderResult> {
    /*
    CDXC:GxserverTerminalRestore 2026-06-08-20:49:
    Missing zmx providers must run gxserver-approved startup text through `/api/startSessionProvider` before native creates the Ghostty attach surface. This keeps restore/resume commands out of post-ready terminal input while preserving the daemon-owned startup decision.
    */
    return rpc<GxserverStartSessionProviderResult>(
      "/api/startSessionProvider",
      params as unknown as Record<string, unknown>,
    );
  }

  function fetchAgentLaunchPlanSync(
    params: GxserverAgentLaunchPlanParams,
  ): GxserverAgentLaunchPlan {
    /*
    CDXC:GxserverAgentCommands 2026-06-01-12:23:
    Agent launch commands are gxserver policy, including Accept All flag insertion or runtime permission config. Native callers that need a command synchronously ask gxserver for the launch plan instead of reconstructing per-agent command rules in React.

    CDXC:GxserverAgentCommands 2026-06-09-14:22:
    OpenCode Accept All uses OPENCODE_CONFIG_CONTENT instead of a CLI flag, so macOS must consume gxserver launch plans verbatim and never append OpenCode permission arguments locally.
    */
    const { plan } = rpcSync<{ plan: GxserverAgentLaunchPlan }>(
      "/api/readAgentLaunchPlan",
      params as unknown as Record<string, unknown>,
    );
    return plan;
  }

  async function fetchAgentResumePlan(
    params: GxserverAgentResumePlanParams,
  ): Promise<GxserverAgentResumePlan> {
    /*
    CDXC:GxserverAgentCommands 2026-06-01-12:23:
    Copy/restore/resume commands come from gxserver so OpenCode can use a base lookup command while the actual launched command carries runtime Accept All config.
    */
    const { plan } = await rpc<{ plan: GxserverAgentResumePlan }>(
      "/api/readAgentResumePlan",
      params as unknown as Record<string, unknown>,
    );
    return plan;
  }

  function fetchAgentResumePlanSync(
    params: GxserverAgentResumePlanParams,
  ): GxserverAgentResumePlan {
    const { plan } = rpcSync<{ plan: GxserverAgentResumePlan }>(
      "/api/readAgentResumePlan",
      params as unknown as Record<string, unknown>,
    );
    return plan;
  }

  function createTerminalSessionSync(
    params: GxserverCreateSessionParams,
  ): GxserverSessionDomainState {
    /*
    CDXC:GxserverSessionIdentity 2026-05-30-18:20:
    The existing macOS creation pipeline is synchronous: callers immediately need the new session ID to place panes, focus tabs, update native mappings, and return CLI summaries. For the gxserver hard cutover, block briefly on the local authenticated daemon createSession RPC so gxserver still generates the canonical G ID before the sidebar mutates client-owned layout state.
    */
    const { session } = rpcSync<{ session: GxserverSessionDomainState }>(
      "/api/createSession",
      params as unknown as Record<string, unknown>,
    );
    return session;
  }

  async function forkSession(
    params: GxserverForkSessionParams,
  ): Promise<GxserverForkSessionResult> {
    /*
    CDXC:GxserverForkSession 2026-06-04-07:42:
    Native sidebar Fork delegates session creation and provider command construction to gxserver. The macOS app remains responsible only for placing the returned session in the clicked tab group.
    */
    const { fork } = await rpc<{ fork: GxserverForkSessionResult }>(
      "/api/forkSession",
      params as unknown as Record<string, unknown>,
    );
    return fork;
  }

  function addProjectPathSync(params: { name?: string; path: string }): GxserverProjectDomainState {
    /*
    CDXC:GxserverProjectIdentity 2026-05-31-17:47:
    Project rows shown in the native sidebar must be registered through gxserver before any shared terminal/session is created. The daemon returns the canonical P-id, keeping macOS aligned with CLI/TUI/mobile clients instead of persisting sidebar-minted `project-*` ids into shared session calls.
    */
    const { project } = rpcSync<{ project: GxserverProjectDomainState }>(
      "/api/addProjectPath",
      params as unknown as Record<string, unknown>,
    );
    return project;
  }

  async function addProjectPath(params: { name?: string; path: string }): Promise<GxserverProjectDomainState> {
    const { project } = await rpc<{ project: GxserverProjectDomainState }>(
      "/api/addProjectPath",
      params as unknown as Record<string, unknown>,
    );
    return project;
  }

  async function removeProject(projectId: string): Promise<GxserverProjectDomainState> {
    const { project } = await rpc<{ project: GxserverProjectDomainState }>("/api/removeProject", {
      projectId,
    });
    return project;
  }

  async function deleteWorktreeProject(
    params: GxserverDeleteWorktreeProjectParams,
  ): Promise<GxserverDeleteWorktreeProjectResult> {
    return rpc<GxserverDeleteWorktreeProjectResult>(
      "/api/deleteWorktreeProject",
      params as unknown as Record<string, unknown>,
    );
  }

  async function probeSessionProvider(
    params: Pick<GxserverAttachSessionMetadataParams, "projectId" | "sessionId">,
  ): Promise<GxserverSessionProviderProbeResponse> {
    return rpc<GxserverSessionProviderProbeResponse>(
      "/api/probeSessionProvider",
      params as unknown as Record<string, unknown>,
    );
  }

  async function updateSessionLifecycle(
    path: "/api/killSession" | "/api/sleepSession",
    params: Pick<GxserverAttachSessionMetadataParams, "projectId" | "sessionId"> & { reason?: string },
  ): Promise<void> {
    await rpc(path, params as unknown as Record<string, unknown>);
  }

  function transitionSessionSync(
    params: GxserverSessionTransitionParams,
  ): GxserverSessionTransitionResult {
    return rpcSync<GxserverSessionTransitionResult>(
      "/api/transitionSession",
      params as unknown as Record<string, unknown>,
    );
  }

  function createHeaders(): Record<string, string> {
    if (!config.authToken) {
      throw new Error(
        `gxserver auth token is not available. Expected native bootstrap to read ${config.tokenFile ?? "~/.ghostex/gxserver/auth/token"}.`,
      );
    }
    return {
      authorization: `Bearer ${config.authToken}`,
      "x-gxserver-protocol-version": String(GXSERVER_PROTOCOL_VERSION),
    };
  }

  return {
    addProjectPath,
    addProjectPathSync,
    applyNativeStatus,
    createTerminalSessionSync,
    fetchAgentLaunchPlanSync,
    fetchAgentResumePlan,
    fetchAgentResumePlanSync,
    fetchAttachSessionMetadata,
    fetchHealth,
    deleteWorktreeProject,
    forkSession,
    installAgentHooks,
    fetchPresentationSnapshot,
    fetchStartupSnapshot,
    fetchWakeSessionMetadata,
    getCurrentStatus,
    probeSessionProvider,
    listPreviousSessions,
    removeProject,
    removeSession,
    resolveGitRootForPath,
    readAgentHookStatus,
    rpc,
    runBeadsAction,
    runGitAction,
    runGitHubAction,
    runWorktreeAction,
    searchSessions,
    startSessionProvider,
    subscribePresentation,
    transitionSessionSync,
    updateAgentSettings,
    updateSessionLifecycle,
  };
}

/*
CDXC:GxserverMacClient 2026-05-31-01:32:
During the main-worktree merge, preserve the native-bridge gxserver request path for sidebar code that cannot use direct fetch. The bridge still uses the same gxserver protocol envelope and response validation as the direct sidebar client, while Swift owns token-file access.
*/
export function createNativeGxserverRequest(
  path: GxserverEndpointPath,
  options: NativeGxserverRequestOptions = {},
): NativeGxserverRequestCommand {
  return {
    method: options.method ?? (path === "/api/health/server" || path === "/api/health" ? "GET" : "POST"),
    paramsJson: options.params ? JSON.stringify(options.params) : undefined,
    path,
    requestId: options.requestId ?? createGxserverRequestId(),
    type: "gxserverRequest",
  };
}

export function parseNativeGxserverResponse<TResult extends Record<string, unknown>>(
  response: NativeGxserverResponseEvent,
): GxserverRpcSuccessResponse<TResult> | Record<string, unknown> {
  if (!response.ok) {
    throw new NativeGxserverClientError(response);
  }
  if (!response.bodyJson) {
    return {};
  }
  return JSON.parse(response.bodyJson) as GxserverRpcSuccessResponse<TResult> | Record<string, unknown>;
}

function parseRpcResponse<TResult>(
  body: unknown,
  status: number,
  context: GxserverRequestContext,
): TResult {
  if (!isRpcSuccess(body)) {
    throw createGxserverError(body, status, context);
  }
  if (body.protocolVersion !== GXSERVER_PROTOCOL_VERSION) {
    throw new Error(
      `gxserver protocol mismatch. Expected protocol ${GXSERVER_PROTOCOL_VERSION}, got ${String(
        body.protocolVersion,
      )}. Update Ghostex and gxserver so their protocol versions match.`,
    );
  }
  return body.result as TResult;
}

async function readJson(response: Response): Promise<unknown> {
  const text = await response.text();
  if (!text.trim()) {
    return undefined;
  }
  return JSON.parse(text) as unknown;
}

async function fetchWithRetry(
  url: string,
  init: RequestInit,
  context: GxserverRequestContext,
): Promise<Response> {
  /*
  CDXC:GxserverSidebarClient 2026-05-30-18:04:
  The desktop app starts gxserver independently and WebKit may issue zmx attach/list requests while the daemon is still binding or completing CORS preflight. Retry transport-level `Load failed`/network errors briefly, but do not retry authenticated HTTP/RPC failures because those are real daemon decisions that should surface immediately.

  CDXC:GxserverSidebarClient 2026-06-08-19:24:
  User-facing gxserver transport toasts must describe the product action that failed, not WebKit's raw `Load failed`, loopback URLs, or internal API paths. Format retry exhaustion at the client boundary so attach, wake, project, Git, and remote bridge callers do not each leak different diagnostics into toast titles.
  */
  let lastError: unknown;
  for (let attempt = 0; attempt <= NETWORK_RETRY_DELAYS_MS.length; attempt += 1) {
    try {
      return await fetch(url, init);
    } catch (error) {
      lastError = error;
      const delayMs = NETWORK_RETRY_DELAYS_MS[attempt];
      if (delayMs === undefined) {
        break;
      }
      await new Promise((resolve) => globalThis.setTimeout(resolve, delayMs));
    }
  }
  throw createGxserverTransportError(context, lastError);
}

function parseObject(payloadJson: string): Record<string, unknown> | undefined {
  try {
    const parsed = JSON.parse(payloadJson) as unknown;
    return typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : undefined;
  } catch {
    return undefined;
  }
}

async function handleRendererCommand(
  socket: WebSocket,
  command: GxserverRendererCommand,
  handler: NonNullable<NativeSidebarPresentationSubscriptionHandlers["onRendererCommand"]>,
): Promise<void> {
  try {
    const result = await handler(command);
    sendRendererCommandResult(socket, {
      commandId: command.commandId,
      ok: true,
      result: isObjectRecord(result) ? result : { ok: true },
      type: "rendererCommandResult",
    });
  } catch (error) {
    sendRendererCommandResult(socket, {
      commandId: command.commandId,
      error: error instanceof Error ? error.message : String(error),
      ok: false,
      type: "rendererCommandResult",
    });
  }
}

function sendRendererCommandResult(socket: WebSocket, payload: Record<string, unknown>): void {
  if (socket.readyState === WebSocket.OPEN) {
    socket.send(JSON.stringify(payload));
  }
}

function parseGxserverEvent(value: unknown): GxserverEvent | undefined {
  try {
    const text = typeof value === "string" ? value : String(value);
    const parsed = JSON.parse(text.trim()) as unknown;
    if (
      typeof parsed !== "object" ||
      parsed === null ||
      Array.isArray(parsed) ||
      (parsed as { protocolVersion?: unknown }).protocolVersion !== GXSERVER_PROTOCOL_VERSION
    ) {
      return undefined;
    }
    return parsed as GxserverEvent;
  } catch {
    return undefined;
  }
}

function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseHealth(value: unknown): GxserverServerHealthResponse {
  if (
    typeof value !== "object" ||
    value === null ||
    Array.isArray(value) ||
    (value as { product?: unknown }).product !== GXSERVER_PRODUCT
  ) {
    throw new Error("gxserver health response did not identify gxserver.");
  }
  if ((value as { protocolVersion?: unknown }).protocolVersion !== GXSERVER_PROTOCOL_VERSION) {
    throw new Error(
      `gxserver protocol mismatch. Expected protocol ${GXSERVER_PROTOCOL_VERSION}, got ${String(
        (value as { protocolVersion?: unknown }).protocolVersion,
      )}. Update Ghostex and gxserver so their protocol versions match.`,
    );
  }
  return value as GxserverServerHealthResponse;
}

function isRpcSuccess(value: unknown): value is GxserverRpcSuccessResponse {
  return (
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value) &&
    (value as { ok?: unknown }).ok === true &&
    (value as { product?: unknown }).product === GXSERVER_PRODUCT
  );
}

function createGxserverError(
  body: unknown,
  status: number,
  context: GxserverRequestContext,
): Error {
  if (
    typeof body === "object" &&
    body !== null &&
    !Array.isArray(body) &&
    (body as GxserverRpcErrorResponse).ok === false &&
    typeof (body as GxserverRpcErrorResponse).message === "string"
  ) {
    return new Error((body as GxserverRpcErrorResponse).message);
  }
  if (status <= 0) {
    return createGxserverTransportError(context, undefined);
  }
  return new Error(createGxserverHttpErrorMessage(context, status));
}

function createGxserverRequestId(): string {
  return `gxserver-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

function createNativeGxserverClientErrorMessage(response: NativeGxserverResponseEvent): string {
  const context: GxserverRequestContext = {
    method: response.path === "/api/health" || response.path === "/api/health/server" ? "GET" : "POST",
    path: response.path,
  };
  const status = response.statusCode;
  if (status !== undefined && status > 0) {
    return createGxserverHttpErrorMessage(context, status);
  }
  return createGxserverTransportErrorMessage(context, response.error);
}

function createGxserverTransportError(
  context: GxserverRequestContext,
  error: unknown,
): Error {
  const detail = error instanceof Error
    ? error.message
    : typeof error === "string"
      ? error
      : undefined;
  return new Error(createGxserverTransportErrorMessage(context, detail));
}

function createGxserverTransportErrorMessage(
  context: GxserverRequestContext,
  rawDetail: string | undefined,
): string {
  return [
    `Could not ${describeGxserverOperation(context.path)}.`,
    describeGxserverTransportFailure(rawDetail),
    gxserverRecoveryHint(),
  ].join(" ");
}

function createGxserverHttpErrorMessage(
  context: GxserverRequestContext,
  status: number,
): string {
  const operation = describeGxserverOperation(context.path);
  if (status === 401 || status === 403) {
    return `Could not ${operation}. Ghostex is not authorized to talk to gxserver. Restart gxserver and try again.`;
  }
  if (status === 404 || status === 405) {
    return `Could not ${operation}. This Ghostex build and gxserver do not recognize the same request. Update or restart Ghostex and gxserver.`;
  }
  if (status >= 500) {
    return `Could not ${operation}. gxserver hit an internal error (${status}). ${gxserverRecoveryHint()}`;
  }
  return `Could not ${operation}. gxserver rejected the request (${status}). ${gxserverRecoveryHint()}`;
}

function describeGxserverTransportFailure(rawDetail: string | undefined): string {
  const detail = rawDetail?.toLowerCase() ?? "";
  if (detail.includes("timed out") || detail.includes("timeout")) {
    return "gxserver did not respond before the timeout.";
  }
  if (
    detail.includes("connection refused") ||
    detail.includes("could not connect") ||
    detail.includes("failed to connect")
  ) {
    return "gxserver is not accepting connections.";
  }
  if (detail.includes("aborted") || detail.includes("cancel")) {
    return "The gxserver request was canceled before it finished.";
  }
  if (detail.includes("unauthorized") || detail.includes("forbidden")) {
    return "Ghostex could not authenticate with gxserver.";
  }
  if (detail.includes("cors") || detail.includes("preflight")) {
    return "Ghostex could not finish the gxserver browser handshake.";
  }
  return "gxserver did not respond.";
}

function gxserverRecoveryHint(): string {
  return "Try again; if it keeps failing, restart gxserver.";
}

function describeGxserverOperation(path: GxserverEndpointPath): string {
  switch (path) {
    case "/api/health":
    case "/api/health/server":
      return "check gxserver status";
    case "/api/readAgentSettings":
      return "load agent settings";
    case "/api/updateAgentSettings":
      return "save agent settings";
    case "/api/readAgentHookStatus":
      return "check agent hook status";
    case "/api/installAgentHooks":
      return "install agent hooks";
    case "/api/createSession":
      return "create the session";
    case "/api/createAgentSession":
      return "create the agent session";
    case "/api/forkSession":
      return "fork the session";
    case "/api/readAgentLaunchPlan":
      return "prepare the agent launch command";
    case "/api/readAgentResumePlan":
      return "prepare the agent resume command";
    case "/api/requestSessionRename":
      return "rename the session";
    case "/api/cancelFirstPromptAutoTitle":
      return "stop automatic session title generation";
    case "/api/readPresentationSnapshot":
      return "load the session list";
    case "/api/searchSessions":
    case "/api/listPreviousSessions":
      return "load previous sessions";
    case "/api/transitionSession":
      return "change the session state";
    case "/api/sleepSession":
      return "sleep the session";
    case "/api/wakeSession":
      return "wake the session";
    case "/api/startSessionProvider":
      return "start the session provider";
    case "/api/killSession":
      return "stop the session";
    case "/api/probeSessionProvider":
      return "check the session provider";
    case "/api/listSessions":
      return "load sessions";
    case "/api/removeSession":
      return "remove the session";
    case "/api/readSessionText":
      return "read session output";
    case "/api/sendSessionText":
    case "/api/sendSessionMessage":
    case "/api/sendSessionEnter":
      return "send text to the session";
    case "/api/focusSession":
      return "focus the session";
    case "/api/dispatchRendererCommand":
      return "run the renderer command";
    case "/api/attachSessionMetadata":
      return "prepare the terminal attach command";
    case "/api/createProject":
    case "/api/addProjectPath":
      return "add the project";
    case "/api/updateProject":
      return "update the project";
    case "/api/listProjects":
      return "load projects";
    case "/api/readProjectStatus":
      return "load project status";
    case "/api/removeProject":
      return "remove the project";
    case "/api/deleteWorktreeProject":
      return "delete the worktree project";
    case "/api/updateSession":
      return "update the session";
    case "/api/updateSessionOrder":
      return "save the session order";
    case "/api/runGitAction":
      return "run the Git action";
    case "/api/runGitHubAction":
      return "run the GitHub action";
    case "/api/runWorktreeAction":
      return "run the worktree action";
    case "/api/runProjectSetupCommand":
      return "run the project setup command";
    case "/api/runBeadsAction":
      return "run the Project Board action";
    case "/api/previewRepositoryClone":
      return "preview the repository clone";
    case "/api/startRepositoryClone":
      return "start cloning the repository";
    case "/api/readRepositoryCloneJob":
      return "load repository clone progress";
    case "/api/cancelRepositoryCloneJob":
      return "cancel the repository clone";
    case "/api/browseProjectDirectories":
      return "browse project folders";
    case "/api/resolveGitRootForPath":
      return "detect the Git repository";
    case "/api/queryLogs":
      return "load support logs";
    case "/api/updateAuth":
      return "update gxserver authentication";
    case "/api/updateListenerConfig":
      return "update gxserver listener settings";
    case "/api/installTool":
      return "install the tool";
    case "/api/browseFilesystem":
      return "browse files";
    case "/api/destructiveAdminAction":
      return "run the gxserver admin action";
    case "/api/events":
    case "/api/control/stop":
    case "/api/control/stopAll":
    case "/api/ingestAgentHookEvent":
    case "/api/ingestSessionStateEvent":
    case "/api/ingestTerminalTitleEvent":
    case "/api/updateAgentActivity":
      return "complete the gxserver request";
  }
}

function parseGxserverErrorMessage(bodyJson: string | undefined): string | undefined {
  if (!bodyJson) {
    return undefined;
  }
  try {
    const parsed = JSON.parse(bodyJson) as { message?: unknown };
    return typeof parsed.message === "string" && parsed.message.trim()
      ? parsed.message
      : undefined;
  } catch {
    return undefined;
  }
}
