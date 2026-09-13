import { useState, type MouseEvent } from 'react';
import { useSessionChatVirtualScroller } from './session-chat-virtual-scroller';
import type { SessionChatMessage } from '@/packages/shared/session-chat';
import './session-chat-minimap.css';

interface SessionChatMinimapTurn {
  user: SessionChatMessage;
  final: SessionChatMessage | null;
}

function previewText(message: SessionChatMessage | null): string {
  return (message?.blocks ?? [])
    .flatMap((block) => (block.type === 'text' ? [block.text] : []))
    .join(' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/**
 * CDXC:SessionChat 2026-09-08 DECISION:
 * User: place message-history navigation in the middle of the chat's left side, with one dash per user prompt, visible-message highlighting, hover previews and keyboard/click navigation.
 * Hovering the marks must not paint a button background behind them.
 * User: enlarge the selector bars and their hover/click area by 30%, keeping the tooltips at their original size.
 */
export function SessionChatMinimap({
  turns,
  onNavigate,
}: {
  turns: readonly SessionChatMinimapTurn[];
  onNavigate: () => void;
}) {
  const { scrollToMessage, visibleMessageIds } = useSessionChatVirtualScroller();
  const [activeId, setActiveId] = useState<string | null>(null);
  const activeIndex = turns.findIndex((turn) => turn.user.id === activeId);
  const activeTurn = turns[activeIndex];
  const visibleIds = new Set(visibleMessageIds);

  if (turns.length < 2) {
    return null;
  }

  const topPercent = (index: number): number => (index / (turns.length - 1)) * 100;
  const pointerIndex = (event: MouseEvent<HTMLButtonElement>): number => {
    const rect = event.currentTarget.getBoundingClientRect();
    const progress = Math.max(0, Math.min(1, (event.clientY - rect.top) / rect.height));
    return Math.round(progress * (turns.length - 1));
  };
  const select = (index: number): void => {
    const turn = turns[index];
    if (!turn) {
      return;
    }
    onNavigate();
    scrollToMessage(turn.user.id, {
      align: 'start',
      behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth',
      scrollMargin: 24,
    });
  };
  const move = (index: number): void => {
    setActiveId(turns[Math.max(0, Math.min(turns.length - 1, index))]!.user.id);
  };
  const userPreview = activeTurn ? previewText(activeTurn.user) || 'User message' : 'User message';
  const assistantPreview = activeTurn ? previewText(activeTurn.final) : '';

  return (
    <nav aria-label='Chat message history' className='ghostex-chat-minimap'>
      <button
        aria-label={`Jump to message: ${userPreview}`}
        className='ghostex-chat-minimap-rail'
        onBlur={() => setActiveId(null)}
        onClick={(event) => {
          select(event.detail === 0 ? Math.max(0, activeIndex) : pointerIndex(event));
          setActiveId(null);
          event.currentTarget.blur();
        }}
        onFocus={() => {
          if (activeId === null) move(0);
        }}
        onKeyDown={(event) => {
          switch (event.key) {
            case 'ArrowDown':
              event.preventDefault();
              move(Math.max(0, activeIndex) + 1);
              break;
            case 'ArrowUp':
              event.preventDefault();
              move(Math.max(0, activeIndex) - 1);
              break;
            case 'Home':
              event.preventDefault();
              move(0);
              break;
            case 'End':
              event.preventDefault();
              move(turns.length - 1);
              break;
            case 'Enter':
            case ' ':
              event.preventDefault();
              select(Math.max(0, activeIndex));
              break;
            case 'Escape':
              setActiveId(null);
              event.currentTarget.blur();
              break;
          }
        }}
        onMouseDown={(event) => event.preventDefault()}
        onMouseLeave={() => setActiveId(null)}
        onMouseMove={(event) => move(pointerIndex(event))}
        style={{ height: `calc(${(turns.length - 1) * 8}px * var(--ghostex-chat-minimap-scale))` }}
        type='button'
      >
        <span aria-hidden='true' className='ghostex-chat-minimap-line' />
        {turns.map((turn, index) => {
          const distance = activeIndex < 0 ? Infinity : Math.abs(index - activeIndex);
          return (
            <span
              aria-hidden='true'
              className='ghostex-chat-minimap-dash'
              data-active={distance === 0}
              data-in-view={visibleIds.has(turn.user.id)}
              key={turn.user.id}
              style={{
                top: `${topPercent(index)}%`,
                width: `calc(${distance === 0 ? 24 : distance === 1 ? 16 : distance === 2 ? 10 : 8}px * var(--ghostex-chat-minimap-scale))`,
              }}
            />
          );
        })}
        {activeTurn ? (
          <span
            aria-hidden='true'
            className='ghostex-chat-minimap-preview'
            style={{
              top: `${topPercent(activeIndex)}%`,
              transform: `translateY(${activeIndex === 0 ? '0%' : activeIndex === turns.length - 1 ? '-100%' : '-50%'})`,
            }}
          >
            <span className='ghostex-chat-minimap-prompt'>{userPreview}</span>
            {assistantPreview ? <span className='ghostex-chat-minimap-reply'>{assistantPreview}</span> : null}
          </span>
        ) : null}
      </button>
    </nav>
  );
}
