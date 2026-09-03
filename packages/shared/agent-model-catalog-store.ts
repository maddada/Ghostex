/*
CDXC:AgentProviders 2026-09-02:
Where a client's current agent model catalog comes from, in order:

1. The snapshot bundled with the build (`agent-model-catalog.json` at the repo
   root, the same file that is published), so a fresh install renders a full
   dropdown before any network call.
2. The copy the last successful remote fetch cached in localStorage, when it
   is newer than the bundled one (an older build keeps benefiting from a
   catalog update it already saw).
3. The remote document, fetched once per page load from the repo's main
   branch. Whenever GitHub is reachable and the file parses, it is the
   source of truth and replaces whatever was showing; the bundled and cached
   copies only cover the time before that fetch lands, or a fetch that fails.

"Newer" between the bundled and cached copies is the document's `updatedAt`,
so a fresh build carrying a newer snapshot is not shadowed by a stale cache.

Subscribers (the composer's option pills, through `useAgentModelCatalog`)
re-render when the current catalog changes. Reads outside React go through
`currentAgentModelCatalog()`.
*/

import { useEffect, useSyncExternalStore } from 'react';
import bundledCatalogJson from '../../agent-model-catalog.json';
import {
  AGENT_MODEL_CATALOG_URL,
  newerAgentModelCatalog,
  parseAgentModelCatalog,
  type AgentModelCatalog,
} from './agent-model-catalog';

const STORAGE_KEY = 'ghostex.agentModelCatalog.v1';

const bundledCatalog: AgentModelCatalog = (() => {
  const parsed = parseAgentModelCatalog(bundledCatalogJson);
  if (parsed === null) {
    throw new Error('agent-model-catalog.json does not parse as a schema v1 catalog');
  }
  return parsed;
})();

function storage(): Storage | null {
  try {
    return typeof window === 'undefined' ? null : window.localStorage;
  } catch {
    return null;
  }
}

function readCachedCatalog(): AgentModelCatalog | null {
  const raw = storage()?.getItem(STORAGE_KEY);
  if (!raw) {
    return null;
  }
  try {
    return parseAgentModelCatalog(JSON.parse(raw));
  } catch {
    return null;
  }
}

function writeCachedCatalog(catalog: AgentModelCatalog): void {
  try {
    storage()?.setItem(STORAGE_KEY, JSON.stringify(catalog));
  } catch {
    // Quota or private mode: the in-memory catalog still serves this load.
  }
}

let current: AgentModelCatalog = (() => {
  const cached = readCachedCatalog();
  return cached === null ? bundledCatalog : newerAgentModelCatalog(bundledCatalog, cached);
})();

const listeners = new Set<() => void>();

function replaceCatalog(next: AgentModelCatalog): void {
  if (next === current) {
    return;
  }
  current = next;
  for (const listener of listeners) {
    listener();
  }
}

export function currentAgentModelCatalog(): AgentModelCatalog {
  return current;
}

export function subscribeAgentModelCatalog(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

let refresh: Promise<AgentModelCatalog> | null = null;

/**
 * Fetches the published catalog once per page load and adopts it as the
 * source of truth when it parses. Resolves to the catalog in effect
 * afterwards, so callers never have to handle a failed fetch themselves.
 */
export function refreshAgentModelCatalog(): Promise<AgentModelCatalog> {
  if (refresh !== null) {
    return refresh;
  }
  refresh = (async () => {
    if (typeof fetch !== 'function') {
      return current;
    }
    try {
      const response = await fetch(AGENT_MODEL_CATALOG_URL, { cache: 'no-store' });
      if (!response.ok) {
        return current;
      }
      const remote = parseAgentModelCatalog(await response.json());
      if (remote === null) {
        return current;
      }
      writeCachedCatalog(remote);
      replaceCatalog(remote);
    } catch {
      // Offline or blocked: the bundled or cached catalog stays in effect.
    }
    return current;
  })();
  return refresh;
}

/** Test seam: installs a catalog as the current one without any fetch. */
export function setAgentModelCatalogForTests(catalog: AgentModelCatalog | null): void {
  refresh = null;
  replaceCatalog(catalog ?? bundledCatalog);
}

/**
 * The current catalog, kept live: subscribes to replacements and kicks off the
 * once-per-load remote refresh on first use.
 */
export function useAgentModelCatalog(): AgentModelCatalog {
  const catalog = useSyncExternalStore(subscribeAgentModelCatalog, currentAgentModelCatalog, currentAgentModelCatalog);
  useEffect(() => {
    void refreshAgentModelCatalog();
  }, []);
  return catalog;
}
