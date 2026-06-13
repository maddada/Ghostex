import { IconCheck, IconFilter2, IconX } from "@tabler/icons-react";
import { createPortal } from "react-dom";
import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import {
  filterPreviousSessions,
  filterPreviousSessionsModalItems,
  groupPreviousSessionsByDay,
  removePreviousSessionByHistoryId,
} from "./previous-session-search";
import { SessionHistoryCard } from "./session-history-card";
import { useSidebarStore } from "./sidebar-store";
import {
  applyTextEditingKey,
  isEditableKeyboardTarget,
  isTextEditingKey,
} from "./text-input-keyboard";
import { TOOLTIP_DELAY_MS } from "./tooltip-delay";
import { TooltipProvider } from "./app-tooltip";
import { SidebarSessionSearchField } from "./sidebar-session-search-overlay";
import {
  SessionTagIcon,
  SIDEBAR_SESSION_TAG_SECTIONS,
  type SidebarSessionTag,
} from "./session-tag-ui";
import type { WebviewApi } from "./webview-api";
import type { ExtensionToSidebarMessage, SidebarPreviousSessionItem } from "../shared/session-grid-contract";

const PREVIOUS_SESSIONS_INITIAL_LOAD_TIMEOUT_MS = 2_000;
const PREVIOUS_SESSIONS_QUERY_DEBOUNCE_MS = 200;
const PREVIOUS_SESSIONS_TAG_FILTER_MENU_GAP_PX = 6;
const PREVIOUS_SESSIONS_TAG_FILTER_MENU_MARGIN_PX = 12;

export type PreviousSessionsModalProps = {
  isOpen: boolean;
  onClose: () => void;
  onInitialLoadReady?: () => void;
  vscode: WebviewApi;
};

function getPreviousSessionsTagFilterMenuStyle(
  buttonElement: HTMLButtonElement | null,
): CSSProperties {
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
  const right = Math.max(
    PREVIOUS_SESSIONS_TAG_FILTER_MENU_MARGIN_PX,
    window.innerWidth - bounds.right,
  );
  const top = Math.min(
    bounds.bottom + PREVIOUS_SESSIONS_TAG_FILTER_MENU_GAP_PX,
    window.innerHeight - PREVIOUS_SESSIONS_TAG_FILTER_MENU_MARGIN_PX,
  );

  return {
    maxHeight: `calc(100vh - ${top + PREVIOUS_SESSIONS_TAG_FILTER_MENU_MARGIN_PX}px)`,
    maxWidth: `calc(100vw - ${PREVIOUS_SESSIONS_TAG_FILTER_MENU_MARGIN_PX * 2}px)`,
    overflowY: "auto",
    right: `${right}px`,
    top: `${top}px`,
  };
}

export function PreviousSessionsModal({
  isOpen,
  onClose,
  onInitialLoadReady,
  vscode,
}: PreviousSessionsModalProps) {
  const previousSessions = useSidebarStore((state) => state.previousSessions);
  const showDebugSessionNumbers = useSidebarStore((state) => state.hud.debuggingMode);
  const [selectedSessionTagFilters, setSelectedSessionTagFilters] = useState<
    SidebarSessionTag[]
  >([]);
  const [isTagFilterMenuOpen, setIsTagFilterMenuOpen] = useState(false);
  const [remotePreviousSessions, setRemotePreviousSessions] = useState<SidebarPreviousSessionItem[] | undefined>(undefined);
  const [hasInitialLoadResolved, setHasInitialLoadResolved] = useState(false);
  const [hasInitialLoadTimedOut, setHasInitialLoadTimedOut] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const searchInputRef = useRef<HTMLInputElement>(null);
  const tagFilterButtonRef = useRef<HTMLButtonElement>(null);
  const tagFilterMenuRef = useRef<HTMLDivElement>(null);
  const hasRequestedInitialLoadRef = useRef(false);
  const latestRequestIdRef = useRef<string | undefined>(undefined);
  const pendingSelectionRef = useRef<{ end: number; start: number } | undefined>(undefined);
  const modalPreviousSessions = useMemo(
    () => filterPreviousSessionsModalItems(remotePreviousSessions ?? previousSessions),
    [previousSessions, remotePreviousSessions],
  );
  const filteredSessions = useMemo(
    () =>
      filterPreviousSessions(modalPreviousSessions, searchQuery, {
        sessionTags: selectedSessionTagFilters,
      }),
    [modalPreviousSessions, searchQuery, selectedSessionTagFilters],
  );
  const groupedSessions = useMemo(
    () => groupPreviousSessionsByDay(filteredSessions),
    [filteredSessions],
  );
  const canShowModal = isOpen && (hasInitialLoadResolved || hasInitialLoadTimedOut);
  const hasTagFilters = selectedSessionTagFilters.length > 0;

  const openTagFilterMenu = () => {
    const bounds = tagFilterButtonRef.current?.getBoundingClientRect();
    if (!bounds) {
      setIsTagFilterMenuOpen((previous) => !previous);
      return;
    }
    setIsTagFilterMenuOpen(true);
  };

  const toggleSessionTagFilter = (sessionTag: SidebarSessionTag) => {
    setSelectedSessionTagFilters((current) =>
      current.includes(sessionTag)
        ? current.filter((tag) => tag !== sessionTag)
        : [...current, sessionTag],
    );
  };

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
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
      if (
        !searchInput ||
        event.target === searchInput ||
        isEditableKeyboardTarget(event.target) ||
        !isTextEditingKey(event)
      ) {
        return;
      }

      const nextSearchState = applyTextEditingKey(
        {
          selectionEnd: searchInput.selectionEnd,
          selectionStart: searchInput.selectionStart,
          value: searchInput.value,
        },
        event.key,
        event,
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

    document.addEventListener("keydown", handleKeyDown, true);
    return () => {
      document.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [isOpen, isTagFilterMenuOpen, onClose]);

  useEffect(() => {
    if (!isTagFilterMenuOpen) {
      return;
    }

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) {
        return;
      }
      if (
        tagFilterButtonRef.current?.contains(target) ||
        tagFilterMenuRef.current?.contains(target)
      ) {
        return;
      }
      setIsTagFilterMenuOpen(false);
    };

    document.addEventListener("pointerdown", handlePointerDown, true);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown, true);
    };
  }, [isTagFilterMenuOpen]);

  useEffect(() => {
    if (!isOpen) {
      setSelectedSessionTagFilters([]);
      setIsTagFilterMenuOpen(false);
      setSearchQuery("");
      setRemotePreviousSessions(undefined);
      setHasInitialLoadResolved(false);
      setHasInitialLoadTimedOut(false);
      hasRequestedInitialLoadRef.current = false;
      latestRequestIdRef.current = undefined;
      pendingSelectionRef.current = undefined;
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
      if (event.data.type !== "previousSessionsResult") {
        return;
      }
      if (event.data.requestId !== latestRequestIdRef.current) {
        return;
      }
      setRemotePreviousSessions(event.data.previousSessions);
      setHasInitialLoadResolved(true);
      onInitialLoadReady?.();
    };
    window.addEventListener("message", handleMessage);
    return () => {
      window.removeEventListener("message", handleMessage);
    };
  }, [isOpen, onInitialLoadReady]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    const requestDelay = hasRequestedInitialLoadRef.current
      ? PREVIOUS_SESSIONS_QUERY_DEBOUNCE_MS
      : 0;
    const timeoutId = window.setTimeout(() => {
      const requestId = `previous-sessions-${Date.now()}-${Math.random().toString(36).slice(2)}`;
      hasRequestedInitialLoadRef.current = true;
      latestRequestIdRef.current = requestId;
      /*
      CDXC:GxserverPresentationSearch 2026-06-01-15:08:
      Previous Sessions no longer depends on a startup-hydrated history array. Request recent/history metadata from gxserver on open and debounce typed search at 200ms so the modal remains bounded by current query results.
      */
      vscode.postMessage({
        limit: 80,
        query: searchQuery.trim() || undefined,
        requestId,
        sessionTags: selectedSessionTagFilters,
        type: "requestPreviousSessions",
      });
    }, requestDelay);
    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [isOpen, searchQuery, selectedSessionTagFilters, vscode]);

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

  if (!canShowModal) {
    return null;
  }

  return createPortal(
    <TooltipProvider delayDuration={TOOLTIP_DELAY_MS}>
      <div className="confirm-modal-root scroll-mask-y" role="presentation">
        <button className="confirm-modal-backdrop" onClick={onClose} type="button" />
        <div
          aria-labelledby="previous-sessions-modal-title"
          aria-modal="true"
          className="confirm-modal previous-sessions-modal scroll-mask-y"
          role="dialog"
        >
          <button
            aria-label="Close previous sessions"
            className="confirm-modal-close-button previous-sessions-close-button"
            onClick={onClose}
            type="button"
          >
            <IconX aria-hidden="true" className="toolbar-tabler-icon" stroke={1.8} />
          </button>
          <div className="confirm-modal-header confirm-modal-header-with-close">
            <div className="confirm-modal-title" id="previous-sessions-modal-title">
              Previous Sessions
            </div>
          </div>
          <div className="previous-sessions-toolbar">
            <SidebarSessionSearchField
              ariaLabel="Search previous sessions"
              clearLabel="Clear previous sessions search"
              inputClassName="previous-sessions-search-input"
              inputRef={searchInputRef}
              placeholder="Search sessions..."
              query={searchQuery}
              setQuery={setSearchQuery}
              toolbarClassName="previous-sessions-search-control"
            />
            <div className="previous-sessions-tag-filter">
              <button
                aria-expanded={isTagFilterMenuOpen}
                aria-haspopup="menu"
                aria-label={
                  hasTagFilters
                    ? `Filter previous sessions by ${selectedSessionTagFilters.length} tags`
                    : "Filter previous sessions by tag"
                }
                className="previous-sessions-favorites-toggle previous-sessions-tag-filter-toggle"
                data-selected={String(hasTagFilters)}
                onClick={(event) => {
                  event.stopPropagation();
                  if (isTagFilterMenuOpen) {
                    setIsTagFilterMenuOpen(false);
                    return;
                  }
                  openTagFilterMenu();
                }}
                ref={tagFilterButtonRef}
                type="button"
              >
                <IconFilter2
                  aria-hidden="true"
                  className="toolbar-tabler-icon"
                  stroke={1.8}
                />
              </button>
              {isTagFilterMenuOpen
                ? createPortal(
                <div
                  aria-label="Previous session tag filters"
                  className="session-context-menu previous-sessions-tag-filter-menu"
                  ref={tagFilterMenuRef}
                  role="menu"
                  style={getPreviousSessionsTagFilterMenuStyle(tagFilterButtonRef.current)}
                >
                  {/*
                   * CDXC:SessionTags 2026-06-05-12:30:
                   * Previous Sessions supports selecting one or more session
                   * tags, matching the active sidebar filter semantics. Empty
                   * selection means all tags and untagged sessions are shown.
                   */}
                  {SIDEBAR_SESSION_TAG_SECTIONS.map((section) => (
                    <div className="session-tag-menu-section" key={section.label}>
                      <div className="session-tag-menu-section-label">{section.label}</div>
                      {section.options.map((option) => {
                        const isSelected = selectedSessionTagFilters.includes(option.value);
                        return (
                          <button
                            aria-checked={isSelected}
                            className="session-context-menu-item previous-sessions-tag-filter-item"
                            data-selected={String(isSelected)}
                            key={option.value}
                            onClick={() => toggleSessionTagFilter(option.value)}
                            role="menuitemcheckbox"
                            type="button"
                          >
                            <SessionTagIcon
                              className="session-context-menu-icon session-tag-colored-icon"
                              fillFavorite
                              size={14}
                              stroke={1.8}
                              tag={option.value}
                            />
                            {option.label}
                            <IconCheck
                              aria-hidden="true"
                              className="session-context-menu-trailing-icon previous-sessions-tag-filter-check"
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
                  document.body,
                )
                : null}
            </div>
          </div>
          <div className="previous-sessions-modal-body scroll-mask-y">
            {groupedSessions.length > 0 ? (
              groupedSessions.map((group) => (
                <section className="previous-sessions-day-group" key={group.dayLabel}>
                  <div className="previous-sessions-day-label">{group.dayLabel}</div>
                  <div className="group-sessions">
                    {group.sessions.map((session) => (
                      <SessionHistoryCard
                        key={session.historyId}
                        onDelete={() => {
                          setRemotePreviousSessions((current) =>
                            removePreviousSessionByHistoryId(current ?? modalPreviousSessions, session.historyId),
                          );
                          vscode.postMessage({
                            historyId: session.historyId,
                            type: "deletePreviousSession",
                          });
                        }}
                        onRestore={() => {
                          vscode.postMessage({
                            historyId: session.historyId,
                            type: "restorePreviousSession",
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
            ) : (
              <div className="group-empty-state previous-sessions-empty-state">
                {searchQuery.trim()
                  ? hasTagFilters
                    ? "No tagged previous sessions match that search."
                    : "No previous sessions match that search."
                  : hasTagFilters
                    ? "No previous sessions match those tags."
                    : "No previous sessions yet."}
              </div>
            )}
          </div>
          {/*
           * CDXC:PreviousSessions 2026-06-13-01:09:
           * Previous Sessions is now a browse, filter, restore, and delete modal only. Do not render footer launch buttons here, and do not expose the removed agent-prompt search workflow from this surface.
           */}
        </div>
      </div>
    </TooltipProvider>,
    document.body,
  );
}
