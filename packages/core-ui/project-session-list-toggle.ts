import {
  clampProjectSessionListCollapsedCount,
  DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
} from '../shared/ghostex-settings';

export const PROJECT_SESSION_LIST_COMPACT_COUNT = DEFAULT_PROJECT_SESSION_LIST_COLLAPSED_COUNT;

/**
 * CDXC:Projects 2026-09-12 DECISION:
 * User: a project's session list has two modes. Compact shows only the first N rows (Compact Session Rows, 13 by default, up to 50) plus a "Show all" row; Full shows every row at natural height and the sidebar is the only scroller.
 * Compact is the default for every project, so this map records only the projects the user switched to Full; a project that was never switched, or that has no more rows than the cap, stays Compact.
 * This supersedes the 2026-05-16 Show more / Show less model, which stored collapsed projects and turned the expanded body into a bounded inner scroller.
 */
export type ProjectSessionListExpandedState = Record<string, true>;

export function getVisibleProjectSessionIds({
  compactCount = PROJECT_SESSION_LIST_COMPACT_COUNT,
  isExpanded,
  isProjectGroup,
  isSessionInCollapsedSection,
  isToggleEnabled,
  sessionIds,
}: {
  compactCount?: number;
  isExpanded: boolean;
  isProjectGroup: boolean;
  isSessionInCollapsedSection?: (sessionId: string) => boolean;
  isToggleEnabled: boolean;
  sessionIds: readonly string[];
}): readonly string[] {
  const normalizedCompactCount = clampProjectSessionListCollapsedCount(compactCount);
  /**
   * CDXC:Projects 2026-09-12 DECISION:
   * User: rows inside a collapsed section (Pinned, Browser, Parked, ...) do not count toward the Compact cap, so ten pinned sessions under a collapsed Pinned heading leave all N Compact rows for the sessions that are actually on screen. They start counting the moment their section is expanded.
   * The result is therefore the list of rows that own a visible card: sessions in collapsed sections are never in it, and in Compact mode it stops after the cap.
   */
  const countableSessionIds = isSessionInCollapsedSection
    ? sessionIds.filter((sessionId) => !isSessionInCollapsedSection(sessionId))
    : sessionIds;
  if (!isProjectGroup || !isToggleEnabled || isExpanded || countableSessionIds.length <= normalizedCompactCount) {
    return countableSessionIds;
  }

  return countableSessionIds.slice(0, normalizedCompactCount);
}
