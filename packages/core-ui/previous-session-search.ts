import type { SidebarPreviousSessionItem, SidebarSessionItem } from '../shared/session-grid-contract';
import { isDefaultSessionSearchTitle } from '../shared/session-grid-contract';
import {
  getEffectiveSidebarSessionTag,
  getSidebarSessionTagLabel,
  sessionMatchesSidebarTagFilters,
  type SidebarSessionTagFilter,
} from '../shared/session-tags';
import { getSessionHistoryCardTitle } from './session-history-card-title';

export type PreviousSessionsModalDayGroup = {
  dayLabel: string;
  sessions: SidebarPreviousSessionItem[];
};

export type FilterPreviousSessionsOptions = {
  sessionTags?: readonly SidebarSessionTagFilter[];
};

type SidebarSearchableSession = Pick<
  SidebarSessionItem,
  'alias' | 'detail' | 'displayTitle' | 'isFavorite' | 'primaryTitle' | 'sessionNumber' | 'sessionTag' | 'terminalTitle'
>;

type SidebarSessionSearchRecord<T extends SidebarSearchableSession> = {
  item: T;
  itemIndex: number;
  searchText: string;
};

export function filterPreviousSessions(
  previousSessions: readonly SidebarPreviousSessionItem[],
  query: string,
  options: FilterPreviousSessionsOptions = {}
): SidebarPreviousSessionItem[] {
  const normalizedQuery = query.trim().toLowerCase();
  const selectedSessionTags = options.sessionTags ?? [];
  const filteredSessions =
    selectedSessionTags.length > 0
      ? previousSessions.filter((session) => sessionMatchesSidebarTagFilters(session, selectedSessionTags))
      : [...previousSessions];
  const dedupedSessions = dedupePreviousSessionsByProjectAndTitle(filteredSessions);

  if (!normalizedQuery) {
    return dedupedSessions;
  }

  return filterSidebarSessionItems(dedupedSessions, query);
}

export function filterPreviousSessionsModalItems(
  previousSessions: readonly SidebarPreviousSessionItem[]
): SidebarPreviousSessionItem[] {
  /**
   * CDXC:PreviousSessions 2026-05-15-09:57
   * The Previous Sessions modal is an agent-session restore surface. Browser
   * page history can still exist in shared storage for compatibility, but the
   * modal must hide web pages so the list only presents agent sessions.
   */
  return previousSessions.filter((session) => !isPreviousSessionWebPage(session));
}

export function removePreviousSessionByHistoryId(
  previousSessions: readonly SidebarPreviousSessionItem[],
  historyId: string
): SidebarPreviousSessionItem[] {
  /*
  CDXC:PreviousSessions 2026-06-04-22:52:
  The full Previous Sessions modal keeps gxserver query results in component state. Delete must remove the clicked row from that modal-owned result page immediately, because native/gxserver deletion is asynchronous and does not send a matching previousSessionsResult request id back to the open modal.
  */
  return previousSessions.filter((session) => session.historyId !== historyId);
}

export function getNextPreviousSessionsModalSelection({
  currentHistoryId,
  direction,
  sessions,
}: {
  currentHistoryId: string | undefined;
  direction: -1 | 1;
  sessions: readonly SidebarPreviousSessionItem[];
}): string | undefined {
  /*
  CDXC:PreviousSessions 2026-06-15-11:26:
  Previous Sessions keyboard navigation is a search-owned selection model. Up/Down wraps through the currently visible result rows while DOM focus stays in the search input, so held arrows repeat and typing continues in the field.
  */
  if (sessions.length === 0) {
    return undefined;
  }

  const currentIndex =
    currentHistoryId === undefined ? -1 : sessions.findIndex((session) => session.historyId === currentHistoryId);
  if (currentIndex === -1) {
    return direction === 1 ? sessions[0]?.historyId : sessions[sessions.length - 1]?.historyId;
  }

  return sessions[(currentIndex + direction + sessions.length) % sessions.length]?.historyId;
}

export function groupPreviousSessionsByDay(
  previousSessions: readonly SidebarPreviousSessionItem[]
): PreviousSessionsModalDayGroup[] {
  /*
  CDXC:PreviousSessions 2026-06-17-17:06:
  The modal's date groups represent when sessions were closed, not when they were last active. Sort by closedAt before grouping so newly closed sessions appear first even when their lastInteractionAt is older than other history rows.
  */
  const formatter = new Intl.DateTimeFormat(undefined, {
    day: 'numeric',
    month: 'long',
    weekday: 'long',
    year: 'numeric',
  });
  const sessionsByDay = new Map<string, SidebarPreviousSessionItem[]>();
  const sortedSessions = sortPreviousSessionsByClosedAt(previousSessions);

  for (const session of sortedSessions) {
    const date = new Date(session.closedAt);
    const key = Number.isNaN(date.getTime()) ? 'Unknown day' : formatter.format(date);
    const grouped = sessionsByDay.get(key);
    if (grouped) {
      grouped.push(session);
      continue;
    }

    sessionsByDay.set(key, [session]);
  }

  return [...sessionsByDay.entries()].map(([dayLabel, sessions]) => ({
    dayLabel,
    sessions,
  }));
}

export function sortPreviousSessionsByClosedAt(
  previousSessions: readonly SidebarPreviousSessionItem[]
): SidebarPreviousSessionItem[] {
  return [...previousSessions].sort(comparePreviousSessionsByClosedAt);
}

export function filterSidebarSessionItems<T extends SidebarSearchableSession>(
  sessions: readonly T[],
  query: string
): T[] {
  const normalizedQuery = normalizeSessionSearchValue(query);
  const searchableSessions = filterDefaultNamedSessionSearchItems(sessions);
  if (!normalizedQuery) {
    return searchableSessions;
  }

  const searchRecords = searchableSessions.map((session, itemIndex) =>
    createSidebarSessionSearchRecord(session, itemIndex)
  );
  const queryTokens = normalizedQuery.split(/\s+/).filter(Boolean);
  const shouldUseAbbreviationMatching = queryTokens.length > 0 && queryTokens.every((token) => token.length <= 3);

  /*
   * CDXC:SidebarSearch 2026-06-28-06:29:
   * Sidebar session search should stay fuzzy enough for short abbreviations
   * and one-character typos, but long terms such as "sidebar" must not match
   * scattered letters across unrelated titles. Match normalized tokens
   * directly against row text instead of using broad Fuse scoring for the
   * main sidebar and Previous Sessions result sets.
   */
  if (shouldUseAbbreviationMatching) {
    return searchableSessions.filter((_, itemIndex) =>
      matchesNormalizedQueryTokens(searchRecords[itemIndex]?.searchText ?? '', queryTokens)
    );
  }

  const matchedItemIndexes = new Set(
    searchRecords
      .filter((record) => matchesNormalizedQueryTokens(record.searchText, queryTokens))
      .map((record) => record.itemIndex)
  );

  return searchableSessions.filter((_, itemIndex) => matchedItemIndexes.has(itemIndex));
}

export function matchesSidebarSessionSearchQuery(session: SidebarSearchableSession, query: string): boolean {
  return filterSidebarSessionItems([session], query).length > 0;
}

export function filterDefaultNamedSessionSearchItems<T extends SidebarSearchableSession>(sessions: readonly T[]): T[] {
  /*
   * CDXC:SessionSearch 2026-06-18-00:01:
   * Sidebar and command-palette session search should not surface placeholder
   * names from supported agent CLIs. Filter at the search helper boundary so
   * current sessions, local previous-session results, and remote previous
   * results share the same exact default-title exclusion.
   */
  return sessions.filter((session) => !isDefaultSessionSearchTitle(getSessionHistoryCardTitle(session)));
}

function createSidebarSessionSearchRecord<T extends SidebarSearchableSession>(
  session: T,
  itemIndex: number
): SidebarSessionSearchRecord<T> {
  return {
    item: session,
    itemIndex,
    searchText: [
      session.alias,
      session.displayTitle,
      session.primaryTitle,
      session.terminalTitle,
      session.detail,
      session.sessionNumber,
      getSidebarSessionTagLabel(getEffectiveSidebarSessionTag(session)),
    ]
      .map((part) => normalizeSessionSearchValue(part))
      .filter(Boolean)
      .join(' '),
  };
}

function normalizeSessionSearchValue(value: string | undefined): string {
  if (!value) {
    return '';
  }

  return value
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/[-_/\\.]+/g, ' ')
    .replace(/[^\p{L}\p{N}]+/gu, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .toLowerCase();
}

function dedupePreviousSessionsByProjectAndTitle(
  sessions: readonly SidebarPreviousSessionItem[]
): SidebarPreviousSessionItem[] {
  /**
   * CDXC:PreviousSessions 2026-05-11-09:04
   * Previous Sessions must not show duplicate historical cards for the same
   * project and session name. Keep only the latest closed item before
   * search so both the modal and sidebar search share the same unique list.
   *
   * CDXC:PreviousSessions 2026-06-17-17:06:
   * Duplicate pruning follows closedAt rather than lastInteractionAt because a
   * just-closed session must remain the restorable row even if an older
   * duplicate had more recent terminal activity.
   */
  const dedupedByKey = new Map<
    string,
    {
      item: SidebarPreviousSessionItem;
      itemIndex: number;
      timestamp: number;
    }
  >();

  sessions.forEach((session, itemIndex) => {
    const key = createPreviousSessionDedupeKey(session);
    const timestamp = getPreviousSessionDedupeTimestamp(session);
    const current = dedupedByKey.get(key);
    if (current && current.timestamp >= timestamp) {
      return;
    }

    dedupedByKey.set(key, {
      item: session,
      itemIndex,
      timestamp,
    });
  });

  return [...dedupedByKey.values()].sort((left, right) => left.itemIndex - right.itemIndex).map((entry) => entry.item);
}

function isPreviousSessionWebPage(session: SidebarPreviousSessionItem): boolean {
  return (
    session.sessionKind === 'browser' || session.sessionRecord?.kind === 'browser' || session.agentIcon === 'browser'
  );
}

function createPreviousSessionDedupeKey(session: SidebarPreviousSessionItem): string {
  const projectKey = normalizeSessionSearchValue(session.projectPath || session.projectId || session.projectName || '');
  const scopedProjectKey = projectKey || `history:${session.historyId}`;
  const titleKey = normalizeSessionSearchValue(getSessionHistoryCardTitle(session));

  return `${scopedProjectKey}\u0000${titleKey}`;
}

function getPreviousSessionDedupeTimestamp(session: SidebarPreviousSessionItem): number {
  const closedTimestamp = parsePreviousSessionTimestamp(session.closedAt);
  const lastInteractionTimestamp = parsePreviousSessionTimestamp(session.lastInteractionAt);

  return closedTimestamp || lastInteractionTimestamp;
}

function comparePreviousSessionsByClosedAt(
  left: SidebarPreviousSessionItem,
  right: SidebarPreviousSessionItem
): number {
  return (
    parsePreviousSessionTimestamp(right.closedAt) - parsePreviousSessionTimestamp(left.closedAt) ||
    left.historyId.localeCompare(right.historyId)
  );
}

function parsePreviousSessionTimestamp(value: string | undefined): number {
  if (!value) {
    return 0;
  }

  const timestamp = Date.parse(value);
  return Number.isNaN(timestamp) ? 0 : timestamp;
}

function matchesNormalizedQueryTokens(searchText: string, queryTokens: readonly string[]): boolean {
  return queryTokens.every((token) => matchesNormalizedQueryToken(searchText, token));
}

function matchesNormalizedQueryToken(searchText: string, query: string): boolean {
  if (query.length <= 3) {
    return fuzzyIncludes(searchText, query);
  }

  if (searchText.includes(query)) {
    return true;
  }

  const compactSearchText = searchText.replace(/\s+/g, '');
  if ((query.length >= 5 && compactSearchText.includes(query)) || hasSingleEditDistance(compactSearchText, query)) {
    return true;
  }

  const words = searchText.split(/\s+/).filter(Boolean);
  return words.some((word) => isLongQueryTypoCandidate(word, query)) || hasAdjacentWordSingleEditDistance(words, query);
}

function fuzzyIncludes(text: string, query: string): boolean {
  let queryIndex = 0;

  for (const character of text) {
    if (character !== query[queryIndex]) {
      continue;
    }

    queryIndex += 1;
    if (queryIndex >= query.length) {
      return true;
    }
  }

  return query.length === 0;
}

function hasSingleEditDistance(candidate: string, query: string): boolean {
  if (candidate === query) {
    return true;
  }

  if (Math.abs(candidate.length - query.length) > 1) {
    return false;
  }

  let candidateIndex = 0;
  let queryIndex = 0;
  let edits = 0;

  while (candidateIndex < candidate.length && queryIndex < query.length) {
    if (candidate[candidateIndex] === query[queryIndex]) {
      candidateIndex += 1;
      queryIndex += 1;
      continue;
    }

    edits += 1;
    if (edits > 1) {
      return false;
    }

    if (candidate.length > query.length) {
      candidateIndex += 1;
    } else if (candidate.length < query.length) {
      queryIndex += 1;
    } else {
      candidateIndex += 1;
      queryIndex += 1;
    }
  }

  return true;
}

function hasAdjacentWordSingleEditDistance(words: readonly string[], query: string): boolean {
  for (let startIndex = 0; startIndex < words.length; startIndex += 1) {
    let joinedWords = '';

    for (let index = startIndex; index < words.length; index += 1) {
      joinedWords += words[index];
      if (joinedWords.length > query.length + 1) {
        break;
      }

      if (isLongQueryTypoCandidate(joinedWords, query)) {
        return true;
      }
    }
  }

  return false;
}

function isLongQueryTypoCandidate(candidate: string, query: string): boolean {
  return candidate.length >= Math.max(4, query.length - 1) && hasSingleEditDistance(candidate, query);
}
