// Monaco-backed chat composer input.
//
// Monaco is loaded at runtime through its AMD loader from a host-provided
// vs base URL (gpui CEF: "./monaco/vs" staged next to chat.html; web:
// "/monaco/vs" served from node_modules by the vite config). There is no ESM
// import of monaco-editor anywhere in the repo — the AMD route is the only
// one that survives the gpui single-file CEF bundle (see
// packages/core-ui/agents-hub-modal.tsx for the precedent) — so all editor access
// goes through minimal structural types instead of monaco's own typings.
//
// The mobile single-file WebView bundle never passes a vs base URL (workers
// and sibling assets are unreachable from a base-URL-less html string), so
// this component is never mounted there and the composer keeps its textarea.

import { useEffect, useRef } from 'react';
import type { SessionChatTheme } from '../../shared/session-chat';
import type { SessionChatComposerInputApi, SessionChatComposerKeyEvent } from './session-chat-composer';
import { SESSION_CHAT_REFERENCE_REVEAL_MARKER } from './session-chat-reference-pills';
import {
  SessionChatMonacoReferenceModel,
  type SessionChatMonacoReferenceOccurrence,
} from './session-chat-monaco-reference-model';

interface MonacoPositionLike {
  column: number;
  lineNumber: number;
}

interface MonacoModelLike {
  getFullModelRange(): unknown;
  getOffsetAt(position: MonacoPositionLike): number;
  getPositionAt(offset: number): MonacoPositionLike;
}

interface MonacoSelectionLike {
  getEndPosition(): MonacoPositionLike;
  getStartPosition(): MonacoPositionLike;
}

interface MonacoKeyboardEventLike {
  browserEvent: KeyboardEvent;
  keyCode: number;
  preventDefault(): void;
  stopPropagation(): void;
}

interface MonacoMouseEventLike {
  event: {
    detail: number;
    preventDefault(): void;
    stopPropagation(): void;
    target: HTMLElement;
  };
}

interface MonacoDisposableLike {
  dispose(): void;
}

interface MonacoClipboardBridge {
  copySelection(cut: boolean): string | null;
}

interface MonacoDecorationsCollectionLike {
  clear(): void;
  set(decorations: MonacoDecorationLike[]): void;
}

interface MonacoEditorInstanceLike {
  createDecorationsCollection(decorations?: MonacoDecorationLike[]): MonacoDecorationsCollectionLike;
  dispose(): void;
  executeEdits(source: string, edits: { range: unknown; text: string }[]): void;
  focus(): void;
  getContentHeight(): number;
  getModel(): MonacoModelLike | null;
  getSelection(): MonacoSelectionLike | null;
  getValue(): string;
  layout(): void;
  onDidChangeCursorPosition(listener: () => void): MonacoDisposableLike;
  onDidChangeModelContent(listener: () => void): MonacoDisposableLike;
  onDidContentSizeChange(listener: () => void): MonacoDisposableLike;
  onKeyDown(listener: (event: MonacoKeyboardEventLike) => void): MonacoDisposableLike;
  onMouseDown(listener: (event: MonacoMouseEventLike) => void): MonacoDisposableLike;
  setSelection(selection: unknown): void;
  setPosition(position: MonacoPositionLike): void;
  setValue(value: string): void;
  trigger(source: string, handlerId: string, payload: unknown): void;
  updateOptions(options: Record<string, unknown>): void;
}

interface MonacoDecorationLike {
  options: Record<string, unknown>;
  range: {
    endColumn: number;
    endLineNumber: number;
    startColumn: number;
    startLineNumber: number;
  };
}

interface MonacoNamespaceLike {
  editor: {
    create(container: HTMLElement, options: Record<string, unknown>): MonacoEditorInstanceLike;
    defineTheme(name: string, theme: Record<string, unknown>): void;
  };
}

interface MonacoAmdRequire {
  (modules: string[], onLoad: () => void, onError?: (error: unknown) => void): void;
  config?: (options: { paths: Record<string, string> }) => void;
}

interface MonacoWindowLike {
  MonacoEnvironment?: { getWorkerUrl: () => string };
  monaco?: MonacoNamespaceLike;
  require?: MonacoAmdRequire;
}

const CHAT_MONACO_THEMES: Record<SessionChatTheme, string> = {
  dark: 'ghostex-session-chat-dark',
  light: 'ghostex-session-chat-light',
};
const MIN_INPUT_HEIGHT_PX = 72;
const MAX_INPUT_HEIGHT_PX = 160;
/*
CDXC:ChatComposerQuickInputHeight 2026-08-19:
Monaco's F1 command palette is an overlay widget *inside* the editor, and it
sizes its list from the editor's layout info. A content-sized composer is a few
lines tall, so the palette both got clipped by the container and rendered a
one-row list. Give the editor a palette-sized box for as long as the palette is
open; the height snaps back to the draft's own content height on close.
*/
const QUICK_INPUT_MIN_HEIGHT_PX = 280;
const REFERENCE_PILL_CLASS = 'ghostex-chat-reference-pill';
const REFERENCE_PILL_ID_CLASS_PREFIX = `${REFERENCE_PILL_CLASS}--id-`;
const REFERENCE_PILL_WORD_JOINER = '\u2060';
const REFERENCE_PILL_ICON_SPACE = '\u00a0\u00a0\u2009';
const REFERENCE_PILL_TRAILING_SPACE = '\u2009';
const REFERENCE_PILL_MAX_LABEL_CHARACTERS = 18;

function referencePillDisplayLabel(label: string, kind: SessionChatMonacoReferenceOccurrence['kind']): string {
  if (kind === 'skill') {
    return label;
  }
  const characters = [...label];
  if (characters.length <= REFERENCE_PILL_MAX_LABEL_CHARACTERS) {
    return label;
  }
  return `${characters.slice(0, REFERENCE_PILL_MAX_LABEL_CHARACTERS - 1).join('')}\u2026`;
}

/** Injected text must be one Monaco wrap unit, not merely one DOM box. */
function referencePillInjectedText(label: string, kind: SessionChatMonacoReferenceOccurrence['kind']): string {
  const text = `${REFERENCE_PILL_ICON_SPACE}${referencePillDisplayLabel(label, kind).replaceAll(' ', '\u00a0')}${REFERENCE_PILL_TRAILING_SPACE}`;
  return [...text].join(REFERENCE_PILL_WORD_JOINER);
}

// Monaco has already normalized the browser event into its cross-platform
// KeyCode enum. GPUI's CEF input path can expose navigation keys with a raw
// `KeyboardEvent.key` value that differs from the DOM spelling expected by
// the composer, so use Monaco's canonical value for the keys the composer
// owns and leave text/editing keys on the original browser value.
function composerKeyForMonacoEvent(event: MonacoKeyboardEventLike): string {
  switch (event.keyCode) {
    case 2: // monaco.KeyCode.Tab
      return 'Tab';
    case 3: // monaco.KeyCode.Enter
      return 'Enter';
    case 9: // monaco.KeyCode.Escape
      return 'Escape';
    case 16: // monaco.KeyCode.UpArrow
      return 'ArrowUp';
    case 18: // monaco.KeyCode.DownArrow
      return 'ArrowDown';
    default:
      return event.browserEvent.key;
  }
}

let monacoPromise: Promise<MonacoNamespaceLike> | undefined;

function loadSessionChatMonaco(vsBaseUrl: string): Promise<MonacoNamespaceLike> {
  if (monacoPromise) {
    return monacoPromise;
  }
  monacoPromise = new Promise<MonacoNamespaceLike>((resolve, reject) => {
    const target = window as unknown as MonacoWindowLike;
    target.MonacoEnvironment = {
      getWorkerUrl: () => `${vsBaseUrl}/base/worker/workerMain.js`,
    };
    const finish = (): void => {
      const amdRequire = target.require;
      if (typeof amdRequire !== 'function') {
        reject(new Error('The Monaco AMD loader did not install.'));
        return;
      }
      amdRequire.config?.({ paths: { vs: vsBaseUrl } });
      amdRequire(
        ['vs/editor/editor.main'],
        () => {
          if (target.monaco) {
            resolve(target.monaco);
          } else {
            reject(new Error('Monaco loaded without installing window.monaco.'));
          }
        },
        (error) => reject(error instanceof Error ? error : new Error(String(error)))
      );
    };
    const existing = target.require;
    if (typeof existing === 'function' && existing.config) {
      finish();
      return;
    }
    const script = document.createElement('script');
    script.src = `${vsBaseUrl}/loader.js`;
    script.onload = finish;
    script.onerror = () => reject(new Error(`Could not load ${vsBaseUrl}/loader.js.`));
    document.body.appendChild(script);
  });
  return monacoPromise;
}

let themeDefined = false;

function ensureChatThemes(monaco: MonacoNamespaceLike): void {
  if (themeDefined) {
    return;
  }
  themeDefined = true;
  // Transparent background so the composer's bg-card container shows through.
  monaco.editor.defineTheme(CHAT_MONACO_THEMES.dark, {
    base: 'vs-dark',
    colors: {
      'editor.background': '#00000000',
      'editor.lineHighlightBackground': '#00000000',
      'editorGutter.background': '#00000000',
    },
    inherit: true,
    rules: [],
  });
  monaco.editor.defineTheme(CHAT_MONACO_THEMES.light, {
    base: 'vs',
    colors: {
      'editor.background': '#00000000',
      'editor.lineHighlightBackground': '#00000000',
      'editorGutter.background': '#00000000',
    },
    inherit: true,
    rules: [],
  });
}

export function SessionChatMonacoInput({
  disabled,
  initialValue,
  onCaretChange,
  onChange,
  onKeyDown,
  onLoadFailed,
  onPasteData,
  placeholder,
  fillHeight,
  registerApi,
  theme,
  vsBaseUrl,
}: {
  disabled: boolean;
  /**
   * Maximized composer: stop sizing the editor to its content and let the
   * flex row own the height, so a short draft still gets the full box.
   */
  fillHeight: boolean;
  initialValue: string;
  onChange: (value: string, caret: number) => void;
  /** Caret offset after a pure selection move (no edit). */
  onCaretChange: (caret: number) => void;
  onKeyDown: (event: SessionChatComposerKeyEvent) => void;
  onLoadFailed: (error: unknown) => void;
  /** Returns true when the paste was handled (image); blocks Monaco's paste. */
  onPasteData: (data: DataTransfer) => boolean;
  placeholder: string;
  registerApi: (api: SessionChatComposerInputApi | null) => void;
  theme: SessionChatTheme;
  vsBaseUrl: string;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const editorRef = useRef<MonacoEditorInstanceLike | null>(null);
  const applyHeightRef = useRef<(() => void) | null>(null);
  const clipboardBridgeRef = useRef<MonacoClipboardBridge | null>(null);
  const insertTextRef = useRef<((text: string) => boolean) | null>(null);
  const disabledRef = useRef(disabled);
  disabledRef.current = disabled;
  const fillHeightRef = useRef(fillHeight);
  fillHeightRef.current = fillHeight;
  const suppressChangeRef = useRef(false);
  const quickInputOpenRef = useRef(false);
  const themeRef = useRef(theme);
  themeRef.current = theme;
  const callbacksRef = useRef({
    onCaretChange,
    onChange,
    onKeyDown,
    onLoadFailed,
    onPasteData,
  });
  callbacksRef.current = { onCaretChange, onChange, onKeyDown, onLoadFailed, onPasteData };

  useEffect(() => {
    let disposed = false;
    const disposables: MonacoDisposableLike[] = [];
    loadSessionChatMonaco(vsBaseUrl)
      .then((monaco) => {
        const container = containerRef.current;
        if (disposed || !container) {
          return;
        }
        ensureChatThemes(monaco);
        const referenceModel = new SessionChatMonacoReferenceModel();
        const initialPresentation = referenceModel.virtualizeCanonical(initialValue);
        const editor = monaco.editor.create(container, {
          acceptSuggestionOnEnter: 'off',
          autoClosingBrackets: 'never',
          autoClosingQuotes: 'never',
          automaticLayout: true,
          autoSurround: 'never',
          bracketPairColorization: { enabled: false },
          codeLens: false,
          // A prompt that mentions #ff0000 should not sprout a color swatch and
          // a click-to-open color picker inside the draft.
          colorDecorators: false,
          contextmenu: false,
          // Monaco's copy carries HTML styling and, with no selection, silently
          // copies the whole line. A composer's clipboard behaviour should read
          // like a plain text field's.
          copyWithSyntaxHighlighting: false,
          dropIntoEditor: { enabled: false },
          emptySelectionClipboard: false,
          folding: false,
          fontFamily: getComputedStyle(container).fontFamily,
          fontSize: 14,
          glyphMargin: false,
          // Indent guides are code-editor chrome: in a prompt composer they just
          // draw stray vertical rules next to any indented line the user typed.
          guides: {
            bracketPairs: false,
            bracketPairsHorizontal: false,
            highlightActiveBracketPair: false,
            highlightActiveIndentation: false,
            indentation: false,
          },
          hover: { enabled: false },
          inlayHints: { enabled: 'off' },
          inlineSuggest: { enabled: false },
          // Plain text on purpose: the draft is a prompt, so markdown *emphasis*
          // styling and syntax colorization inside the input read as noise.
          language: 'plaintext',
          lightbulb: { enabled: 'off' },
          lineDecorationsWidth: 0,
          lineHeight: 24,
          lineNumbers: 'off',
          lineNumbersMinChars: 0,
          // URL detection turns pasted links into underlined, ctrl-clickable
          // spans with their own hover widget; the draft is text to send, not a
          // document to navigate.
          links: false,
          matchBrackets: 'never',
          minimap: { enabled: false },
          occurrencesHighlight: 'off',
          overviewRulerBorder: false,
          overviewRulerLanes: 0,
          padding: { bottom: 0, top: 0 },
          parameterHints: { enabled: false },
          placeholder,
          quickSuggestions: false,
          // Control characters and whitespace both render as boxes/dots inside
          // a selection; in prose that reads as corruption rather than detail.
          renderControlCharacters: false,
          renderLineHighlight: 'none',
          renderWhitespace: 'none',
          scrollBeyondLastLine: false,
          scrollbar: {
            alwaysConsumeMouseWheel: false,
            horizontal: 'hidden',
            useShadows: false,
            vertical: 'auto',
            verticalScrollbarSize: 3,
          },
          selectionHighlight: false,
          snippetSuggestions: 'none',
          stickyScroll: { enabled: false },
          suggestOnTriggerCharacters: false,
          theme: CHAT_MONACO_THEMES[themeRef.current],
          unicodeHighlight: {
            ambiguousCharacters: false,
            invisibleCharacters: false,
          },
          value: initialPresentation,
          wordBasedSuggestions: 'off',
          wordWrap: 'on',
          wrappingStrategy: 'advanced',
        });
        editorRef.current = editor;
        const referenceDecorations = editor.createDecorationsCollection();
        disposables.push({ dispose: () => referenceDecorations.clear() });
        let references: SessionChatMonacoReferenceOccurrence[] = [];
        const canonicalValue = (): string => referenceModel.expand(editor.getValue());
        const canonicalCaretOffset = (): number => {
          const model = editor.getModel();
          const selection = editor.getSelection();
          if (!model || !selection) {
            return canonicalValue().length;
          }
          return referenceModel.modelOffsetToCanonical(
            editor.getValue(),
            model.getOffsetAt(selection.getEndPosition())
          );
        };
        const modelRangeForOffsets = (startOffset: number, endOffset: number): MonacoDecorationLike['range'] | null => {
          const model = editor.getModel();
          if (!model) {
            return null;
          }
          const start = model.getPositionAt(startOffset);
          const end = model.getPositionAt(endOffset);
          return {
            endColumn: end.column,
            endLineNumber: end.lineNumber,
            startColumn: start.column,
            startLineNumber: start.lineNumber,
          };
        };
        const revealReferenceSource = (reference: SessionChatMonacoReferenceOccurrence): void => {
          const model = editor.getModel();
          if (!model) {
            return;
          }
          const labelEnd = reference.source.indexOf('](');
          if (labelEnd < 0) {
            return;
          }
          const range = modelRangeForOffsets(reference.start, reference.end);
          if (!range) {
            return;
          }
          const revealed = `${reference.source.slice(0, labelEnd)}${SESSION_CHAT_REFERENCE_REVEAL_MARKER}${reference.source.slice(labelEnd)}`;
          editor.executeEdits('ghostex-reference-pill-reveal', [{ range, text: revealed }]);
          const nextCaret = reference.start + labelEnd + 1;
          editor.setPosition(model.getPositionAt(nextCaret));
        };
        const syncReferenceTitles = (): void => {
          for (const [index, reference] of references.entries()) {
            for (const pill of container.querySelectorAll<HTMLElement>(`.${REFERENCE_PILL_ID_CLASS_PREFIX}${index}`)) {
              pill.title = reference.path;
            }
          }
        };
        const renderReferencePills = (): void => {
          const model = editor.getModel();
          if (!model) {
            return;
          }
          references = referenceModel.occurrences(editor.getValue());
          referenceDecorations.set(
            references.map((reference, index) => {
              const start = model.getPositionAt(reference.start);
              const end = model.getPositionAt(reference.end);
              const sourceRange = {
                endColumn: end.column,
                endLineNumber: end.lineNumber,
                startColumn: start.column,
                startLineNumber: start.lineNumber,
              };
              return {
                options: {
                  after: {
                    attachedData: { referenceIndex: index },
                    content: referencePillInjectedText(reference.label, reference.kind),
                    // The pill is projected after its one invisible model
                    // token. Monaco's only caret stop for that injected text
                    // is therefore its right edge; the token's start remains
                    // the natural left edge. No cursor correction is needed.
                    cursorStops: 1,
                    inlineClassName: `${REFERENCE_PILL_CLASS} ${REFERENCE_PILL_CLASS}--${reference.kind} ${REFERENCE_PILL_ID_CLASS_PREFIX}${index}`,
                    inlineClassNameAffectsLetterSpacing: true,
                  },
                  inlineClassName: 'ghostex-chat-composer-reference-source',
                  inlineClassNameAffectsLetterSpacing: true,
                  stickiness: 1,
                },
                range: sourceRange,
              };
            })
          );
          queueMicrotask(syncReferenceTitles);
        };
        const referenceDomObserver = new MutationObserver(syncReferenceTitles);
        referenceDomObserver.observe(container, { childList: true, subtree: true });
        disposables.push({ dispose: () => referenceDomObserver.disconnect() });
        renderReferencePills();
        const applyHeight = (): void => {
          if (fillHeightRef.current) {
            // Clear the inline height so the stylesheet's stretched container
            // wins; monaco's automaticLayout then follows the flex row.
            container.style.height = '';
          } else {
            const height = Math.min(Math.max(editor.getContentHeight(), MIN_INPUT_HEIGHT_PX), MAX_INPUT_HEIGHT_PX);
            container.style.height = `${
              quickInputOpenRef.current ? Math.max(height, QUICK_INPUT_MIN_HEIGHT_PX) : height
            }px`;
          }
          editor.layout();
        };
        applyHeightRef.current = applyHeight;
        // The palette has no open/close event, so the widget's own display
        // toggle is the signal. It is created lazily on first use and then
        // stays in the DOM, so watch for it once and observe it directly
        // afterwards instead of keeping a subtree style observer alive.
        let quickInputWidgetObserver: MutationObserver | null = null;
        const syncQuickInputOpen = (widget: HTMLElement): void => {
          const open = widget.style.display !== 'none';
          if (open === quickInputOpenRef.current) {
            return;
          }
          quickInputOpenRef.current = open;
          applyHeight();
        };
        const observeQuickInputWidget = (): void => {
          const widget = container.querySelector<HTMLElement>('.quick-input-widget');
          if (!widget || quickInputWidgetObserver) {
            return;
          }
          quickInputWidgetObserver = new MutationObserver(() => syncQuickInputOpen(widget));
          quickInputWidgetObserver.observe(widget, {
            attributeFilter: ['style'],
            attributes: true,
          });
          quickInputMountObserver.disconnect();
          syncQuickInputOpen(widget);
        };
        const quickInputMountObserver = new MutationObserver(observeQuickInputWidget);
        quickInputMountObserver.observe(container, { childList: true, subtree: true });
        disposables.push({
          dispose: () => {
            quickInputMountObserver.disconnect();
            quickInputWidgetObserver?.disconnect();
          },
        });
        const insertCanonicalText = (text: string, source: string): boolean => {
          editor.focus();
          editor.trigger(source, 'type', { text: referenceModel.virtualizeInsertion(text) });
          return true;
        };
        insertTextRef.current = (text) => insertCanonicalText(text, 'keyboard');
        clipboardBridgeRef.current = {
          copySelection: (cut) => {
            const model = editor.getModel();
            const selection = editor.getSelection();
            if (!model || !selection) {
              return null;
            }
            const start = model.getOffsetAt(selection.getStartPosition());
            const end = model.getOffsetAt(selection.getEndPosition());
            if (start === end) {
              return null;
            }
            const selected = referenceModel.expand(editor.getValue().slice(start, end));
            if (cut && !disabledRef.current) {
              const range = modelRangeForOffsets(start, end);
              if (range) {
                editor.executeEdits('ghostex-reference-pill-cut', [{ range, text: '' }]);
              }
            }
            return selected;
          },
        };
        disposables.push(
          editor.onDidChangeModelContent(() => {
            renderReferencePills();
            if (suppressChangeRef.current) {
              return;
            }
            callbacksRef.current.onChange(canonicalValue(), canonicalCaretOffset());
            // Monaco updates the selection as part of the same command that
            // raised this event; reading it again once that command has fully
            // unwound is what makes the reported caret independent of the
            // order monaco happens to fire content and cursor events in.
            queueMicrotask(() => {
              // Skip once the editor is gone: a disposed instance has no model
              // and reading it would throw inside the microtask.
              if (editorRef.current === editor) {
                callbacksRef.current.onCaretChange(canonicalCaretOffset());
              }
            });
          }),
          editor.onDidChangeCursorPosition(() => {
            if (suppressChangeRef.current) {
              return;
            }
            callbacksRef.current.onCaretChange(canonicalCaretOffset());
          }),
          editor.onDidContentSizeChange(applyHeight),
          editor.onKeyDown((event) => {
            const key = composerKeyForMonacoEvent(event);
            callbacksRef.current.onKeyDown({
              altKey: event.browserEvent.altKey,
              ctrlKey: event.browserEvent.ctrlKey,
              isComposing: event.browserEvent.isComposing,
              key,
              metaKey: event.browserEvent.metaKey,
              preventDefault: () => {
                event.preventDefault();
                event.stopPropagation();
              },
              shiftKey: event.browserEvent.shiftKey,
            });
          }),
          editor.onMouseDown((event) => {
            if (event.event.detail !== 2) {
              return;
            }
            const pill = event.event.target.closest<HTMLElement>(`.${REFERENCE_PILL_CLASS}`);
            if (!pill || !container.contains(pill)) {
              return;
            }
            const idClass = [...pill.classList].find((className) =>
              className.startsWith(REFERENCE_PILL_ID_CLASS_PREFIX)
            );
            const index = Number.parseInt(idClass?.slice(REFERENCE_PILL_ID_CLASS_PREFIX.length) ?? '', 10);
            const reference = references[index];
            if (!reference) {
              return;
            }
            event.event.preventDefault();
            event.event.stopPropagation();
            revealReferenceSource(reference);
          })
        );
        applyHeight();
        registerApi({
          applyValue: (next, caret) => {
            const model = editor.getModel();
            if (!model) {
              return;
            }
            const currentPresentation = editor.getValue();
            if (referenceModel.expand(currentPresentation) !== next) {
              suppressChangeRef.current = true;
              try {
                if (next === '') {
                  // Sending or explicitly clearing a composer is a hard draft
                  // boundary. Clear the owned decorations and Monaco undo
                  // state together, then recycle the invisible token registry
                  // for the next draft. Full-range executeEdits can retain a
                  // zero-width injected decoration at column one.
                  referenceDecorations.clear();
                  references = [];
                  editor.setValue('');
                  referenceModel.reset();
                } else {
                  const nextPresentation = referenceModel.virtualizeCanonical(next, currentPresentation);
                  editor.executeEdits('ghostex-composer', [
                    { range: model.getFullModelRange(), text: nextPresentation },
                  ]);
                }
              } finally {
                suppressChangeRef.current = false;
              }
            }
            const clampedCaret = Math.min(caret, next.length);
            editor.setPosition(
              model.getPositionAt(referenceModel.canonicalOffsetToModel(editor.getValue(), clampedCaret))
            );
          },
          focus: () => editor.focus(),
          getSelection: () => {
            const model = editor.getModel();
            const selection = editor.getSelection();
            if (!model || !selection) {
              const length = canonicalValue().length;
              return { end: length, start: length };
            }
            const presentation = editor.getValue();
            return {
              end: referenceModel.modelOffsetToCanonical(presentation, model.getOffsetAt(selection.getEndPosition())),
              start: referenceModel.modelOffsetToCanonical(
                presentation,
                model.getOffsetAt(selection.getStartPosition())
              ),
            };
          },
          getValue: canonicalValue,
          insertSavedPrompt: (text) => insertCanonicalText(text, 'ghostex-saved-prompt'),
          insertText: (text) => insertCanonicalText(text, 'keyboard'),
          selectAll: () => {
            const model = editor.getModel();
            if (!model) {
              return;
            }
            editor.focus();
            editor.setSelection(model.getFullModelRange());
            callbacksRef.current.onCaretChange(canonicalValue().length);
          },
        });
      })
      .catch((error: unknown) => {
        if (!disposed) {
          callbacksRef.current.onLoadFailed(error);
        }
      });
    return () => {
      disposed = true;
      registerApi(null);
      applyHeightRef.current = null;
      clipboardBridgeRef.current = null;
      insertTextRef.current = null;
      for (const disposable of disposables) {
        disposable.dispose();
      }
      editorRef.current?.dispose();
      editorRef.current = null;
    };
    // The editor is created once per mount; live option updates go through
    // the dedicated effects below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [vsBaseUrl]);

  useEffect(() => {
    applyHeightRef.current?.();
  }, [fillHeight]);

  useEffect(() => {
    editorRef.current?.updateOptions({ readOnly: disabled });
  }, [disabled]);

  useEffect(() => {
    editorRef.current?.updateOptions({ placeholder });
  }, [placeholder]);

  useEffect(() => {
    editorRef.current?.updateOptions({ theme: CHAT_MONACO_THEMES[theme] });
  }, [theme]);

  useEffect(() => {
    const updateFontFamily = (): void => {
      const container = containerRef.current;
      if (container) {
        editorRef.current?.updateOptions({
          fontFamily: getComputedStyle(container).fontFamily,
        });
      }
    };
    window.addEventListener('ghostex-session-chat-font-family-changed', updateFontFamily);
    return () => {
      window.removeEventListener('ghostex-session-chat-font-family-changed', updateFontFamily);
    };
  }, []);

  /*
  CDXC:MonacoPasteCapture 2026-08-01:
  Monaco swallows paste events before they reach ancestors of its hidden
  textarea, so clipboard interception must hook the window capture phase
  (same lesson as the standalone prompt editor). Text enters through the
  canonical-to-presentation bridge so private pill tokens never escape to or
  arrive from the system clipboard.
  */
  useEffect(() => {
    const isEditorClipboardEvent = (event: globalThis.ClipboardEvent): boolean => {
      const container = containerRef.current;
      const target = event.target;
      return Boolean(container && target instanceof Node && container.contains(target));
    };
    const handlePasteCapture = (event: globalThis.ClipboardEvent): void => {
      if (!event.clipboardData || !isEditorClipboardEvent(event)) {
        return;
      }
      if (callbacksRef.current.onPasteData(event.clipboardData)) {
        event.preventDefault();
        event.stopPropagation();
        return;
      }
      const text = event.clipboardData.getData('text/plain');
      if (text !== '' && insertTextRef.current?.(text)) {
        event.preventDefault();
        event.stopPropagation();
      }
    };
    const handleCopyCapture = (event: globalThis.ClipboardEvent): void => {
      if (!event.clipboardData || !isEditorClipboardEvent(event)) {
        return;
      }
      const selected = clipboardBridgeRef.current?.copySelection(false);
      if (selected === null || selected === undefined) {
        return;
      }
      event.clipboardData.setData('text/plain', selected);
      event.preventDefault();
      event.stopPropagation();
    };
    const handleCutCapture = (event: globalThis.ClipboardEvent): void => {
      if (!event.clipboardData || !isEditorClipboardEvent(event)) {
        return;
      }
      const selected = clipboardBridgeRef.current?.copySelection(true);
      if (selected === null || selected === undefined) {
        return;
      }
      event.clipboardData.setData('text/plain', selected);
      event.preventDefault();
      event.stopPropagation();
    };
    window.addEventListener('copy', handleCopyCapture, true);
    window.addEventListener('cut', handleCutCapture, true);
    window.addEventListener('paste', handlePasteCapture, true);
    return () => {
      window.removeEventListener('copy', handleCopyCapture, true);
      window.removeEventListener('cut', handleCutCapture, true);
      window.removeEventListener('paste', handlePasteCapture, true);
    };
  }, []);

  return (
    <div
      className='ghostex-chat-composer-monaco w-full min-w-0 flex-1 overflow-hidden'
      data-session-chat-typing-redirect-ignore='true'
      ref={containerRef}
    />
  );
}
