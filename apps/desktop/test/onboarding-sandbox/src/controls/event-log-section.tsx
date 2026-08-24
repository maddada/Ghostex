/*
 * Annotated event log. Chronological (oldest first) with sticky auto-scroll,
 * per-kind chips + filters, t+ms since launch, expandable detail and the real
 * code anchor as a dim mono suffix. "warning" rows are the restart-required
 * bugs the sandbox exists to make visible, so they get their own style.
 */
import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useSandboxStore } from '../state/store';
import type { SimEvent, SimEventKind } from '../state/types';
import { usePersistedState } from './controls-storage';

const EVENT_KINDS: readonly SimEventKind[] = ['flow', 'state', 'modal', 'toast', 'message', 'warning'];

function formatOffset(at: number): string {
  if (at < 1000) return `+${at}ms`;
  return `+${(at / 1000).toFixed(2)}s`;
}

function EventRow({ event, expanded, onToggle }: { event: SimEvent; expanded: boolean; onToggle: () => void }) {
  const expandable = event.detail !== undefined || event.codeRef !== undefined;
  return (
    <div className={`cp-event cp-event--${event.kind}${expanded ? ' is-expanded' : ''}`} data-kind={event.kind}>
      <button className='cp-event-head' onClick={onToggle} title={expandable ? 'Show detail' : undefined} type='button'>
        <span className='cp-event-time'>{formatOffset(event.at)}</span>
        <span className={`cp-event-chip cp-event-chip--${event.kind}`}>{event.kind}</span>
        <span className='cp-event-label'>{event.label}</span>
        {expandable ? <span className='cp-event-caret'>{expanded ? '▾' : '▸'}</span> : null}
      </button>
      {expanded ? (
        <div className='cp-event-detail'>
          {event.detail === undefined ? null : <p>{event.detail}</p>}
          {event.codeRef === undefined ? null : <code className='cp-event-ref'>{event.codeRef}</code>}
        </div>
      ) : null}
      {!expanded && event.codeRef !== undefined ? (
        <code className='cp-event-ref cp-event-ref--inline'>{event.codeRef}</code>
      ) : null}
    </div>
  );
}

export function EventLogSection() {
  const events = useSandboxStore((s) => s.events);
  const clearEvents = useSandboxStore((s) => s.clearEvents);
  const [collapsed, setCollapsed] = usePersistedState('eventLog.collapsed', false);
  const [hiddenKinds, setHiddenKinds] = usePersistedState<SimEventKind[]>('eventLog.hidden', []);
  const [expandedIds, setExpandedIds] = useState<number[]>([]);

  const scrollRef = useRef<HTMLDivElement | null>(null);
  const stickyRef = useRef(true);

  const visible = useMemo(() => events.filter((event) => !hiddenKinds.includes(event.kind)), [events, hiddenKinds]);

  useLayoutEffect(() => {
    const node = scrollRef.current;
    if (node === null || !stickyRef.current) return;
    node.scrollTop = node.scrollHeight;
  }, [visible.length, collapsed]);

  const toggleKind = (kind: SimEventKind) => {
    setHiddenKinds(hiddenKinds.includes(kind) ? hiddenKinds.filter((k) => k !== kind) : [...hiddenKinds, kind]);
  };

  let lastLaunchIndex: number | null = null;

  return (
    <div className={collapsed ? 'cp-log is-collapsed' : 'cp-log'}>
      <div className='cp-log-head'>
        <button className='cp-log-toggle' onClick={() => setCollapsed(!collapsed)} type='button'>
          <span className='cp-caret'>{collapsed ? '▸' : '▾'}</span>
          Event log
          <span className='cp-section-badge'>{visible.length}</span>
        </button>
        <button className='cp-btn cp-btn--ghost' onClick={clearEvents} type='button'>
          Clear
        </button>
      </div>
      {collapsed ? null : (
        <>
          <div className='cp-log-filters'>
            {EVENT_KINDS.map((kind) => (
              <button
                className={`cp-filter cp-filter--${kind}${hiddenKinds.includes(kind) ? '' : ' is-on'}`}
                key={kind}
                onClick={() => toggleKind(kind)}
                type='button'
              >
                {kind}
              </button>
            ))}
          </div>
          <div
            className='cp-log-scroll'
            onScroll={(e) => {
              const node = e.currentTarget;
              stickyRef.current = node.scrollHeight - node.scrollTop - node.clientHeight < 24;
            }}
            ref={scrollRef}
          >
            {visible.length === 0 ? <p className='cp-log-empty'>No events yet — launch the app.</p> : null}
            {visible.map((event) => {
              const showLaunchDivider = event.launchIndex !== lastLaunchIndex;
              lastLaunchIndex = event.launchIndex;
              return (
                <div key={event.id}>
                  {showLaunchDivider ? <div className='cp-log-divider'>launch #{event.launchIndex}</div> : null}
                  <EventRow
                    event={event}
                    expanded={expandedIds.includes(event.id)}
                    onToggle={() =>
                      setExpandedIds(
                        expandedIds.includes(event.id)
                          ? expandedIds.filter((id) => id !== event.id)
                          : [...expandedIds, event.id]
                      )
                    }
                  />
                </div>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}
