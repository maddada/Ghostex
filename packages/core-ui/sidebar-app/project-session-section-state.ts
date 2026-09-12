import { useCallback, useState } from 'react';
import type { SidebarSessionItem } from '../../shared/session-grid-contract';
import { isSidebarSessionSnoozed } from '../../shared/session-snooze';

export type ProjectSessionSection = 'browser' | 'pinned' | 'sessions' | 'parked' | 'snoozed';
export type ProjectSessionSectionCollapseState = Readonly<Record<ProjectSessionSection, boolean>>;
export type ProjectSessionSectionCollapseStateById = Record<string, ProjectSessionSectionCollapseState>;

export const DEFAULT_PROJECT_SESSION_SECTION_COLLAPSE_STATE: ProjectSessionSectionCollapseState = {
  browser: false,
  pinned: false,
  sessions: false,
  parked: true,
  snoozed: true,
};

/**
 * CDXC:Projects 2026-09-06 DECISION:
 * User: remember Pinned, Sessions, and Parked expansion per project when switching Spaces; keep Pinned and Sessions across app restarts, but start Parked collapsed.
 * SidebarApp owns the map because filtering a project out of a Space unmounts its row.
 */
export function useProjectSessionSectionCollapseState(initialState: ProjectSessionSectionCollapseStateById) {
  const [collapsedProjectSessionSectionsById, setState] = useState(initialState);
  const setProjectSessionSectionCollapsed = useCallback(
    (projectId: string, section: ProjectSessionSection, collapsed: boolean) => {
      setState((previous) => {
        const current = previous[projectId] ?? DEFAULT_PROJECT_SESSION_SECTION_COLLAPSE_STATE;
        if (current[section] === collapsed) return previous;
        return { ...previous, [projectId]: { ...current, [section]: collapsed } };
      });
    },
    []
  );
  return { collapsedProjectSessionSectionsById, setProjectSessionSectionCollapsed };
}

export function normalizeProjectSessionSectionCollapseState(
  candidate: unknown
): ProjectSessionSectionCollapseStateById {
  if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) return {};
  return Object.fromEntries(
    Object.entries(candidate).flatMap(([projectId, state]) => {
      if (!state || typeof state !== 'object' || Array.isArray(state)) return [];
      return [
        [
          projectId,
          {
            ...DEFAULT_PROJECT_SESSION_SECTION_COLLAPSE_STATE,
            pinned: state.pinned === true,
            sessions: state.sessions === true,
          },
        ],
      ];
    })
  );
}

export function persistedProjectSessionSectionCollapseState(state: ProjectSessionSectionCollapseStateById) {
  return Object.fromEntries(
    Object.entries(state).map(([projectId, { pinned, sessions }]) => [projectId, { pinned, sessions }])
  );
}

export function getProjectSessionSection(
  session: SidebarSessionItem | undefined,
  enableSessionParking: boolean
): ProjectSessionSection {
  if (session?.kind === 'browser' || session?.sessionKind === 'browser') {
    return 'browser';
  }
  /*
   * CDXC:Sessions 2026-09-12 WHY:
   * Snoozed outranks parked and pinned because the wake time is the stronger statement about when the row matters; the flags stay on the row, so an expired snooze returns the session to Parked or Pinned as it was.
   * Snoozed starts collapsed like Parked, and is not persisted across restarts for the same reason.
   */
  if (isSidebarSessionSnoozed(session)) {
    return 'snoozed';
  }
  if (enableSessionParking && session?.isParked === true) {
    return 'parked';
  }
  return session?.isPinned === true ? 'pinned' : 'sessions';
}
