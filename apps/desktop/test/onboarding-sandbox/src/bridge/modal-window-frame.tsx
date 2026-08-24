/*
 * One fake NSPanel: title bar + close button + the real modal-host iframe.
 *
 * Faithfulness notes (SPEC.md "The modal pipeline"):
 * - The real gpui app keeps the child window hidden until the modal host posts
 *   `{type:"presented"}`, so the content here stays behind a spinner until
 *   `window.presented` flips in the store.
 * - Fit-height modals (`height === "fit"`) are sized by the host's one-shot
 *   `{type:"contentHeightMeasured"}` outbound message, exactly like the native
 *   window resize.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import type { CSSProperties, PointerEvent as ReactPointerEvent } from 'react';
import { useSandboxStore } from '../state/store';
import type { SimModalWindow } from '../state/types';
import { registerModalIframe, subscribeModalWindowOutbound, unregisterModalIframe } from './modal-connections';
import { forgetTutorialVideoSimulation, simulateTutorialVideoFullscreenKey } from './tutorial-video-window';
import './modal-window-frame.css';

/** Title bar height; must match --sandbox-modal-chrome-height in the CSS. */
const CHROME_HEIGHT = 34;
/** Placeholder body height for fit-height modals before the host measures. */
const FIT_FALLBACK_HEIGHT = 320;

interface DragState {
  originX: number;
  originY: number;
  pointerId: number;
  startX: number;
  startY: number;
}

export function ModalWindowFrame({ window: modalWindow }: { window: SimModalWindow }) {
  const { height, nonReactHostUrl, presented, title, width, windowId } = modalWindow;
  const closeModalWindow = useSandboxStore((state) => state.closeModalWindow);
  const emitEvent = useSandboxStore((state) => state.emitEvent);

  const panelRef = useRef<HTMLDivElement | null>(null);
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const dragRef = useRef<DragState | null>(null);
  const [position, setPosition] = useState<{ left: number; top: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const [measuredHeight, setMeasuredHeight] = useState<number | null>(null);

  useEffect(() => {
    const element = iframeRef.current;
    if (!element) {
      return;
    }
    if (nonReactHostUrl) {
      /*
       * Non-React host window (only the tutorial video, mirroring
       * `uses_react_modal_host() == false`): the document is YouTube's own
       * watch page, so there is no modal-host bridge to bind — no hydrate, no
       * open message, no ready/presented handshake.
       */
      return () => {
        forgetTutorialVideoSimulation(windowId);
      };
    }
    registerModalIframe(windowId, element);
    return () => {
      unregisterModalIframe(windowId);
    };
  }, [nonReactHostUrl, windowId]);

  useEffect(() => {
    if (height !== 'fit') {
      return;
    }
    return subscribeModalWindowOutbound(windowId, (message) => {
      if (message.type !== 'contentHeightMeasured') {
        return;
      }
      const nextHeight = message.height;
      if (typeof nextHeight === 'number' && Number.isFinite(nextHeight) && nextHeight > 0) {
        setMeasuredHeight(Math.round(nextHeight));
      }
    });
  }, [height, windowId]);

  const handlePointerMove = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) {
      return;
    }
    /*
     * Offsets are relative to the panel's offsetParent, which the desktop
     * centers with a zero-size slot — so the natural left/top are negative.
     * Never clamp to 0 here or the panel jumps on the first drag pixel.
     */
    setPosition({
      left: drag.originX + (event.clientX - drag.startX),
      top: drag.originY + (event.clientY - drag.startY),
    });
  }, []);

  const endDrag = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) {
      return;
    }
    dragRef.current = null;
    setDragging(false);
    event.currentTarget.releasePointerCapture?.(event.pointerId);
  }, []);

  const handlePointerDown = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) {
      return;
    }
    const panel = panelRef.current;
    if (!panel) {
      return;
    }
    const panelRect = panel.getBoundingClientRect();
    const parent = panel.offsetParent as HTMLElement | null;
    const parentRect = parent?.getBoundingClientRect();
    dragRef.current = {
      originX: panelRect.left - (parentRect?.left ?? 0),
      originY: panelRect.top - (parentRect?.top ?? 0),
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
    };
    setDragging(true);
    event.currentTarget.setPointerCapture?.(event.pointerId);
  }, []);

  const bodyHeight = height === 'fit' ? (measuredHeight ?? FIT_FALLBACK_HEIGHT) : height;
  const panelStyle: CSSProperties = {
    height: bodyHeight + CHROME_HEIGHT,
    width,
    ...(position
      ? { left: position.left, top: position.top }
      : { left: '50%', top: '50%', transform: 'translate(-50%, -50%)' }),
  };

  return (
    <div
      className={`sandbox-modal-window${dragging ? ' sandbox-modal-window-dragging' : ''}`}
      data-modal-kind={modalWindow.modal}
      ref={panelRef}
      style={panelStyle}
    >
      <div
        className='sandbox-modal-window-titlebar'
        onPointerCancel={endDrag}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={endDrag}
      >
        <button
          aria-label='Close window'
          className='sandbox-modal-window-close'
          onClick={() => closeModalWindow(windowId)}
          onPointerDown={(event) => event.stopPropagation()}
          type='button'
        >
          <span>×</span>
        </button>
        <div className='sandbox-modal-window-title'>{title}</div>
        <div className='sandbox-modal-window-badge'>{modalWindow.forced ? 'forced' : 'auto'}</div>
      </div>
      <div className='sandbox-modal-window-body'>
        <iframe
          allow='autoplay; encrypted-media; fullscreen; picture-in-picture'
          className={`sandbox-modal-window-iframe${
            presented ? (dragging ? ' sandbox-modal-window-iframe-inert' : '') : ' sandbox-modal-window-iframe-hidden'
          }`}
          onLoad={
            nonReactHostUrl
              ? () => {
                  const element = iframeRef.current;
                  if (!element) {
                    return;
                  }
                  simulateTutorialVideoFullscreenKey(windowId, element, (event) => {
                    emitEvent({
                      kind: 'modal',
                      label: event.label,
                      detail: event.detail,
                      codeRef: 'apps/desktop/src/app/consts.rs GHOSTEX_TUTORIAL_VIDEO_URL host key injection',
                    });
                  });
                }
              : undefined
          }
          ref={iframeRef}
          src={nonReactHostUrl ?? `/modal-window.html?windowId=${encodeURIComponent(windowId)}`}
          title={title}
        />
        {presented ? null : (
          <div className='sandbox-modal-window-loading'>
            <div className='sandbox-modal-window-spinner' />
            <div>waiting for {modalWindow.modal} to present…</div>
          </div>
        )}
      </div>
    </div>
  );
}
