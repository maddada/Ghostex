import { IconCheck, IconFilter2 } from '@tabler/icons-react';
import { createPortal } from 'react-dom';
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { Button } from '@/components/ui/button';
import {
  filterPreviousSessions,
  filterPreviousSessionsModalItems,
  filterSidebarSessionItems,
  removePreviousSessionByHistoryId,
  sortPreviousSessionsByClosedAt,
} from './previous-session-search';
import { SessionHistoryCard } from './session-history-card';
import { useSidebarStore } from './sidebar-store';
import { applyTextEditingKey, isEditableKeyboardTarget, isTextEditingKey } from './text-input-keyboard';
import { TOOLTIP_DELAY_MS } from './tooltip-delay';
import { TooltipProvider } from './app-tooltip';
import { QuickAccessSearchInput } from './quick-access-search-input';
import { QuickAccessHeader } from './quick-access-tabs';
import { getEffectiveSessionTag, SessionTagIcon, type SidebarSessionTag } from './session-tag-ui';
import type { WebviewApi } from './webview-api';
import type {
  ExtensionToSidebarMessage,
  SidebarPreviousSessionItem,
  SidebarSessionItem,
} from '../shared/session-grid-contract';
import { getEnabledVisibleSidebarSessionTagSections } from '../shared/session-tags';

const PREVIOUS_SESSIONS_PAGE_SIZE = 80;
const PREVIOUS_SESSIONS_QUERY_DEBOUNCE_MS = 200;
const PREVIOUS_SESSIONS_SCROLL_LOAD_MORE_THRESHOLD_PX = 96;
const PREVIOUS_SESSIONS_TAG_FILTER_MENU_GAP_PX = 6;
const PREVIOUS_SESSIONS_TAG_FILTER_MENU_MARGIN_PX = 12;
const PREVIOUS_SESSIONS_VISIBLE_WINDOW_MS = 14 * 24 * 60 * 60 * 1_000;
const SESSIONS_SCOPE_TOGGLE_HOTKEY = '⌘⇧C';

type PreviousSessionsRequestMode = 'append' | 'replace';

type QuickAccessSessionItem =
  | {
      groupId: string;
      key: string;
      kind: 'open';
      projectLabel: string;
      session: SidebarSessionItem;
      timestamp: number;
    }
  | {
      key: string;
      kind: 'closed';
      session: SidebarPreviousSessionItem;
      timestamp: number;
    };

type QuickAccessSessionDayGroup = {
  dayLabel: string;
  sessions: QuickAccessSessionItem[];
};

export type PreviousSessionsModalProps = {
  isOpen: boolean;
  onClose: () => void;
  onInitialLoadReady?: () => void;
  shouldPreload?: boolean;
  vscode: WebviewApi;
};

function getPreviousSessionsTagFilterMenuStyle(buttonElement: HTMLButtonElement | null): CSSProperties {
  const bounds = buttonElement?.getBoundingClientRect();
  if (!bounds) {
    return {};
  }

  /*
  CDXC:PreviousSessions 2026-06-05-19:25:
  The tag filter dropdown should open directly below the filter button with its
  right edge aligned to the button. Anchor with `right` instead of a fixed-width
  `left` calculation because the grouped menu can grow wider than its minimum.
  */
  const right = Math.max(PREVIOUS_SESSIONS_TAG_FILTER_MENU_MARGIN_PX, window.innerWidth - bounds.right);
  const top = Math.min(
    bounds.bottom + PREVIOUS_SESSIONS_TAG_FILTER_MENU_GAP_PX,
    window.innerHeight - PREVIOUS_SESSIONS_TAG_FILTER_MENU_MARGIN_PX
  );

  return {
    maxHeight: `calc(100vh - ${top + PREVIOUS_SESSIONS_TAG_FILTER_MENU_MARGIN_PX}px)`,
    maxWidth: `calc(100vw - ${PREVIOUS_SESSIONS_TAG_FILTER_MENU_MARGIN_PX * 2}px)`,
    overflowY: 'auto',
    right: `${right}px`,
    top: `${top}px`,
  };
}

function mergePreviousSessionPages(
  current: readonly SidebarPreviousSessionItem[],
  next: readonly SidebarPreviousSessionItem[]
): SidebarPreviousSessionItem[] {
  const seenHistoryIds = new Set(current.map((session) => session.historyId));
  const merged = [...current];
  for (const session of next) {
    if (seenHistoryIds.has(session.historyId)) {
      continue;
    }
    seenHistoryIds.add(session.historyId);
    merged.push(session);
  }
  return merged;
}

function getPreviousSessionsQueryKey(query: string, sessionTags: readonly SidebarSessionTag[]): string {
  return JSON.stringify([query.trim(), [...sessionTags].sort()]);
}

function parsePreviousSessionClosedAt(session: SidebarPreviousSessionItem): number {
  return parseSessionTimestamp(session.closedAt);
}

function parseSessionTimestamp(value: string | undefined): number {
  if (!value) {
    return 0;
  }
  const timestamp = Date.parse(value);
  return Number.isNaN(timestamp) ? 0 : timestamp;
}

function groupQuickAccessSessionsByDay(sessions: readonly QuickAccessSessionItem[]): QuickAccessSessionDayGroup[] {
  const formatter = new Intl.DateTimeFormat(undefined, {
    day: 'numeric',
    month: 'long',
    weekday: 'long',
    year: 'numeric',
  });
  const sessionsByDay = new Map<string, QuickAccessSessionItem[]>();
  for (const session of sessions) {
    const dayLabel = session.timestamp === 0 ? 'Unknown day' : formatter.format(new Date(session.timestamp));
    const grouped = sessionsByDay.get(dayLabel);
    if (grouped) {
      grouped.push(session);
    } else {
      sessionsByDay.set(dayLabel, [session]);
    }
  }
  return [...sessionsByDay.entries()].map(([dayLabel, daySessions]) => ({
    dayLabel,
    sessions: daySessions,
  }));
}

export function PreviousSessionsModal({
  isOpen,
  onClose,
  onInitialLoadReady,
  shouldPreload = false,
  vscode,
}: PreviousSessionsModalProps) {
  const previousSessions = useSidebarStore((state) => state.previousSessions);
  const groupsById = useSidebarStore((state) => state.groupsById);
  const sessionIdsByGroup = useSidebarStore((state) => state.sessionIdsByGroup);
  const sessionsById = useSidebarStore((state) => state.sessionsById);
  const showDebugSessionNumbers = useSidebarStore((state) => state.hud.debuggingMode);
  const sidebarSessionTagListItems = useSidebarStore((state) => state.hud.settings?.sidebarSessionTagListItems);
  const previousSessionTagFilterSections = useMemo(
    () => getEnabledVisibleSidebarSessionTagSections(sidebarSessionTagListItems),
    [sidebarSessionTagListItems]
  );
  const enabledPreviousSessionTagFilterSet = useMemo(
    () => new Set(previousSessionTagFilterSections.flatMap((section) => section.options.map((option) => option.value))),
    [previousSessionTagFilterSections]
  );
  const [selectedSessionTagFilters, setSelectedSessionTagFilters] = useState<SidebarSessionTag[]>([]);
  const [isTagFilterMenuOpen, setIsTagFilterMenuOpen] = useState(false);
  const [remotePreviousSessions, setRemotePreviousSessions] = useState<SidebarPreviousSessionItem[] | undefined>(
    undefined
  );
  const [remotePreviousSessionsCursor, setRemotePreviousSessionsCursor] = useState<string | undefined>(undefined);
  const [isLoadingMorePreviousSessions, setIsLoadingMorePreviousSessions] = useState(false);
  const [resolvedPreviousSessionsQueryKey, setResolvedPreviousSessionsQueryKey] = useState<string>();
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedSessionKey, setSelectedSessionKey] = useState<string | undefined>(undefined);
  const [showClosedSessionsOnly, setShowClosedSessionsOnly] = useState(false);
  const [visibleHistoryWindowCount, setVisibleHistoryWindowCount] = useState(1);
  const previousSessionsBodyRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const tagFilterButtonRef = useRef<HTMLButtonElement>(null);
  const tagFilterMenuRef = useRef<HTMLDivElement>(null);
  const hasRequestedInitialLoadRef = useRef(false);
  const isLoadingMorePreviousSessionsRef = useRef(false);
  const latestRequestRef = useRef<
    { mode: PreviousSessionsRequestMode; queryKey: string; requestId: string } | undefined
  >(undefined);
  const pendingSelectionRef = useRef<{ end: number; start: number } | undefined>(undefined);
  const selectedSessionKeyRef = useRef<string | undefined>(undefined);
  const visibleHistoryAnchorRef = useRef(Date.now());
  const lastHistoryWindowRevealAtRef = useRef(0);
  const isDataActive = isOpen || shouldPreload;
  const modalPreviousSessions = useMemo(
    () => filterPreviousSessionsModalItems(remotePreviousSessions ?? previousSessions),
    [previousSessions, remotePreviousSessions]
  );
  const hasTagFilters = selectedSessionTagFilters.length > 0;
  const openSessions = useMemo(
    () =>
      Object.entries(sessionIdsByGroup).flatMap(([groupId, sessionIds]) => {
        const group = groupsById[groupId];
        return sessionIds.flatMap((sessionId) => {
          const session = sessionsById[sessionId];
          if (!session) {
            return [];
          }
          return [
            {
              groupId,
              key: `open:${session.sessionId}`,
              kind: 'open' as const,
              projectLabel: group?.title?.trim() ? `Open · ${group.title.trim()}` : 'Open',
              session,
              timestamp: parseSessionTimestamp(session.lastInteractionAt),
            },
          ];
        });
      }),
    [groupsById, sessionIdsByGroup, sessionsById]
  );
  const filteredOpenSessions = useMemo(() => {
    const tagFilteredSessions = hasTagFilters
      ? openSessions.filter((item) => {
          const sessionTag = getEffectiveSessionTag(item.session);
          return sessionTag ? selectedSessionTagFilters.includes(sessionTag) : false;
        })
      : openSessions;
    const matchedSessions = new Set(filterSidebarSessionItems(tagFilteredSessions.map((item) => item.session), searchQuery));
    return tagFilteredSessions.filter((item) => matchedSessions.has(item.session));
  }, [hasTagFilters, openSessions, searchQuery, selectedSessionTagFilters]);
  const filteredClosedSessions = useMemo(
    () =>
      filterPreviousSessions(modalPreviousSessions, searchQuery, {
        sessionTags: selectedSessionTagFilters,
      }),
    [modalPreviousSessions, searchQuery, selectedSessionTagFilters]
  );
  const sortedFilteredClosedSessions = useMemo(
    () => sortPreviousSessionsByClosedAt(filteredClosedSessions),
    [filteredClosedSessions]
  );
  const visibleHistoryCutoff =
    visibleHistoryAnchorRef.current - visibleHistoryWindowCount * PREVIOUS_SESSIONS_VISIBLE_WINDOW_MS;
  const visibleClosedSessions = useMemo(
    () =>
      searchQuery.trim() || hasTagFilters
        ? sortedFilteredClosedSessions
        : sortedFilteredClosedSessions.filter(
            (session) => parsePreviousSessionClosedAt(session) >= visibleHistoryCutoff
          ),
    [hasTagFilters, searchQuery, sortedFilteredClosedSessions, visibleHistoryCutoff]
  );
  const visibleSessionItems = useMemo(
    () =>
      [
        ...(showClosedSessionsOnly ? [] : filteredOpenSessions),
        ...visibleClosedSessions.map((session) => ({
          key: `closed:${session.historyId}`,
          kind: 'closed' as const,
          session,
          timestamp: parsePreviousSessionClosedAt(session),
        })),
      ].sort((left, right) => right.timestamp - left.timestamp || left.key.localeCompare(right.key)),
    [filteredOpenSessions, showClosedSessionsOnly, visibleClosedSessions]
  );
  const groupedSessions = useMemo(
    () => groupQuickAccessSessionsByDay(visibleSessionItems),
    [visibleSessionItems]
  );
  const hasClosedSessionsResolved = remotePreviousSessions !== undefined || previousSessions.length > 0;
  const currentPreviousSessionsQueryKey = useMemo(
    () => getPreviousSessionsQueryKey(searchQuery, selectedSessionTagFilters),
    [searchQuery, selectedSessionTagFilters]
  );
  const hasResolvedCurrentPreviousSessionsQuery =
    hasClosedSessionsResolved && resolvedPreviousSessionsQueryKey === currentPreviousSessionsQueryKey;
  const oldestLoadedSessionClosedAt = useMemo(
    () =>
      modalPreviousSessions.reduce((oldest, session) => {
        const closedAt = parsePreviousSessionClosedAt(session);
        return closedAt > 0 ? Math.min(oldest, closedAt) : oldest;
      }, Number.POSITIVE_INFINITY),
    [modalPreviousSessions]
  );
  const hasLoadedVisibleHistoryWindow =
    hasClosedSessionsResolved &&
    (remotePreviousSessionsCursor === undefined || oldestLoadedSessionClosedAt <= visibleHistoryCutoff);
  const canShowModal = isOpen && (openSessions.length > 0 || hasClosedSessionsResolved);

  const requestPreviousSessionsPage = useCallback(
    (input: { cursor?: string; mode: PreviousSessionsRequestMode }) => {
      if (input.mode === 'append' && !input.cursor) {
        return;
      }
      if (input.mode === 'append' && isLoadingMorePreviousSessionsRef.current) {
        return;
      }

      const requestId = `previous-sessions-${Date.now()}-${Math.random().toString(36).slice(2)}`;
      hasRequestedInitialLoadRef.current = true;
      latestRequestRef.current = {
        mode: input.mode,
        queryKey: currentPreviousSessionsQueryKey,
        requestId,
      };
      if (input.mode === 'append') {
        isLoadingMorePreviousSessionsRef.current = true;
        setIsLoadingMorePreviousSessions(true);
      } else {
        isLoadingMorePreviousSessionsRef.current = false;
        setIsLoadingMorePreviousSessions(false);
        setRemotePreviousSessionsCursor(undefined);
      }
      /*
      CDXC:GxserverPresentationSearch 2026-07-07-16:15:
      The modal uses gxserver's cursor-backed history API as a paged restore
      surface. Keep the cursor opaque in React; native owns merging local and
      remote daemon pages by close time.
      */
      vscode.postMessage({
        cursor: input.cursor,
        limit: PREVIOUS_SESSIONS_PAGE_SIZE,
        query: searchQuery.trim() || undefined,
        requestId,
        sessionTags: selectedSessionTagFilters,
        type: 'requestPreviousSessions',
      });
    },
    [currentPreviousSessionsQueryKey, searchQuery, selectedSessionTagFilters, vscode]
  );

  const revealOlderPreviousSessionsIfNeeded = useCallback(() => {
    const body = previousSessionsBodyRef.current;
    if (!body) {
      return;
    }
    const remainingScrollPx = body.scrollHeight - body.scrollTop - body.clientHeight;
    if (remainingScrollPx > PREVIOUS_SESSIONS_SCROLL_LOAD_MORE_THRESHOLD_PX) {
      return;
    }

    const now = Date.now();
    if (now - lastHistoryWindowRevealAtRef.current < 150) {
      return;
    }
    lastHistoryWindowRevealAtRef.current = now;

    if (!searchQuery.trim() && !hasTagFilters) {
      setVisibleHistoryWindowCount((current) => current + 1);
      return;
    }
    if (!remotePreviousSessionsCursor || isLoadingMorePreviousSessions) {
      return;
    }
    requestPreviousSessionsPage({
      cursor: remotePreviousSessionsCursor,
      mode: 'append',
    });
  }, [
    hasTagFilters,
    isLoadingMorePreviousSessions,
    remotePreviousSessionsCursor,
    requestPreviousSessionsPage,
    searchQuery,
  ]);

  const activateQuickAccessSession = useCallback(
    (item: QuickAccessSessionItem) => {
      if (item.kind === 'open') {
        useSidebarStore.getState().applyLocalFocus(item.groupId, item.session.sessionId);
        vscode.postMessage({
          sessionId: item.session.sessionId,
          type: 'focusSession',
        });
      } else {
        if (!item.session.isRestorable) {
          return;
        }
        vscode.postMessage({
          historyId: item.session.historyId,
          type: 'restorePreviousSession',
        });
      }
      onClose();
    },
    [onClose, vscode]
  );

  const selectSessionByKeyboard = useCallback((direction: -1 | 1) => {
    if (visibleSessionItems.length === 0) {
      return false;
    }

    const currentIndex = selectedSessionKeyRef.current
      ? visibleSessionItems.findIndex((item) => item.key === selectedSessionKeyRef.current)
      : -1;
    const nextIndex =
      currentIndex < 0
        ? direction === 1
          ? 0
          : visibleSessionItems.length - 1
        : (currentIndex + direction + visibleSessionItems.length) % visibleSessionItems.length;
    const nextSessionKey = visibleSessionItems[nextIndex]?.key;
    if (!nextSessionKey) {
      return false;
    }

    selectedSessionKeyRef.current = nextSessionKey;
    setSelectedSessionKey(nextSessionKey);
    searchInputRef.current?.focus({ preventScroll: true });
    return true;
  }, [visibleSessionItems]);

  const toggleClosedSessionsOnly = useCallback(() => {
    setShowClosedSessionsOnly((current) => !current);
    searchInputRef.current?.focus({ preventScroll: true });
  }, []);

  const openTagFilterMenu = () => {
    const bounds = tagFilterButtonRef.current?.getBoundingClientRect();
    if (!bounds) {
      setIsTagFilterMenuOpen((previous) => !previous);
      return;
    }
    setIsTagFilterMenuOpen(true);
  };

  const toggleSessionTagFilter = (sessionTag: SidebarSessionTag) => {
    if (!enabledPreviousSessionTagFilterSet.has(sessionTag)) {
      return;
    }
    setSelectedSessionTagFilters((current) =>
      current.includes(sessionTag) ? current.filter((tag) => tag !== sessionTag) : [...current, sessionTag]
    );
    searchInputRef.current?.focus({ preventScroll: true });
  };

  useEffect(() => {
    /*
     * CDXC:SessionTagFilters 2026-06-15-22:33:
     * Previous Sessions tag filters mirror the Settings-managed sidebar tag
     * list. If Reset to Default or another settings change disables a selected
     * tag, clear that stale filter before the next local or gxserver query.
     */
    setSelectedSessionTagFilters((current) => {
      const next = current.filter((tag) => enabledPreviousSessionTagFilterSet.has(tag));
      return next.length === current.length ? current : next;
    });
  }, [enabledPreviousSessionTagFilterSet]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (
        event.metaKey &&
        event.shiftKey &&
        !event.altKey &&
        !event.ctrlKey &&
        event.key.toLocaleLowerCase() === 'c'
      ) {
        event.preventDefault();
        event.stopPropagation();
        toggleClosedSessionsOnly();
        return;
      }

      if (event.key === 'Escape') {
        if (isTagFilterMenuOpen) {
          event.preventDefault();
          event.stopPropagation();
          setIsTagFilterMenuOpen(false);
          return;
        }
        onClose();
        return;
      }

      const searchInput = searchInputRef.current;
      const isSearchInputTarget = event.target === searchInput;
      if (
        searchInput &&
        !event.altKey &&
        !event.ctrlKey &&
        !event.metaKey &&
        (isSearchInputTarget || !isEditableKeyboardTarget(event.target)) &&
        (event.key === 'ArrowDown' || event.key === 'ArrowUp')
      ) {
        /*
        CDXC:PreviousSessions 2026-06-15-11:26:
        The modal search field remains the focused text owner while Up/Down walks the visible previous-session rows. Keep selection in React state instead of focusing row buttons so held arrows repeat normally and the next typed character still lands in search.
        */
        if (!selectSessionByKeyboard(event.key === 'ArrowUp' ? -1 : 1)) {
          return;
        }

        event.preventDefault();
        event.stopPropagation();
        return;
      }

      if (
        event.key === 'Enter' &&
        !event.altKey &&
        !event.ctrlKey &&
        !event.metaKey &&
        !event.shiftKey &&
        (isSearchInputTarget || !isEditableKeyboardTarget(event.target))
      ) {
        const selectedSession = visibleSessionItems.find((item) => item.key === selectedSessionKeyRef.current);
        if (selectedSession) {
          event.preventDefault();
          event.stopPropagation();
          activateQuickAccessSession(selectedSession);
          return;
        }
      }

      if (!searchInput || isSearchInputTarget || isEditableKeyboardTarget(event.target) || !isTextEditingKey(event)) {
        return;
      }

      const nextSearchState = applyTextEditingKey(
        {
          selectionEnd: searchInput.selectionEnd,
          selectionStart: searchInput.selectionStart,
          value: searchInput.value,
        },
        event.key,
        event
      );
      if (!nextSearchState) {
        return;
      }

      event.preventDefault();
      pendingSelectionRef.current = {
        end: nextSearchState.selectionEnd,
        start: nextSearchState.selectionStart,
      };
      searchInput.focus();
      setSearchQuery(nextSearchState.value);
    };

    document.addEventListener('keydown', handleKeyDown, true);
    return () => {
      document.removeEventListener('keydown', handleKeyDown, true);
    };
  }, [activateQuickAccessSession, isOpen, isTagFilterMenuOpen, onClose, selectSessionByKeyboard, toggleClosedSessionsOnly, visibleSessionItems]);

  useEffect(() => {
    selectedSessionKeyRef.current = selectedSessionKey;
  }, [selectedSessionKey]);

  useEffect(() => {
    if (!selectedSessionKey) {
      return;
    }

    if (visibleSessionItems.some((session) => session.key === selectedSessionKey)) {
      return;
    }

    selectedSessionKeyRef.current = undefined;
    setSelectedSessionKey(undefined);
  }, [selectedSessionKey, visibleSessionItems]);

  useEffect(() => {
    if (!isTagFilterMenuOpen) {
      return;
    }

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) {
        return;
      }
      if (tagFilterButtonRef.current?.contains(target) || tagFilterMenuRef.current?.contains(target)) {
        return;
      }
      setIsTagFilterMenuOpen(false);
    };

    document.addEventListener('pointerdown', handlePointerDown, true);
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown, true);
    };
  }, [isTagFilterMenuOpen]);

  useEffect(() => {
    if (!isOpen) {
      setSelectedSessionTagFilters([]);
      setIsTagFilterMenuOpen(false);
      setSearchQuery('');
      setShowClosedSessionsOnly(false);
      visibleHistoryAnchorRef.current = Date.now();
      setVisibleHistoryWindowCount(1);
      lastHistoryWindowRevealAtRef.current = 0;
      pendingSelectionRef.current = undefined;
      selectedSessionKeyRef.current = undefined;
      setSelectedSessionKey(undefined);
    }
  }, [isOpen]);

  useEffect(() => {
    if (isDataActive) {
      return;
    }
    isLoadingMorePreviousSessionsRef.current = false;
    setIsLoadingMorePreviousSessions(false);
    latestRequestRef.current = undefined;
  }, [isDataActive]);

  useEffect(() => {
    if (!isDataActive) {
      return;
    }
    const handleMessage = (event: MessageEvent<ExtensionToSidebarMessage>) => {
      if (event.data.type !== 'previousSessionsResult') {
        return;
      }
      const resultMessage = event.data;
      if (resultMessage.requestId !== latestRequestRef.current?.requestId) {
        return;
      }
      const latestRequest = latestRequestRef.current;
      const requestMode = latestRequest.mode;
      if (requestMode === 'append') {
        setRemotePreviousSessions((current) =>
          mergePreviousSessionPages(current ?? [], resultMessage.previousSessions)
        );
      } else {
        setRemotePreviousSessions(resultMessage.previousSessions);
        setResolvedPreviousSessionsQueryKey(latestRequest.queryKey);
        if (latestRequest.queryKey === getPreviousSessionsQueryKey('', [])) {
          visibleHistoryAnchorRef.current = Date.now();
          setVisibleHistoryWindowCount(1);
        }
      }
      setRemotePreviousSessionsCursor(resultMessage.cursor);
      isLoadingMorePreviousSessionsRef.current = false;
      setIsLoadingMorePreviousSessions(false);
    };
    window.addEventListener('message', handleMessage);
    return () => {
      window.removeEventListener('message', handleMessage);
    };
  }, [isDataActive]);

  useEffect(() => {
    if (!isDataActive) {
      return;
    }
    const requestDelay = hasRequestedInitialLoadRef.current ? PREVIOUS_SESSIONS_QUERY_DEBOUNCE_MS : 0;
    const timeoutId = window.setTimeout(() => {
      /*
      CDXC:GxserverPresentationSearch 2026-06-01-15:08:
      Previous Sessions no longer depends on a startup-hydrated history array. Request recent/history metadata from gxserver on open and debounce typed search at 200ms so the modal remains bounded by current query results.
      */
      requestPreviousSessionsPage({ mode: 'replace' });
    }, requestDelay);
    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [isDataActive, requestPreviousSessionsPage]);

  useEffect(() => {
    if (
      !isDataActive ||
      searchQuery.trim() ||
      hasTagFilters ||
      !hasClosedSessionsResolved ||
      hasLoadedVisibleHistoryWindow ||
      !remotePreviousSessionsCursor ||
      isLoadingMorePreviousSessions
    ) {
      return;
    }
    requestPreviousSessionsPage({
      cursor: remotePreviousSessionsCursor,
      mode: 'append',
    });
  }, [
    hasClosedSessionsResolved,
    hasLoadedVisibleHistoryWindow,
    hasTagFilters,
    isDataActive,
    isLoadingMorePreviousSessions,
    remotePreviousSessionsCursor,
    requestPreviousSessionsPage,
    searchQuery,
  ]);

  useEffect(() => {
    if (!isOpen || (openSessions.length === 0 && !hasResolvedCurrentPreviousSessionsQuery)) {
      return;
    }
    onInitialLoadReady?.();
  }, [hasResolvedCurrentPreviousSessionsQuery, isOpen, onInitialLoadReady, openSessions.length]);

  useEffect(() => {
    if (!canShowModal) {
      return;
    }

    const timeoutId = window.setTimeout(() => {
      const input = searchInputRef.current;
      if (!input) {
        return;
      }

      input.focus();
      const selectionIndex = input.value.length;
      input.setSelectionRange(selectionIndex, selectionIndex);
    }, 0);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [canShowModal]);

  useEffect(() => {
    if (!canShowModal) {
      pendingSelectionRef.current = undefined;
      return;
    }

    const pendingSelection = pendingSelectionRef.current;
    if (!pendingSelection) {
      return;
    }

    const input = searchInputRef.current;
    if (!input) {
      return;
    }

    pendingSelectionRef.current = undefined;
    input.focus();
    input.setSelectionRange(pendingSelection.start, pendingSelection.end);
  }, [canShowModal, searchQuery]);

  useEffect(() => {
    if (!canShowModal || !selectedSessionKey) {
      return;
    }

    const animationFrame = window.requestAnimationFrame(() => {
      const selectedElement = Array.from(
        document.querySelectorAll<HTMLElement>('.previous-sessions-modal [data-quick-access-session-key]')
      ).find((element) => element.dataset.quickAccessSessionKey === selectedSessionKey);
      selectedElement?.scrollIntoView({ block: 'nearest' });
      searchInputRef.current?.focus({ preventScroll: true });
    });

    return () => {
      window.cancelAnimationFrame(animationFrame);
    };
  }, [canShowModal, selectedSessionKey]);

  if (!isOpen) {
    return null;
  }

  return createPortal(
    <TooltipProvider delayDuration={TOOLTIP_DELAY_MS}>
      <div className='confirm-modal-root scroll-mask-y' role='presentation'>
        <button className='confirm-modal-backdrop' onClick={onClose} type='button' />
        <div
          aria-label='Ghostex Quick Access'
          aria-modal='true'
          className='confirm-modal ghostex-settings-shadcn previous-sessions-modal quick-access-surface scroll-mask-y'
          role='dialog'
        >
          <QuickAccessHeader activeTab='recentSessions' />
          <div className='previous-sessions-toolbar'>
            <QuickAccessSearchInput
              ariaLabel='Search sessions'
              clearLabel='Clear session search'
              inputRef={searchInputRef}
              placeholder='Search sessions...'
              query={searchQuery}
              setQuery={setSearchQuery}
              trailingControl={
                <>
                  <button
                    aria-label={`${showClosedSessionsOnly ? 'Show all sessions' : 'Show closed sessions'} (${SESSIONS_SCOPE_TOGGLE_HOTKEY})`}
                    aria-pressed={showClosedSessionsOnly}
                    className='quick-access-session-scope-toggle'
                    data-selected={String(showClosedSessionsOnly)}
                    onClick={(event) => {
                      event.stopPropagation();
                      toggleClosedSessionsOnly();
                    }}
                    onMouseDown={(event) => {
                      event.preventDefault();
                    }}
                    type='button'
                  >
                    <span>{showClosedSessionsOnly ? 'Closed Sessions' : 'All Sessions'}</span>
                    <kbd>{SESSIONS_SCOPE_TOGGLE_HOTKEY}</kbd>
                  </button>
                  {/*
                   * CDXC:PreviousSessions 2026-06-13-15:59:
                   * The tag filter belongs inside the search field's right-side icon slot so the search box can span the modal evenly from left to right instead of reserving a separate external action column.
                   */}
                  <button
                    aria-expanded={isTagFilterMenuOpen}
                    aria-haspopup='menu'
                    aria-label={
                      hasTagFilters
                        ? `Filter sessions by ${selectedSessionTagFilters.length} tags`
                        : 'Filter sessions by tag'
                    }
                    className='previous-sessions-favorites-toggle previous-sessions-tag-filter-toggle'
                    data-selected={String(hasTagFilters)}
                    onClick={(event) => {
                      event.stopPropagation();
                      if (isTagFilterMenuOpen) {
                        setIsTagFilterMenuOpen(false);
                        return;
                      }
                      openTagFilterMenu();
                    }}
                    onMouseDown={(event) => {
                      event.preventDefault();
                    }}
                    ref={tagFilterButtonRef}
                    type='button'
                  >
                    <IconFilter2 aria-hidden='true' className='toolbar-tabler-icon' stroke={1.8} />
                  </button>
                </>
              }
            />
            {isTagFilterMenuOpen
              ? createPortal(
                  <div
                    aria-label='Session tag filters'
                    className='session-context-menu previous-sessions-tag-filter-menu'
                    ref={tagFilterMenuRef}
                    role='menu'
                    style={getPreviousSessionsTagFilterMenuStyle(tagFilterButtonRef.current)}
                  >
                    {/*
                     * CDXC:SessionTags 2026-06-05-12:30:
                     * Previous Sessions supports selecting one or more session
                     * tags, matching the active sidebar filter semantics. Empty
                     * selection means all tags and untagged sessions are shown.
                     *
                     * CDXC:SessionTagFilters 2026-06-16-00:05:
                     * Shared tag context menus omit Priority, Progress, and Type
                     * heading rows while preserving section order and dividers.
                     */}
                    {previousSessionTagFilterSections.map((section) => (
                      <div className='session-tag-menu-section' key={section.label}>
                        {section.options.map((option) => {
                          const isSelected = selectedSessionTagFilters.includes(option.value);
                          return (
                            <button
                              aria-checked={isSelected}
                              className='session-context-menu-item previous-sessions-tag-filter-item'
                              data-selected={String(isSelected)}
                              key={option.value}
                              onClick={() => toggleSessionTagFilter(option.value)}
                              role='menuitemcheckbox'
                              type='button'
                            >
                              <SessionTagIcon
                                className='session-context-menu-icon session-tag-colored-icon'
                                fillFavorite
                                size={14}
                                stroke={1.8}
                                tag={option.value}
                              />
                              {option.label}
                              <IconCheck
                                aria-hidden='true'
                                className='session-context-menu-trailing-icon previous-sessions-tag-filter-check'
                                data-visible={String(isSelected)}
                                size={14}
                                stroke={2}
                              />
                            </button>
                          );
                        })}
                      </div>
                    ))}
                  </div>,
                  document.body
                )
              : null}
          </div>
          <div
            className='previous-sessions-modal-body scroll-mask-y'
            onScroll={revealOlderPreviousSessionsIfNeeded}
            onWheel={(event) => {
              const body = previousSessionsBodyRef.current;
              if (
                event.deltaY > 0 &&
                body &&
                body.scrollHeight <= body.clientHeight + PREVIOUS_SESSIONS_SCROLL_LOAD_MORE_THRESHOLD_PX
              ) {
                revealOlderPreviousSessionsIfNeeded();
              }
            }}
            ref={previousSessionsBodyRef}
          >
            {groupedSessions.length > 0 ? (
              groupedSessions.map((group) => (
                <section className='previous-sessions-day-group' key={group.dayLabel}>
                  <div className='previous-sessions-day-label'>{group.dayLabel}</div>
                  <div className='group-sessions'>
                    {group.sessions.map((item) => (
                      <SessionHistoryCard
                        displayTimestamp={
                          item.kind === 'closed' ? item.session.closedAt : item.session.lastInteractionAt
                        }
                        isSearchSelected={selectedSessionKey === item.key}
                        key={item.key}
                        onDelete={
                          item.kind === 'closed'
                            ? () => {
                                setRemotePreviousSessions((current) =>
                                  removePreviousSessionByHistoryId(
                                    current ?? modalPreviousSessions,
                                    item.session.historyId
                                  )
                                );
                                searchInputRef.current?.focus({ preventScroll: true });
                                vscode.postMessage({
                                  historyId: item.session.historyId,
                                  type: 'deletePreviousSession',
                                });
                              }
                            : undefined
                        }
                        onPointerMove={() => {
                          selectedSessionKeyRef.current = item.key;
                          setSelectedSessionKey(item.key);
                        }}
                        onRestore={() => activateQuickAccessSession(item)}
                        projectLabel={item.kind === 'open' ? item.projectLabel : undefined}
                        quickAccessSessionKey={item.key}
                        session={item.session}
                        showDebugSessionNumbers={showDebugSessionNumbers}
                      />
                    ))}
                  </div>
                </section>
              ))
            ) : hasResolvedCurrentPreviousSessionsQuery ? (
              <div className='group-empty-state previous-sessions-empty-state'>
                {searchQuery.trim()
                  ? hasTagFilters
                    ? `No tagged ${showClosedSessionsOnly ? 'closed ' : ''}sessions match that search.`
                    : `No ${showClosedSessionsOnly ? 'closed ' : ''}sessions match that search.`
                  : hasTagFilters
                    ? `No ${showClosedSessionsOnly ? 'closed ' : ''}sessions match those tags.`
                    : `No ${showClosedSessionsOnly ? 'closed ' : ''}sessions yet.`}
              </div>
            ) : null}
          </div>
          {/*
           * CDXC:PreviousSessions 2026-06-13-01:09:
           * Previous Sessions is now a browse, filter, restore, and delete modal only. Do not render footer launch buttons here, and do not expose the removed agent-prompt search workflow from this surface.
           */}
        </div>
      </div>
    </TooltipProvider>,
    document.body
  );
}
