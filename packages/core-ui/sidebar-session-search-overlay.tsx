import { IconSearch, IconX } from '@tabler/icons-react';
import { useEffect, type ComponentProps, type KeyboardEventHandler, type ReactNode, type RefObject } from 'react';
import type { SidebarPreviousSessionItem } from '../shared/session-grid-contract';
import { SessionHistoryCard } from './session-history-card';

export type SidebarSessionSearchFieldProps = {
  ariaLabel?: string;
  autoCapitalize?: ComponentProps<'input'>['autoCapitalize'];
  autoComplete?: string;
  autoCorrect?: ComponentProps<'input'>['autoCorrect'];
  clearLabel?: string;
  inputClassName?: string;
  inputRef: RefObject<HTMLInputElement | null>;
  onEmptyBlur?: () => void;
  onKeyDown?: KeyboardEventHandler<HTMLInputElement>;
  placeholder?: string;
  query: string;
  shellClassName?: string;
  setQuery: (query: string) => void;
  shouldFocusOnQueryChange?: (inputElement: HTMLInputElement) => boolean;
  spellCheck?: ComponentProps<'input'>['spellCheck'];
  toolbarClassName?: string;
  trailingControl?: ReactNode;
};

export function SidebarSessionSearchField({
  ariaLabel = 'Search current sessions and sessions to reopen',
  autoCapitalize,
  autoComplete,
  autoCorrect,
  clearLabel = 'Clear session search',
  inputClassName,
  inputRef,
  onEmptyBlur,
  onKeyDown,
  placeholder = 'Search sessions',
  query,
  shellClassName,
  setQuery,
  shouldFocusOnQueryChange,
  spellCheck,
  toolbarClassName,
  trailingControl,
}: SidebarSessionSearchFieldProps) {
  const hasQuery = query.length > 0;
  const hasTrailingControl = trailingControl != null;
  const clearQueryAndFocus = () => {
    setQuery('');
    inputRef.current?.focus();
  };

  useEffect(() => {
    const timeoutId = window.setTimeout(() => {
      const inputElement = inputRef.current;
      if (!inputElement || shouldFocusOnQueryChange?.(inputElement) === false) {
        return;
      }
      inputElement.focus();
      inputElement.setSelectionRange(query.length, query.length);
    }, 0);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [inputRef, query.length, shouldFocusOnQueryChange]);

  return (
    <div
      className={['session-search-toolbar', toolbarClassName].filter(Boolean).join(' ')}
      data-empty-space-blocking='true'
    >
      {/*
       * CDXC:DesignSystem 2026-06-04-02:59:
       * Settings and Previous Sessions search fields must reuse the Mac sidebar search affordance: show the search icon on the right while empty, then replace it with an X button that clears the typed query and keeps focus in the field.
       *
       * CDXC:DesignSystem 2026-06-04-03:11:
       * Daemon search surfaces use this same field, and Escape on a focused non-empty field must clear the query the same way as the X button instead of moving focus or closing the surrounding surface.
       *
       * CDXC:DesignSystem 2026-06-13-15:59:
       * Some modal search rows own a real filter action at the right edge. Let callers replace the decorative idle Search icon with that button while preserving the shared clear-X behavior and input focus handling.
       *
       * CDXC:Settings 2026-06-25-21:21:
       * Settings search keeps this shared query-change focus behavior only when
       * the Settings modal says no editable field already owns typing focus.
       */}
      <div
        className={['session-search-input-shell', shellClassName].filter(Boolean).join(' ')}
        data-has-query={String(hasQuery)}
        data-has-trailing-control={String(hasTrailingControl)}
      >
        <input
          aria-label={ariaLabel}
          autoCapitalize={autoCapitalize}
          autoComplete={autoComplete}
          autoCorrect={autoCorrect}
          className={['group-title-input session-search-input', inputClassName].filter(Boolean).join(' ')}
          onBlur={() => {
            /**
             * CDXC:Sidebar 2026-05-08-11:49
             * In combined mode, an empty Search sessions field is only a
             * transient replacement for the Search nav button. Any focus-away
             * action should restore the button automatically, while typed
             * content keeps the search UI open for result review.
             */
            if (query.trim().length === 0) {
              onEmptyBlur?.();
            }
          }}
          onChange={(event) => {
            setQuery(event.target.value);
          }}
          onKeyDown={(event) => {
            if (event.key === 'Escape' && query.length > 0) {
              event.preventDefault();
              event.stopPropagation();
              clearQueryAndFocus();
              return;
            }
            onKeyDown?.(event);
          }}
          placeholder={placeholder}
          ref={inputRef}
          spellCheck={spellCheck}
          type='text'
          value={query}
        />
        {hasQuery ? (
          <button
            aria-label={clearLabel}
            className='session-search-clear-button'
            onClick={clearQueryAndFocus}
            type='button'
          >
            <IconX aria-hidden='true' className='session-search-input-icon' size={16} stroke={1.9} />
          </button>
        ) : null}
        {hasTrailingControl ? (
          <span className='session-search-trailing-control'>{trailingControl}</span>
        ) : !hasQuery ? (
          <IconSearch aria-hidden='true' className='session-search-input-icon' size={16} stroke={1.9} />
        ) : null}
      </div>
    </div>
  );
}

export type SidebarPreviousSessionsSearchGroupProps = {
  onDeletePreviousSession: (historyId: string) => void;
  onRestorePreviousSession: (historyId: string) => void;
  previousSessions: readonly SidebarPreviousSessionItem[];
  selectedHistoryId?: string;
  showDebugSessionNumbers: boolean;
};

export function SidebarPreviousSessionsSearchGroup({
  onDeletePreviousSession,
  onRestorePreviousSession,
  previousSessions,
  selectedHistoryId,
  showDebugSessionNumbers,
}: SidebarPreviousSessionsSearchGroupProps) {
  if (previousSessions.length === 0) {
    return null;
  }

  return (
    <section className='group session-search-previous-group' data-search-results='true'>
      <div className='group-head'>
        <div className='group-title-wrap'>
          <div className='group-title-row'>
            <div className='group-title-handle'>
              <div className='group-title'>Reopen a Session</div>
            </div>
          </div>
        </div>
      </div>
      <div className='group-sessions'>
        {previousSessions.map((session) => (
          <SessionHistoryCard
            key={session.historyId}
            isSearchSelected={selectedHistoryId === session.historyId}
            onDelete={() => onDeletePreviousSession(session.historyId)}
            onRestore={() => onRestorePreviousSession(session.historyId)}
            session={session}
            showDebugSessionNumbers={showDebugSessionNumbers}
          />
        ))}
      </div>
    </section>
  );
}
