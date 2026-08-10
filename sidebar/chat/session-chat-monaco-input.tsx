// Monaco-backed chat composer input.
//
// Monaco is loaded at runtime through its AMD loader from a host-provided
// vs base URL (gpui CEF: "./monaco/vs" staged next to chat.html; web:
// "/monaco/vs" served from node_modules by the vite config). There is no ESM
// import of monaco-editor anywhere in the repo — the AMD route is the only
// one that survives the gpui single-file CEF bundle (see
// sidebar/agents-hub-modal.tsx for the precedent) — so all editor access
// goes through minimal structural types instead of monaco's own typings.
//
// The mobile single-file WebView bundle never passes a vs base URL (workers
// and sibling assets are unreachable from a base-URL-less html string), so
// this component is never mounted there and the composer keeps its textarea.

import { useEffect, useRef } from "react";
import type { SessionChatTheme } from "../../shared/session-chat";
import type { SessionChatComposerInputApi, SessionChatComposerKeyEvent } from "./session-chat-composer";

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
  preventDefault(): void;
  stopPropagation(): void;
}

interface MonacoDisposableLike {
  dispose(): void;
}

interface MonacoEditorInstanceLike {
  dispose(): void;
  executeEdits(source: string, edits: { range: unknown; text: string }[]): void;
  focus(): void;
  getContentHeight(): number;
  getModel(): MonacoModelLike | null;
  getSelection(): MonacoSelectionLike | null;
  getValue(): string;
  layout(): void;
  onDidChangeModelContent(listener: () => void): MonacoDisposableLike;
  onDidContentSizeChange(listener: () => void): MonacoDisposableLike;
  onKeyDown(listener: (event: MonacoKeyboardEventLike) => void): MonacoDisposableLike;
  setPosition(position: MonacoPositionLike): void;
  trigger(source: string, handlerId: string, payload: unknown): void;
  updateOptions(options: Record<string, unknown>): void;
}

interface MonacoNamespaceLike {
  editor: {
    create(
      container: HTMLElement,
      options: Record<string, unknown>,
    ): MonacoEditorInstanceLike;
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
  dark: "ghostex-session-chat-dark",
  light: "ghostex-session-chat-light",
};
const MIN_INPUT_HEIGHT_PX = 24;
const MAX_INPUT_HEIGHT_PX = 160;

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
      if (typeof amdRequire !== "function") {
        reject(new Error("The Monaco AMD loader did not install."));
        return;
      }
      amdRequire.config?.({ paths: { vs: vsBaseUrl } });
      amdRequire(
        ["vs/editor/editor.main"],
        () => {
          if (target.monaco) {
            resolve(target.monaco);
          } else {
            reject(new Error("Monaco loaded without installing window.monaco."));
          }
        },
        (error) => reject(error instanceof Error ? error : new Error(String(error))),
      );
    };
    const existing = target.require;
    if (typeof existing === "function" && existing.config) {
      finish();
      return;
    }
    const script = document.createElement("script");
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
    base: "vs-dark",
    colors: {
      "editor.background": "#00000000",
      "editor.lineHighlightBackground": "#00000000",
      "editorGutter.background": "#00000000",
    },
    inherit: true,
    rules: [],
  });
  monaco.editor.defineTheme(CHAT_MONACO_THEMES.light, {
    base: "vs",
    colors: {
      "editor.background": "#00000000",
      "editor.lineHighlightBackground": "#00000000",
      "editorGutter.background": "#00000000",
    },
    inherit: true,
    rules: [],
  });
}

export function SessionChatMonacoInput({
  disabled,
  initialValue,
  onChange,
  onKeyDown,
  onLoadFailed,
  onPasteData,
  placeholder,
  registerApi,
  theme,
  vsBaseUrl,
}: {
  disabled: boolean;
  initialValue: string;
  onChange: (value: string) => void;
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
  const suppressChangeRef = useRef(false);
  const themeRef = useRef(theme);
  themeRef.current = theme;
  const callbacksRef = useRef({ onChange, onKeyDown, onLoadFailed, onPasteData });
  callbacksRef.current = { onChange, onKeyDown, onLoadFailed, onPasteData };

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
        const editor = monaco.editor.create(container, {
          autoClosingBrackets: "never",
          automaticLayout: true,
          contextmenu: false,
          folding: false,
          fontFamily:
            'ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
          fontSize: 14,
          glyphMargin: false,
          // Plain text on purpose: the draft is a prompt, so markdown *emphasis*
          // styling and syntax colorization inside the input read as noise.
          language: "plaintext",
          lineDecorationsWidth: 0,
          lineHeight: 24,
          lineNumbers: "off",
          lineNumbersMinChars: 0,
          minimap: { enabled: false },
          occurrencesHighlight: "off",
          overviewRulerBorder: false,
          overviewRulerLanes: 0,
          padding: { bottom: 0, top: 0 },
          placeholder,
          quickSuggestions: false,
          renderLineHighlight: "none",
          scrollBeyondLastLine: false,
          scrollbar: {
            alwaysConsumeMouseWheel: false,
            horizontal: "hidden",
            useShadows: false,
            vertical: "auto",
            verticalScrollbarSize: 3,
          },
          suggestOnTriggerCharacters: false,
          theme: CHAT_MONACO_THEMES[themeRef.current],
          unicodeHighlight: { ambiguousCharacters: false },
          value: initialValue,
          wordBasedSuggestions: "off",
          wordWrap: "on",
          wrappingStrategy: "advanced",
        });
        editorRef.current = editor;
        const applyHeight = (): void => {
          const height = Math.min(
            Math.max(editor.getContentHeight(), MIN_INPUT_HEIGHT_PX),
            MAX_INPUT_HEIGHT_PX,
          );
          container.style.height = `${height}px`;
          editor.layout();
        };
        disposables.push(
          editor.onDidChangeModelContent(() => {
            if (!suppressChangeRef.current) {
              callbacksRef.current.onChange(editor.getValue());
            }
          }),
          editor.onDidContentSizeChange(applyHeight),
          editor.onKeyDown((event) => {
            callbacksRef.current.onKeyDown({
              altKey: event.browserEvent.altKey,
              ctrlKey: event.browserEvent.ctrlKey,
              isComposing: event.browserEvent.isComposing,
              key: event.browserEvent.key,
              metaKey: event.browserEvent.metaKey,
              preventDefault: () => {
                event.preventDefault();
                event.stopPropagation();
              },
              shiftKey: event.browserEvent.shiftKey,
            });
          }),
        );
        applyHeight();
        registerApi({
          applyValue: (next, caret) => {
            const model = editor.getModel();
            if (!model) {
              return;
            }
            if (editor.getValue() !== next) {
              suppressChangeRef.current = true;
              try {
                editor.executeEdits("ghostex-composer", [
                  { range: model.getFullModelRange(), text: next },
                ]);
              } finally {
                suppressChangeRef.current = false;
              }
            }
            editor.setPosition(model.getPositionAt(Math.min(caret, next.length)));
          },
          focus: () => editor.focus(),
          getSelection: () => {
            const model = editor.getModel();
            const selection = editor.getSelection();
            if (!model || !selection) {
              const length = editor.getValue().length;
              return { end: length, start: length };
            }
            return {
              end: model.getOffsetAt(selection.getEndPosition()),
              start: model.getOffsetAt(selection.getStartPosition()),
            };
          },
          getValue: () => editor.getValue(),
          insertText: (text) => {
            editor.focus();
            editor.trigger("keyboard", "type", { text });
            return true;
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
    editorRef.current?.updateOptions({ readOnly: disabled });
  }, [disabled]);

  useEffect(() => {
    editorRef.current?.updateOptions({ placeholder });
  }, [placeholder]);

  useEffect(() => {
    editorRef.current?.updateOptions({ theme: CHAT_MONACO_THEMES[theme] });
  }, [theme]);

  /*
  CDXC:MonacoPasteCapture 2026-08-01:
  Monaco swallows paste events before they reach ancestors of its hidden
  textarea, so image-paste interception must hook the window capture phase
  (same lesson as the standalone prompt editor). Text pastes return false
  from onPasteData and fall through to Monaco untouched.
  */
  useEffect(() => {
    const handlePasteCapture = (event: globalThis.ClipboardEvent): void => {
      const container = containerRef.current;
      const target = event.target;
      if (
        !container ||
        !event.clipboardData ||
        !(target instanceof Node) ||
        !container.contains(target)
      ) {
        return;
      }
      if (callbacksRef.current.onPasteData(event.clipboardData)) {
        event.preventDefault();
        event.stopPropagation();
      }
    };
    window.addEventListener("paste", handlePasteCapture, true);
    return () => {
      window.removeEventListener("paste", handlePasteCapture, true);
    };
  }, []);

  return (
    <div
      className="min-h-6 w-full min-w-0 flex-1 overflow-hidden"
      data-session-chat-typing-redirect-ignore="true"
      ref={containerRef}
    />
  );
}
