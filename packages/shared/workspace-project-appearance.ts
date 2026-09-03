import {
  isSidebarCommandIcon,
  normalizeSidebarCommandIconColor,
  type SidebarCommandIcon,
} from './sidebar-command-icons';

export type WorkspaceProjectIcon =
  { kind: 'image'; dataUrl: string } | { color?: string; icon: SidebarCommandIcon; kind: 'tabler' };

export type WorkspaceProjectIconSource = {
  icon?: WorkspaceProjectIcon;
  iconDataUrl?: string;
};

export const DEFAULT_WORKSPACE_THEME_COLOR = '#2f6feb';
const MAX_WORKSPACE_THEME_COLOR_HISTORY = 8;

/**
 * CDXC:Theming 2026-05-05-02:58
 * Workspaces can carry an optional custom theme color selected from the Theme
 * context menu. Keep persisted values and the recent-color palette as validated
 * hex colors so the UI can inject them into CSS variables without accepting
 * arbitrary CSS text.
 */
export function normalizeWorkspaceThemeColor(value: unknown): string | undefined {
  return normalizeSidebarCommandIconColor(value);
}

export function getWorkspaceThemeForeground(themeColor: string): '#111111' | '#ffffff' {
  const normalizedColor = normalizeWorkspaceThemeColor(themeColor);
  if (!normalizedColor) {
    return '#ffffff';
  }

  const hex = normalizedColor.replace('#', '');
  const red = Number.parseInt(hex.slice(0, 2), 16);
  const green = Number.parseInt(hex.slice(2, 4), 16);
  const blue = Number.parseInt(hex.slice(4, 6), 16);
  const luminance = (red * 299 + green * 587 + blue * 114) / 1000;
  return luminance > 154 ? '#111111' : '#ffffff';
}

export function normalizeWorkspaceThemeColorHistory(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return [];
  }

  const colors: string[] = [];
  for (const candidate of value) {
    const color = normalizeWorkspaceThemeColor(candidate);
    if (color && !colors.includes(color)) {
      colors.push(color);
    }
  }

  return colors.slice(0, MAX_WORKSPACE_THEME_COLOR_HISTORY);
}

export function updateWorkspaceThemeColorHistory(history: readonly string[], value: unknown): string[] {
  const color = normalizeWorkspaceThemeColor(value);
  if (!color) {
    return normalizeWorkspaceThemeColorHistory([...history]);
  }

  return normalizeWorkspaceThemeColorHistory([
    color,
    ...history.filter((candidate) => normalizeWorkspaceThemeColor(candidate) !== color),
  ]);
}

export function normalizeWorkspaceProjectIcon(value: unknown): WorkspaceProjectIcon | undefined {
  if (!value || typeof value !== 'object') {
    return undefined;
  }
  const icon = value as Partial<WorkspaceProjectIcon>;
  if (icon.kind === 'image') {
    const dataUrl = normalizeWorkspaceProjectIconDataUrl(icon.dataUrl);
    return dataUrl ? { dataUrl, kind: 'image' } : undefined;
  }
  if (icon.kind === 'tabler' && isSidebarCommandIcon(icon.icon)) {
    return {
      color: normalizeSidebarCommandIconColor(icon.color),
      icon: icon.icon,
      kind: 'tabler',
    };
  }
  return undefined;
}

/**
 * CDXC:Icons 2026-04-28-01:19
 * Workspace icons are no longer image-only. Persist a typed icon so users can
 * choose first-class Tabler glyphs while existing saved PNG/SVG data URLs keep
 * rendering through the image variant after upgrade.
 */
export function normalizeWorkspaceProjectIconDataUrl(value: unknown): string | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }
  return /^data:image\/(?:png|svg\+xml);base64,/u.test(value) ? value : undefined;
}

/**
 * CDXC:Icons 2026-07-29 (discovered icons):
 * The data URL a gxserver publishes for an icon it discovered inside a project's
 * checkout (`GxserverPresentationProject.discoveredIconDataUrl`).
 *
 * It has its own validator rather than reusing
 * `normalizeWorkspaceProjectIconDataUrl` because the two accept different
 * things for different reasons. A user-attached icon is produced by Ghostex's
 * own picker, so PNG and SVG are the only shapes it can ever have. A discovered
 * icon is whatever the repository ships — `favicon.ico` is the single most
 * common one in the wild — so the set matches the formats the discovery probe
 * is allowed to read, which are exactly the formats the sidebar's Chromium
 * surface can render.
 *
 * This is a boundary check, not a formality: the value arrives from a daemon
 * (possibly a remote machine's) and lands in an `<img src>`, so anything that
 * is not a base64 image data URL is dropped rather than rendered.
 */
const DISCOVERED_PROJECT_ICON_DATA_URL_PATTERN =
  /^data:image\/(?:png|svg\+xml|x-icon|vnd\.microsoft\.icon|jpeg|webp|gif);base64,[A-Za-z0-9+/]+=*$/u;

export function normalizeDiscoveredProjectIconDataUrl(value: unknown): string | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }
  const trimmed = value.trim();
  return DISCOVERED_PROJECT_ICON_DATA_URL_PATTERN.test(trimmed) ? trimmed : undefined;
}

/**
 * CDXC:Icons 2026-05-11-01:50
 * Project icons need one shared React/native source so macOS notifications and
 * future React titlebar project chrome render the same user-selected image
 * instead of each surface inventing separate icon lookup rules.
 */
export function resolveWorkspaceProjectIconDataUrl(
  project: WorkspaceProjectIconSource | undefined
): string | undefined {
  const icon = normalizeWorkspaceProjectIcon(project?.icon);
  if (icon?.kind === 'image') {
    return icon.dataUrl;
  }
  return normalizeWorkspaceProjectIconDataUrl(project?.iconDataUrl);
}
