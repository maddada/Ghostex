import { isRecord } from './primitives';

export const CUSTOM_VIEW_ID_PREFIX = 'custom-view-';

/**
 * CDXC:Extensions 2026-09-03 DECISION: Users can add any number of custom titlebar views, arrange them in their preferred order, and turn individual views off without deleting their name and HTTP or HTTPS URL.
 * CDXC:Extensions 2026-09-03 SEE-ALSO: The editor, native titlebar projection, and isolated CEF workarea must keep this ordered, enabled, name-and-URL contract aligned. See packages/core-ui/settings-modal/tabs/extensions.tsx, apps/desktop/src/app/helpers/titlebar.rs, and apps/desktop/src/app/workarea.rs.
 */
export type GhostexCustomView = {
  enabled: boolean;
  id: string;
  name: string;
  url: string;
};

export function normalizeCustomViewUrl(candidate: unknown): string | undefined {
  if (typeof candidate !== 'string') {
    return undefined;
  }
  const value = candidate.trim();
  try {
    const url = new URL(value);
    if ((url.protocol !== 'http:' && url.protocol !== 'https:') || !url.hostname) {
      return undefined;
    }
    return url.toString();
  } catch {
    return undefined;
  }
}

export function normalizeGhostexCustomViews(candidate: unknown): GhostexCustomView[] {
  if (!Array.isArray(candidate)) {
    return [];
  }
  const ids = new Set<string>();
  const views: GhostexCustomView[] = [];
  for (const entry of candidate) {
    if (!isRecord(entry)) {
      continue;
    }
    const id = typeof entry.id === 'string' ? entry.id.trim() : '';
    const name = typeof entry.name === 'string' ? entry.name.trim() : '';
    const url = normalizeCustomViewUrl(entry.url);
    if (!id.startsWith(CUSTOM_VIEW_ID_PREFIX) || !/^[a-z0-9-]+$/u.test(id) || ids.has(id) || !name || !url) {
      continue;
    }
    ids.add(id);
    views.push({ enabled: entry.enabled !== false, id, name, url });
  }
  return views;
}
