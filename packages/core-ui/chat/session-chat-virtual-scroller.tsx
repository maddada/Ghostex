import { createContext, useContext, useCallback, type ComponentProps, type CSSProperties } from 'react';
import { cn } from '@/packages/components/utils';
import type { VirtualItem } from '@tanstack/react-virtual';
import type { SessionChatVirtualTranscript } from './use-session-chat-virtual-transcript';

export const SessionChatVirtualScrollerContext = createContext<SessionChatVirtualTranscript | null>(null);

export function useSessionChatVirtualScroller() {
  const controller = useContext(SessionChatVirtualScrollerContext);
  if (!controller) throw new Error('Transcript scrolling requires its controller');
  return controller;
}

export function MessageScroller({ className, ...props }: ComponentProps<'div'>) {
  return (
    <div
      data-slot='message-scroller'
      className={cn('group/message-scroller relative flex size-full min-h-0 flex-col overflow-hidden', className)}
      {...props}
    />
  );
}

export function MessageScrollerViewport({ className, ...props }: ComponentProps<'div'>) {
  return (
    <div
      data-slot='message-scroller-viewport'
      role='region'
      aria-label='Messages'
      tabIndex={0}
      className={cn(
        'size-full min-h-0 min-w-0 scroll-fade-b scrollbar-thin scrollbar-gutter-stable overflow-y-auto overscroll-contain contain-content',
        className
      )}
      style={{ overflowAnchor: 'none' }}
      {...props}
    />
  );
}

export function MessageScrollerContent({ className, style, ...props }: ComponentProps<'div'>) {
  const controller = useSessionChatVirtualScroller();
  return (
    <div
      data-slot='message-scroller-content'
      role='log'
      aria-relevant='additions'
      className={cn('relative min-h-full', className)}
      style={{ height: controller.virtualizer.getTotalSize(), ...style }}
      {...props}
    />
  );
}

export function MessageScrollerItem({
  className,
  messageId,
  virtualItem,
  style,
  ref,
  ...props
}: ComponentProps<'div'> & { messageId: string; virtualItem: VirtualItem }) {
  const { measureElement } = useSessionChatVirtualScroller();
  const setRef = useCallback(
    (element: HTMLDivElement | null) => {
      measureElement(element);
      if (typeof ref === 'function') ref(element);
      else if (ref) ref.current = element;
    },
    [ref, measureElement]
  );
  return (
    <div
      data-slot='message-scroller-item'
      data-message-id={messageId}
      data-index={virtualItem.index}
      ref={setRef}
      className={cn('min-w-0', className)}
      style={{ position: 'absolute', top: virtualItem.start, left: 16, right: 16, ...style } as CSSProperties}
      {...props}
    />
  );
}
