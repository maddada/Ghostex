import { GXSERVER_PROTOCOL_VERSION, type GxserverProtocolVersion } from '@/packages/shared/gxserver-protocol';
import type {
  GhostexExtensionStatePatch,
  GhostexExtensionsCatalogResult,
  GhostexInstallExtensionResult,
  GhostexListExtensionsResult,
  GhostexSetExtensionStateResult,
  GhostexUninstallExtensionResult,
} from '@/packages/shared/ghostex-extensions';

export interface ExtensionsModalTransport {
  catalog(): Promise<GhostexExtensionsCatalogResult>;
  install(id: string): Promise<GhostexInstallExtensionResult>;
  list(): Promise<GhostexListExtensionsResult>;
  setState(id: string, patch: GhostexExtensionStatePatch): Promise<GhostexSetExtensionStateResult>;
  uninstall(id: string): Promise<GhostexUninstallExtensionResult>;
}

type GxserverBootstrap = {
  authToken: string;
  baseUrl: string;
  protocolVersion?: GxserverProtocolVersion;
};

function gxserverBootstrap(): GxserverBootstrap | undefined {
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

export function extensionStaticAssetUrl(id: string, path: string): string | undefined {
  const bootstrap = gxserverBootstrap();
  if (!bootstrap) return undefined;
  const encodedPath = path
    .split('/')
    .filter(Boolean)
    .map((segment) => encodeURIComponent(segment))
    .join('/');
  return `${bootstrap.baseUrl}/ext/${encodeURIComponent(id)}/${encodedPath}`;
}

async function rpc<TResult>(path: string, params: Record<string, unknown>): Promise<TResult> {
  const bootstrap = gxserverBootstrap();
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
  const envelope = body as { error?: { message?: string }; ok?: boolean; result?: TResult } | undefined;
  if (!response.ok || envelope?.ok !== true) {
    throw new Error(
      typeof envelope?.error?.message === 'string'
        ? envelope.error.message
        : `gxserver rejected ${path} (${response.status || 'no response'}).`
    );
  }
  return envelope.result as TResult;
}

/**
 * CDXC:Extensions 2026-08-30:
 * The Extensions settings page is part of the shared Settings modal, which the
 * web app also mounts. Only the desktop shell injects
 * `window.ghostexGpui.gxserverBootstrap`, so this returns `undefined` where
 * there is no gxserver to talk to and the page hides the store instead of
 * rendering a surface whose every request would fail.
 */
export function createExtensionsModalTransport(): ExtensionsModalTransport | undefined {
  if (!gxserverBootstrap()) {
    return undefined;
  }
  return {
    catalog: () => rpc<GhostexExtensionsCatalogResult>('/api/extensionsCatalog', {}),
    install: (id) => rpc<GhostexInstallExtensionResult>('/api/installExtension', { id }),
    list: () => rpc<GhostexListExtensionsResult>('/api/listExtensions', {}),
    setState: (id, patch) => rpc<GhostexSetExtensionStateResult>('/api/updateExtensionState', { id, patch }),
    uninstall: (id) => rpc<GhostexUninstallExtensionResult>('/api/uninstallExtension', { id }),
  };
}
