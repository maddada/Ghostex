import type {
  GhostexExtensionCatalogEntry,
  GhostexExtensionManifest,
  GhostexExtensionPlacement,
} from '@/packages/shared/ghostex-extensions';

export type ExtensionSourceFilter = 'all' | 'built-in' | 'installed' | 'store' | 'custom';
export type ExtensionTypeFilter = 'all' | GhostexExtensionPlacement | 'terminal-pane' | 'header-button' | 'menu-item';

/**
 * CDXC:Extensions 2026-09-24 DECISION:
 * User: the search, type and category filters and the "N shown" count apply to every extension on the Settings
 * Extensions page, not only the Store list. One filter object is owned by the page and every group (built-in
 * categories, installed, Store, the user's own views) is matched against it; groups with no match disappear.
 */
export type ExtensionFilter = {
  /** A category label, or 'all'. Built-in categories and Store categories share the one list. */
  category: string;
  query: string;
  source: ExtensionSourceFilter;
  type: ExtensionTypeFilter;
};

export const DEFAULT_EXTENSION_FILTER: ExtensionFilter = { category: 'all', query: '', source: 'all', type: 'all' };

/** What one card offers to the filter. */
export type ExtensionFilterSubject = {
  categories: readonly string[];
  /** Author, description and the like: matched by the text query only. */
  searchText: readonly string[];
  source: Exclude<ExtensionSourceFilter, 'all'>;
  title: string;
  types: readonly ExtensionTypeFilter[];
};

export function isExtensionFilterActive(filter: ExtensionFilter): boolean {
  return filter.query.trim() !== '' || filter.source !== 'all' || filter.type !== 'all' || filter.category !== 'all';
}

export function extensionFilterMatches(filter: ExtensionFilter, subject: ExtensionFilterSubject): boolean {
  if (filter.source !== 'all' && filter.source !== subject.source) return false;
  if (filter.type !== 'all' && !subject.types.includes(filter.type)) return false;
  if (filter.category !== 'all' && !subject.categories.includes(filter.category)) return false;
  const query = filter.query.trim().toLocaleLowerCase();
  if (!query) return true;
  return [subject.title, ...subject.searchText, ...subject.categories].join(' ').toLocaleLowerCase().includes(query);
}

export function catalogTypes(
  entry: Pick<GhostexExtensionCatalogEntry | GhostexExtensionManifest, 'kind' | 'placements'>
): ExtensionTypeFilter[] {
  return entry.kind === 'terminal-pane' ? ['terminal-pane'] : [...(entry.placements ?? [])];
}

export function extensionTypeLabel(type: ExtensionTypeFilter): string {
  switch (type) {
    case 'all':
      return 'All types';
    case 'chat-bar':
      return 'Chat bar';
    case 'terminal-pane':
      return 'Terminal pane';
    case 'header-button':
      return 'Header button';
    case 'menu-item':
      return 'Menu item';
    default:
      return type[0].toUpperCase() + type.slice(1);
  }
}

export const EXTENSION_TYPE_FILTERS: readonly ExtensionTypeFilter[] = [
  'all',
  'view',
  'header-button',
  'menu-item',
  'chat-bar',
  'popup',
  'modal',
  'terminal-pane',
];

export function extensionSourceLabel(source: ExtensionSourceFilter): string {
  switch (source) {
    case 'all':
      return 'All sources';
    case 'built-in':
      return 'Built-in';
    case 'installed':
      return 'Installed';
    case 'store':
      return 'Store';
    case 'custom':
      return 'Your views';
  }
}
