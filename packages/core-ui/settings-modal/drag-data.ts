export type SettingsAgentDragData = {
  agentId: string;
  kind: 'settings-agent';
};

export type SettingsCommandDragData = {
  commandId: string;
  kind: 'settings-command';
};

export type SettingsSidebarTagListItemDragData = {
  itemId: string;
  kind: 'settings-sidebar-tag-list-item';
};

export function createSettingsAgentDragData(agentId: string): SettingsAgentDragData {
  return {
    agentId,
    kind: 'settings-agent',
  };
}

export function getSettingsAgentDragData(candidate: unknown): SettingsAgentDragData | undefined {
  if (!hasData(candidate)) {
    return undefined;
  }

  const data = candidate.data;
  if (!isObjectRecord(data) || data.kind !== 'settings-agent' || typeof data.agentId !== 'string') {
    return undefined;
  }

  return {
    agentId: data.agentId,
    kind: 'settings-agent',
  };
}

export function createSettingsCommandDragData(commandId: string): SettingsCommandDragData {
  return {
    commandId,
    kind: 'settings-command',
  };
}

export function getSettingsCommandDragData(candidate: unknown): SettingsCommandDragData | undefined {
  if (!hasData(candidate)) {
    return undefined;
  }

  const data = candidate.data;
  if (!isObjectRecord(data) || data.kind !== 'settings-command' || typeof data.commandId !== 'string') {
    return undefined;
  }

  return {
    commandId: data.commandId,
    kind: 'settings-command',
  };
}

export function createSettingsSidebarTagListItemDragData(itemId: string): SettingsSidebarTagListItemDragData {
  return {
    itemId,
    kind: 'settings-sidebar-tag-list-item',
  };
}

export function getSettingsSidebarTagListItemDragData(
  candidate: unknown
): SettingsSidebarTagListItemDragData | undefined {
  if (!hasData(candidate)) {
    return undefined;
  }

  const data = candidate.data;
  if (!isObjectRecord(data) || data.kind !== 'settings-sidebar-tag-list-item' || typeof data.itemId !== 'string') {
    return undefined;
  }

  return {
    itemId: data.itemId,
    kind: 'settings-sidebar-tag-list-item',
  };
}

export function moveId(ids: readonly string[], initialIndex: number, index: number): string[] {
  const nextIds = [...ids];
  const [id] = nextIds.splice(initialIndex, 1);
  if (id === undefined) {
    return nextIds;
  }

  nextIds.splice(index, 0, id);
  return nextIds;
}

export function mergeIds(draftIds: readonly string[], syncedIds: readonly string[]): string[] {
  const syncedIdSet = new Set(syncedIds);
  const mergedIds = draftIds.filter((id) => syncedIdSet.has(id));

  for (const id of syncedIds) {
    if (!mergedIds.includes(id)) {
      mergedIds.push(id);
    }
  }

  return mergedIds;
}

export function reconcileDraftIds<Item extends Record<Key, string>, Key extends keyof Item>(
  draftIds: readonly string[] | undefined,
  items: readonly Item[],
  key: Key
): string[] | undefined {
  if (!draftIds) {
    return undefined;
  }

  const syncedIds = items.map((item) => item[key]);
  const nextDraftIds = mergeIds(draftIds, syncedIds);
  return haveSameOrder(nextDraftIds, syncedIds) ? undefined : nextDraftIds;
}

export function haveSameOrder(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length && left.every((id, index) => id === right[index]);
}

export function createSettingsReorderRequestId(kind: 'actions' | 'agents' | 'globalActions'): string {
  return `settings-${kind}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

export function hasData(candidate: unknown): candidate is { data?: unknown } {
  return isObjectRecord(candidate) && 'data' in candidate;
}

export function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
