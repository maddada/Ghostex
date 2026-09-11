import { KEEP_AWAKE_DURATION_OPTIONS, type KeepAwakeDurationMinutes } from '../../shared/ghostex-settings';
import { readLegacyCollapsedSidebarProjectCollectionIds } from '../project-collections';
import {
  readProjectSessionListCollapsedState,
  type ProjectSessionListCollapsedState,
} from '../project-session-list-toggle';
import type { SidebarKeepAwakeRuntimeState } from './types';
import {
  normalizeProjectSessionSectionCollapseState,
  persistedProjectSessionSectionCollapseState,
  type ProjectSessionSectionCollapseStateById,
} from './project-session-section-state';

export const SIDEBAR_KEEP_AWAKE_RUNTIME_STORAGE_KEY = 'ghostex.titlebar.keepAwakeRuntime';
export function isSidebarRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}
export function isKeepAwakeDurationMinutes(value: unknown): value is KeepAwakeDurationMinutes {
  return KEEP_AWAKE_DURATION_OPTIONS.some((option) => option.value === value);
}
export function readSidebarKeepAwakeRuntime(): SidebarKeepAwakeRuntimeState | undefined {
  if (typeof window === 'undefined') {
    return undefined;
  }

  try {
    const rawRuntime = window.localStorage.getItem(SIDEBAR_KEEP_AWAKE_RUNTIME_STORAGE_KEY);
    if (!rawRuntime) {
      return undefined;
    }
    const parsedRuntime: unknown = JSON.parse(rawRuntime);
    if (!isSidebarRecord(parsedRuntime) || !isKeepAwakeDurationMinutes(parsedRuntime.durationMinutes)) {
      return undefined;
    }
    const fireAtMs = parsedRuntime.fireAtMs;
    if (typeof fireAtMs === 'number' && Number.isFinite(fireAtMs) && fireAtMs <= Date.now()) {
      return undefined;
    }
    return {
      durationMinutes: parsedRuntime.durationMinutes,
    };
  } catch {
    return undefined;
  }
}
export type SidebarUiCollapseState = {
  collapsedGroupsById: Record<string, true>;
  collapsedProjectCollectionsByKey: Record<string, true>;
  collapsedProjectSessionListsById: ProjectSessionListCollapsedState;
  collapsedProjectSessionSectionsById: ProjectSessionSectionCollapseStateById;
  isReferenceChatsCollapsed: boolean;
  /*
   * CDXC:Spaces 2026-08-27:
   * The Space each gxserver section is filtered by, keyed by section key
   * ("local" / "remote:<machineId>", see sidebar-app/space-filtering.ts). A
   * value is either a real Space id owned by that section's daemon or the
   * reserved built-in id `other`.
   *
   * CDXC:Spaces 2026-09-02:
   * The ABSENCE of a key means "never switched", not a view of its own: it
   * resolves at render time to the section's first Space, or to Other when it
   * has none. A stored id whose Space no longer exists resolves the same way
   * instead of being pruned here — the daemon's Space state arrives long after
   * this record is read.
   */
  selectedSpaceIdBySectionKey: Record<string, string>;
  /*
   * CDXC:Spaces 2026-09-11 DECISION:
   * User: switching Spaces restores the state each Space was last in, and that
   * memory lives here in the client, beside the selected Space: one client has
   * one work area, so "what I had open in this Space" shares the selection's
   * scope and lifetime instead of being split across gxserver daemons.
   * Keyed by section key, then Space id (real id or the built-in `other`), to
   * the section's session ids, most recent first. Ids are the sidebar
   * vocabulary, so a remote machine's section holds machine-scoped ids and
   * several remotes never collide. Dead ids are skipped at restore time, not
   * pruned here: the presentation that could vouch for them arrives later.
   */
  recentSessionIdsBySpace: Record<string, Record<string, string[]>>;
};

/** Per Space; deep enough to walk past a handful of closed sessions. */
export const MAX_RECENT_SIDEBAR_SPACE_SESSION_IDS = 20;

/*
 * Version 3 added `selectedSpaceIdBySectionKey`, and later (2026-09-11)
 * `recentSessionIdsBySpace` without a bump: an absent map normalizes to empty
 * and a Space with no memory simply has nothing to restore.
 * Version 2 payloads still load:
 * they normalize to an empty selection map, which every section resolves
 * through the default rule (its first Space, else Other). Version 3 payloads
 * written while the built-in view was still "All Projects" need no migration
 * for the same reason: an absent key was that view and now resolves through
 * the same rule.
 *
 * CDXC:Sidebar 2026-09-02:
 * `isReferenceProjectsCollapsed` and `collapsedRemoteMachineSectionsById` were
 * dropped when the Projects area and the remote machine sections stopped being
 * collapsible. Stored payloads that still carry them need no version bump:
 * `normalizeSidebarUiCollapseState` reads only the keys it knows, so the stale
 * ones are ignored on read and gone after the next write.
 */
export type SidebarUiCollapseStorage = {
  state: Omit<SidebarUiCollapseState, 'collapsedProjectSessionSectionsById'> & {
    collapsedProjectSessionSectionsById: ReturnType<typeof persistedProjectSessionSectionCollapseState>;
  };
  version: 3;
};

const SUPPORTED_SIDEBAR_UI_COLLAPSE_STORAGE_VERSIONS = new Set([2, 3]);

export type SidebarUiCollapseStateReadResult = {
  reason?: 'invalid-shape' | 'missing' | 'parse-error' | 'storage-unavailable';
  state: SidebarUiCollapseState;
  storedByteLength?: number;
};

export type SidebarUiCollapseStateWriteResult = {
  ok: boolean;
  reason?: 'storage-error' | 'storage-unavailable';
  storedByteLength?: number;
};
export const SIDEBAR_UI_COLLAPSE_STATE_STORAGE_KEY = 'ghostex-sidebar-ui-collapse-state';
/*
 * Collapse preferences belong to one app window. The current GPUI host uses
 * "main"; future windows must pass their own stable scope id so their sidebars
 * persist independently without sending presentation state through gxserver.
 */
export const DEFAULT_SIDEBAR_WINDOW_SCOPE_ID = 'main';
export function createDefaultSidebarUiCollapseState(): SidebarUiCollapseState {
  return {
    collapsedGroupsById: {},
    collapsedProjectCollectionsByKey: {},
    collapsedProjectSessionListsById: {},
    collapsedProjectSessionSectionsById: {},
    isReferenceChatsCollapsed: false,
    recentSessionIdsBySpace: {},
    selectedSpaceIdBySectionKey: {},
  };
}

export function normalizeSidebarWindowScopeId(value: string): string {
  const normalized = value.trim().slice(0, 120);
  return normalized || DEFAULT_SIDEBAR_WINDOW_SCOPE_ID;
}

export function getSidebarUiCollapseStateStorageKey(windowScopeId: string): string {
  return `${SIDEBAR_UI_COLLAPSE_STATE_STORAGE_KEY}:window:${encodeURIComponent(windowScopeId)}`;
}

export function createLocalProjectCollectionCollapseKey(collectionId: string): string {
  return `local:${collectionId}`;
}

export function createRemoteProjectCollectionCollapseKey(machineId: string, collectionId: string): string {
  return `remote:${machineId}:${collectionId}`;
}

export function normalizeSidebarUiCollapseState(candidate: unknown): SidebarUiCollapseState {
  if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
    return createDefaultSidebarUiCollapseState();
  }
  const state = candidate as Partial<SidebarUiCollapseState>;
  return {
    collapsedGroupsById: normalizeStoredCollapsedGroupsById(state.collapsedGroupsById),
    collapsedProjectCollectionsByKey: normalizeStoredCollapsedGroupsById(state.collapsedProjectCollectionsByKey),
    collapsedProjectSessionListsById: normalizeStoredCollapsedGroupsById(state.collapsedProjectSessionListsById),
    collapsedProjectSessionSectionsById: normalizeProjectSessionSectionCollapseState(
      state.collapsedProjectSessionSectionsById
    ),
    isReferenceChatsCollapsed: state.isReferenceChatsCollapsed === true,
    recentSessionIdsBySpace: normalizeStoredRecentSessionIdsBySpace(state.recentSessionIdsBySpace),
    selectedSpaceIdBySectionKey: normalizeStoredSelectedSpaceIdBySectionKey(state.selectedSpaceIdBySectionKey),
  };
}

export function normalizeStoredRecentSessionIdsBySpace(candidate: unknown): Record<string, Record<string, string[]>> {
  if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
    return {};
  }
  const recentSessionIdsBySpace: Record<string, Record<string, string[]>> = {};
  for (const [sectionKey, spaces] of Object.entries(candidate)) {
    if (!sectionKey || !spaces || typeof spaces !== 'object' || Array.isArray(spaces)) {
      continue;
    }
    const sessionIdsBySpaceId: Record<string, string[]> = {};
    for (const [spaceId, sessionIds] of Object.entries(spaces as Record<string, unknown>)) {
      if (!spaceId || !Array.isArray(sessionIds)) {
        continue;
      }
      const normalized: string[] = [];
      for (const sessionId of sessionIds) {
        if (typeof sessionId === 'string' && sessionId.length > 0 && !normalized.includes(sessionId)) {
          normalized.push(sessionId);
        }
      }
      if (normalized.length > 0) {
        sessionIdsBySpaceId[spaceId] = normalized.slice(0, MAX_RECENT_SIDEBAR_SPACE_SESSION_IDS);
      }
    }
    if (Object.keys(sessionIdsBySpaceId).length > 0) {
      recentSessionIdsBySpace[sectionKey] = sessionIdsBySpaceId;
    }
  }
  return recentSessionIdsBySpace;
}

/**
 * Moves `sessionId` to the front of the Space's list, returning the same
 * object when it is already there so React state stays referentially stable.
 */
export function rememberSidebarSpaceSession(
  recentSessionIdsBySpace: Record<string, Record<string, string[]>>,
  sectionKey: string,
  spaceId: string,
  sessionId: string
): Record<string, Record<string, string[]>> {
  const sectionMemory = recentSessionIdsBySpace[sectionKey] ?? {};
  const current = sectionMemory[spaceId] ?? [];
  if (current[0] === sessionId) {
    return recentSessionIdsBySpace;
  }
  const next = [sessionId, ...current.filter((candidate) => candidate !== sessionId)].slice(
    0,
    MAX_RECENT_SIDEBAR_SPACE_SESSION_IDS
  );
  return {
    ...recentSessionIdsBySpace,
    [sectionKey]: { ...sectionMemory, [spaceId]: next },
  };
}

export function normalizeStoredSelectedSpaceIdBySectionKey(candidate: unknown): Record<string, string> {
  if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
    return {};
  }

  const selectedSpaceIdBySectionKey: Record<string, string> = {};
  for (const [sectionKey, spaceId] of Object.entries(candidate)) {
    if (typeof spaceId === 'string' && spaceId.length > 0 && sectionKey.length > 0) {
      selectedSpaceIdBySectionKey[sectionKey] = spaceId;
    }
  }
  return selectedSpaceIdBySectionKey;
}

export function readSidebarUiCollapseState(windowScopeId: string): SidebarUiCollapseStateReadResult {
  if (typeof window === 'undefined') {
    return {
      reason: 'storage-unavailable',
      state: createDefaultSidebarUiCollapseState(),
    };
  }

  try {
    const scopedStoredValue = window.localStorage.getItem(getSidebarUiCollapseStateStorageKey(windowScopeId));
    if (scopedStoredValue !== null) {
      const scopedCandidate = JSON.parse(scopedStoredValue) as { state?: unknown; version?: unknown };
      if (
        !scopedCandidate ||
        typeof scopedCandidate !== 'object' ||
        typeof scopedCandidate.version !== 'number' ||
        !SUPPORTED_SIDEBAR_UI_COLLAPSE_STORAGE_VERSIONS.has(scopedCandidate.version)
      ) {
        return {
          reason: 'invalid-shape',
          state: createDefaultSidebarUiCollapseState(),
          storedByteLength: scopedStoredValue.length,
        };
      }
      return {
        state: normalizeSidebarUiCollapseState(scopedCandidate.state),
        storedByteLength: scopedStoredValue.length,
      };
    }

    const legacyStoredValue = window.localStorage.getItem(SIDEBAR_UI_COLLAPSE_STATE_STORAGE_KEY);
    const candidate = JSON.parse(legacyStoredValue ?? 'null');
    if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
      const state = createDefaultSidebarUiCollapseState();
      state.collapsedProjectCollectionsByKey = Object.fromEntries(
        Object.keys(readLegacyCollapsedSidebarProjectCollectionIds()).map((collectionId) => [
          createLocalProjectCollectionCollapseKey(collectionId),
          true,
        ])
      );
      state.collapsedProjectSessionListsById = readProjectSessionListCollapsedState();
      return { reason: 'missing', state };
    }

    const migrated = normalizeSidebarUiCollapseState(candidate);
    migrated.collapsedProjectCollectionsByKey = Object.fromEntries(
      Object.keys(readLegacyCollapsedSidebarProjectCollectionIds()).map((collectionId) => [
        createLocalProjectCollectionCollapseKey(collectionId),
        true,
      ])
    );
    migrated.collapsedProjectSessionListsById = readProjectSessionListCollapsedState();
    return { state: migrated, storedByteLength: legacyStoredValue?.length ?? 0 };
  } catch {
    return {
      reason: 'parse-error',
      state: createDefaultSidebarUiCollapseState(),
    };
  }
}

export function normalizeStoredCollapsedGroupsById(candidate: unknown): Record<string, true> {
  if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
    return {};
  }

  const collapsedGroupsById: Record<string, true> = {};
  for (const [groupId, collapsed] of Object.entries(candidate)) {
    if (collapsed === true) {
      collapsedGroupsById[groupId] = true;
    }
  }
  return collapsedGroupsById;
}

export function summarizeSidebarUiCollapseState(state: SidebarUiCollapseState): Record<string, unknown> {
  return {
    collapsedGroupCount: Object.keys(state.collapsedGroupsById).length,
    collapsedProjectCollectionCount: Object.keys(state.collapsedProjectCollectionsByKey).length,
    collapsedProjectSessionListCount: Object.keys(state.collapsedProjectSessionListsById).length,
    isReferenceChatsCollapsed: state.isReferenceChatsCollapsed,
    rememberedSpaceSectionCount: Object.keys(state.recentSessionIdsBySpace).length,
    selectedSpaceSectionCount: Object.keys(state.selectedSpaceIdBySectionKey).length,
  };
}

export function summarizeSidebarUiCollapseRead(result: SidebarUiCollapseStateReadResult): Record<string, unknown> {
  return {
    ...summarizeSidebarUiCollapseState(result.state),
    readReason: result.reason ?? 'stored',
    storedByteLength: result.storedByteLength ?? 0,
  };
}

export function writeSidebarUiCollapseState(
  windowScopeId: string,
  state: SidebarUiCollapseState
): SidebarUiCollapseStateWriteResult {
  if (typeof window === 'undefined') {
    return { ok: false, reason: 'storage-unavailable' };
  }

  try {
    const serialized = JSON.stringify({
      state: {
        ...state,
        collapsedProjectSessionSectionsById: persistedProjectSessionSectionCollapseState(
          state.collapsedProjectSessionSectionsById
        ),
      },
      version: 3,
    } satisfies SidebarUiCollapseStorage);
    window.localStorage.setItem(getSidebarUiCollapseStateStorageKey(windowScopeId), serialized);
    return { ok: true, storedByteLength: serialized.length };
  } catch {
    // Ignore storage failures; the in-memory collapse state should still update.
    return { ok: false, reason: 'storage-error' };
  }
}
