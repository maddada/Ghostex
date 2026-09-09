import { useEffect, useRef, type ClipboardEvent, type FormEvent, type KeyboardEvent, type MouseEvent } from 'react';
import type { SessionChatComposerInputApi } from './session-chat-composer';
import { revealSessionChatComposerCaret } from './session-chat-composer-scroll';
import { sessionChatBreaksKillSequence, sessionChatTerminalShortcut } from './session-chat-edit-shortcuts';
import { createSessionChatTerminalEditing } from './session-chat-terminal-editing';
import {
  SESSION_CHAT_REFERENCE_REVEAL_MARKER,
  sessionChatComposerReferences,
  sessionChatReferencePillText,
  type SessionChatComposerReference,
} from './session-chat-reference-pills';

const REFERENCE_SOURCE_ATTRIBUTE = 'data-ghostex-reference-source';

function referenceSource(node: Node): string | null {
  return node instanceof HTMLElement ? node.getAttribute(REFERENCE_SOURCE_ATTRIBUTE) : null;
}

function canonicalNodeText(node: Node): string {
  const source = referenceSource(node);
  if (source !== null) {
    return source;
  }
  if (node instanceof HTMLBRElement) {
    return '\n';
  }
  if (node instanceof Text) {
    return node.data;
  }
  return [...node.childNodes].map(canonicalNodeText).join('');
}

function canonicalEditorText(editor: HTMLElement): string {
  const value = canonicalNodeText(editor);
  // Chromium leaves a lone line break behind when a contenteditable becomes
  // empty. It is presentation scaffolding, not a user-authored newline.
  return value === '\n' && editor.textContent === '' ? '' : value;
}

function createReferencePill(reference: SessionChatComposerReference, source: string): HTMLSpanElement {
  const pill = document.createElement('span');
  pill.className = `ghostex-chat-reference-pill ghostex-chat-reference-pill--${reference.kind}`;
  pill.contentEditable = 'false';
  pill.setAttribute(REFERENCE_SOURCE_ATTRIBUTE, source);
  pill.setAttribute('data-ghostex-reference-path', reference.path);
  pill.setAttribute('role', 'img');
  pill.setAttribute('aria-label', `${reference.label}: ${reference.path}`);
  pill.title = reference.path;
  pill.textContent = sessionChatReferencePillText(reference.label, reference.kind);
  return pill;
}

function renderCanonicalValue(editor: HTMLElement, canonical: string): void {
  editor.dataset.empty = canonical === '' ? 'true' : 'false';
  const fragment = document.createDocumentFragment();
  const references = sessionChatComposerReferences(canonical);
  let cursor = 0;
  for (const reference of references) {
    if (reference.start > cursor) {
      fragment.append(document.createTextNode(canonical.slice(cursor, reference.start)));
    }
    const source = canonical.slice(reference.start, reference.end);
    fragment.append(createReferencePill(reference, source));
    cursor = reference.end;
  }
  if (cursor < canonical.length) {
    fragment.append(document.createTextNode(canonical.slice(cursor)));
  }
  editor.replaceChildren(fragment);
}

function canonicalOffsetForBoundary(editor: HTMLElement, container: Node, offset: number): number {
  if (container !== editor && !editor.contains(container)) {
    return canonicalEditorText(editor).length;
  }
  const range = document.createRange();
  range.setStart(editor, 0);
  try {
    range.setEnd(container, offset);
  } catch {
    return canonicalEditorText(editor).length;
  }
  return canonicalNodeText(range.cloneContents()).length;
}

function editorSelection(editor: HTMLElement): { end: number; focus: number; start: number } {
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0) {
    const end = canonicalEditorText(editor).length;
    return { end, focus: end, start: end };
  }
  const range = selection.getRangeAt(0);
  if (!editor.contains(range.commonAncestorContainer) && range.commonAncestorContainer !== editor) {
    const end = canonicalEditorText(editor).length;
    return { end, focus: end, start: end };
  }
  return {
    end: canonicalOffsetForBoundary(editor, range.endContainer, range.endOffset),
    focus: selection.focusNode
      ? canonicalOffsetForBoundary(editor, selection.focusNode, selection.focusOffset)
      : canonicalOffsetForBoundary(editor, range.endContainer, range.endOffset),
    start: canonicalOffsetForBoundary(editor, range.startContainer, range.startOffset),
  };
}

function boundaryForCanonicalOffset(editor: HTMLElement, canonicalOffset: number): { node: Node; offset: number } {
  const target = Math.max(0, canonicalOffset);
  let cursor = 0;
  for (const child of editor.childNodes) {
    const source = referenceSource(child);
    const length = source?.length ?? canonicalNodeText(child).length;
    if (child instanceof Text && target <= cursor + length) {
      return { node: child, offset: target - cursor };
    }
    if (source !== null && target <= cursor + length) {
      const childIndex = [...editor.childNodes].indexOf(child);
      // Like Monaco, a canonical offset inside an atomic reference resolves
      // to its useful insertion edge on the right.
      return target <= cursor ? { node: editor, offset: childIndex } : { node: editor, offset: childIndex + 1 };
    }
    cursor += length;
  }
  return { node: editor, offset: editor.childNodes.length };
}

function setEditorSelection(editor: HTMLElement, start: number, end = start, focus = end): void {
  const selection = window.getSelection();
  if (!selection) {
    return;
  }
  const startBoundary = boundaryForCanonicalOffset(editor, start);
  const endBoundary = boundaryForCanonicalOffset(editor, end);
  const [anchor, head] = focus === start ? [endBoundary, startBoundary] : [startBoundary, endBoundary];
  selection.setBaseAndExtent(anchor.node, anchor.offset, head.node, head.offset);
}

function referencePillCount(editor: HTMLElement): number {
  return editor.querySelectorAll(`[${REFERENCE_SOURCE_ATTRIBUTE}]`).length;
}

export function SessionChatPlainInput({
  invalid,
  initialValue,
  onCaretChange,
  onChange,
  onKeyDown,
  onPasteData,
  placeholder,
  registerApi,
}: {
  invalid: boolean;
  initialValue: string;
  onCaretChange: (caret: number) => void;
  onChange: (value: string, caret: number) => void;
  onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
  onPasteData: (data: DataTransfer) => boolean;
  placeholder: string;
  registerApi: (api: SessionChatComposerInputApi | null) => void;
}) {
  const editorRef = useRef<HTMLDivElement | null>(null);
  const valueRef = useRef(initialValue);
  const selectionRef = useRef({ end: initialValue.length, focus: initialValue.length, start: initialValue.length });
  const composingRef = useRef(false);
  const revealFrameRef = useRef(0);
  const terminalEditingRef = useRef<ReturnType<typeof createSessionChatTerminalEditing> | null>(null);
  const registerApiRef = useRef(registerApi);
  registerApiRef.current = registerApi;
  const callbacksRef = useRef({ onCaretChange, onChange, onPasteData });
  callbacksRef.current = { onCaretChange, onChange, onPasteData };

  const scheduleCaretReveal = (): void => {
    if (revealFrameRef.current) return;
    revealFrameRef.current = requestAnimationFrame(() => {
      revealFrameRef.current = 0;
      const editor = editorRef.current;
      if (editor) revealSessionChatComposerCaret(editor);
    });
  };

  useEffect(() => () => cancelAnimationFrame(revealFrameRef.current), []);

  const readSelection = (): typeof selectionRef.current => {
    const editor = editorRef.current;
    const selection = window.getSelection();
    if (editor && selection?.focusNode && editor.contains(selection.focusNode)) {
      selectionRef.current = editorSelection(editor);
    }
    return selectionRef.current;
  };

  const focus = (): void => {
    const editor = editorRef.current;
    if (!editor || document.activeElement === editor) return;
    const selection = readSelection();
    editor.focus();
    setEditorSelection(editor, selection.start, selection.end, selection.focus);
  };

  const applyValue = (next: string, caret: number): void => {
    terminalEditingRef.current?.breakSequence();
    const editor = editorRef.current;
    valueRef.current = next;
    if (!editor) {
      return;
    }
    if (
      canonicalEditorText(editor) !== next ||
      referencePillCount(editor) !== sessionChatComposerReferences(next).length
    ) {
      renderCanonicalValue(editor, next);
    }
    const position = Math.min(caret, next.length);
    selectionRef.current = { end: position, focus: position, start: position };
    setEditorSelection(editor, position);
    scheduleCaretReveal();
  };

  const insertText = (text: string): boolean => {
    terminalEditingRef.current?.breakSequence();
    const editor = editorRef.current;
    if (!editor) {
      return false;
    }
    const selection = readSelection();
    const current = canonicalEditorText(editor);
    const next = `${current.slice(0, selection.start)}${text}${current.slice(selection.end)}`;
    const caret = selection.start + text.length;
    editor.focus();
    renderCanonicalValue(editor, next);
    setEditorSelection(editor, caret);
    selectionRef.current = { end: caret, focus: caret, start: caret };
    valueRef.current = next;
    callbacksRef.current.onChange(next, caret);
    scheduleCaretReveal();
    return true;
  };

  useEffect(() => {
    const editor = editorRef.current;
    if (!editor) {
      return;
    }
    renderCanonicalValue(editor, initialValue);
    valueRef.current = initialValue;
  }, []);

  useEffect(() => {
    const editor = editorRef.current;
    if (!editor || canonicalEditorText(editor) === initialValue) {
      return;
    }
    const caret = Math.min(readSelection().focus, initialValue.length);
    renderCanonicalValue(editor, initialValue);
    setEditorSelection(editor, caret);
    selectionRef.current = { end: caret, focus: caret, start: caret };
    valueRef.current = initialValue;
  }, [initialValue]);

  useEffect(() => {
    const api: SessionChatComposerInputApi = {
      applyValue,
      focus,
      getSelection: () => {
        const editor = editorRef.current;
        if (!editor) {
          const end = valueRef.current.length;
          return { end, start: end };
        }
        const selection = readSelection();
        return { end: selection.end, start: selection.start };
      },
      getValue: () => (editorRef.current ? canonicalEditorText(editorRef.current) : valueRef.current),
      insertSavedPrompt: insertText,
      insertText,
      editText: (command) => {
        focus();
        if (terminalEditingRef.current?.run(command)) return;
        if (command === 'undo' || command === 'redo') {
          document.execCommand(command);
          return;
        }
        const selection = window.getSelection();
        if (!selection) return;
        const backward = command.endsWith('Left');
        if (selection.isCollapsed) {
          selection.modify(
            'extend',
            backward ? 'backward' : 'forward',
            command.startsWith('deleteWord') ? 'word' : 'lineboundary'
          );
        }
        document.execCommand(backward ? 'delete' : 'forwardDelete');
      },
      navigateCaret: ({ direction, select, unit }) => {
        terminalEditingRef.current?.breakSequence();
        const selection = window.getSelection();
        if (!selection) return;
        const granularity = {
          character: 'character',
          word: 'word',
          line: 'line',
          lineBoundary: 'lineboundary',
          paragraph: 'paragraphboundary',
          document: 'documentboundary',
        }[unit];
        selection.modify(
          select ? 'extend' : 'move',
          direction === 'up' ? 'backward' : direction === 'down' ? 'forward' : direction,
          granularity
        );
        callbacksRef.current.onCaretChange(readSelection().focus);
      },
      selectAll: () => {
        terminalEditingRef.current?.breakSequence();
        const editor = editorRef.current;
        if (!editor) {
          return;
        }
        editor.focus();
        setEditorSelection(editor, 0, canonicalEditorText(editor).length);
        selectionRef.current = editorSelection(editor);
        callbacksRef.current.onCaretChange(canonicalEditorText(editor).length);
      },
    };
    terminalEditingRef.current = createSessionChatTerminalEditing({
      getValue: api.getValue,
      getSelection: readSelection,
      setSelection: (start, end = start) => {
        focus();
        const editor = editorRef.current;
        if (!editor) return;
        setEditorSelection(editor, start, end);
        selectionRef.current = { start, end, focus: end };
        callbacksRef.current.onCaretChange(end);
      },
      insertText,
    });
    registerApiRef.current(api);
    return () => {
      terminalEditingRef.current = null;
      registerApiRef.current(null);
    };
  }, []);

  useEffect(() => {
    const handleSelectionChange = (): void => {
      const editor = editorRef.current;
      const selection = window.getSelection();
      if (
        !editor ||
        !selection?.focusNode ||
        (selection.focusNode !== editor && !editor.contains(selection.focusNode))
      ) {
        return;
      }
      callbacksRef.current.onCaretChange(readSelection().focus);
    };
    document.addEventListener('selectionchange', handleSelectionChange);
    return () => document.removeEventListener('selectionchange', handleSelectionChange);
  }, []);

  const synchronizeInput = (event: FormEvent<HTMLDivElement>): void => {
    const editor = event.currentTarget;
    const canonical = canonicalEditorText(editor);
    editor.dataset.empty = canonical === '' ? 'true' : 'false';
    let caret = editorSelection(editor).focus;
    if (!composingRef.current && referencePillCount(editor) !== sessionChatComposerReferences(canonical).length) {
      renderCanonicalValue(editor, canonical);
      setEditorSelection(editor, caret);
    }
    caret = Math.min(caret, canonical.length);
    valueRef.current = canonical;
    selectionRef.current = editorSelection(editor);
    callbacksRef.current.onChange(canonical, caret);
    scheduleCaretReveal();
  };

  const copySelection = (event: ClipboardEvent<HTMLDivElement>, cut: boolean): void => {
    const editor = editorRef.current;
    if (!editor) {
      return;
    }
    const selection = editorSelection(editor);
    if (selection.start === selection.end) {
      return;
    }
    const current = canonicalEditorText(editor);
    event.clipboardData.setData('text/plain', current.slice(selection.start, selection.end));
    event.preventDefault();
    if (!cut) {
      return;
    }
    const next = `${current.slice(0, selection.start)}${current.slice(selection.end)}`;
    renderCanonicalValue(editor, next);
    setEditorSelection(editor, selection.start);
    valueRef.current = next;
    callbacksRef.current.onChange(next, selection.start);
    scheduleCaretReveal();
  };

  const revealReferenceSource = (event: MouseEvent<HTMLDivElement>): void => {
    if (event.detail !== 2) {
      return;
    }
    const pill =
      event.target instanceof Element ? event.target.closest<HTMLElement>(`[${REFERENCE_SOURCE_ATTRIBUTE}]`) : null;
    const editor = editorRef.current;
    const source = pill ? pill.getAttribute(REFERENCE_SOURCE_ATTRIBUTE) : null;
    if (!pill || !editor || source === null || !editor.contains(pill)) {
      return;
    }
    const pillRange = document.createRange();
    pillRange.selectNode(pill);
    const start = canonicalOffsetForBoundary(editor, pillRange.startContainer, pillRange.startOffset);
    const labelEnd = source.indexOf('](');
    if (labelEnd < 0) {
      return;
    }
    event.preventDefault();
    const revealed = `${source.slice(0, labelEnd)}${SESSION_CHAT_REFERENCE_REVEAL_MARKER}${source.slice(labelEnd)}`;
    const current = canonicalEditorText(editor);
    const next = `${current.slice(0, start)}${revealed}${current.slice(start + source.length)}`;
    const caret = start + labelEnd + 1;
    renderCanonicalValue(editor, next);
    setEditorSelection(editor, caret);
    valueRef.current = next;
    callbacksRef.current.onChange(next, caret);
    scheduleCaretReveal();
  };

  return (
    <div
      aria-invalid={invalid}
      aria-multiline='true'
      className='ghostex-chat-composer-input ghostex-chat-composer-plain-input scroll-fade-y max-h-40 min-w-0 flex-1 overflow-y-auto bg-transparent text-sm leading-6 text-foreground outline-none'
      contentEditable
      data-empty={initialValue === '' ? 'true' : 'false'}
      data-placeholder={placeholder}
      data-session-chat-typing-redirect-ignore='true'
      onBeforeInput={(event) => {
        if (event.nativeEvent.inputType !== 'insertParagraph' && event.nativeEvent.inputType !== 'insertLineBreak') {
          return;
        }
        event.preventDefault();
        insertText('\n');
      }}
      onCompositionEnd={(event) => {
        composingRef.current = false;
        synchronizeInput(event);
      }}
      onBlur={readSelection}
      onCompositionStart={() => {
        composingRef.current = true;
      }}
      onCopy={(event) => copySelection(event, false)}
      onCut={(event) => copySelection(event, true)}
      onDoubleClick={revealReferenceSource}
      onInput={synchronizeInput}
      onPointerDown={() => terminalEditingRef.current?.breakSequence()}
      onKeyDown={(event) => {
        const terminal = sessionChatTerminalShortcut(event.nativeEvent);
        if (sessionChatBreaksKillSequence(event.nativeEvent)) terminalEditingRef.current?.breakSequence();
        onKeyDown(event);
        if (!event.defaultPrevented && terminal && terminalEditingRef.current?.run(terminal)) {
          event.preventDefault();
          event.stopPropagation();
        }
      }}
      onPaste={(event) => {
        if (callbacksRef.current.onPasteData(event.clipboardData)) {
          event.preventDefault();
          return;
        }
        const text = event.clipboardData.getData('text/plain');
        if (text !== '') {
          event.preventDefault();
          insertText(text);
        }
      }}
      ref={editorRef}
      role='textbox'
      suppressContentEditableWarning
    />
  );
}
