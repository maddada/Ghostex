import { useLayoutEffect, useState, type RefObject } from 'react';
import { Button } from '@/packages/components/ui/button';

export const FOLLOW_BOTTOM_ATTRIBUTE = 'data-ghostex-follow-bottom';
export const STREAM_HOLD_ATTRIBUTE = 'data-ghostex-stream-hold';

interface SessionChatScrollBottomButtonProps {
  contentRef: RefObject<HTMLDivElement | null>;
  viewportRef: RefObject<HTMLDivElement | null>;
  edgeThreshold: number;
  shortcutLabel: string;
  onJump: () => void;
}

/** CDXC:SessionChat 2026-09-12 WHY:
 * A stream hold can still be at the bottom when the reply fits in the viewport. Forcing the scroller's inactive button visible showed it there and left it inert when the reply grew.
 * Measure the actual remaining scroll distance after scrolling and layout changes, independently of follow intent. Keep the follow-intent check so ordinary bottom-follow growth does not flash the button before scrolling settles.
 */
export function SessionChatScrollBottomButton({
  contentRef,
  viewportRef,
  edgeThreshold,
  shortcutLabel,
  onJump,
}: SessionChatScrollBottomButtonProps) {
  const [visible, setVisible] = useState(false);

  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    const content = contentRef.current;
    if (!viewport || !content) return;

    const measure = () => {
      const awayFromBottom = viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight > edgeThreshold;
      const following = viewport.getAttribute(FOLLOW_BOTTOM_ATTRIBUTE) !== 'false';
      const holding = viewport.getAttribute(STREAM_HOLD_ATTRIBUTE) === 'true';
      setVisible(awayFromBottom && (holding || !following));
    };
    const resizeObserver = new ResizeObserver(measure);
    resizeObserver.observe(viewport);
    resizeObserver.observe(content);
    const intentObserver = new MutationObserver(measure);
    intentObserver.observe(viewport, {
      attributes: true,
      attributeFilter: [FOLLOW_BOTTOM_ATTRIBUTE, STREAM_HOLD_ATTRIBUTE],
    });
    viewport.addEventListener('scroll', measure, { passive: true });
    measure();
    return () => {
      resizeObserver.disconnect();
      intentObserver.disconnect();
      viewport.removeEventListener('scroll', measure);
    };
  }, [contentRef, edgeThreshold, viewportRef]);

  return (
    <Button
      aria-hidden={!visible}
      className='ghostex-chat-scroll-bottom-button absolute inset-s-1/2 -translate-x-1/2 h-6 rounded-full px-2.5 text-[11px] font-medium'
      data-active={visible ? 'true' : 'false'}
      data-direction='end'
      inert={!visible}
      onClick={(event) => {
        event.currentTarget.blur();
        onJump();
      }}
      size='xs'
      tabIndex={visible ? 0 : -1}
      type='button'
      variant='secondary'
    >
      Scroll to bottom{shortcutLabel ? ` (${shortcutLabel})` : ''}
    </Button>
  );
}
