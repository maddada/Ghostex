import { IconHistory, IconWorld, IconX } from '@tabler/icons-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { SegmentedControl, SegmentedControlItem } from '../components/ui/segmented-control';
import { AppTooltip, TooltipProvider, useDismissSidebarTooltipsOnScroll } from './app-tooltip';
import { postAppModalHostMessage } from './app-modal-host-bridge';
import { QuickAccessSearchInput } from './quick-access-search-input';
import { formatRelativeTimeLabel } from './relative-time';
import { useSidebarTooltipDelayMs } from './tooltip-delay';
import { useRelativeTimeTick } from './use-relative-time-tick';
import './browser-history-modal.css';

type HistoryEntry = {
  id: string;
  projectId: string;
  projectName: string;
  title: string;
  url: string;
  faviconUrl: string | null;
  visitedAt: number;
};

type HistoryResult = {
  type: 'browserHistoryResult';
  requestId: string;
  entries: HistoryEntry[];
  hasMore: boolean;
  error?: string;
};

export type BrowserHistoryTarget = { paneId: number; runtimeKey: number };

function HistoryFavicon({ url }: { url: string | null }) {
  const [failedUrl, setFailedUrl] = useState<string>();
  return (
    <span className='browser-history-favicon'>
      {url && url !== failedUrl ? (
        <img alt='' src={url} referrerPolicy='no-referrer' onError={() => setFailedUrl(url)} />
      ) : (
        <IconWorld aria-hidden='true' size={16} />
      )}
    </span>
  );
}

function dayLabel(timestamp: number) {
  return timestamp > 0
    ? new Date(timestamp).toLocaleDateString(undefined, {
        weekday: 'long',
        month: 'long',
        day: 'numeric',
        year: 'numeric',
      })
    : 'Earlier tab history';
}

/**
 * CDXC:Browser 2026-09-09 DECISION:
 * User: Browser history uses the Sessions list look and the same borderless GPUI popup, opens on All Projects, and offers Current Project; cards show the favicon and page title with the URL on a second line and full details on hover. When the page title equals its URL, show that text once.
 * The native app-modal host owns the window; shared Quick Access layout, search, row chrome, scope control, and tooltips own the contents.
 */
export function BrowserHistoryModal({ target, onClose }: { target: BrowserHistoryTarget; onClose: () => void }) {
  const [scope, setScope] = useState('all');
  const [query, setQuery] = useState('');
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [selectedIndex, setSelectedIndex] = useState(0);
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const pendingRequest = useRef<{ id: string; offset: number } | null>(null);
  const tooltipDelay = useSidebarTooltipDelayMs();
  const now = useRelativeTimeTick(true, 30_000);
  useDismissSidebarTooltipsOnScroll();

  function requestEntries(offset: number) {
    const requestId = crypto.randomUUID();
    const lastEntry = offset > 0 ? entries[offset - 1] : undefined;
    pendingRequest.current = { id: requestId, offset };
    setLoading(true);
    setError(undefined);
    postAppModalHostMessage(
      {
        type: 'browserHistoryQuery',
        requestId,
        scope,
        query: query.trim(),
        before: lastEntry ? { id: lastEntry.id, visitedAt: lastEntry.visitedAt } : undefined,
      },
      'Browser:history'
    );
  }

  useEffect(() => {
    const receive = (event: Event) => {
      const result = (event as CustomEvent<HistoryResult | { type: 'browserHistoryOpenError'; error: string }>).detail;
      if (result?.type === 'browserHistoryOpenError') {
        setError(result.error);
        return;
      }
      const pending = pendingRequest.current;
      if (result?.type !== 'browserHistoryResult' || !pending || result.requestId !== pending.id) return;
      pendingRequest.current = null;
      setLoading(false);
      setError(result.error);
      setHasMore(result.hasMore);
      setEntries((previous) => (pending.offset === 0 ? result.entries : [...previous, ...result.entries]));
    };
    window.addEventListener('ghostex-app-modal-host-message', receive);
    searchRef.current?.focus();
    return () => window.removeEventListener('ghostex-app-modal-host-message', receive);
  }, []);

  useEffect(() => {
    pendingRequest.current = null;
    setEntries([]);
    setHasMore(false);
    setLoading(true);
    setError(undefined);
    setSelectedIndex(0);
    listRef.current?.scrollTo({ top: 0 });
    const timer = window.setTimeout(() => requestEntries(0), 150);
    return () => {
      window.clearTimeout(timer);
      pendingRequest.current = null;
    };
  }, [scope, query]);

  useEffect(() => {
    listRef.current?.querySelector('[data-search-selected="true"]')?.scrollIntoView({ block: 'nearest' });
  }, [selectedIndex]);

  function openEntry(entry: HistoryEntry) {
    postAppModalHostMessage({ type: 'browserHistoryOpen', id: entry.id, ...target }, 'Browser:history');
  }

  const groups = useMemo(() => {
    const grouped: { label: string; entries: { entry: HistoryEntry; index: number }[] }[] = [];
    entries.forEach((entry, index) => {
      const label = dayLabel(entry.visitedAt);
      let group = grouped[grouped.length - 1];
      if (!group || group.label !== label) {
        group = { label, entries: [] };
        grouped.push(group);
      }
      group.entries.push({ entry, index });
    });
    return grouped;
  }, [entries]);

  return createPortal(
    <TooltipProvider delayDuration={tooltipDelay}>
      <div className='confirm-modal-root scroll-mask-y' role='presentation'>
        <button aria-label='Close browser history' className='confirm-modal-backdrop' onClick={onClose} type='button' />
        <div
          aria-label='Browser History'
          aria-modal='true'
          className='confirm-modal ghostex-settings-shadcn previous-sessions-modal quick-access-surface browser-history-modal scroll-mask-y'
          role='dialog'
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault();
              event.stopPropagation();
              onClose();
            }
            if (event.target !== searchRef.current) return;
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
              event.preventDefault();
              setSelectedIndex((index) =>
                Math.max(0, Math.min(entries.length - 1, index + (event.key === 'ArrowDown' ? 1 : -1)))
              );
            }
            if (event.key === 'Enter' && entries[selectedIndex]) {
              event.preventDefault();
              openEntry(entries[selectedIndex]);
            }
          }}
        >
          <div className='browser-history-heading'>
            <IconHistory aria-hidden='true' size={17} />
            <span>Browser History</span>
            <AppTooltip content='Close history'>
              <button aria-label='Close history' className='browser-history-close' onClick={onClose} type='button'>
                <IconX size={16} />
              </button>
            </AppTooltip>
          </div>
          <div className='previous-sessions-toolbar'>
            <QuickAccessSearchInput
              ariaLabel='Search browser history'
              clearLabel='Clear history search'
              inputRef={searchRef}
              placeholder='Search titles, URLs, and projects...'
              query={query}
              setQuery={setQuery}
            />
            <div className='quick-access-filter-toolbar'>
              <SegmentedControl aria-label='History project scope' value={scope} onValueChange={setScope}>
                <SegmentedControlItem value='all'>All Projects</SegmentedControlItem>
                <SegmentedControlItem value='current'>Current Project</SegmentedControlItem>
              </SegmentedControl>
            </div>
          </div>
          <div aria-busy={loading} className='previous-sessions-modal-body scroll-mask-y' ref={listRef}>
            {groups.map((group) => (
              <section className='previous-sessions-day-group' key={group.label}>
                <div className='previous-sessions-day-label'>{group.label}</div>
                <div className='group-sessions'>
                  {group.entries.map(({ entry, index }) => (
                    <AppTooltip
                      key={entry.id}
                      align='start'
                      side='bottom'
                      content={
                        <div className='browser-history-tooltip'>
                          <strong>{entry.title || entry.url}</strong>
                          {entry.title.trim() && entry.title.trim() !== entry.url.trim() && <span>{entry.url}</span>}
                          {entry.projectName && <span>{entry.projectName}</span>}
                          {entry.visitedAt > 0 && <span>{new Date(entry.visitedAt).toLocaleString()}</span>}
                        </div>
                      }
                    >
                      <div className='session-frame session-history-frame' data-focused='false' data-visible='false'>
                        <button
                          className='session session-history-card browser-history-card'
                          data-focused='false'
                          data-visible='false'
                          data-search-selected={String(index === selectedIndex)}
                          aria-label={`Open ${entry.title || entry.url} in a new tab`}
                          onClick={() => openEntry(entry)}
                          type='button'
                        >
                          <HistoryFavicon url={entry.faviconUrl} />
                          <span className='browser-history-page'>
                            <span className='browser-history-title'>{entry.title || entry.url}</span>
                            {entry.title.trim() && entry.title.trim() !== entry.url.trim() && (
                              <span className='browser-history-url'>{entry.url}</span>
                            )}
                          </span>
                          <span className='browser-history-meta'>
                            {scope === 'all' && <span>{entry.projectName}</span>}
                            {entry.visitedAt > 0 && (
                              <span>
                                {formatRelativeTimeLabel(new Date(entry.visitedAt).toISOString(), { nowMs: now })}
                              </span>
                            )}
                          </span>
                        </button>
                      </div>
                    </AppTooltip>
                  ))}
                </div>
              </section>
            ))}
            {error && (
              <p className='browser-history-status' role='alert'>
                {error}
              </p>
            )}
            {!loading && !error && entries.length === 0 && (
              <p className='browser-history-status' role='status'>
                {query
                  ? 'No matching history.'
                  : scope === 'current'
                    ? 'No browser history for this project yet.'
                    : 'No browser history yet. Pages you visit will appear here.'}
              </p>
            )}
            {loading && (
              <p className='browser-history-status' role='status'>
                Loading history...
              </p>
            )}
            {hasMore && !loading && (
              <button
                className='browser-history-load-more'
                onClick={() => requestEntries(entries.length)}
                type='button'
              >
                Load more
              </button>
            )}
          </div>
        </div>
      </div>
    </TooltipProvider>,
    document.body
  );
}
