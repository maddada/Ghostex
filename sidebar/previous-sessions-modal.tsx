import { IconCheck, IconFilter2 } from '@tabler/icons-react';
import { createPortal } from 'react-dom';
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { Button } from '@/components/ui/button';
import {
  filterPreviousSessions,
  filterPreviousSessionsModalItems,
  getNextPreviousSessionsModalSelection,
  groupPreviousSessionsByDay,
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
import { SessionTagIcon, type SidebarSessionTag } from './session-tag-ui';
import type { WebviewApi } from './webview-api';
import type { ExtensionToSidebarMessage, SidebarPreviousSessionItem } from '../shared/session-grid-contract';
import { getEnabledVisibleSidebarSessionTagSections } from '../shared/session-tags';

const PREVIOUS_SESSIONS_INITIAL_LOAD_TIMEOUT_MS = 2_000;
const PREVIOUS_SESSIONS_PAGE_SIZE = 80;
const PREVIOUS_SESSIONS_QUERY_DEBOUNCE_MS = 200;
const PREVIOUS_SESSIONS_SCROLL_LOAD_MORE_THRESHOLD_PX = 96;
const PREVIOUS_SESSIONS_TAG_FILTER_MENU_GAP_PX = 6;
const PREVIOUS_SESSIONS_TAG_FILTER_MENU_MARGIN_PX = 12;

type PreviousSessionsRequestMode = 'append' | 'replace';

export type PreviousSessionsModalProps = {
  isOpen: boolean;
  onClose: () => void;
  onInitialLoadReady?: () => void;
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

export function PreviousSessionsModal({ isOpen, onClose, onInitialLoadReady, vscode }: PreviousSessionsModalProps) {
  const previousSessions = useSidebarStore((state) => state.previousSessions);
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
  const [hasInitialLoadResolved, setHasInitialLoadResolved] = useState(false);
  const [hasInitialLoadTimedOut, setHasInitialLoadTimedOut] = useState(false);
  const [resolvedPreviousSessionsQueryKey, setResolvedPreviousSessionsQueryKey] = useState<string>();
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedHistoryId, setSelectedHistoryId] = useState<string | undefined>(undefined);
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
  const selectedHistoryIdRef = useRef<string | undefined>(undefined);
  const modalPreviousSessions = useMemo(
    () => filterPreviousSessionsModalItems(remotePreviousSessions ?? previousSessions),
    [previousSessions, remotePreviousSessions]
  );
  const filteredSessions = useMemo(
    () =>
      filterPreviousSessions(modalPreviousSessions, searchQuery, {
        sessionTags: selectedSessionTagFilters,
      }),
    [modalPreviousSessions, searchQuery, selectedSessionTagFilters]
  );
  const visibleSessions = useMemo(() => sortPreviousSessionsByClosedAt(filteredSessions), [filteredSessions]);
  const groupedSessions = useMemo(() => groupPreviousSessionsByDay(visibleSessions), [visibleSessions]);
  const canShowModal = isOpen && (hasInitialLoadResolved || hasInitialLoadTimedOut);
  const hasTagFilters = selectedSessionTagFilters.length > 0;
  const currentPreviousSessionsQueryKey = useMemo(
    () => getPreviousSessionsQueryKey(searchQuery, selectedSessionTagFilters),
    [searchQuery, selectedSessionTagFilters]
  );
  const hasResolvedCurrentPreviousSessionsQuery =
    hasInitialLoadResolved && resolvedPreviousSessionsQueryKey === currentPreviousSessionsQueryKey;

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

  const requestMorePreviousSessionsIfNeeded = useCallback(() => {
    if (!remotePreviousSessionsCursor || isLoadingMorePreviousSessions) {
      return;
    }
    const body = previousSessionsBodyRef.current;
    if (!body) {
      return;
    }
    const remainingScrollPx = body.scrollHeight - body.scrollTop - body.clientHeight;
    if (remainingScrollPx > PREVIOUS_SESSIONS_SCROLL_LOAD_MORE_THRESHOLD_PX) {
      return;
    }
    requestPreviousSessionsPage({
      cursor: remotePreviousSessionsCursor,
      mode: 'append',
    });
  }, [isLoadingMorePreviousSessions, remotePreviousSessionsCursor, requestPreviousSessionsPage]);

  const selectPreviousSessionByKeyboard = (direction: -1 | 1) => {
    const nextHistoryId = getNextPreviousSessionsModalSelection({
      currentHistoryId: selectedHistoryIdRef.current,
      direction,
      sessions: visibleSessions,
    });
    if (!nextHistoryId) {
      return false;
    }

    selectedHistoryIdRef.current = nextHistoryId;
    setSelectedHistoryId(nextHistoryId);
    searchInputRef.current?.focus({ preventScroll: true });
    return true;
  };

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
        if (!selectPreviousSessionByKeyboard(event.key === 'ArrowUp' ? -1 : 1)) {
          return;
        }

        event.preventDefault();
        event.stopPropagation();
        return;
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
  }, [visibleSessions, isOpen, isTagFilterMenuOpen, onClose]);

  useEffect(() => {
    selectedHistoryIdRef.current = selectedHistoryId;
  }, [selectedHistoryId]);

  useEffect(() => {
    if (!selectedHistoryId) {
      return;
    }

    if (visibleSessions.some((session) => session.historyId === selectedHistoryId)) {
      return;
    }

    selectedHistoryIdRef.current = undefined;
    setSelectedHistoryId(undefined);
  }, [visibleSessions, selectedHistoryId]);

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
      setRemotePreviousSessions(undefined);
      setRemotePreviousSessionsCursor(undefined);
      isLoadingMorePreviousSessionsRef.current = false;
      setIsLoadingMorePreviousSessions(false);
      setHasInitialLoadResolved(false);
      setHasInitialLoadTimedOut(false);
      setResolvedPreviousSessionsQueryKey(undefined);
      hasRequestedInitialLoadRef.current = false;
      latestRequestRef.current = undefined;
      pendingSelectionRef.current = undefined;
      selectedHistoryIdRef.current = undefined;
      setSelectedHistoryId(undefined);
    }
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen || hasInitialLoadResolved) {
      return;
    }

    /*
    CDXC:PreviousSessions 2026-06-02-20:39:
    Opening Previous Sessions must not flash the empty, short modal while gxserver history is still loading. Keep the modal hidden until the first result proves sessions exist or do not exist, with a two-second max cap so the UI cannot appear stuck behind an unreachable query.
    */
    const timeoutId = window.setTimeout(() => {
      setHasInitialLoadTimedOut(true);
      onInitialLoadReady?.();
    }, PREVIOUS_SESSIONS_INITIAL_LOAD_TIMEOUT_MS);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [hasInitialLoadResolved, isOpen, onInitialLoadReady]);

  useEffect(() => {
    if (!isOpen) {
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
      }
      setRemotePreviousSessionsCursor(resultMessage.cursor);
      isLoadingMorePreviousSessionsRef.current = false;
      setIsLoadingMorePreviousSessions(false);
      setHasInitialLoadResolved(true);
      onInitialLoadReady?.();
    };
    window.addEventListener('message', handleMessage);
    return () => {
      window.removeEventListener('message', handleMessage);
    };
  }, [isOpen, onInitialLoadReady]);

  useEffect(() => {
    if (!isOpen) {
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
  }, [isOpen, requestPreviousSessionsPage]);

  useEffect(() => {
    if (!canShowModal) {
      return;
    }
    requestMorePreviousSessionsIfNeeded();
  }, [canShowModal, requestMorePreviousSessionsIfNeeded, visibleSessions.length]);

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
    if (!canShowModal || !selectedHistoryId) {
      return;
    }

    const animationFrame = window.requestAnimationFrame(() => {
      const selectedElement = Array.from(
        document.querySelectorAll<HTMLElement>('.previous-sessions-modal [data-sidebar-history-id]')
      ).find((element) => element.dataset.sidebarHistoryId === selectedHistoryId);
      selectedElement?.scrollIntoView({ block: 'nearest' });
      searchInputRef.current?.focus({ preventScroll: true });
    });

    return () => {
      window.cancelAnimationFrame(animationFrame);
    };
  }, [canShowModal, selectedHistoryId]);

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
              ariaLabel='Search sessions to reopen'
              clearLabel='Clear Reopen a Session search'
              inputRef={searchInputRef}
              placeholder='Search sessions...'
              query={searchQuery}
              setQuery={setSearchQuery}
              trailingControl={
                /*
                 * CDXC:PreviousSessions 2026-06-13-15:59:
                 * The tag filter belongs inside the search field's right-side icon slot so the search box can span the modal evenly from left to right instead of reserving a separate external action column.
                 */
                <button
                  aria-expanded={isTagFilterMenuOpen}
                  aria-haspopup='menu'
                  aria-label={
                    hasTagFilters
                      ? `Filter sessions to reopen by ${selectedSessionTagFilters.length} tags`
                      : 'Filter sessions to reopen by tag'
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
              }
            />
            {isTagFilterMenuOpen
              ? createPortal(
                  <div
                    aria-label='Reopen a Session tag filters'
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
            onScroll={requestMorePreviousSessionsIfNeeded}
            ref={previousSessionsBodyRef}
          >
            {!hasInitialLoadResolved ? (
              <div className='group-empty-state previous-sessions-empty-state'>Loading recent sessions…</div>
            ) : groupedSessions.length > 0 ? (
              groupedSessions.map((group) => (
                <section className='previous-sessions-day-group' key={group.dayLabel}>
                  <div className='previous-sessions-day-label'>{group.dayLabel}</div>
                  <div className='group-sessions'>
                    {group.sessions.map((session) => (
                      <SessionHistoryCard
                        isSearchSelected={selectedHistoryId === session.historyId}
                        key={session.historyId}
                        onDelete={() => {
                          setRemotePreviousSessions((current) =>
                            removePreviousSessionByHistoryId(current ?? modalPreviousSessions, session.historyId)
                          );
                          searchInputRef.current?.focus({ preventScroll: true });
                          vscode.postMessage({
                            historyId: session.historyId,
                            type: 'deletePreviousSession',
                          });
                        }}
                        onRestore={() => {
                          vscode.postMessage({
                            historyId: session.historyId,
                            type: 'restorePreviousSession',
                          });
                          onClose();
                        }}
                        session={session}
                        showDebugSessionNumbers={showDebugSessionNumbers}
                      />
                    ))}
                  </div>
                </section>
              ))
            ) : !hasResolvedCurrentPreviousSessionsQuery ? (
              <div className='group-empty-state previous-sessions-empty-state'>Loading recent sessions…</div>
            ) : (
              <div className='group-empty-state previous-sessions-empty-state'>
                {searchQuery.trim()
                  ? hasTagFilters
                    ? 'No tagged sessions to reopen match that search.'
                    : 'No sessions to reopen match that search.'
                  : hasTagFilters
                    ? 'No sessions to reopen match those tags.'
                    : 'No sessions to reopen yet.'}
              </div>
            )}
            {isLoadingMorePreviousSessions ? (
              <div
                aria-label='Loading more sessions to reopen'
                className='previous-sessions-loading-more'
                role='status'
              />
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
