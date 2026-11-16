import { useEffect } from 'react';
import './app-scrollbars.css';

let users = 0;
let dispose: (() => void) | undefined;

/**
 * CDXC:DesignSystem 2026-09-15 DECISION:
 * User: app scrollbars share the modal style: 5px, light/dark theme colors, hover-only, floating without pushing content. Preserve the sidebar scrollbar.
 *
 * CDXC:DesignSystem 2026-09-15 WHY:
 * Chromium's custom native scrollbar still consumes a gutter, so these draggable tracks decorate the existing scroll containers without wrapping or resizing their content.
 */
export function useAppScrollbars() {
  useEffect(retainAppScrollbars, []);
}

export function retainAppScrollbars() {
  if (users++ === 0) dispose = installAppScrollbars();
  return () => {
    if (--users === 0) {
      dispose?.();
      dispose = undefined;
    }
  };
}

/**
 * CDXC:Docs 2026-11-14 WHY:
 * The Files HTML preview runs in its own sealed origin, so Files can no longer reach into it to install these scrollbars; the preview's helper script runs this function's source inside the frame instead (`buildManageHtmlFrameHelperScript`). Keep it self-contained: everything it uses is declared inside it or read from `ownerWindow`.
 */
export function installAppScrollbars(ownerWindow: Window & typeof globalThis = globalThis.window) {
  const MODAL =
    '[data-slot="dialog-content"], [role="dialog"], [role="menu"], [role="listbox"], [data-slot="popover-content"], .confirm-modal';
  const PRESERVE = '.sidebar-reference-layout, .manage-sidebar';
  const BAR = '[data-app-scrollbar]';
  const window = ownerWindow;
  const { document, HTMLElement, Element, ResizeObserver, MutationObserver } = window;
  const getComputedStyle = window.getComputedStyle.bind(window);
  const requestAnimationFrame = window.requestAnimationFrame.bind(window);
  const cancelAnimationFrame = window.cancelAnimationFrame.bind(window);
  const documentRoot = document.documentElement;
  documentRoot.classList.add('gx-app-scrollbars');
  let hovered: Element | null = null;
  let frame = 0;
  let drag: { bar: HTMLDivElement; viewport: HTMLElement; horizontal: boolean; start: number; offset: number } | null =
    null;
  const layers = new Map<HTMLElement, HTMLDivElement>();
  const bars = new Map<HTMLElement, HTMLDivElement[]>();
  const observed = new Set<Element>();

  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(measure);
  };
  const resize = new ResizeObserver(schedule);

  function makeBar(viewport: HTMLElement, root: HTMLElement, horizontal: boolean) {
    let layer = layers.get(root);
    if (!layer) {
      layer = document.createElement('div');
      layer.dataset.appScrollbar = 'layer';
      layer.setAttribute('aria-hidden', 'true');
      if (root === document.body) layer.style.position = 'fixed';
      root.append(layer);
      layers.set(root, layer);
    }
    const bar = document.createElement('div');
    bar.dataset.appScrollbar = horizontal ? 'horizontal' : 'vertical';
    if (viewport.matches('.ghostex-chat-composer-lexical-content')) bar.dataset.scrollReveal = 'scroll';
    const thumb = document.createElement('div');
    thumb.dataset.appScrollbar = 'thumb';
    bar.append(thumb);
    layer.append(bar);
    bar.addEventListener(
      'wheel',
      (event) => {
        event.preventDefault();
        const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? viewport.clientHeight : 1;
        viewport.scrollBy({ left: event.deltaX * unit, top: event.deltaY * unit, behavior: 'instant' });
      },
      { passive: false }
    );
    bar.addEventListener('pointerdown', (event) => {
      if (event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      const rect = bar.getBoundingClientRect();
      const position = horizontal ? event.clientX : event.clientY;
      const size = horizontal ? viewport.clientWidth : viewport.clientHeight;
      const total = horizontal ? viewport.scrollWidth : viewport.scrollHeight;
      const length = horizontal ? rect.width : rect.height;
      const thumbLength = horizontal ? thumb.getBoundingClientRect().width : thumb.getBoundingClientRect().height;
      if (event.target === bar && length > thumbLength) {
        const offset =
          ((position - (horizontal ? rect.left : rect.top) - thumbLength / 2) / (length - thumbLength)) *
          (total - size);
        if (horizontal) viewport.scrollLeft = offset;
        else viewport.scrollTop = offset;
      }
      drag = {
        bar,
        viewport,
        horizontal,
        start: position,
        offset: horizontal ? viewport.scrollLeft : viewport.scrollTop,
      };
      bar.setPointerCapture(event.pointerId);
      schedule();
    });
    bar.addEventListener('pointermove', (event) => {
      if (!drag || drag.bar !== bar) return;
      const rect = bar.getBoundingClientRect();
      const thumbRect = thumb.getBoundingClientRect();
      const travel = horizontal ? rect.width - thumbRect.width : rect.height - thumbRect.height;
      if (travel <= 0) return;
      const range = horizontal
        ? viewport.scrollWidth - viewport.clientWidth
        : viewport.scrollHeight - viewport.clientHeight;
      const offset = drag.offset + (((horizontal ? event.clientX : event.clientY) - drag.start) / travel) * range;
      if (horizontal) viewport.scrollLeft = offset;
      else viewport.scrollTop = offset;
    });
    bar.addEventListener('pointerup', (event) => {
      const target = document.elementFromPoint(event.clientX, event.clientY);
      hovered = target?.closest(BAR) ? viewport : target;
      if (bar.hasPointerCapture(event.pointerId)) bar.releasePointerCapture(event.pointerId);
    });
    bar.addEventListener('lostpointercapture', () => {
      drag = null;
      schedule();
    });
    return bar;
  }

  function measure() {
    frame = 0;
    const active = new Set<HTMLElement>();
    const nextObserved = new Set<Element>();
    const targets = new Set<Element | null>([
      drag?.viewport ?? hovered,
      hovered?.closest('.ghostex-chat-markdown-table')?.querySelector('.ghostex-chat-markdown-table-scroll') ?? null,
      ...document.querySelectorAll('.ghostex-chat-composer-lexical-content[data-scrolling="true"]'),
    ]);
    for (const target of targets) {
      const root = target?.closest<HTMLElement>(MODAL) ?? document.body;
      if (root?.isConnected) {
        nextObserved.add(root);
        for (let node = target; node && (root === document.body || root.contains(node)); node = node.parentElement) {
          if (!(node instanceof HTMLElement) || node.closest(PRESERVE)) continue;
          // Chat owns its full-height track and drag navigation policy.
          // Decorating the same viewport here rendered two transcript scrollbars.
          if (node.dataset.appScrollbarOwner === 'chat') continue;
          if (
            node.matches('.ghostex-chat-composer-lexical-content') &&
            node.dataset.scrolling !== 'true' &&
            drag?.viewport !== node
          )
            continue;
          const style = getComputedStyle(node);
          const isDocument = node === document.scrollingElement;
          const vertical =
            (isDocument || /auto|scroll/.test(style.overflowY)) && node.scrollHeight > node.clientHeight + 1;
          const horizontal = /auto|scroll/.test(style.overflowX) && node.scrollWidth > node.clientWidth + 1;
          if (!vertical && !horizontal) continue;
          active.add(node);
          nextObserved.add(node);
          for (const child of node.children) if (!child.matches(BAR)) nextObserved.add(child);
          let tracks = bars.get(node);
          if (!tracks) {
            tracks = [makeBar(node, root, false), makeBar(node, root, true)];
            bars.set(node, tracks);
          }
          const layer = layers.get(root)!;
          const origin = layer.getBoundingClientRect();
          const rootRect = root.getBoundingClientRect();
          const scaleX = root.offsetWidth ? rootRect.width / root.offsetWidth : 1;
          const scaleY = root.offsetHeight ? rootRect.height / root.offsetHeight : 1;
          const rect = node.getBoundingClientRect();
          let left = rect.left + node.clientLeft * scaleX;
          let top = rect.top + node.clientTop * scaleY;
          let right = Math.min(window.innerWidth, left + node.clientWidth * scaleX);
          let bottom = Math.min(window.innerHeight, top + node.clientHeight * scaleY);
          if (isDocument) {
            left = 0;
            top = 0;
            right = window.innerWidth;
            bottom = window.innerHeight;
          }
          for (let parent = node.parentElement; parent && root.contains(parent); parent = parent.parentElement) {
            const bounds = parent.getBoundingClientRect();
            const overflow = getComputedStyle(parent);
            if (overflow.overflowX !== 'visible') {
              left = Math.max(left, bounds.left + parent.clientLeft * scaleX);
              right = Math.min(right, bounds.left + (parent.clientLeft + parent.clientWidth) * scaleX);
            }
            if (overflow.overflowY !== 'visible') {
              top = Math.max(top, bounds.top + parent.clientTop * scaleY);
              bottom = Math.min(bottom, bounds.top + (parent.clientTop + parent.clientHeight) * scaleY);
            }
          }
          tracks.forEach((bar, index) => {
            const horizontalAxis = index === 1;
            bar.hidden = !(horizontalAxis ? horizontal : vertical) || right <= left || bottom <= top;
            if (bar.hidden) return;
            bar.style.opacity = '1';
            bar.style.pointerEvents = 'auto';
            // The floating layer can live outside a locally themed chat or modal.
            bar.style.colorScheme = style.colorScheme;
            const schemes = style.colorScheme.split(/\s+/);
            bar.dataset.appScrollbarTheme =
              schemes.includes('dark') &&
              (!schemes.includes('light') || window.matchMedia('(prefers-color-scheme: dark)').matches)
                ? 'dark'
                : 'light';
            const length = Math.max(
              0,
              (horizontalAxis ? (right - left) / scaleX : (bottom - top) / scaleY) -
                4 -
                (vertical && horizontal ? 5 : 0)
            );
            const size = horizontalAxis ? node.clientWidth : node.clientHeight;
            const total = horizontalAxis ? node.scrollWidth : node.scrollHeight;
            const offset = horizontalAxis ? node.scrollLeft : node.scrollTop;
            const thumbLength = Math.min(length, Math.max(24, (size / total) * length));
            const progress = Math.max(0, Math.min(1, offset / (total - size)));
            Object.assign(bar.style, {
              left: `${((horizontalAxis ? left : right) - origin.left) / scaleX + (horizontalAxis ? 2 : -7)}px`,
              top: `${((horizontalAxis ? bottom : top) - origin.top) / scaleY + (horizontalAxis ? -7 : 2)}px`,
              width: `${horizontalAxis ? length : 5}px`,
              height: `${horizontalAxis ? 5 : length}px`,
            });
            Object.assign((bar.firstElementChild as HTMLElement).style, {
              width: horizontalAxis ? `${thumbLength}px` : '5px',
              height: horizontalAxis ? '5px' : `${thumbLength}px`,
              transform: `translate${horizontalAxis ? 'X' : 'Y'}(${progress * (length - thumbLength)}px)`,
            });
          });
        }
      }
    }
    for (const [viewport, tracks] of bars) {
      if (active.has(viewport)) continue;
      if (viewport.isConnected && viewport.matches('.ghostex-chat-composer-lexical-content')) {
        tracks.forEach((bar) => {
          bar.style.opacity = '0';
          bar.style.pointerEvents = 'none';
        });
        continue;
      }
      tracks.forEach((bar) => bar.remove());
      bars.delete(viewport);
    }
    for (const [root, layer] of layers) {
      if (layer.childElementCount) continue;
      layer.remove();
      layers.delete(root);
    }
    for (const element of observed) {
      if (nextObserved.has(element)) continue;
      resize.unobserve(element);
      observed.delete(element);
    }
    for (const element of nextObserved) {
      if (observed.has(element)) continue;
      resize.observe(element);
      observed.add(element);
    }
  }

  const hover = (event: PointerEvent) => {
    if (drag || !(event.target instanceof Element) || event.target.closest(BAR)) return;
    hovered = event.target;
    schedule();
  };
  const leave = () => {
    hovered = null;
    schedule();
  };
  const mutations = new MutationObserver((records) => {
    if (records.some((record) => !(record.target instanceof Element && record.target.closest(BAR)))) schedule();
  });
  mutations.observe(document.body, {
    childList: true,
    subtree: true,
    characterData: true,
    attributes: true,
    attributeFilter: ['data-scrolling', 'data-chat-theme', 'data-sidebar-theme', 'class'],
  });
  document.addEventListener('pointerover', hover, true);
  documentRoot.addEventListener('pointerleave', leave);
  document.addEventListener('scroll', schedule, true);
  document.addEventListener('input', schedule, true);
  window.addEventListener('resize', schedule);
  window.addEventListener('blur', leave);
  return () => {
    cancelAnimationFrame(frame);
    mutations.disconnect();
    resize.disconnect();
    layers.forEach((layer) => layer.remove());
    documentRoot.classList.remove('gx-app-scrollbars');
    document.removeEventListener('pointerover', hover, true);
    documentRoot.removeEventListener('pointerleave', leave);
    document.removeEventListener('scroll', schedule, true);
    document.removeEventListener('input', schedule, true);
    window.removeEventListener('resize', schedule);
    window.removeEventListener('blur', leave);
  };
}
