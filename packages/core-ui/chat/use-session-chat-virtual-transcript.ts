import { useCallback, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react';
import { defaultRangeExtractor, elementScroll, useVirtualizer, type Range } from '@tanstack/react-virtual';
import {
  captureTranscriptAnchor,
  registerTranscriptMaterialization,
  TRANSCRIPT_RENDERED_EVENT,
  transcriptRowElements,
  type TranscriptMaterializationReason,
} from './session-chat-transcript-mode';
import { useSessionChatTranscriptSelection } from './use-session-chat-transcript-selection';
import type { SessionChatScrollSnapshot } from './session-chat-interaction-state';

export interface TranscriptVirtualRow {
  key: string;
  messageId: string;
}

/** CDXC:SessionChat 2026-09-13 DECISION:
 * User: use TanStack Virtual for transcript virtualization, preserving reading anchors, streaming-top hold, search and selection.
 *
 * CDXC:SessionChat 2026-09-13 WHY:
 * Find and whole-transcript selection intentionally materialize all loaded rows while active so their rendered-text semantics remain exact.
 */
export function useSessionChatVirtualTranscript({
  rows,
  viewportRef,
  contentRef,
  canFollow,
  snapshot,
  pinnedMessageIds,
  programmaticScrollTopRef,
}: {
  rows: readonly TranscriptVirtualRow[];
  viewportRef: RefObject<HTMLDivElement | null>;
  contentRef: RefObject<HTMLDivElement | null>;
  canFollow: boolean;
  snapshot: SessionChatScrollSnapshot | undefined;
  pinnedMessageIds: readonly string[];
  programmaticScrollTopRef: RefObject<number | null>;
}) {
  const [reasons, setReasons] = useState<ReadonlySet<TranscriptMaterializationReason>>(new Set());
  const reasonsRef = useRef(reasons);
  const [selectionIds, setSelectionIds] = useState<readonly string[]>([]);
  const [jumpId, setJumpId] = useState<string | null>(null);
  const transitionAnchorRef = useRef<ReturnType<typeof captureTranscriptAnchor>>(null);
  const rowIndexes = useMemo(() => new Map(rows.map((row, index) => [row.messageId, index])), [rows]);
  const materialized = reasons.size > 0;
  const [paddingEnd, setPaddingEnd] = useState(16);
  useLayoutEffect(() => {
    const content = contentRef.current;
    if (!content) return;
    const measurePadding = () => setPaddingEnd(Number.parseFloat(getComputedStyle(content).paddingBottom) || 0);
    measurePadding();
    // The composer overlay changes this inherited padding as it grows or
    // collapses. Absolute rows need the same reserved band in virtual geometry.
    const observer = new ResizeObserver(measurePadding);
    observer.observe(content);
    return () => observer.disconnect();
  }, [contentRef]);
  const getItemKey = useCallback((index: number) => rows[index]!.key, [rows]);
  const pinnedIndexes = useMemo(() => {
    const ids = [...pinnedMessageIds, ...selectionIds];
    if (jumpId) ids.push(jumpId);
    if (transitionAnchorRef.current) ids.push(transitionAnchorRef.current.id);
    return ids.flatMap((id) => {
      const index = rowIndexes.get(id);
      return index === undefined ? [] : [index];
    });
  }, [jumpId, materialized, pinnedMessageIds, rowIndexes, selectionIds]);
  const rangeExtractor = useCallback(
    (range: Range) =>
      materialized
        ? Array.from({ length: rows.length }, (_, index) => index)
        : [...new Set([...defaultRangeExtractor(range), ...pinnedIndexes])].sort((left, right) => left - right),
    [materialized, pinnedIndexes, rows.length]
  );
  const virtualizer = useVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: rows.length,
    getScrollElement: () => viewportRef.current,
    estimateSize: () => 240,
    getItemKey,
    overscan: 4,
    paddingStart: 32,
    paddingEnd,
    scrollPaddingStart: 24,
    rangeExtractor,
    anchorTo: 'end',
    // A negative threshold disables end-pinning during explicit reading holds
    // while retaining TanStack's keyed prepend compensation.
    scrollEndThreshold: canFollow ? 10 : -1,
    followOnAppend: canFollow,
    initialOffset: snapshot?.top ?? Math.max(0, rows.length * 240 - 600),
    scrollToFn: (offset, options, instance) => {
      programmaticScrollTopRef.current = Math.max(0, offset + (options.adjustments ?? 0));
      elementScroll(offset, options, instance);
    },
  });
  const virtualItems = virtualizer.getVirtualItems();
  const measurementQueue = useRef(new Set<HTMLDivElement>());
  const measurementScheduled = useRef(false);
  const measureElement = useCallback(
    (element: HTMLDivElement | null) => {
      if (element) measurementQueue.current.add(element);
      if (measurementScheduled.current) return;
      measurementScheduled.current = true;
      // React invokes callback refs inside its commit. TanStack can synchronously
      // flush an anchor correction when an estimate changes, so measure the batch
      // immediately after that commit, before paint. ResizeObserver corrections
      // keep the library's normal synchronous path.
      queueMicrotask(() => {
        measurementScheduled.current = false;
        const elements = [...measurementQueue.current];
        measurementQueue.current.clear();
        virtualizer.measureElement(null);
        for (const element of elements) {
          if (element.isConnected && viewportRef.current?.contains(element)) virtualizer.measureElement(element);
        }
      });
    },
    [viewportRef, virtualizer]
  );
  const initializedRef = useRef(Boolean(snapshot));
  useLayoutEffect(() => {
    if (!initializedRef.current && rows.length && viewportRef.current?.clientHeight) {
      initializedRef.current = true;
      virtualizer.scrollToEnd({ behavior: 'auto' });
    }
  }, [rows.length, viewportRef, virtualizer]);

  const materialize = useCallback(
    (reason: TranscriptMaterializationReason, active: boolean) => {
      if (reasonsRef.current.has(reason) === active) return;
      const viewport = viewportRef.current;
      const content = contentRef.current;
      if (viewport && content) transitionAnchorRef.current = captureTranscriptAnchor(viewport, content);
      const next = new Set(reasonsRef.current);
      if (active) next.add(reason);
      else next.delete(reason);
      reasonsRef.current = next;
      setReasons(next);
    },
    [contentRef, viewportRef]
  );
  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    if (viewport) return registerTranscriptMaterialization(viewport, materialize);
  }, [materialize, viewportRef]);

  useLayoutEffect(() => {
    const anchor = transitionAnchorRef.current;
    if (!anchor) return;
    const restore = () => {
      const viewport = viewportRef.current;
      const content = contentRef.current;
      const row = content && transcriptRowElements(content).find((row) => row.dataset.messageId === anchor.id);
      if (viewport && row) {
        const top =
          viewport.scrollTop + row.getBoundingClientRect().top - viewport.getBoundingClientRect().top - anchor.offset;
        programmaticScrollTopRef.current = Math.max(0, top);
        viewport.scrollTop = top;
      }
    };
    restore();
    let frame = requestAnimationFrame(() => {
      restore();
      frame = requestAnimationFrame(() => {
        restore();
        transitionAnchorRef.current = null;
      });
    });
    return () => cancelAnimationFrame(frame);
  }, [materialized, contentRef, programmaticScrollTopRef, viewportRef]);

  const scrollToMessage = useCallback(
    (
      id: string,
      options: { align?: 'start' | 'center' | 'end' | 'nearest'; behavior?: ScrollBehavior; scrollMargin?: number } = {}
    ) => {
      const index = rowIndexes.get(id);
      if (index === undefined) return false;
      setJumpId(id);
      virtualizer.scrollToIndex(index, {
        align: options.align === 'nearest' ? 'auto' : (options.align ?? 'start'),
        behavior: options.behavior ?? 'auto',
      });
      return true;
    },
    [rowIndexes, virtualizer]
  );
  useLayoutEffect(() => {
    if (!jumpId) return;
    const viewport = viewportRef.current;
    const cancel = () => setJumpId(null);
    viewport?.addEventListener('wheel', cancel, { passive: true });
    viewport?.addEventListener('touchstart', cancel, { passive: true });
    const timer = setTimeout(cancel, 1000);
    return () => {
      clearTimeout(timer);
      viewport?.removeEventListener('wheel', cancel);
      viewport?.removeEventListener('touchstart', cancel);
    };
  }, [jumpId, viewportRef]);
  const scrollToEnd = useCallback(
    (options: { behavior?: ScrollBehavior } = {}) => {
      virtualizer.scrollToEnd(options);
      return true;
    },
    [virtualizer]
  );
  useSessionChatTranscriptSelection({ viewportRef, contentRef, rows, materialize, setSelectionIds });
  // Find observes this after React commits newly materialized rows or streams.
  useLayoutEffect(() => {
    contentRef.current?.dispatchEvent(new Event(TRANSCRIPT_RENDERED_EVENT, { bubbles: true }));
  });
  const start = virtualizer.scrollOffset ?? 0;
  const end = start + (virtualizer.scrollRect?.height ?? 0);
  const visibleMessageIds = virtualItems
    .filter((row) => row.end > start && row.start < end)
    .map((row) => rows[row.index]!.messageId);
  return {
    virtualizer,
    virtualItems,
    visibleMessageIds,
    scrollToMessage,
    scrollToEnd,
    rowIndexes,
    materialized,
    measureElement,
  };
}

export type SessionChatVirtualTranscript = ReturnType<typeof useSessionChatVirtualTranscript>;
