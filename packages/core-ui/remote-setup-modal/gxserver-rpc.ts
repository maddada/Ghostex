import {
  GXSERVER_PROTOCOL_VERSION,
  type GxserverProtocolVersion,
  type GxserverRpcEndpointPath,
} from '@/packages/shared/gxserver-protocol';

/**
 * CDXC:RemotePairing 2026-09-03:
 * The Remote Setup modal talks to the gxserver that serves the page it runs in:
 * the desktop modal host through its injected bootstrap, the web app through
 * its local machine connection. The caller hands in the callback; when no
 * daemon is reachable the callback is absent and Connect is disabled with the
 * reason instead of pretending to work.
 */
export type RemoteSetupRpc = (path: GxserverRpcEndpointPath, params: Record<string, unknown>) => Promise<unknown>;

type GxserverBootstrap = {
  authToken: string;
  baseUrl: string;
  protocolVersion?: GxserverProtocolVersion;
};

function gpuiGxserverBootstrap(): GxserverBootstrap | undefined {
  const candidate = (window as unknown as { ghostexGpui?: { gxserverBootstrap?: unknown } }).ghostexGpui
    ?.gxserverBootstrap;
  if (!candidate || typeof candidate !== 'object') {
    return undefined;
  }
  const bootstrap = candidate as Partial<GxserverBootstrap>;
  if (
    typeof bootstrap.authToken !== 'string' ||
    !bootstrap.authToken.trim() ||
    typeof bootstrap.baseUrl !== 'string' ||
    !bootstrap.baseUrl.trim() ||
    (bootstrap.protocolVersion !== undefined && bootstrap.protocolVersion !== GXSERVER_PROTOCOL_VERSION)
  ) {
    return undefined;
  }
  return bootstrap as GxserverBootstrap;
}

const GPUI_BOOTSTRAP_REMOTE_SETUP_RPC: RemoteSetupRpc = async (path, params) => {
  const bootstrap = gpuiGxserverBootstrap();
  if (!bootstrap) {
    throw new Error('The Ghostex server connection is unavailable.');
  }
  const response = await fetch(`${bootstrap.baseUrl}${path}`, {
    body: JSON.stringify({ params, protocolVersion: GXSERVER_PROTOCOL_VERSION }),
    headers: {
      authorization: `Bearer ${bootstrap.authToken}`,
      'content-type': 'application/json',
      'x-gxserver-protocol-version': String(GXSERVER_PROTOCOL_VERSION),
    },
    method: 'POST',
  });
  let body: unknown;
  try {
    body = await response.json();
  } catch {
    body = undefined;
  }
  const envelope = body as { error?: { message?: string }; ok?: boolean; result?: unknown } | undefined;
  if (!response.ok || envelope?.ok !== true) {
    throw new Error(
      typeof envelope?.error?.message === 'string'
        ? envelope.error.message
        : `gxserver rejected ${path} (${response.status || 'no response'}).`
    );
  }
  return envelope.result;
};

/**
 * The desktop modal host is a CEF page with the sidebar gxserver bootstrap
 * installed on it. Returns undefined while this page has no bootstrap; the
 * identity is stable so callers may re-ask on every render.
 */
export function gpuiBootstrapRemoteSetupRpc(): RemoteSetupRpc | undefined {
  return gpuiGxserverBootstrap() ? GPUI_BOOTSTRAP_REMOTE_SETUP_RPC : undefined;
}
