import {
  clampProjectSessionListCollapsedCount,
  DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
} from '../shared/ghostex-settings';

export const PROJECT_SESSION_LIST_COLLAPSED_COUNT = DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT;
export const PROJECT_SESSION_LIST_REFERENCE_ROW_HEIGHT_PX = 34;
export const PROJECT_SESSION_LIST_REFERENCE_ROW_BOUNDARY_PX = 1;
export const PROJECT_SESSION_LIST_COLLAPSED_STORAGE_KEY = 'ghostex-sidebar-project-session-list-collapsed';
export const PROJECT_SESSION_LIST_COLLAPSED_CHANGED_EVENT = 'ghostex-sidebar-project-session-list-collapsed-changed';

export type ProjectSessionListCollapsedState = Record<string, true>;

export function normalizeStoredProjectSessionListCollapsedState(candidate: unknown): ProjectSessionListCollapsedState {
  if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
    return {};
  }

  const collapsedState: ProjectSessionListCollapsedState = {};
  for (const [projectId, isCollapsed] of Object.entries(candidate)) {
    if (projectId.trim().length > 0 && isCollapsed === true) {
      collapsedState[projectId] = true;
    }
  }
  return collapsedState;
}

export function getVisibleProjectSessionIds({
  collapsedCount = PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  isCollapsed,
  isProjectGroup,
  isToggleEnabled,
  sessionIds,
}: {
  collapsedCount?: number;
  isCollapsed: boolean;
  isProjectGroup: boolean;
  isToggleEnabled: boolean;
  sessionIds: readonly string[];
}): readonly string[] {
  const normalizedCollapsedCount = clampProjectSessionListCollapsedCount(collapsedCount);
  /**
   * CDXC:ProjectSessionLists 2026-05-16-21:50:
   * Project groups with more than the collapsed-count threshold need a
   * per-project Show less / Show more toggle. Default to all sessions, and
   * only trim rendering after the user explicitly collapses that project list.
   *
   * CDXC:ProjectSessionLists 2026-05-26-22:27:
   * Show less must be literal: render only the first collapsed-count sessions
   * in project order. Live zmx-backed rows still remain in sidebar inventory
   * and Show more, but they must not expand the collapsed card list past the
   * user-requested cap.
   *
   * CDXC:ProjectSessionLists 2026-06-10-13:39:
   * The collapsed cap is now Settings-owned, but the default remains six. Use the normalized cap for both row rendering and shortcut slot calculations so Show less can mean ten or another configured count without diverging from the visible sidebar.
   */
  if (!isProjectGroup || !isToggleEnabled || !isCollapsed || sessionIds.length <= normalizedCollapsedCount) {
    return sessionIds;
  }

  return sessionIds.slice(0, normalizedCollapsedCount);
}

export function getExpandedProjectSessionListScrollHeight({ rowCount }: { rowCount: number }): number {
  /*
   * CDXC:ProjectSessionLists 2026-06-30-12:55:
   * Reference sidebar session rows paint a 34px card plus a 1px row boundary in
   * the actual list stack. Size the expanded project inner scroller from that
   * full row step so a configured Show more count of 10 reveals ten complete
   * rows at the top and bottom without reintroducing per-list DOM measurement.
   */
  const normalizedRowCount = Math.max(0, Math.floor(rowCount));
  return (
    normalizedRowCount * (PROJECT_SESSION_LIST_REFERENCE_ROW_HEIGHT_PX + PROJECT_SESSION_LIST_REFERENCE_ROW_BOUNDARY_PX)
  );
}

export function getProjectSessionListBoundaryHeight({
  boundarySessionId,
  includeMoreToggle = false,
  sessionListElement,
}: {
  boundarySessionId: string | undefined;
  includeMoreToggle?: boolean;
  sessionListElement: HTMLElement | null | undefined;
}): number | undefined {
  if (!sessionListElement) {
    return undefined;
  }

  /**
   * CDXC:ProjectSessionLists 2026-06-12-23:53:
   * Show more and Show less should use the same measured max-height motion as project expand/collapse. Measure the bottom of the last user-visible session row so Show less can clip the still-mounted overflow rows smoothly instead of removing them before the collapse animation can run.
   *
   * CDXC:ProjectSessionLists 2026-06-30-02:45:
   * Collapsed Show more rows still need DOM measurement because the synthetic
   * reveal row participates in the clipped edge. Expanded lists use fixed row
   * math instead so large projects avoid layout reads and ResizeObserver work.
   */
  const sessionElement = boundarySessionId
    ? Array.from(sessionListElement.querySelectorAll<HTMLElement>('[data-sidebar-session-id]')).find(
        (element) => element.dataset.sidebarSessionId === boundarySessionId
      )
    : undefined;
  const rowElement = sessionElement?.closest<HTMLElement>('.session-frame') ?? sessionElement;
  const listBounds = sessionListElement.getBoundingClientRect();
  const rowBounds = rowElement?.getBoundingClientRect();
  /**
   * CDXC:ProjectSessionLists 2026-06-13-19:49:
   * A collapsed project list now shows a regular session-card reveal row after
   * the last visible session. Include that row in the measured clipped height
   * so it stays visible as the bottom of the collapsed list.
   */
  const moreToggleElement = includeMoreToggle
    ? sessionListElement.querySelector<HTMLElement>("[data-project-session-list-more-toggle='true']")
    : null;
  if (!rowBounds && !moreToggleElement) {
    return boundarySessionId ? undefined : 0;
  }
  const clippedBottom = Math.max(
    rowBounds?.bottom ?? listBounds.top,
    moreToggleElement?.getBoundingClientRect().bottom ?? rowBounds?.bottom ?? listBounds.top
  );
  return Math.max(0, Math.ceil(clippedBottom - listBounds.top));
}

export function getProjectSessionListCollapsedHeight({
  lastVisibleSessionId,
  sessionListElement,
}: {
  lastVisibleSessionId: string | undefined;
  sessionListElement: HTMLElement | null | undefined;
}): number | undefined {
  return getProjectSessionListBoundaryHeight({
    boundarySessionId: lastVisibleSessionId,
    includeMoreToggle: true,
    sessionListElement,
  });
}

export function readProjectSessionListCollapsedState(
  storage: Pick<Storage, 'getItem'> | undefined = typeof localStorage === 'undefined' ? undefined : localStorage
): ProjectSessionListCollapsedState {
  if (!storage) {
    return {};
  }

  try {
    return normalizeStoredProjectSessionListCollapsedState(
      JSON.parse(storage.getItem(PROJECT_SESSION_LIST_COLLAPSED_STORAGE_KEY) ?? 'null')
    );
  } catch {
    return {};
  }
}

export function writeProjectSessionListCollapsedState(state: ProjectSessionListCollapsedState): void {
  /**
   * CDXC:ProjectSessionLists 2026-05-16-21:50:
   * Show less / Show more is per-project navigation state, not session data.
   * Persist only the collapsed project ids so new projects and projects the
   * user has never collapsed continue to start with all sessions shown.
   *
   * CDXC:WorktreeProjectOrder 2026-06-02-15:27:
   * gxserver owns worktree creation, but the macOS sidebar owns the local Show less state for the source project after submit. Broadcast same-document updates because localStorage storage events do not fire in the writing webview.
   *
   * CDXC:ProjectSessionLists 2026-06-05-20:53:
   * Cmd+number session slots and project row rendering must share the same Show less / Show more state so shortcuts target the sessions currently visible in the sidebar, not hidden rows from a collapsed project list.
   */
  localStorage.setItem(PROJECT_SESSION_LIST_COLLAPSED_STORAGE_KEY, JSON.stringify(state));
  window.dispatchEvent(new Event(PROJECT_SESSION_LIST_COLLAPSED_CHANGED_EVENT));
}
