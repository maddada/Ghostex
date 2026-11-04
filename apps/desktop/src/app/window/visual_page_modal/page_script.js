// What the floating page window adds to every published page it shows (page.rs puts this file into
// each document the window loads). Everything runs inside the page; the page tells the app nothing.
//
// 1. A picture viewer over the page. Clicking a picture opens it centred over a dark backdrop,
//    with the chat image viewer's top-right buttons. Clicking it steps the zoom, scrolling zooms
//    around the pointer, and dragging pans. Arrows near the edges and the Left/Right keys step
//    through the page's pictures, and Escape, the close button or a click beside the picture closes it.
// 2. Scroll memory. The page saves its own scroll position on gxserver (`/visual/<id>/scroll`)
//    while the reader scrolls and when the page goes away, and goes back to it once it has
//    loaded again (a reopen, Refresh, or a restart).
(() => {
  if (window.top !== window || window.__ghostexPageScript) return;
  Object.defineProperty(window, '__ghostexPageScript', { value: true });

  // ---------------------------------------------------------------------------------------------
  // Scroll memory
  // ---------------------------------------------------------------------------------------------

  /** Waits this long after the last scroll before saving the position. */
  const SAVE_AFTER_MS = 400;
  /** Going back is tried this often, this many times, while pictures and fonts still grow the page. */
  const RESTORE_EVERY_MS = 100;
  const RESTORE_TRIES = 30;

  // The page puts its position back itself; Chromium's own restoring would race it on a reload.
  try { history.scrollRestoration = 'manual'; } catch (_) {}
  const pagePath = location.pathname.replace(/\/+$/, '');
  const scrollUrl = /^\/visual\/[0-9a-f]{32}$/.test(pagePath) ? pagePath + '/scroll' : null;
  /** The position gxserver has, so an unchanged one is not sent again. */
  let savedY = null;
  let saveTimer = 0;
  /** Stops the restore that is running; it carries the position it is going to (`target`). */
  let restoring = null;
  /** Set while the app pictures the top of the page for the chat card (`hold`), with the position to go back to. */
  let holding = null;
  /** The reader scrolled, clicked or typed, so their position wins over the remembered one. */
  let readerMoved = false;

  const saveScroll = (leaving) => {
    clearTimeout(saveTimer);
    saveTimer = 0;
    if (!scrollUrl || holding || restoring) return;
    const y = Math.round(scrollY);
    if (y === savedY) return;
    savedY = y;
    const body = JSON.stringify({ y });
    if (leaving && navigator.sendBeacon && navigator.sendBeacon(scrollUrl, body)) return;
    fetch(scrollUrl, { method: 'POST', body, keepalive: true }).catch(() => {});
  };

  const scrollBack = (y) => {
    if (restoring) restoring();
    let tries = 0;
    let timer = 0;
    const stop = () => {
      clearTimeout(timer);
      restoring = null;
    };
    stop.target = y;
    restoring = stop;
    const go = () => {
      scrollTo({ left: scrollX, top: y, behavior: 'instant' });
      if (Math.abs(scrollY - y) > 1 && ++tries < RESTORE_TRIES) timer = setTimeout(go, RESTORE_EVERY_MS);
      else stop();
    };
    go();
  };

  for (const kind of ['wheel', 'keydown', 'pointerdown', 'touchstart']) {
    addEventListener(kind, () => {
      readerMoved = true;
      if (restoring) restoring();
      if (holding) holding.touched = true;
    }, { passive: true, capture: true });
  }
  addEventListener('scroll', () => {
    // Held at the top for the picture: keys or the scrollbar moved it, so it goes straight back.
    if (holding && (scrollX || scrollY)) scrollTo({ left: 0, top: 0, behavior: 'instant' });
    if (holding || restoring) return;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(saveScroll, SAVE_AFTER_MS);
  }, { passive: true });
  addEventListener('pagehide', () => saveScroll(true));
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') saveScroll(true);
  });

  if (scrollUrl) {
    fetch(scrollUrl)
      .then((response) => (response.ok ? response.json() : null))
      .then((answer) => {
        const y = answer && Number.isFinite(answer.y) ? Math.max(0, answer.y) : 0;
        if (savedY === null) savedY = y;
        if (y <= 0 || readerMoved) return;
        const start = () => {
          if (holding) holding.y = y;
          else if (!readerMoved) scrollBack(y);
        };
        if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start, { once: true });
        else start();
      })
      .catch(() => {});
  }

  // The chat card's picture is the top of the page, and the app takes it from the page on screen
  // (app/visual_page_thumbnails.rs), so it asks the page to wait at the top while it does. The
  // reader's wheel and finger do not move it meanwhile: a page scrolled when the picture is taken
  // comes out as an empty band above a strip of its middle, and a trackpad's momentum from the
  // reader's first flick outlasts a single scroll back to the top. Only during the hold, because
  // a wheel listener that can cancel takes scrolling off Chromium's fast path.
  const stopScrolling = (event) => {
    if (event.cancelable) event.preventDefault();
  };
  const PINNED = { passive: false, capture: true };
  const release = () => {
    const held = holding;
    holding = null;
    removeEventListener('wheel', stopScrolling, PINNED);
    removeEventListener('touchmove', stopScrolling, PINNED);
    if (held && !held.touched && held.y > 0) scrollBack(held.y);
  };
  Object.defineProperty(window, '__ghostexPageScroll', {
    value: Object.freeze({
      hold() {
        if (holding) return;
        const y = restoring ? restoring.target : scrollY;
        if (restoring) restoring();
        holding = { y, touched: false };
        addEventListener('wheel', stopScrolling, PINNED);
        addEventListener('touchmove', stopScrolling, PINNED);
        scrollTo({ left: 0, top: 0, behavior: 'instant' });
        // Never stuck at the top when the app's release does not come.
        setTimeout(() => { if (holding) release(); }, 15000);
      },
      release,
    }),
  });

  // ---------------------------------------------------------------------------------------------
  // Picture viewer
  // ---------------------------------------------------------------------------------------------

  // Pictures inside links, buttons, form controls and the question kit's choices keep doing what
  // the page made them do; so does a click the page handled itself (`defaultPrevented`).
  const SKIP = 'a[href],button,label,summary,select,input,textarea,[role=button],[role=link],[role=radio],[role=checkbox],[role=option],[onclick],[contenteditable],gx-option,gx-questions .gx-choice,[data-gx-no-viewer]';
  /** Pictures smaller than this on the page are icons, not pictures to look at. */
  const MIN_SIZE = 24;
  /** The fitted picture's share of the window, as in the chat's viewer. */
  const FIT_WIDTH = 0.9;
  const FIT_HEIGHT = 0.75;
  /** Clicks between the fitted size and the picture's own pixels; one more goes back to fitted. */
  const ZOOM_STEPS = 3;
  /** A press that travels further than this is a drag that pans, not a click that zooms. */
  const DRAG_THRESHOLD = 4;
  /** How close to a side the pointer comes for that side's arrow to show. */
  const EDGE_REACH = 120;

  const sourceOf = (img) => img.currentSrc || img.src || '';
  const viewable = (img) => {
    if (!(img instanceof HTMLImageElement) || !img.isConnected || !img.complete || !img.naturalWidth) return false;
    if (img.closest(SKIP) || img.closest('[data-gx-picture-viewer]')) return false;
    const box = img.getBoundingClientRect();
    return box.width >= MIN_SIZE && box.height >= MIN_SIZE && /^(data:image\/|https:|blob:)/i.test(sourceOf(img));
  };
  const nameOf = (src) => {
    if (/^data:/i.test(src)) {
      const type = ((/^data:image\/([a-z0-9.+-]+)/i.exec(src) || [])[1] || 'png').toLowerCase();
      return 'image.' + ({ jpeg: 'jpg', 'svg+xml': 'svg' }[type] || type);
    }
    try { return decodeURIComponent(new URL(src).pathname.split('/').pop()) || 'image'; } catch (_) { return 'image'; }
  };

  // The chat viewer's icons (apps/desktop/assets/titlebar).
  const icon = (paths, width = 2) => `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${width}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths}</svg>`;
  const ICONS = {
    zoomIn: icon('<path d="M3 10a7 7 0 1 0 14 0a7 7 0 1 0 -14 0"/><path d="M7 10h6"/><path d="M10 7v6"/><path d="M21 21l-6 -6"/>'),
    zoomOut: icon('<path d="M3 10a7 7 0 1 0 14 0a7 7 0 1 0 -14 0"/><path d="M7 10h6"/><path d="M21 21l-6 -6"/>'),
    save: icon('<path d="M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2 -2v-2"/><path d="M7 11l5 5l5 -5"/><path d="M12 4l0 12"/>'),
    open: icon('<path d="M12 6h-6a2 2 0 0 0 -2 2v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2 -2v-6"/><path d="M11 13l9 -9"/><path d="M15 4h5v5"/>'),
    copy: icon('<path d="M7 9.667a2.667 2.667 0 0 1 2.667 -2.667h8.666a2.667 2.667 0 0 1 2.667 2.667v8.666a2.667 2.667 0 0 1 -2.667 2.667h-8.666a2.667 2.667 0 0 1 -2.667 -2.667l0 -8.666"/><path d="M4.012 16.737a2.005 2.005 0 0 1 -1.012 -1.737v-10c0 -1.1 .9 -2 2 -2h10c.75 0 1.158 .385 1.5 1"/>'),
    done: icon('<path d="m5 12 4 4L19 6"/>'),
    failed: icon('<path d="M18 6l-12 12"/><path d="M6 6l12 12"/>'),
    close: icon('<path d="M18 6l-12 12"/><path d="M6 6l12 12"/>', 1.8),
    previous: icon('<path d="M15 6l-6 6l6 6"/>', 2.4),
    next: icon('<path d="M9 6l6 6l-6 6"/>', 2.4),
  };

  // In a shadow root, so the page's own styles never reach the viewer and the viewer's never leak out.
  const STYLE = `
    :host { all: initial; position: fixed; inset: 0; z-index: 2147483647; }
    .viewer { position: absolute; inset: 0; overflow: hidden; background: rgba(0, 0, 0, 0.7);
      font: 13px -apple-system, BlinkMacSystemFont, system-ui, sans-serif; touch-action: none;
      user-select: none; -webkit-user-select: none; outline: none; }
    .picture { position: absolute; max-width: none; max-height: none; -webkit-user-drag: none; }
    .group, .close, .arrow button { --surface: rgba(0, 0, 0, 0.5); --edge: rgba(255, 255, 255, 0.14);
      --ink: #fafafa; --hover: rgba(255, 255, 255, 0.12); }
    .light .group, .light .close, .light .arrow button { --surface: rgba(255, 255, 255, 0.9);
      --edge: rgba(0, 0, 0, 0.12); --ink: #171717; --hover: rgba(0, 0, 0, 0.06); }
    button { all: unset; box-sizing: border-box; display: flex; align-items: center; justify-content: center;
      color: var(--ink); cursor: pointer; }
    button:hover, button:focus-visible { background: var(--hover); }
    svg { display: block; }
    .group { position: absolute; top: 12px; right: 56px; display: flex; border: 1px solid var(--edge);
      border-radius: 8px; background: var(--surface); overflow: hidden; }
    .group:empty { display: none; }
    .group button { width: 28px; height: 28px; }
    .group svg { width: 15px; height: 15px; }
    .close { position: absolute; top: 12px; right: 12px; width: 32px; height: 32px; border-radius: 50%;
      background: var(--surface); }
    .light .close { border: 1px solid var(--edge); }
    .close svg { width: 18px; height: 18px; }
    .arrow { position: absolute; top: 0; bottom: 0; display: flex; align-items: center;
      pointer-events: none; opacity: 0; transition: opacity 120ms; }
    .arrow.previous { left: 16px; }
    .arrow.next { right: 16px; }
    .arrow.shown { opacity: 1; }
    .arrow.shown button { pointer-events: auto; }
    .arrow button { width: 44px; height: 44px; border-radius: 50%; background: var(--surface); }
    .light .arrow button { border: 1px solid var(--edge); }
    .arrow svg { width: 22px; height: 22px; }
  `;

  // A button's name: the page kit's tooltip (`data-gx-tip`, theme.rs and kit.rs in
  // server/src/visual_pages), the app's bubble with its key as a cap, or the browser's own `title`
  // on a page the kit is not in.
  const label = (element, text, keys) => {
    element.setAttribute('aria-label', keys ? `${text} (${keys})` : text);
    if (window.__ghostexTips) {
      element.removeAttribute('title');
      element.setAttribute('data-gx-tip', text);
      if (keys) element.setAttribute('data-gx-tip-keys', keys);
      else element.removeAttribute('data-gx-tip-keys');
      window.__ghostexTips.refresh(element);
    } else {
      element.title = keys ? `${text} (${keys})` : text;
    }
  };

  const button = (className, text, svg, keys) => {
    const element = document.createElement('button');
    element.type = 'button';
    if (className) element.className = className;
    label(element, text, keys);
    element.innerHTML = svg;
    return element;
  };

  const canCopy = () => window.isSecureContext && !!navigator.clipboard && !!navigator.clipboard.write && typeof ClipboardItem === 'function';

  /** The picture as PNG bytes, read from the page's own copy of it. */
  const pngOf = (picture) => new Promise((resolve, reject) => {
    const canvas = document.createElement('canvas');
    canvas.width = picture.width;
    canvas.height = picture.height;
    try {
      canvas.getContext('2d').drawImage(picture.element, 0, 0, canvas.width, canvas.height);
      canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error('no picture'))), 'image/png');
    } catch (error) {
      reject(error);
    }
  });

  let open = null;

  const openViewer = (pictures, index) => {
    if (open) open.close();
    const host = document.createElement('div');
    host.setAttribute('data-gx-picture-viewer', '');
    const root = host.attachShadow({ mode: 'open' });
    const theme = document.documentElement.getAttribute('data-gx-theme');
    const light = theme ? theme === 'light' : matchMedia('(prefers-color-scheme: light)').matches;
    root.innerHTML = `<style>${STYLE}</style>`;
    const viewer = document.createElement('div');
    viewer.className = 'viewer' + (light ? ' light' : '');
    viewer.tabIndex = -1;
    viewer.setAttribute('role', 'dialog');
    viewer.setAttribute('aria-label', 'Image preview');
    const img = document.createElement('img');
    img.className = 'picture';
    img.alt = '';
    img.draggable = false;
    const group = document.createElement('div');
    group.className = 'group';
    const close = button('close', 'Close', ICONS.close, 'Esc');
    const arrows = [-1, 1].map((delta) => {
      const side = document.createElement('div');
      side.className = 'arrow ' + (delta < 0 ? 'previous' : 'next');
      const step = button('', delta < 0 ? 'Previous image' : 'Next image', delta < 0 ? ICONS.previous : ICONS.next, delta < 0 ? '←' : '→');
      step.addEventListener('click', () => show(index + delta));
      side.append(step);
      return side;
    });
    viewer.append(img, ...(pictures.length > 1 ? arrows : []), group, close);
    root.append(viewer);

    // The picture's box: `width` across, top-left corner at `x`, `y` in the window.
    let fitted = 0;
    let widths = [];
    let level = 0;
    let width = 0;
    let x = 0;
    let y = 0;
    let press = null;
    const picture = () => pictures[index];
    const heightOf = (w) => (w * picture().height) / picture().width;
    const zoomable = () => widths.length > 0;

    const place = () => {
      const vw = innerWidth;
      const vh = innerHeight;
      const h = heightOf(width);
      x = width <= vw ? (vw - width) / 2 : Math.min(0, Math.max(vw - width, x));
      y = h <= vh ? (vh - h) / 2 : Math.min(0, Math.max(vh - h, y));
      img.style.left = x + 'px';
      img.style.top = y + 'px';
      img.style.width = width + 'px';
      img.style.height = h + 'px';
      img.style.cursor = press && press.panning ? 'grabbing'
        : !zoomable() ? 'default' : level >= ZOOM_STEPS ? 'zoom-out' : 'zoom-in';
      const zoomButton = group.querySelector('.zoom');
      if (zoomButton) {
        const out = level >= ZOOM_STEPS;
        zoomButton.innerHTML = out ? ICONS.zoomOut : ICONS.zoomIn;
        label(zoomButton, out ? 'Zoom out' : 'Zoom in');
      }
    };

    // The chat viewer's sizes: fitted inside 90% × 75% of the window and never past the picture's
    // own pixels; the zoom steps grow geometrically from there to 1:1, and a picture already shown
    // at 1:1 has none.
    const fit = () => {
      const { width: nw, height: nh } = picture();
      fitted = nw * Math.min((innerWidth * FIT_WIDTH) / nw, (innerHeight * FIT_HEIGHT) / nh, 1);
      widths = [];
      if (nw > fitted + 1) {
        const ratio = Math.pow(nw / fitted, 1 / ZOOM_STEPS);
        for (let step = 1; step < ZOOM_STEPS; step++) widths.push(fitted * Math.pow(ratio, step));
        widths.push(nw);
      }
    };

    // Zooms to `next` keeping the spot under (`atX`, `atY`) where it is.
    const zoomTo = (next, atX, atY) => {
      const fx = (atX - x) / width;
      const fy = (atY - y) / heightOf(width);
      x = atX - fx * next;
      y = atY - fy * heightOf(next);
      width = next;
      place();
    };

    const stepZoom = (atX, atY) => {
      if (!zoomable()) return;
      level = level >= ZOOM_STEPS ? 0 : level + 1;
      zoomTo(level === 0 ? fitted : widths[level - 1], atX, atY);
    };

    const actions = () => {
      group.replaceChildren();
      const src = picture().src;
      if (zoomable()) {
        const zoom = button('zoom', 'Zoom in', ICONS.zoomIn);
        zoom.addEventListener('click', () => stepZoom(innerWidth / 2, innerHeight / 2));
        group.append(zoom);
      }
      // A sandboxed page may not download, so only a picture on the web can be handed on: it
      // opens in the reader's browser.
      if (/^https:/i.test(src)) {
        const openButton = button('', 'Open image', ICONS.open);
        openButton.addEventListener('click', () => window.open(src, '_blank', 'noopener'));
        group.append(openButton);
      }
      if (canCopy()) {
        const copy = button('', 'Copy image', ICONS.copy);
        let reset = 0;
        copy.addEventListener('click', () => {
          const shown = picture();
          const outcome = (ok) => {
            if (picture() !== shown) return;
            copy.innerHTML = ok ? ICONS.done : ICONS.failed;
            label(copy, ok ? 'Image copied' : "This image can't be copied from the page");
            clearTimeout(reset);
            reset = setTimeout(() => { copy.innerHTML = ICONS.copy; label(copy, 'Copy image'); }, 1600);
          };
          // The bytes are passed as a promise so the click still counts as the reader's gesture.
          navigator.clipboard.write([new ClipboardItem({ 'image/png': pngOf(shown) })])
            .then(() => outcome(true), () => outcome(false));
        });
        group.append(copy);
      }
    };

    const show = (next) => {
      index = ((next % pictures.length) + pictures.length) % pictures.length;
      img.src = picture().src;
      img.setAttribute('aria-label', picture().name);
      fit();
      level = 0;
      width = fitted;
      actions();
      place();
    };

    const stop = (event) => {
      event.preventDefault();
      event.stopImmediatePropagation();
    };

    const keys = (event) => {
      switch (event.key) {
        case 'Escape': stop(event); open.close(); break;
        case 'ArrowLeft': case 'ArrowUp': stop(event); show(index - 1); break;
        case 'ArrowRight': case 'ArrowDown': stop(event); show(index + 1); break;
        case 'Enter': case ' ':
          // The buttons keep their own Enter and Space.
          if (root.activeElement && root.activeElement.tagName === 'BUTTON') return;
          stop(event); stepZoom(innerWidth / 2, innerHeight / 2); break;
        // Nothing scrolls the page underneath while the viewer is open.
        case 'PageUp': case 'PageDown': case 'Home': case 'End': stop(event); break;
      }
    };

    viewer.addEventListener('wheel', (event) => {
      event.preventDefault();
      if (!zoomable()) return;
      // A pinch arrives as a wheel with Ctrl held, in finer steps than a wheel's.
      const factor = Math.exp(-event.deltaY * (event.ctrlKey ? 0.01 : 0.002));
      const next = Math.min(widths[widths.length - 1], Math.max(fitted, width * factor));
      level = next >= widths[widths.length - 1] - 0.5 ? ZOOM_STEPS : widths.filter((w) => next >= w - 0.5).length;
      zoomTo(next, event.clientX, event.clientY);
    }, { passive: false });

    img.addEventListener('pointerdown', (event) => {
      if (event.button !== 0) return;
      event.preventDefault();
      img.setPointerCapture(event.pointerId);
      press = { id: event.pointerId, startX: event.clientX, startY: event.clientY, x, y, panning: false };
    });
    img.addEventListener('pointermove', (event) => {
      if (!press || press.id !== event.pointerId) return;
      const dx = event.clientX - press.startX;
      const dy = event.clientY - press.startY;
      if (!press.panning && Math.hypot(dx, dy) < DRAG_THRESHOLD) return;
      press.panning = true;
      x = press.x + dx;
      y = press.y + dy;
      place();
    });
    const release = (event) => {
      if (!press || press.id !== event.pointerId) return;
      const clicked = !press.panning && event.type === 'pointerup';
      press = null;
      if (clicked) stepZoom(event.clientX, event.clientY);
      else place();
    };
    img.addEventListener('pointerup', release);
    img.addEventListener('pointercancel', release);

    // A press beside the picture (not on a button) closes the viewer, as in the chat.
    viewer.addEventListener('pointerdown', (event) => {
      if (event.target === viewer) open.close();
    });
    viewer.addEventListener('contextmenu', (event) => {
      if (event.target === viewer) { event.preventDefault(); open.close(); }
    });
    viewer.addEventListener('pointermove', (event) => {
      const edge = event.clientX < EDGE_REACH ? -1 : event.clientX > innerWidth - EDGE_REACH ? 1 : 0;
      arrows[0].classList.toggle('shown', edge < 0);
      arrows[1].classList.toggle('shown', edge > 0);
    });
    close.addEventListener('click', () => open.close());
    const resized = () => {
      fit();
      if (level === 0 || width < fitted) {
        level = 0;
        width = fitted;
      }
      place();
    };

    const before = document.activeElement;
    addEventListener('keydown', keys, true);
    addEventListener('resize', resized);
    open = {
      close() {
        removeEventListener('keydown', keys, true);
        removeEventListener('resize', resized);
        host.remove();
        open = null;
        if (before && before.focus) before.focus({ preventScroll: true });
      },
    };
    document.documentElement.append(host);
    show(index);
    viewer.focus({ preventScroll: true });
  };

  document.addEventListener('mouseover', (event) => {
    const img = event.target;
    if (img instanceof HTMLImageElement && !img.style.cursor && viewable(img)) img.style.cursor = 'zoom-in';
  }, true);
  document.addEventListener('click', (event) => {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    const clicked = event.target;
    if (!viewable(clicked)) return;
    event.preventDefault();
    const pictures = [];
    let index = -1;
    for (const img of document.images) {
      if (!viewable(img)) continue;
      if (img === clicked) index = pictures.length;
      pictures.push({ element: img, src: sourceOf(img), name: nameOf(sourceOf(img)), width: img.naturalWidth, height: img.naturalHeight });
    }
    if (index >= 0) openViewer(pictures, index);
  });
})();
