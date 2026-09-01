import { useEffect, useRef, type ClipboardEvent, type FormEvent, type KeyboardEvent, type MouseEvent } from 'react';
import type { SessionChatComposerInputApi } from './session-chat-composer';
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

function setEditorSelection(editor: HTMLElement, start: number, end = start): void {
  const selection = window.getSelection();
  if (!selection) {
    return;
  }
  const startBoundary = boundaryForCanonicalOffset(editor, start);
  const endBoundary = boundaryForCanonicalOffset(editor, end);
  const range = document.createRange();
  range.setStart(startBoundary.node, startBoundary.offset);
  range.setEnd(endBoundary.node, endBoundary.offset);
  selection.removeAllRanges();
  selection.addRange(range);
}

function referencePillCount(editor: HTMLElement): number {
  return editor.querySelectorAll(`[${REFERENCE_SOURCE_ATTRIBUTE}]`).length;
}

export function SessionChatPlainInput({
  disabled,
  invalid,
  initialValue,
  onCaretChange,
  onChange,
  onKeyDown,
  onPasteData,
  placeholder,
  registerApi,
}: {
  disabled: boolean;
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
  const composingRef = useRef(false);
  const disabledRef = useRef(disabled);
  disabledRef.current = disabled;
  const registerApiRef = useRef(registerApi);
  registerApiRef.current = registerApi;
  const callbacksRef = useRef({ onCaretChange, onChange, onPasteData });
  callbacksRef.current = { onCaretChange, onChange, onPasteData };

  const applyValue = (next: string, caret: number): void => {
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
    setEditorSelection(editor, Math.min(caret, next.length));
  };

  const insertText = (text: string): boolean => {
    const editor = editorRef.current;
    if (!editor || disabledRef.current) {
      return false;
    }
    const selection = editorSelection(editor);
    const current = canonicalEditorText(editor);
    const next = `${current.slice(0, selection.start)}${text}${current.slice(selection.end)}`;
    const caret = selection.start + text.length;
    renderCanonicalValue(editor, next);
    setEditorSelection(editor, caret);
    valueRef.current = next;
    callbacksRef.current.onChange(next, caret);
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
    const caret = Math.min(editorSelection(editor).focus, initialValue.length);
    renderCanonicalValue(editor, initialValue);
    setEditorSelection(editor, caret);
    valueRef.current = initialValue;
  }, [initialValue]);

  useEffect(() => {
    const api: SessionChatComposerInputApi = {
      applyValue,
      focus: () => editorRef.current?.focus(),
      getSelection: () => {
        const editor = editorRef.current;
        if (!editor) {
          const end = valueRef.current.length;
          return { end, start: end };
        }
        const selection = editorSelection(editor);
        return { end: selection.end, start: selection.start };
      },
      getValue: () => (editorRef.current ? canonicalEditorText(editorRef.current) : valueRef.current),
      insertSavedPrompt: insertText,
      insertText,
      selectAll: () => {
        const editor = editorRef.current;
        if (!editor) {
          return;
        }
        editor.focus();
        setEditorSelection(editor, 0, canonicalEditorText(editor).length);
        callbacksRef.current.onCaretChange(canonicalEditorText(editor).length);
      },
    };
    registerApiRef.current(api);
    return () => registerApiRef.current(null);
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
      callbacksRef.current.onCaretChange(
        canonicalOffsetForBoundary(editor, selection.focusNode, selection.focusOffset)
      );
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
    callbacksRef.current.onChange(canonical, caret);
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
    if (!cut || disabled) {
      return;
    }
    const next = `${current.slice(0, selection.start)}${current.slice(selection.end)}`;
    renderCanonicalValue(editor, next);
    setEditorSelection(editor, selection.start);
    valueRef.current = next;
    callbacksRef.current.onChange(next, selection.start);
  };

  const revealReferenceSource = (event: MouseEvent<HTMLDivElement>): void => {
    if (event.detail !== 2 || disabled) {
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
  };

  return (
    <div
      aria-disabled={disabled ? 'true' : undefined}
      aria-invalid={invalid}
      aria-multiline='true'
      className='ghostex-chat-composer-input ghostex-chat-composer-plain-input max-h-40 min-w-0 flex-1 overflow-y-auto bg-transparent text-sm leading-6 text-foreground outline-none'
      contentEditable={!disabled}
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
      onCompositionStart={() => {
        composingRef.current = true;
      }}
      onCopy={(event) => copySelection(event, false)}
      onCut={(event) => copySelection(event, true)}
      onDoubleClick={revealReferenceSource}
      onInput={synchronizeInput}
      onKeyDown={onKeyDown}
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
