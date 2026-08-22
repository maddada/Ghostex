// @ts-nocheck
import { gitDiffLineFlagsField } from './gitDiffGutter';
import { getLiveGitCollapsedBlockAtLine, getLiveRenderedBlockAtLine } from './liveRenderedBlocks';

const hoverDelayMs = 0;
const defaultGutterHoverHitLeftPx = 0;
const defaultGutterHoverHitWidthPx = 10;
const gutterClickDragThresholdPx = 4;
const gutterClickDragThresholdSquared = gutterClickDragThresholdPx * gutterClickDragThresholdPx;

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function parseCssPixelValue(value, fallback) {
  const parsed = Number.parseFloat(value ?? '');
  return Number.isFinite(parsed) ? parsed : fallback;
}

function getGutterHoverBounds(gutter, gutterRect) {
  const marker = gutter.querySelector('.meo-git-gutter-marker');
  const markerBeforeStyle = marker instanceof HTMLElement ? window.getComputedStyle(marker, '::before') : null;
  const hitLeftOffset = parseCssPixelValue(
    markerBeforeStyle?.left,
    defaultGutterHoverHitLeftPx
  );
  const hitWidth = Math.max(
    0,
    parseCssPixelValue(markerBeforeStyle?.width, defaultGutterHoverHitWidthPx)
  );

  return {
    left: gutterRect.left + Math.min(0, hitLeftOffset),
    right: gutterRect.left + Math.max(gutterRect.width, hitLeftOffset + hitWidth)
  };
}

function formatAbsoluteDate(unixSeconds) {
  if (!Number.isFinite(unixSeconds) || unixSeconds <= 0) {
    return '';
  }
  try {
    return new Intl.DateTimeFormat(undefined, {
      year: 'numeric',
      month: 'short',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit'
    }).format(new Date(unixSeconds * 1000));
  } catch {
    return new Date(unixSeconds * 1000).toLocaleString();
  }
}

function buildTooltipDom() {
  const root = document.createElement('div');
  root.className = 'meo-git-blame-tooltip';
  root.hidden = true;

  const title = document.createElement('div');
  title.className = 'meo-git-blame-title';
  root.appendChild(title);

  const meta = document.createElement('div');
  meta.className = 'meo-git-blame-meta';
  root.appendChild(meta);

  return { root, title, meta };
}

function buildGutterHoverOverlayDom() {
  const root = document.createElement('div');
  root.className = 'meo-git-hover-overlay';
  root.hidden = true;
  return root;
}

function renderBlameResult(ui, result) {
  if (!result || typeof result !== 'object') {
    ui.title.textContent = 'Blame unavailable';
    ui.meta.textContent = '';
    return;
  }

  if (result.kind === 'commit') {
    ui.title.textContent = result.summary || '(no commit message)';
    const date = formatAbsoluteDate(result.authorTimeUnix);
    const parts = [
      result.author || 'Unknown',
      result.shortCommit || (result.commit ? result.commit.slice(0, 8) : '')
    ].filter(Boolean);
    if (date) {
      parts.push(date);
    }
    ui.meta.textContent = parts.join(' · ');
    return;
  }

  if (result.kind === 'uncommitted') {
    ui.title.textContent = 'Uncommitted changes';
    ui.meta.textContent = '';
    return;
  }

  ui.title.textContent = 'Blame unavailable';
  ui.meta.textContent = result.reason ? `${result.reason}` : '';
}

function shouldShowBlameTooltip(result) {
  if (!result || typeof result !== 'object') {
    return false;
  }
  return result.kind === 'commit' || result.kind === 'uncommitted';
}

function isSupportedMode(mode) {
  return mode === 'source' || mode === 'live';
}

function getRenderedBlockTarget(target) {
  if (!(target instanceof Element)) {
    return null;
  }
  return target.closest('[data-meo-rendered-block-kind][data-meo-rendered-block-start-line][data-meo-rendered-block-end-line]');
}

function getRenderedBlockRangeFromElement(block) {
  if (!(block instanceof HTMLElement)) {
    return null;
  }
  const startLine = Number.parseInt(block.dataset.meoRenderedBlockStartLine ?? '', 10);
  const endLine = Number.parseInt(block.dataset.meoRenderedBlockEndLine ?? '', 10);
  if (!Number.isInteger(startLine) || !Number.isInteger(endLine) || startLine < 1 || endLine < startLine) {
    return null;
  }
  return { startLine, endLine };
}

function getRenderedBlockLineRange(target) {
  return getRenderedBlockRangeFromElement(getRenderedBlockTarget(target));
}

function getRenderedBlockLineRangeAtClientY(view, clientY) {
  for (const node of view.dom.querySelectorAll('[data-meo-rendered-block-kind][data-meo-rendered-block-start-line][data-meo-rendered-block-end-line]')) {
    const range = getRenderedBlockRangeFromElement(node);
    if (!range) continue;
    const rect = node.getBoundingClientRect();
    const top = Math.min(rect.top, rect.bottom);
    const bottom = Math.max(rect.top, rect.bottom);
    if (clientY < top || clientY > bottom) {
      continue;
    }
    return range;
  }

  const viewAny = /** @type {any} */ (view);
  if (typeof viewAny.lineBlockAtHeight === 'function') {
    const block = viewAny.lineBlockAtHeight(clientY - view.documentTop);
    if (block && Number.isFinite(block.from)) {
      const lineNo = view.state.doc.lineAt(block.from).number;
      const renderedBlock = getLiveRenderedBlockAtLine(view.state, lineNo);
      if (renderedBlock) {
        return { startLine: renderedBlock.startLine, endLine: renderedBlock.endLine };
      }
    }
  }

  const contentRect = view.contentDOM.getBoundingClientRect();
  const probeX = clamp(
    contentRect.left + 4,
    contentRect.left + 1,
    Math.max(contentRect.left + 1, contentRect.right - 1)
  );
  const pos = view.posAtCoords({ x: probeX, y: clientY });
  if (pos !== null) {
    const lineNo = view.state.doc.lineAt(pos).number;
    const renderedBlock = getLiveRenderedBlockAtLine(view.state, lineNo);
    if (renderedBlock) {
      return { startLine: renderedBlock.startLine, endLine: renderedBlock.endLine };
    }
  }

  return null;
}

function getRenderedBlockElement(view, lineRange, target = null) {
  const targetBlock = getRenderedBlockTarget(target);
  const targetRange = getRenderedBlockRangeFromElement(targetBlock);
  if (
    targetBlock instanceof HTMLElement &&
    (
      !lineRange ||
      (targetRange && targetRange.startLine === lineRange.startLine && targetRange.endLine === lineRange.endLine)
    )
  ) {
    return targetBlock;
  }

  if (
    !lineRange ||
    !Number.isInteger(lineRange.startLine) ||
    !Number.isInteger(lineRange.endLine)
  ) {
    return null;
  }

  const selector = (
    `[data-meo-rendered-block-kind][data-meo-rendered-block-start-line="${lineRange.startLine}"]` +
    `[data-meo-rendered-block-end-line="${lineRange.endLine}"]`
  );
  const block = view.dom.querySelector(selector);
  return block instanceof HTMLElement ? block : null;
}

function getLiveBlockLineRangeFromMarker(marker) {
  if (!(marker instanceof HTMLElement)) {
    return null;
  }
  const startLine = Number.parseInt(marker.dataset.meoLiveBlockStartLine ?? '', 10);
  const endLine = Number.parseInt(marker.dataset.meoLiveBlockEndLine ?? '', 10);
  if (!Number.isInteger(startLine) || !Number.isInteger(endLine) || startLine < 1 || endLine < startLine) {
    return null;
  }
  return { startLine, endLine };
}

function positionTooltip(ui, anchorRect) {
  const tooltip = ui.root;
  tooltip.style.left = '0px';
  tooltip.style.top = '0px';
  tooltip.hidden = false;

  const rect = tooltip.getBoundingClientRect();
  let left = anchorRect.right + 8;
  if (left + rect.width > window.innerWidth - 8) {
    left = anchorRect.left - rect.width - 8;
  }
  left = clamp(left, 8, Math.max(8, window.innerWidth - rect.width - 8));
  const anchorMidY = ((anchorRect.top ?? 0) + (anchorRect.bottom ?? anchorRect.top ?? 0)) / 2;
  const top = clamp(anchorMidY - rect.height / 2, 8, Math.max(8, window.innerHeight - rect.height - 8));

  tooltip.style.left = `${Math.round(left)}px`;
  tooltip.style.top = `${Math.round(top)}px`;
}

function isTrailingEofVisualLine(doc, lineNumber) {
  if (!doc || doc.length <= 0 || doc.lines <= 1 || lineNumber !== doc.lines) {
    return false;
  }
  const lastLine = doc.line(doc.lines);
  return lastLine.from === lastLine.to;
}

function getMarkerChangeKind(marker) {
  if (!(marker instanceof HTMLElement)) {
    return null;
  }
  if (marker.classList.contains('is-added')) {
    return 'added';
  }
  if (marker.classList.contains('is-modified')) {
    return 'modified';
  }
  return null;
}

function isChangedMarker(marker) {
  return getMarkerChangeKind(marker) !== null;
}

function getLineFlagChangeKind(lineFlags, lineNumber) {
  if (!Array.isArray(lineFlags) || !Number.isInteger(lineNumber) || lineNumber < 1) {
    return null;
  }
  const flags = lineFlags[lineNumber - 1];
  if (!flags) {
    return null;
  }
  if (flags.modified || flags.trailingEofProxyOnly) {
    return 'modified';
  }
  if (flags.added) {
    return 'added';
  }
  return null;
}

function getLineRangeChangeKind(lineFlags, startLine, endLine) {
  if (
    !Array.isArray(lineFlags) ||
    !Number.isInteger(startLine) ||
    !Number.isInteger(endLine) ||
    startLine < 1 ||
    endLine < startLine
  ) {
    return null;
  }

  let hasAdded = false;
  for (let lineNo = startLine; lineNo <= endLine; lineNo += 1) {
    const lineKind = getLineFlagChangeKind(lineFlags, lineNo);
    if (lineKind === 'modified') {
      return 'modified';
    }
    if (lineKind === 'added') {
      hasAdded = true;
    }
  }

  return hasAdded ? 'added' : null;
}

function normalizeTrailingEofVisualLineHit(doc, lineNumber, gutterRowElement, markerElement = null) {
  const rowMarker = gutterRowElement?.querySelector?.('.meo-git-gutter-marker') ?? null;
  const hitMarker = markerElement instanceof HTMLElement ? markerElement : null;
  if (!isTrailingEofVisualLine(doc, lineNumber)) {
    const changedMarker = (
      isChangedMarker(rowMarker)
        ? rowMarker
        : isChangedMarker(hitMarker)
          ? hitMarker
          : null
    );
    return {
      lineNumber,
      requestLineNumber: lineNumber,
      proxiedFromTrailingEof: false,
      effectiveChangeKind: getMarkerChangeKind(changedMarker)
    };
  }
  const previousRowMarker = (
    gutterRowElement instanceof HTMLElement
      ? gutterRowElement.previousElementSibling?.querySelector?.('.meo-git-gutter-marker') ?? null
      : null
  );
  const changedMarker = (
    isChangedMarker(rowMarker)
      ? rowMarker
      : isChangedMarker(hitMarker)
        ? hitMarker
      : isChangedMarker(previousRowMarker)
        ? previousRowMarker
        : null
  );
  // The synthetic trailing EOF row should always proxy to the previous real line so
  // unchanged last lines still support blame hover/click like the line above.
  return {
    lineNumber: Math.max(1, lineNumber - 1),
    requestLineNumber: lineNumber,
    proxiedFromTrailingEof: true,
    effectiveChangeKind: getMarkerChangeKind(changedMarker)
  };
}

export function createGitBlameHoverController({
  view,
  getMode,
  requestBlame,
  openRevisionForLine,
  openWorktreeForLine
}) {
  const ui = buildTooltipDom();
  const hoverOverlay = buildGutterHoverOverlayDom();
  document.body.appendChild(ui.root);
  document.body.appendChild(hoverOverlay);

  let hoverTimer = null;
  let activeLineNumber = 0;
  let hoverToken = 0;
  let destroyed = false;
  let lastAnchorRect = null;
  let activeMarkerElements = [];
  let activeGutterRowElement = null;
  let activeGutterRowHoverKind = null;
  let activeRenderedBlockRange = null;
  let pendingBlameLineNumber = 0;
  let pointerDownInBand = false;
  let pointerDownWasLeftButton = false;
  let pointerDownClientX = 0;
  let pointerDownClientY = 0;

  const pointerMoveDistanceSquaredFromDown = (event) => {
    const deltaX = event.clientX - pointerDownClientX;
    const deltaY = event.clientY - pointerDownClientY;
    return (deltaX * deltaX) + (deltaY * deltaY);
  };

  const hasPointerMovedPastDragThreshold = (event) => (
    pointerMoveDistanceSquaredFromDown(event) > gutterClickDragThresholdSquared
  );

  const getGutterBandLayout = () => {
    const gutter = view.dom.querySelector('.cm-gutter.meo-git-gutter');
    if (!(gutter instanceof HTMLElement)) {
      return null;
    }
    const gutterRect = gutter.getBoundingClientRect();
    const contentRect = view.contentDOM.getBoundingClientRect();
    const firstBlock = view.lineBlockAt(0);
    const lastBlock = view.lineBlockAt(view.state.doc.length);
    const docTop = Number.isFinite(firstBlock?.top) ? view.documentTop + firstBlock.top : gutterRect.top;
    const docBottom = Number.isFinite(lastBlock?.bottom) ? view.documentTop + lastBlock.bottom : gutterRect.bottom;
    const hoverBounds = getGutterHoverBounds(gutter, gutterRect);
    return {
      gutter,
      gutterRect,
      contentRect,
      bandLeft: hoverBounds.left,
      bandRight: hoverBounds.right,
      bandTop: Math.max(gutterRect.top, docTop),
      bandBottom: Math.min(gutterRect.bottom, docBottom),
      docTop,
      docBottom
    };
  };

  const isWithinBand = (layout, clientX, clientY) => (
    clientX >= layout.bandLeft &&
    clientX < layout.bandRight &&
    clientY >= layout.bandTop &&
    clientY <= layout.bandBottom &&
    clientY >= layout.docTop &&
    clientY <= layout.docBottom
  );

  const gutterProbeXs = (layout, clientX) => {
    const probeXs = [];
    const pushProbeX = (value) => {
      if (!Number.isFinite(value)) {
        return;
      }
      const rounded = Math.round(value);
      if (!probeXs.includes(rounded)) {
        probeXs.push(rounded);
      }
    };
    const bandMinX = Math.round(layout.bandLeft);
    const bandMaxX = Math.max(bandMinX, Math.round(layout.bandRight - 1));
    pushProbeX(clamp(clientX, bandMinX, bandMaxX));

    const gutterRect = layout.gutterRect;
    const minX = Math.round(gutterRect.left + 1);
    const maxX = Math.max(minX, Math.round(gutterRect.right - 1));
    const centerX = clamp(Math.round(gutterRect.left + gutterRect.width / 2), minX, maxX);
    pushProbeX(centerX);
    pushProbeX(minX);
    pushProbeX(maxX);
    return probeXs;
  };

  const getMarkerAtY = (layout, clientX, clientY) => {
    for (const sampleX of gutterProbeXs(layout, clientX)) {
      const stack = typeof document.elementsFromPoint === 'function'
        ? document.elementsFromPoint(sampleX, clientY)
        : [document.elementFromPoint(sampleX, clientY)];
      for (const hit of stack) {
        if (!(hit instanceof Element)) {
          continue;
        }
        const marker = hit.closest('.meo-git-gutter-marker');
        if (marker instanceof HTMLElement) {
          return marker;
        }
      }
    }
    return null;
  };

  const getGutterRowAtY = (layout, clientX, clientY) => {
    for (const sampleX of gutterProbeXs(layout, clientX)) {
      const stack = typeof document.elementsFromPoint === 'function'
        ? document.elementsFromPoint(sampleX, clientY)
        : [document.elementFromPoint(sampleX, clientY)];
      for (const hit of stack) {
        if (!(hit instanceof Element)) {
          continue;
        }
        const row = hit.closest('.cm-gutterElement');
        if (
          row instanceof HTMLElement &&
          row.closest('.cm-gutter.meo-git-gutter')
        ) {
          return row;
        }
      }
    }
    return null;
  };

  const getRawLineNumberAtGutterRow = (gutterRowElement) => {
    if (!(gutterRowElement instanceof HTMLElement)) {
      return null;
    }
    const rowRect = gutterRowElement.getBoundingClientRect();
    const probeY = (rowRect.top + rowRect.bottom) / 2;
    const viewAny = /** @type {any} */ (view);
    if (typeof viewAny.lineBlockAtHeight === 'function') {
      const block = viewAny.lineBlockAtHeight(probeY - view.documentTop);
      if (block && Number.isFinite(block.from)) {
        return view.state.doc.lineAt(block.from).number;
      }
    }

    const contentRect = view.contentDOM.getBoundingClientRect();
    const x = clamp(
      contentRect.left + 4,
      contentRect.left + 1,
      Math.max(contentRect.left + 1, contentRect.right - 1)
    );
    const pos = view.posAtCoords({ x, y: probeY });
    return pos === null ? null : view.state.doc.lineAt(pos).number;
  };

  const setBandCursor = (active) => {
    const cursor = active ? 'pointer' : '';
    view.dom.style.cursor = cursor;
    view.scrollDOM.style.cursor = cursor;
  };

  const sameElements = (left, right) => {
    if (left.length !== right.length) {
      return false;
    }
    for (let index = 0; index < left.length; index += 1) {
      if (left[index] !== right[index]) {
        return false;
      }
    }
    return true;
  };

  const sameLineRange = (left, right) => (
    left === right ||
    (
      left &&
      right &&
      left.startLine === right.startLine &&
      left.endLine === right.endLine
    )
  );

  const getChangedMarkerForRow = (gutterRowElement, markerElement = null) => {
    const rowMarker = gutterRowElement?.querySelector?.('.meo-git-gutter-marker') ?? null;
    const marker = markerElement instanceof Element ? markerElement.closest('.meo-git-gutter-marker') : null;
    let changedMarker = isChangedMarker(rowMarker)
      ? rowMarker
      : isChangedMarker(marker)
        ? marker
        : null;

    if (!changedMarker && gutterRowElement instanceof HTMLElement) {
      const rawLineNumber = getRawLineNumberAtGutterRow(gutterRowElement);
      if (rawLineNumber !== null && isTrailingEofVisualLine(view.state.doc, rawLineNumber)) {
        const previousRowMarker = gutterRowElement.previousElementSibling?.querySelector?.('.meo-git-gutter-marker') ?? null;
        if (isChangedMarker(previousRowMarker)) {
          changedMarker = previousRowMarker;
        }
      }
    }

    return changedMarker instanceof HTMLElement ? changedMarker : null;
  };

  const getGutterRowsInLineRange = (startLine, endLine) => {
    const gutter = view.dom.querySelector('.cm-gutter.meo-git-gutter');
    if (!(gutter instanceof HTMLElement)) {
      return [];
    }

    const rows = [];
    for (const row of gutter.querySelectorAll('.cm-gutterElement')) {
      if (!(row instanceof HTMLElement)) {
        continue;
      }
      const rowLineNumber = getRawLineNumberAtGutterRow(row);
      if (rowLineNumber === null) {
        continue;
      }
      if (rowLineNumber < startLine || rowLineNumber > endLine) {
        continue;
      }
      rows.push(row);
    }
    return rows;
  };

  const getMarkersForLiveBlockRange = (startLine, endLine) => {
    const gutter = view.dom.querySelector('.cm-gutter.meo-git-gutter');
    if (!(gutter instanceof HTMLElement)) {
      return [];
    }

    const markers = [];
    const seen = new Set();
    const selector = `.meo-git-gutter-marker[data-meo-live-block-start-line="${startLine}"][data-meo-live-block-end-line="${endLine}"]`;
    for (const node of gutter.querySelectorAll(selector)) {
      if (!(node instanceof HTMLElement) || seen.has(node)) {
        continue;
      }
      seen.add(node);
      markers.push(node);
    }
    return markers;
  };

  const clearMarkerHover = () => {
    if (activeMarkerElements.length) {
      for (const marker of activeMarkerElements) {
        marker.classList.remove('is-hit-hover');
      }
      activeMarkerElements = [];
    }
    activeGutterRowElement = null;
    activeGutterRowHoverKind = null;
    activeRenderedBlockRange = null;
    hoverOverlay.hidden = true;
    view.dom.classList.remove('meo-git-hover-band');
    setBandCursor(false);
  };

  const syncHoverOverlay = (anchorRect = null) => {
    const kind = activeGutterRowHoverKind;
    if (
      !kind ||
      !anchorRect ||
      !Number.isFinite(anchorRect.left) ||
      !Number.isFinite(anchorRect.top) ||
      !Number.isFinite(anchorRect.bottom)
    ) {
      hoverOverlay.hidden = true;
      hoverOverlay.classList.remove('is-empty', 'is-added', 'is-modified');
      return;
    }

    const top = Math.round(Math.min(anchorRect.top, anchorRect.bottom));
    const bottom = Math.round(Math.max(anchorRect.top, anchorRect.bottom));
    const height = Math.max(0, bottom - top);
    if (height <= 0) {
      hoverOverlay.hidden = true;
      hoverOverlay.classList.remove('is-empty', 'is-added', 'is-modified');
      return;
    }

    const width = 3;
    const left = Math.round(anchorRect.left);

    hoverOverlay.classList.toggle('is-empty', kind === 'empty');
    hoverOverlay.classList.toggle('is-added', kind === 'added');
    hoverOverlay.classList.toggle('is-modified', kind === 'modified');
    hoverOverlay.style.left = `${left}px`;
    hoverOverlay.style.top = `${top}px`;
    hoverOverlay.style.width = `${width}px`;
    hoverOverlay.style.height = `${height}px`;
    hoverOverlay.hidden = false;
  };

  const updateMarkerHoverForY = (layout, x, y, renderedBlockRange = null) => {
    const hit = getMarkerAtY(layout, x, y);
    const gutterRowElement = getGutterRowAtY(layout, x, y);
    activeGutterRowElement = gutterRowElement;

    let nextMarkers = [];
    let nextGutterRowHoverKind = null;
    let nextRenderedBlockRange = null;
    const changedMarker = getChangedMarkerForRow(gutterRowElement, hit);
    if (changedMarker) {
      nextMarkers = [changedMarker];
    }

    const rawLineNumber = (
      gutterRowElement instanceof HTMLElement
        ? getRawLineNumberAtGutterRow(gutterRowElement)
        : null
    );

    const lineFlags = (
      getMode?.() === 'live'
        ? view.state.field(gitDiffLineFlagsField, false)
        : null
    );

    if (gutterRowElement instanceof HTMLElement && getMode?.() === 'live') {
      if (rawLineNumber !== null && Array.isArray(lineFlags)) {
        const block = getLiveGitCollapsedBlockAtLine(view.state, lineFlags, rawLineNumber);
        if (block) {
          nextRenderedBlockRange = { startLine: block.startLine, endLine: block.endLine };
          const markerBlockRange = getLiveBlockLineRangeFromMarker(changedMarker);
          const blockMarkers = markerBlockRange
            ? getMarkersForLiveBlockRange(markerBlockRange.startLine, markerBlockRange.endLine)
            : getMarkersForLiveBlockRange(block.startLine, block.endLine);
          if (!blockMarkers.length) {
            const seen = new Set();
            for (const row of getGutterRowsInLineRange(block.startLine, block.endLine)) {
              const rowMarker = getChangedMarkerForRow(row);
              if (rowMarker && !seen.has(rowMarker)) {
                seen.add(rowMarker);
                blockMarkers.push(rowMarker);
              }
            }
          }
          if (blockMarkers.length) {
            nextMarkers = blockMarkers;
          } else {
            nextGutterRowHoverKind = block.aggregateChangeKind;
          }
        }
      }
    }

    if (!nextMarkers.length && !nextGutterRowHoverKind && getMode?.() === 'live') {
      const fallbackBlock = (
        renderedBlockRange ??
        (rawLineNumber === null ? null : getLiveRenderedBlockAtLine(view.state, rawLineNumber))
      );
      if (fallbackBlock) {
        nextRenderedBlockRange = { startLine: fallbackBlock.startLine, endLine: fallbackBlock.endLine };
        nextGutterRowHoverKind = (
          getLineRangeChangeKind(lineFlags, fallbackBlock.startLine, fallbackBlock.endLine) ??
          'empty'
        );
      }
    }

    if (!nextMarkers.length && !nextGutterRowHoverKind) {
      nextGutterRowHoverKind = 'empty';
    }

    if (
      sameElements(nextMarkers, activeMarkerElements) &&
      nextGutterRowHoverKind === activeGutterRowHoverKind &&
      sameLineRange(nextRenderedBlockRange, activeRenderedBlockRange)
    ) {
      return;
    }
    if (activeMarkerElements.length) {
      for (const marker of activeMarkerElements) {
        marker.classList.remove('is-hit-hover');
      }
    }
    activeMarkerElements = nextMarkers;
    for (const marker of activeMarkerElements) {
      marker.classList.add('is-hit-hover');
    }
    activeRenderedBlockRange = nextRenderedBlockRange;
    activeGutterRowHoverKind = nextGutterRowHoverKind;
  };

  const clearHoverTimer = () => {
    if (hoverTimer !== null) {
      window.clearTimeout(hoverTimer);
      hoverTimer = null;
    }
  };

  const hideTooltipOnly = () => {
    clearHoverTimer();
    pendingBlameLineNumber = 0;
    activeLineNumber = 0;
    hoverToken += 1;
    ui.root.hidden = true;
  };

  const hide = () => {
    hideTooltipOnly();
    clearMarkerHover();
  };

  const remapLiveHit = (hit) => {
    if (!hit || hit.lineNumber === null || hit.proxiedFromTrailingEof || getMode?.() !== 'live') {
      return hit;
    }

    const lineFlags = view.state.field(gitDiffLineFlagsField, false);
    if (!Array.isArray(lineFlags)) {
      return hit;
    }

    const block = getLiveGitCollapsedBlockAtLine(view.state, lineFlags, hit.lineNumber);
    if (block) {
      return {
        lineNumber: block.canonicalLine,
        requestLineNumber: block.canonicalLine,
        proxiedFromTrailingEof: false,
        effectiveChangeKind: block.aggregateChangeKind,
        collapsedBlock: block
      };
    }

    const renderedBlock = getLiveRenderedBlockAtLine(view.state, hit.lineNumber);
    if (!renderedBlock) {
      return hit;
    }

    const aggregateChangeKind = getLineRangeChangeKind(
      lineFlags,
      renderedBlock.startLine,
      renderedBlock.endLine
    );
    if (!aggregateChangeKind) {
      return hit;
    }

    return {
      ...hit,
      effectiveChangeKind: aggregateChangeKind
    };
  };

  const lineNumberAtClientY = (layout, clientY, gutterRowElement = null, markerElement = null) => {
    const viewAny = /** @type {any} */ (view);
    if (typeof viewAny.lineBlockAtHeight === 'function') {
      const rowRect = gutterRowElement instanceof HTMLElement ? gutterRowElement.getBoundingClientRect() : null;
      const probeY = rowRect ? (rowRect.top + rowRect.bottom) / 2 : clientY;
      const block = viewAny.lineBlockAtHeight(probeY - view.documentTop);
      if (block && Number.isFinite(block.from)) {
        const lineNumber = view.state.doc.lineAt(block.from).number;
        return remapLiveHit(normalizeTrailingEofVisualLineHit(view.state.doc, lineNumber, gutterRowElement, markerElement));
      }
    }

    const x = clamp(
      layout.contentRect.left + 4,
      layout.contentRect.left + 1,
      Math.max(layout.contentRect.left + 1, layout.contentRect.right - 1)
    );
    const pos = view.posAtCoords({ x, y: clientY });
    if (pos === null) {
      return {
        lineNumber: null,
        requestLineNumber: null,
        proxiedFromTrailingEof: false,
        effectiveChangeKind: null,
        collapsedBlock: null
      };
    }
    const lineNumber = view.state.doc.lineAt(pos).number;
    return remapLiveHit(normalizeTrailingEofVisualLineHit(view.state.doc, lineNumber, gutterRowElement, markerElement));
  };

  const getLineAnchorRect = (
    lineNumber,
    layout,
    gutterRowElement,
    clientY,
    lineRange = null,
    { target = null, collapseTall = true } = {}
  ) => {
    if (
      lineRange &&
      Number.isInteger(lineRange.startLine) &&
      Number.isInteger(lineRange.endLine)
    ) {
      const renderedBlockElement = getRenderedBlockElement(view, lineRange, target);
      const gutterRows = getGutterRowsInLineRange(lineRange.startLine, lineRange.endLine);
      const topCandidates = [];
      const bottomCandidates = [];

      if (renderedBlockElement) {
        const blockRect = renderedBlockElement.getBoundingClientRect();
        topCandidates.push(Math.min(blockRect.top, blockRect.bottom));
        bottomCandidates.push(Math.max(blockRect.top, blockRect.bottom));
      }

      if (gutterRows.length) {
        const firstRect = gutterRows[0].getBoundingClientRect();
        const lastRect = gutterRows[gutterRows.length - 1].getBoundingClientRect();
        topCandidates.push(Math.min(firstRect.top, lastRect.top));
        bottomCandidates.push(Math.max(firstRect.bottom, lastRect.bottom));
      }

      if (topCandidates.length && bottomCandidates.length) {
        const visibleTop = Math.max(
          Math.min(...topCandidates),
          layout.gutterRect.top,
          8
        );
        const visibleBottom = Math.min(
          Math.max(...bottomCandidates),
          layout.gutterRect.bottom,
          Math.max(8, window.innerHeight - 8)
        );
        const visibleHeight = Math.max(0, visibleBottom - visibleTop);

        if (collapseTall && visibleHeight > 160) {
          const hoverTop = clamp(clientY - 10, visibleTop, visibleBottom);
          const hoverBottom = clamp(clientY + 10, hoverTop, visibleBottom);
          return {
            left: layout.gutterRect.left,
            right: layout.gutterRect.right,
            top: hoverTop,
            bottom: hoverBottom
          };
        }

        return {
          left: layout.gutterRect.left,
          right: layout.gutterRect.right,
          top: visibleTop,
          bottom: visibleBottom
        };
      }

      if (gutterRowElement instanceof HTMLElement) {
        const rowRect = gutterRowElement.getBoundingClientRect();
        return {
          left: layout.gutterRect.left,
          right: layout.gutterRect.right,
          top: clamp(Math.min(rowRect.top, clientY - 10), layout.gutterRect.top, layout.gutterRect.bottom),
          bottom: clamp(Math.max(rowRect.bottom, clientY + 10), layout.gutterRect.top, layout.gutterRect.bottom)
        };
      }

      return {
        left: layout.gutterRect.left,
        right: layout.gutterRect.right,
        top: clamp(clientY - 10, layout.gutterRect.top, layout.gutterRect.bottom),
        bottom: clamp(clientY + 10, layout.gutterRect.top, layout.gutterRect.bottom)
      };
    }

    const line = view.state.doc.line(lineNumber);
    const rowRect = gutterRowElement instanceof HTMLElement ? gutterRowElement.getBoundingClientRect() : null;
    const startCoords = view.coordsAtPos(line.from);
    const endCoords = view.coordsAtPos(line.to, -1) || view.coordsAtPos(line.to);

    const topCandidates = [
      startCoords?.top,
      endCoords?.top,
      rowRect?.top,
      clientY
    ].filter(Number.isFinite);
    const bottomCandidates = [
      startCoords?.bottom,
      endCoords?.bottom,
      rowRect?.bottom,
      clientY
    ].filter(Number.isFinite);

    const top = topCandidates.length ? Math.min(...topCandidates) : clientY;
    const bottom = bottomCandidates.length ? Math.max(...bottomCandidates) : clientY;

    return {
      left: layout.gutterRect.left,
      right: layout.gutterRect.right,
      top,
      bottom
    };
  };

  const triggerHover = (
    lineNumber,
    anchorRect,
    { proxiedFromTrailingEof = false, effectiveChangeKind = null, requestLineNumber = lineNumber } = {}
  ) => {
    if (destroyed || !isSupportedMode(getMode?.()) || lineNumber < 1) {
      hide();
      return;
    }

    clearHoverTimer();
    activeLineNumber = lineNumber;
    lastAnchorRect = anchorRect;
    const token = hoverToken + 1;
    hoverToken = token;
    hoverTimer = window.setTimeout(async () => {
      hoverTimer = null;
      if (destroyed || token !== hoverToken || activeLineNumber !== lineNumber || !isSupportedMode(getMode?.())) {
        return;
      }

      // Pure inserted lines should always show as uncommitted. The synthetic EOF row
      // is a proxy interaction though, so let the extension resolve history/mapping.
      if (!proxiedFromTrailingEof && effectiveChangeKind === 'added') {
        pendingBlameLineNumber = 0;
        renderBlameResult(ui, { kind: 'uncommitted' });
        if (lastAnchorRect) {
          positionTooltip(ui, lastAnchorRect);
        }
        return;
      }

      // Avoid showing a transient loading tooltip to reduce hover flicker.

      let result = null;
      pendingBlameLineNumber = lineNumber;
      try {
        result = await requestBlame?.({
          lineNumber: requestLineNumber
        });
      } catch {
        result = { kind: 'unavailable', reason: 'error' };
      }

      if (destroyed || token !== hoverToken || activeLineNumber !== lineNumber || !isSupportedMode(getMode?.())) {
        if (pendingBlameLineNumber === lineNumber) {
          pendingBlameLineNumber = 0;
        }
        return;
      }

      if (!shouldShowBlameTooltip(result)) {
        pendingBlameLineNumber = 0;
        ui.root.hidden = true;
        return;
      }
      renderBlameResult(ui, result);
      pendingBlameLineNumber = 0;
      if (lastAnchorRect) {
        positionTooltip(ui, lastAnchorRect);
      }
    }, hoverDelayMs);
  };

  const onMouseMove = (event) => {
    if (destroyed) {
      return;
    }

    if ((event.buttons & 1) === 1) {
      if (pointerDownWasLeftButton && hasPointerMovedPastDragThreshold(event)) {
        hide();
        return;
      }
      const hasSelection = view.state.selection.ranges.some((range) => !range.empty);
      if (hasSelection) {
        hide();
        return;
      }
    }

    const target = event.target instanceof Element ? event.target : null;
    if (target?.closest('.meo-md-fold-toggle')) {
      hide();
      return;
    }

    const mode = getMode?.();
    if (!isSupportedMode(mode) || view.dom.classList.contains('meo-git-gutter-hidden')) {
      clearMarkerHover();
      if (!ui.root.contains(target)) {
        hideTooltipOnly();
      }
      return;
    }

    const layout = getGutterBandLayout();
    if (!layout) {
      if (!ui.root.contains(target)) {
        hide();
      }
      return;
    }

    const withinBand = isWithinBand(layout, event.clientX, event.clientY);
    const renderedBlockRange = (mode === 'live' && withinBand)
      ? (
          getRenderedBlockLineRange(target) ??
          getRenderedBlockLineRangeAtClientY(view, event.clientY)
        )
      : null;
    if (!withinBand) {
      clearMarkerHover();
      if (!ui.root.contains(target)) {
        hideTooltipOnly();
      }
      return;
    }

    view.dom.classList.add('meo-git-hover-band');
    setBandCursor(true);
    updateMarkerHoverForY(layout, event.clientX, event.clientY, renderedBlockRange);
    const hoveredGutterRowElement = activeGutterRowElement;
    const hoveredMarkerElement = activeMarkerElements[0] ?? null;

    const hit = lineNumberAtClientY(layout, event.clientY, hoveredGutterRowElement, hoveredMarkerElement);
    if (hit.lineNumber === null && activeRenderedBlockRange) {
      hit.lineNumber = activeRenderedBlockRange.startLine;
      hit.requestLineNumber = activeRenderedBlockRange.startLine;
    }
    if (hit.lineNumber === null) {
      hide();
      return;
    }
    const lineNumber = hit.lineNumber;
    const requestLineNumber = (
      Number.isFinite(hit.requestLineNumber) ? hit.requestLineNumber : lineNumber
    );
    const effectiveChangeKind = (
      hit.effectiveChangeKind ??
      getMarkerChangeKind(hoveredMarkerElement) ??
      (activeGutterRowHoverKind === 'added' || activeGutterRowHoverKind === 'modified'
        ? activeGutterRowHoverKind
        : null)
    );
    const proxiedFromTrailingEof = hit.proxiedFromTrailingEof === true;
    const renderedBlock = (
      mode === 'live'
        ? (
            activeRenderedBlockRange ??
            renderedBlockRange ??
            getLiveRenderedBlockAtLine(view.state, lineNumber)
          )
        : null
    );
    const anchorRect = getLineAnchorRect(
      lineNumber,
      layout,
      hoveredGutterRowElement,
      event.clientY,
      hit.collapsedBlock ?? renderedBlock ?? null,
      { target, collapseTall: true }
    );
    const hoverRect = getLineAnchorRect(
      lineNumber,
      layout,
      hoveredGutterRowElement,
      event.clientY,
      hit.collapsedBlock ?? renderedBlock ?? null,
      { target, collapseTall: false }
    );
    syncHoverOverlay(hoverRect);

    if (lineNumber === activeLineNumber && (hoverTimer !== null || pendingBlameLineNumber === lineNumber)) {
      lastAnchorRect = anchorRect;
      if (!ui.root.hidden) {
        positionTooltip(ui, anchorRect);
      }
      return;
    }

    if (lineNumber === activeLineNumber && !ui.root.hidden) {
      lastAnchorRect = anchorRect;
      positionTooltip(ui, anchorRect);
      return;
    }

    triggerHover(lineNumber, anchorRect, { proxiedFromTrailingEof, effectiveChangeKind, requestLineNumber });
  };

  const onMouseLeave = (event) => {
    const nextTarget = event.relatedTarget instanceof Node ? event.relatedTarget : null;
    if (nextTarget && ui.root.contains(nextTarget)) {
      return;
    }
    hide();
  };

  const onScroll = () => hide();
  const onPointerDown = (event) => {
    hide();
    pointerDownWasLeftButton = event.button === 0;
    pointerDownClientX = event.clientX;
    pointerDownClientY = event.clientY;
    pointerDownInBand = false;

    if (
      !pointerDownWasLeftButton ||
      destroyed ||
      !isSupportedMode(getMode?.()) ||
      view.dom.classList.contains('meo-git-gutter-hidden')
    ) {
      return;
    }

    const layout = getGutterBandLayout();
    if (!layout) {
      return;
    }

    pointerDownInBand = isWithinBand(layout, event.clientX, event.clientY);
  };
  const pointerDownCapture = true;
  const onClick = (event) => {
    if (destroyed || !isSupportedMode(getMode?.()) || view.dom.classList.contains('meo-git-gutter-hidden')) {
      return;
    }
    if (event.button !== 0 || event.defaultPrevented) {
      return;
    }
    if (!pointerDownWasLeftButton || !pointerDownInBand) {
      return;
    }
    if (hasPointerMovedPastDragThreshold(event)) {
      return;
    }
    const target = event.target instanceof Element ? event.target : null;
    if (target?.closest('.meo-md-fold-toggle')) {
      return;
    }

    const layout = getGutterBandLayout();
    if (!layout) {
      return;
    }
    if (!isWithinBand(layout, event.clientX, event.clientY)) {
      return;
    }

    const marker = getMarkerAtY(layout, event.clientX, event.clientY);

    const hoveredGutterRowElement = getGutterRowAtY(layout, event.clientX, event.clientY);
    const hit = lineNumberAtClientY(layout, event.clientY, hoveredGutterRowElement, marker);
    const lineNumber = hit.lineNumber;
    if (lineNumber === null) {
      return;
    }
    const requestLineNumber = (
      Number.isFinite(hit.requestLineNumber) ? hit.requestLineNumber : lineNumber
    );
    const effectiveChangeKind = (
      hit.effectiveChangeKind ??
      getMarkerChangeKind(marker)
    );
    const proxiedFromTrailingEof = hit.proxiedFromTrailingEof === true;

    event.preventDefault();
    event.stopPropagation();
    // Synthetic trailing EOF row clicks intentionally open history (revision), not
    // worktree diff, because the row is a visual proxy for the previous real line.
    if (proxiedFromTrailingEof) {
      void openRevisionForLine?.({ lineNumber: requestLineNumber });
      return;
    }
    if (effectiveChangeKind === 'added') {
      return;
    }
    if (effectiveChangeKind === 'modified') {
      void openWorktreeForLine?.({ lineNumber });
      return;
    }
    void openRevisionForLine?.({ lineNumber: requestLineNumber });
  };

  view.dom.addEventListener('mousemove', onMouseMove);
  view.dom.addEventListener('mouseleave', onMouseLeave);
  view.dom.addEventListener('mousedown', onPointerDown, pointerDownCapture);
  view.dom.addEventListener('click', onClick, true);
  view.scrollDOM.addEventListener('scroll', onScroll, { passive: true });

  return {
    hide,
    destroy() {
      if (destroyed) {
        return;
      }
      destroyed = true;
      hide();
      view.dom.removeEventListener('mousemove', onMouseMove);
      view.dom.removeEventListener('mouseleave', onMouseLeave);
      view.dom.removeEventListener('mousedown', onPointerDown, pointerDownCapture);
      view.dom.removeEventListener('click', onClick, true);
      view.scrollDOM.removeEventListener('scroll', onScroll);
      ui.root.remove();
      hoverOverlay.remove();
    }
  };
}
