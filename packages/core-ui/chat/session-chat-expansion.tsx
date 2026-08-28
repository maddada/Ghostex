import { IconChevronRight } from '@tabler/icons-react';
import { useRef, useState, type ReactNode } from 'react';
import { cn } from '@/packages/components/utils';
import { Button } from '../../components/ui/button';

/** Center a transcript row after React has committed its expanded content. */
export function centerSessionChatExpansion(target: HTMLElement | null): void {
  if (!target) {
    return;
  }
  window.requestAnimationFrame(() => {
    // MessageScroller reconciles resized content on its own animation frame.
    // Center on the following frame so its bottom-follow correction cannot
    // overwrite this explicit user navigation.
    window.requestAnimationFrame(() => {
      if (target.isConnected) {
        target.scrollIntoView({
          behavior: 'smooth',
          block: 'center',
          inline: 'nearest',
        });
      }
    });
  });
}

/** Keep the disclosure heading visible above a newly expanded long body. */
export function anchorSessionChatExpansionTop(target: HTMLElement | null): void {
  if (!target) {
    return;
  }
  window.requestAnimationFrame(() => {
    // Let both React and MessageScroller commit the new body before anchoring
    // its heading. Content that grows afterwards stays below this point.
    window.requestAnimationFrame(() => {
      if (target.isConnected) {
        target.scrollIntoView({
          behavior: 'smooth',
          block: 'start',
          inline: 'nearest',
        });
      }
    });
  });
}

export function SessionChatDisclosure({
  children,
  label,
  onExpand,
}: {
  children: ReactNode;
  label: string;
  onExpand: (target: HTMLElement | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);

  return (
    <div className='ghostex-chat-completed-work'>
      <Button
        aria-expanded={open}
        aria-label={open ? `Hide ${label.toLowerCase()}` : `Show ${label.toLowerCase()}`}
        className='ghostex-chat-completed-work-trigger'
        onClick={() => {
          if (!open) {
            onExpand(triggerRef.current);
          }
          setOpen((value) => !value);
        }}
        ref={triggerRef}
        size='xs'
        type='button'
        variant='ghost'
      >
        <span className='ghostex-chat-marker-slot'>
          <IconChevronRight aria-hidden='true' className={cn('ghostex-chat-disclosure-chevron', open && 'is-open')} />
        </span>
        <span>{label}</span>
      </Button>
      {open ? (
        <SessionChatExpansion
          bodyClassName='ghostex-chat-completed-work-content'
          label={`Collapse ${label.toLowerCase()}`}
          onCollapse={() => setOpen(false)}
        >
          {children}
        </SessionChatExpansion>
      ) : null}
    </div>
  );
}

export function SessionChatExpansion({
  bodyClassName,
  children,
  className,
  label,
  onCollapse,
}: {
  bodyClassName?: string;
  children: ReactNode;
  className?: string;
  label: string;
  onCollapse: () => void;
}) {
  return (
    <div className={cn('ghostex-chat-expansion', className)}>
      <button aria-label={label} className='ghostex-chat-expansion-rail' onClick={onCollapse} type='button' />
      <div className={cn('ghostex-chat-expansion-body', bodyClassName)}>{children}</div>
    </div>
  );
}
