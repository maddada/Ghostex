const STORAGE_KEY = 'ghostex.sidebar.hidden-items.v1';

export type SidebarHiddenItems = { groupIds: string[] };

export function readSidebarHiddenItems(): SidebarHiddenItems {
  try {
    const value = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? 'null') as Partial<SidebarHiddenItems> | null;
    return { groupIds: uniqueStrings(value?.groupIds) };
  } catch {
    return { groupIds: [] };
  }
}

export function writeSidebarHiddenItems(value: SidebarHiddenItems): void {
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
}

function uniqueStrings(value: unknown): string[] {
  return Array.isArray(value)
    ? [...new Set(value.filter((item): item is string => typeof item === 'string' && item.length > 0))]
    : [];
}
