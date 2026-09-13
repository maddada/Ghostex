import { useLayoutEffect, useRef, type RefObject } from 'react';
import { flushSync } from 'react-dom';
import { transcriptRowElements, type TranscriptMaterializationReason } from './session-chat-transcript-mode';
import type { TranscriptVirtualRow } from './use-session-chat-virtual-transcript';

const EDITABLE_SELECTOR = 'input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="textbox"]';

export function useSessionChatTranscriptSelection({
  viewportRef,
  contentRef,
  rows,
  materialize,
  setSelectionIds,
}: {
  viewportRef: RefObject<HTMLDivElement | null>;
  contentRef: RefObject<HTMLDivElement | null>;
  rows: readonly TranscriptVirtualRow[];
  materialize: (reason: TranscriptMaterializationReason, active: boolean) => void;
  setSelectionIds: (ids: readonly string[]) => void;
}) {
  const rowsRef = useRef(rows);
  rowsRef.current = rows;
  const lastIdsRef = useRef('');
  const selectingAllRef = useRef(false);
  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    const content = contentRef.current;
    if (!viewport || !content) return;
    const root = viewport.closest('.ghostex-session-chat-scope') ?? viewport;
    let selectedIds: readonly string[] = [];
    let dragId: string | undefined;
    let focusedId: string | undefined;
    let popupIds: string[] = [];
    const pin = () => {
      const ids = [
        ...new Set([...selectedIds, ...popupIds, ...(dragId ? [dragId] : []), ...(focusedId ? [focusedId] : [])]),
      ];
      const key = JSON.stringify(ids);
      if (lastIdsRef.current !== key) {
        lastIdsRef.current = key;
        setSelectionIds(ids);
      }
    };
    const selectAll = () => {
      selectingAllRef.current = true;
      flushSync(() => materialize('selection', true));
      const range = document.createRange();
      range.selectNodeContents(content);
      const selection = document.getSelection();
      selection?.removeAllRanges();
      selection?.addRange(range);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        event.defaultPrevented ||
        event.isComposing ||
        event.altKey ||
        !(event.metaKey || event.ctrlKey) ||
        event.key.toLowerCase() !== 'a'
      )
        return;
      const target = event.target instanceof Element ? event.target : null;
      if (target?.closest(EDITABLE_SELECTOR) || !viewport.clientHeight) return;
      if (target && target !== document.body && !root.contains(target)) return;
      event.preventDefault();
      event.stopPropagation();
      selectAll();
    };
    const onSelectionChange = () => {
      const selection = document.getSelection();
      if (!selection || selection.isCollapsed || !selection.rangeCount) {
        selectingAllRef.current = false;
        materialize('selection', false);
        selectedIds = [];
        pin();
        return;
      }
      const range = selection.getRangeAt(0);
      // Native Edit > Select All can bypass keyboard events. Its range covers
      // the chat root, unlike a drag whose endpoints lie in message text.
      if (
        !selectingAllRef.current &&
        range.commonAncestorContainer !== content &&
        range.commonAncestorContainer.contains(viewport)
      ) {
        const mounted = transcriptRowElements(content);
        if (
          mounted.length &&
          selection.containsNode(mounted[0]!, false) &&
          selection.containsNode(mounted[mounted.length - 1]!, false)
        ) {
          selectAll();
          return;
        }
      }
      const rowFor = (node: Node | null) =>
        (node instanceof Element ? node : node?.parentElement)?.closest<HTMLElement>('[data-message-id]');
      const anchor = rowFor(selection.anchorNode);
      const focus = rowFor(selection.focusNode);
      if (anchor && focus && content.contains(anchor) && content.contains(focus)) {
        const indexes = rowsRef.current.map((row) => row.messageId);
        const start = indexes.indexOf(anchor.dataset.messageId!);
        const end = indexes.indexOf(focus.dataset.messageId!);
        if (start >= 0 && end >= 0) {
          selectedIds = indexes.slice(Math.min(start, end), Math.max(start, end) + 1);
          pin();
        }
      } else if (!content.contains(range.commonAncestorContainer)) {
        selectingAllRef.current = false;
        materialize('selection', false);
        selectedIds = [];
        pin();
      }
    };
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target instanceof Element ? event.target : null;
      const row = target?.closest<HTMLElement>('[data-message-id]');
      dragId = row && content.contains(row) ? row.dataset.messageId : undefined;
      pin();
    };
    const onPointerUp = () => {
      dragId = undefined;
      onSelectionChange();
      pin();
    };
    const onFocus = () => {
      const focused = document.activeElement?.closest<HTMLElement>('[data-message-id]');
      focusedId = focused && content.contains(focused) ? focused.dataset.messageId : undefined;
      pin();
    };
    const updatePopups = () => {
      popupIds = [
        ...content.querySelectorAll('[aria-haspopup][aria-expanded="true"], [data-session-chat-retain="true"]'),
      ].flatMap((element) => {
        const id = element.closest<HTMLElement>('[data-message-id]')?.dataset.messageId;
        return id ? [id] : [];
      });
      pin();
    };
    const popupObserver = new MutationObserver(updatePopups);
    popupObserver.observe(content, {
      attributes: true,
      subtree: true,
      attributeFilter: ['aria-expanded', 'data-session-chat-retain'],
    });
    updatePopups();
    window.addEventListener('keydown', onKeyDown, true);
    document.addEventListener('selectionchange', onSelectionChange);
    content.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('focusin', onFocus);
    document.addEventListener('pointerup', onPointerUp);
    document.addEventListener('pointercancel', onPointerUp);
    return () => {
      popupObserver.disconnect();
      window.removeEventListener('keydown', onKeyDown, true);
      document.removeEventListener('selectionchange', onSelectionChange);
      content.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('focusin', onFocus);
      document.removeEventListener('pointerup', onPointerUp);
      document.removeEventListener('pointercancel', onPointerUp);
    };
  }, [contentRef, materialize, setSelectionIds, viewportRef]);
}
