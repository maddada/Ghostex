// Session chat composer (upstream chat spec §1.1/§11.6 port). Enter sends,
// Shift+Enter inserts a newline, Escape interrupts, the IME guard swallows
// composition Enter, and ArrowUp/Down recall draft history. Typing a
// line-leading "/" opens the slash-command picker (per-agent catalog):
// ArrowUp/Down highlight, Tab/Enter complete, Enter on an exact match sends,
// Escape dismisses the picker without interrupting.
//
// Layout (§1.1): input row, then a footer row — session identity/options on
// the left, with Attach and Send/Stop on the right. Styled with shadcn tokens
// to sit under the shadcn chat conversation.

import {
  IconArrowUp,
  IconLoader2,
  IconPaperclip,
  IconPlayerStopFilled,
  IconRobot,
  IconX,
} from "@tabler/icons-react";
import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { cn } from "../../lib/utils";
import { Button } from "../../components/ui/button";
import { AppTooltip } from "../app-tooltip";
import {
  EMPTY_SESSION_CHAT_COMPOSER_HISTORY,
  pushSessionChatComposerHistory,
  recallNextSessionChatDraft,
  recallPreviousSessionChatDraft,
  resetSessionChatComposerHistoryIndex,
} from "./session-chat-composer-state";
import {
  filterSessionChatSlashCommands,
  sessionChatSlashQuery,
  type SessionChatSlashCommand,
} from "./session-chat-slash-commands";
import { SessionChatMonacoInput } from "./session-chat-monaco-input";
import {
  sessionChatImageTargetForHref,
  useSessionChatImageViewer,
} from "./session-chat-image-viewer";
import type { SessionChatTheme } from "../../shared/session-chat";

export interface SessionChatComposerHandle {
  /** Clear the draft only when it still matches the supplied snapshot. */
  clearDraft: (expected: string) => boolean;
  focus: () => void;
  getDraft: () => string;
  /** Insert text at the caret; returns false when the composer cannot take it. */
  insertTypedText: (text: string) => boolean;
}

/**
 * Backend-neutral key event: the textarea path adapts React's KeyboardEvent,
 * the Monaco path adapts monaco's IKeyboardEvent (whose preventDefault also
 * stops monaco's own handling of the key).
 */
export interface SessionChatComposerKeyEvent {
  altKey: boolean;
  ctrlKey: boolean;
  isComposing: boolean;
  key: string;
  metaKey: boolean;
  shiftKey: boolean;
  preventDefault: () => void;
}

/**
 * Imperative surface of the active input backend. `draft` state stays the
 * source of truth; applyValue only synchronizes the visual input (and caret)
 * after the composer has already updated the draft itself.
 */
export interface SessionChatComposerInputApi {
  applyValue: (next: string, caret: number) => void;
  focus: () => void;
  getSelection: () => { end: number; start: number };
  getValue: () => string;
  insertText: (text: string) => boolean;
}

export interface SessionChatComposerProps {
  disabled?: boolean;
  isWorking: boolean;
  /** Stable conversation identity used to restore this session's unsent draft. */
  sessionKey?: string;
  placeholder?: string;
  /** Agent slash commands offered by the "/" picker; empty disables it. */
  slashCommands?: readonly SessionChatSlashCommand[];
  /** Section heading shown above the picker rows (usually the agent name). */
  slashHeading?: string;
  onSend: (text: string) => void | Promise<void>;
  onInterrupt: () => void;
  /**
   * Saves a pasted image onto the session's machine and resolves with the
   * absolute path there. When set, pasting an image inserts the terminal
   * paste reference "[Image #N](path)" and shows a preview thumbnail above
   * the input; when omitted, image pastes fall through untouched.
   */
  onPasteImage?: (payload: {
    base64Data: string;
    suggestedName?: string;
  }) => Promise<string>;
  /**
   * Saves any non-image attachment onto the session's machine and resolves
   * with the absolute path there, inserted as "[File #N](path)". When
   * omitted, the attach button only accepts images.
   */
  onAttachFile?: (payload: {
    base64Data: string;
    suggestedName?: string;
  }) => Promise<string>;
  /**
   * Host-native attach picker resolving with absolute paths on the session's
   * machine (may include folders). When set, the attach button uses it
   * instead of the browser file input; image paths insert "[Image #N](path)"
   * and everything else "[File #N](path)".
   */
  onPickPaths?: () => Promise<string[]>;
  /**
   * Loads a preview data URL for an image path picked natively (no bytes in
   * the page otherwise). Optional garnish: picks insert their reference even
   * when the preview cannot load.
   */
  onLoadImagePreview?: (path: string) => Promise<string>;
  /**
   * Session-option pills rendered in the footer, left of Send (§1.1). The view
   * builds them so the composer stays about input mechanics; agents without an
   * option catalog pass nothing.
   */
  optionPills?: ReactNode;
  /**
   * Base URL of monaco-editor's min/vs directory on this surface. When set,
   * the input is a Monaco editor (editing hotkeys work); when omitted (the
   * mobile single-file bundle, where Monaco's sibling assets are
   * unreachable), the plain textarea renders instead.
   */
  monacoVsBaseUrl?: string;
  /** Palette used by the chat-owned Monaco prompt input. */
  theme?: SessionChatTheme;
}

interface PastedImagePreview {
  dataUrl: string;
  id: string;
  path: string;
}

/** Rich Prompt Editor numbering: max existing [Image #N]( in the draft, +1. */
function nextImageReferenceIndex(text: string): number {
  let highest = 0;
  for (const match of text.matchAll(/\[Image #(\d+)\]\(/g)) {
    const index = Number.parseInt(match[1] ?? "", 10);
    if (Number.isFinite(index)) {
      highest = Math.max(highest, index);
    }
  }
  return highest + 1;
}

/** Same numbering scheme for non-image attachments: "[File #N](path)". */
function nextFileReferenceIndex(text: string): number {
  let highest = 0;
  for (const match of text.matchAll(/\[File #(\d+)\]\(/g)) {
    const index = Number.parseInt(match[1] ?? "", 10);
    if (Number.isFinite(index)) {
      highest = Math.max(highest, index);
    }
  }
  return highest + 1;
}

const IMAGE_PATH_PATTERN = /\.(avif|bmp|gif|heic|heif|ico|jpe?g|png|svg|tiff?|webp)$/i;
const LINKED_IMAGE_REFERENCE_PATTERN = /\[Image #\d+\]\(([^)\r\n]+)\)/g;
const SESSION_CHAT_DRAFT_STORAGE_PREFIX = "ghostex.sessionChat.draft.";

function composerDraftStorage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

function readStoredSessionChatDraft(sessionKey: string | undefined): string {
  if (!sessionKey) {
    return "";
  }
  return composerDraftStorage()?.getItem(
    `${SESSION_CHAT_DRAFT_STORAGE_PREFIX}${sessionKey}`,
  ) ?? "";
}

function writeStoredSessionChatDraft(
  sessionKey: string | undefined,
  draft: string,
): void {
  if (!sessionKey) {
    return;
  }
  try {
    const storage = composerDraftStorage();
    const key = `${SESSION_CHAT_DRAFT_STORAGE_PREFIX}${sessionKey}`;
    if (draft === "") {
      storage?.removeItem(key);
    } else {
      storage?.setItem(key, draft);
    }
  } catch {
    // Storage quota/private-mode failures must not break the composer.
  }
}

function linkedImageReferenceHrefs(text: string): string[] {
  return [...text.matchAll(LINKED_IMAGE_REFERENCE_PATTERN)]
    .map((match) => match[1]?.trim() ?? "")
    .filter(Boolean);
}

function isImageFile(file: File): boolean {
  return file.type.startsWith("image/") || IMAGE_PATH_PATTERN.test(file.name);
}

function clipboardImageFiles(data: DataTransfer): File[] {
  const files: File[] = [];
  for (const item of Array.from(data.items)) {
    if (item.kind !== "file") {
      continue;
    }
    const file = item.getAsFile();
    if (file && isImageFile(file)) {
      files.push(file);
    }
  }
  return files;
}

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () =>
      reject(reader.error ?? new Error("Could not read the pasted image."));
    reader.readAsDataURL(file);
  });
}

function reactKeyEventAdapter(
  event: KeyboardEvent<HTMLTextAreaElement>,
): SessionChatComposerKeyEvent {
  return {
    altKey: event.altKey,
    ctrlKey: event.ctrlKey,
    isComposing: event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229,
    key: event.key,
    metaKey: event.metaKey,
    preventDefault: () => event.preventDefault(),
    shiftKey: event.shiftKey,
  };
}

export const SessionChatComposer = forwardRef<
  SessionChatComposerHandle,
  SessionChatComposerProps
>(function SessionChatComposer(
  {
    disabled = false,
    isWorking,
    monacoVsBaseUrl,
    onAttachFile,
    onInterrupt,
    onLoadImagePreview,
    onPasteImage,
    onPickPaths,
    onSend,
    optionPills,
    placeholder,
    sessionKey,
    slashCommands,
    slashHeading,
    theme = "dark",
  },
  ref,
) {
  const [draft, setDraft] = useState(() => readStoredSessionChatDraft(sessionKey));
  const [history, setHistory] = useState(EMPTY_SESSION_CHAT_COMPOSER_HISTORY);
  const [slashDismissed, setSlashDismissed] = useState(false);
  const [slashIndex, setSlashIndex] = useState(0);
  const [pastedImages, setPastedImages] = useState<readonly PastedImagePreview[]>([]);
  const [pendingImagePastes, setPendingImagePastes] = useState(0);
  const [monacoFailed, setMonacoFailed] = useState(false);
  const imageViewer = useSessionChatImageViewer();
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const slashListRef = useRef<HTMLDivElement | null>(null);
  const pasteSequenceRef = useRef(0);
  const previewLoadsRef = useRef(new Set<string>());
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const monacoApiRef = useRef<SessionChatComposerInputApi | null>(null);
  const pendingFocusRef = useRef(false);
  const pendingInsertTextRef = useRef("");
  const sendInFlightRef = useRef(false);
  const useMonaco = monacoVsBaseUrl !== undefined && !monacoFailed;

  // Previews mirror the draft: deleting a reference (by any means, including
  // sending, which clears the draft) drops its thumbnail.
  useEffect(() => {
    const referencedHrefs = new Set(linkedImageReferenceHrefs(draft));
    setPastedImages((current) =>
      current.filter((image) => referencedHrefs.has(image.path)),
    );
  }, [draft]);

  const slashQuery = sessionChatSlashQuery(draft);
  const slashMatches = useMemo(
    () =>
      slashQuery !== null && !slashDismissed && slashCommands !== undefined
        ? filterSessionChatSlashCommands(slashCommands, slashQuery)
        : [],
    [slashCommands, slashDismissed, slashQuery],
  );
  const slashOpen = slashMatches.length > 0 && !disabled;
  const highlightedIndex = Math.min(slashIndex, Math.max(slashMatches.length - 1, 0));

  useEffect(() => {
    if (!slashOpen) {
      return;
    }
    slashListRef.current
      ?.querySelector('[data-highlighted="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [highlightedIndex, slashOpen]);

  const updateDraft = (next: string): void => {
    writeStoredSessionChatDraft(sessionKey, next);
    setDraft(next);
    setHistory((current) => resetSessionChatComposerHistoryIndex(current));
    if (sessionChatSlashQuery(next) === null) {
      setSlashDismissed(false);
    }
    setSlashIndex(0);
  };

  const textareaApi: SessionChatComposerInputApi = {
    applyValue: (next, caret) => {
      // Value arrives through the controlled `draft`; only the caret needs
      // repositioning once React has committed it.
      requestAnimationFrame(() => {
        const clamped = Math.min(caret, next.length);
        textareaRef.current?.setSelectionRange(clamped, clamped);
      });
    },
    focus: () => textareaRef.current?.focus(),
    getSelection: () => {
      const textarea = textareaRef.current;
      const fallback = textarea?.value.length ?? draft.length;
      return {
        end: textarea?.selectionEnd ?? fallback,
        start: textarea?.selectionStart ?? fallback,
      };
    },
    getValue: () => textareaRef.current?.value ?? draft,
    insertText: (text) => {
      const textarea = textareaRef.current;
      if (!textarea) {
        return false;
      }
      const start = textarea.selectionStart ?? textarea.value.length;
      const end = textarea.selectionEnd ?? textarea.value.length;
      const next = `${textarea.value.slice(0, start)}${text}${textarea.value.slice(end)}`;
      updateDraft(next);
      textarea.focus();
      requestAnimationFrame(() => {
        const caret = start + text.length;
        textarea.setSelectionRange(caret, caret);
      });
      return true;
    },
  };

  // Resolved lazily: the Monaco backend registers its api into a ref after
  // an async load, without a re-render, so a render-scoped const would go
  // stale between load and the next state change.
  const getInputApi = (): SessionChatComposerInputApi | null =>
    useMonaco ? monacoApiRef.current : textareaApi;

  useEffect(() => {
    if (!useMonaco && textareaRef.current) {
      if (pendingInsertTextRef.current) {
        const pending = pendingInsertTextRef.current;
        pendingInsertTextRef.current = "";
        textareaApi.insertText(pending);
      }
      if (pendingFocusRef.current) {
        pendingFocusRef.current = false;
        textareaRef.current.focus();
      }
    }
  }, [useMonaco]);

  useImperativeHandle(ref, () => ({
    clearDraft: (expected: string): boolean => {
      const current = getInputApi()?.getValue() ?? draftRef.current;
      if (current !== expected) {
        return false;
      }
      writeStoredSessionChatDraft(sessionKey, "");
      draftRef.current = "";
      setDraft("");
      setHistory((value) => resetSessionChatComposerHistoryIndex(value));
      getInputApi()?.applyValue("", 0);
      setSlashDismissed(false);
      setSlashIndex(0);
      return true;
    },
    focus: () => {
      const input = getInputApi();
      if (!input) {
        // Monaco loads asynchronously. Preserve the host's one-shot focus
        // handoff until the real editor API exists instead of dropping it.
        pendingFocusRef.current = true;
        return;
      }
      pendingFocusRef.current = false;
      input.focus();
    },
    getDraft: () => getInputApi()?.getValue() ?? draftRef.current,
    insertTypedText: (text: string): boolean => {
      if (disabled) {
        return false;
      }
      const input = getInputApi();
      if (!input) {
        pendingInsertTextRef.current += text;
        return true;
      }
      return input.insertText(text);
    },
  }));

  const send = (text: string = draft): void => {
    if (text.trim() === "" || disabled || sendInFlightRef.current) {
      return;
    }
    sendInFlightRef.current = true;
    void Promise.resolve()
      .then(() => onSend(text))
      .then(() => {
        // Preservation and terminal submission are asynchronous. Clear only
        // the exact snapshot that succeeded; text added while the request was
        // in flight remains the next draft.
        const current = getInputApi()?.getValue() ?? draftRef.current;
        if (current === text) {
          writeStoredSessionChatDraft(sessionKey, "");
          draftRef.current = "";
          setDraft("");
          getInputApi()?.applyValue("", 0);
        }
        setHistory((value) => pushSessionChatComposerHistory(value, text));
        setSlashDismissed(false);
        setSlashIndex(0);
      })
      .catch(() => {
        // The transport rejected the send before submission. Keep the exact
        // chat draft so preserving terminal input can be retried safely.
      })
      .finally(() => {
        sendInFlightRef.current = false;
      });
  };

  const insertReference = (reference: string): void => {
    const api = getInputApi();
    const current = api?.getValue() ?? draft;
    const { end, start } = api?.getSelection() ?? {
      end: current.length,
      start: current.length,
    };
    const needsLeadingSpace = start > 0 && !/\s/.test(current[start - 1] ?? "");
    const inserted = `${needsLeadingSpace ? " " : ""}${reference} `;
    const next = `${current.slice(0, start)}${inserted}${current.slice(end)}`;
    updateDraft(next);
    api?.focus();
    api?.applyValue(next, start + inserted.length);
  };

  const addImagePreview = useCallback((path: string, dataUrl: string): void => {
    setPastedImages((currentImages) => {
      if (currentImages.some((image) => image.path === path)) {
        return currentImages;
      }
      pasteSequenceRef.current += 1;
      return [
        ...currentImages,
        { dataUrl, id: `${path}#${pasteSequenceRef.current}`, path },
      ];
    });
  }, []);

  // A pasted/typed literal "[Image #N](path)" is the same attachment as one
  // inserted by the paperclip. Resolve it through the shared image viewer so
  // it gains a thumbnail without requiring a second attach action.
  useEffect(() => {
    if (!imageViewer) {
      return;
    }
    for (const href of linkedImageReferenceHrefs(draft)) {
      if (
        pastedImages.some((image) => image.path === href) ||
        previewLoadsRef.current.has(href)
      ) {
        continue;
      }
      const pending = imageViewer.resolve(sessionChatImageTargetForHref(href));
      if (!pending) {
        continue;
      }
      previewLoadsRef.current.add(href);
      void pending
        .then((dataUrl) => {
          if (linkedImageReferenceHrefs(draftRef.current).includes(href)) {
            addImagePreview(href, dataUrl);
          }
        })
        .catch(() => {
          // Keep the literal reference when its preview cannot be loaded.
        })
        .finally(() => {
          previewLoadsRef.current.delete(href);
        });
    }
  }, [addImagePreview, draft, imageViewer, pastedImages]);

  const insertImageReference = (path: string, dataUrl?: string): void => {
    const api = getInputApi();
    const current = api?.getValue() ?? draft;
    insertReference(`[Image #${nextImageReferenceIndex(current)}](${path})`);
    if (dataUrl !== undefined) {
      addImagePreview(path, dataUrl);
    }
  };

  const insertFileReference = (path: string): void => {
    const api = getInputApi();
    const current = api?.getValue() ?? draft;
    insertReference(`[File #${nextFileReferenceIndex(current)}](${path})`);
  };

  const removePastedImage = (image: PastedImagePreview): void => {
    const api = getInputApi();
    const current = api?.getValue() ?? draft;
    const escapedPath = image.path.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const pattern = new RegExp(`\\s?\\[Image #\\d+\\]\\(${escapedPath}\\) ?`);
    const matchIndex = current.search(pattern);
    if (matchIndex < 0) {
      // The reference text is already gone; just drop the thumbnail.
      setPastedImages((images) => images.filter((entry) => entry.id !== image.id));
      return;
    }
    const next = current.replace(pattern, "");
    updateDraft(next);
    api?.applyValue(next, matchIndex);
  };

  /**
   * The image intake path: clipboard paste and the footer's attach button
   * both land here, so an attached image becomes the same "[Image #N](path)"
   * reference plus preview thumbnail a pasted one does.
   */
  const consumeImageFiles = (files: readonly File[]): void => {
    void (async () => {
      for (const file of files) {
        setPendingImagePastes((count) => count + 1);
        try {
          const dataUrl = await readFileAsDataUrl(file);
          const base64Data = dataUrl.split(",", 2)[1] ?? "";
          if (base64Data === "") {
            continue;
          }
          const path = await onPasteImage?.({
            base64Data,
            ...(file.name ? { suggestedName: file.name } : {}),
          });
          if (path !== undefined) {
            insertImageReference(path, dataUrl);
          }
        } catch (error) {
          console.error("[session-chat] image attach failed", error);
        } finally {
          setPendingImagePastes((count) => count - 1);
        }
      }
    })();
  };

  /** Non-image attach intake: upload the bytes, insert "[File #N](path)". */
  const consumeAttachmentFiles = (files: readonly File[]): void => {
    void (async () => {
      for (const file of files) {
        setPendingImagePastes((count) => count + 1);
        try {
          const dataUrl = await readFileAsDataUrl(file);
          const base64Data = dataUrl.split(",", 2)[1] ?? "";
          if (base64Data === "") {
            continue;
          }
          const path = await onAttachFile?.({
            base64Data,
            ...(file.name ? { suggestedName: file.name } : {}),
          });
          if (path !== undefined) {
            insertFileReference(path);
          }
        } catch (error) {
          console.error("[session-chat] file attach failed", error);
        } finally {
          setPendingImagePastes((count) => count - 1);
        }
      }
    })();
  };

  /**
   * Host-native picker intake: absolute paths on the session's machine
   * (folders included), no byte upload. Image paths keep the image reference
   * format and fetch their preview thumbnail lazily; everything else becomes
   * a "[File #N](path)" reference.
   */
  const attachFromNativePicker = (): void => {
    void (async () => {
      try {
        const paths = (await onPickPaths?.()) ?? [];
        for (const path of paths) {
          if (IMAGE_PATH_PATTERN.test(path)) {
            insertImageReference(path);
            onLoadImagePreview?.(path)
              .then((dataUrl) => {
                addImagePreview(path, dataUrl);
              })
              .catch(() => {
                // The preview is garnish; the reference is already inserted.
              });
          } else {
            insertFileReference(path);
          }
          // Let the input backend commit before the next caret-relative
          // insert (the textarea backend applies values on the next frame).
          await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
        }
      } catch (error) {
        console.error("[session-chat] attach picker failed", error);
      }
    })();
  };

  /** Returns true when the clipboard held images this composer consumed. */
  const processClipboardData = (data: DataTransfer): boolean => {
    if (!onPasteImage || disabled) {
      return false;
    }
    const files = clipboardImageFiles(data);
    if (files.length === 0) {
      return false;
    }
    consumeImageFiles(files);
    return true;
  };

  const completeSlashCommand = (command: SessionChatSlashCommand): void => {
    const next = `/${command.name}`;
    updateDraft(next);
    const api = getInputApi();
    api?.focus();
    api?.applyValue(next, next.length);
  };

  const handleSlashKeyDown = (event: SessionChatComposerKeyEvent): boolean => {
    if (!slashOpen) {
      return false;
    }
    const highlighted = slashMatches[highlightedIndex];
    if (event.key === "Escape") {
      event.preventDefault();
      setSlashDismissed(true);
      return true;
    }
    if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      const delta = event.key === "ArrowUp" ? -1 : 1;
      setSlashIndex(
        (highlightedIndex + delta + slashMatches.length) % slashMatches.length,
      );
      return true;
    }
    if (event.key === "Tab") {
      event.preventDefault();
      completeSlashCommand(highlighted);
      return true;
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      // A fully typed (or previously completed) command sends immediately;
      // a partial token completes first so arguments can still be added.
      if (draft === `/${highlighted.name}`) {
        send();
      } else {
        completeSlashCommand(highlighted);
      }
      return true;
    }
    return false;
  };

  const handleKeyDown = (event: SessionChatComposerKeyEvent): void => {
    // IME guard: composition Enter confirms the composition; letting it fall
    // through would submit a partial draft. (The textarea wrapper additionally
    // preventDefaults composition Enter; Monaco manages its own IME.)
    if (event.isComposing) {
      return;
    }
    if (handleSlashKeyDown(event)) {
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      onInterrupt();
      return;
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      send();
      return;
    }
    if (event.key === "ArrowUp" && (draft === "" || history.index !== null)) {
      const recalled = recallPreviousSessionChatDraft(history);
      if (recalled) {
        event.preventDefault();
        setHistory(recalled.history);
        setDraft(recalled.draft);
        getInputApi()?.applyValue(recalled.draft, recalled.draft.length);
      }
      return;
    }
    if (event.key === "ArrowDown" && history.index !== null) {
      const recalled = recallNextSessionChatDraft(history);
      if (recalled) {
        event.preventDefault();
        setHistory(recalled.history);
        setDraft(recalled.draft);
        getInputApi()?.applyValue(recalled.draft, recalled.draft.length);
      }
    }
  };

  const sendDisabled = isWorking ? false : disabled || draft.trim() === "";

  return (
    // min-w-0 all the way down to the input: this sits in a grid/flex column,
    // whose items are min-width:auto by default, so an unbreakable pasted run
    // would otherwise widen the composer past the pane and scroll the page.
    <div className="relative min-w-0">
      {slashOpen ? (
        <div className="absolute inset-x-0 bottom-full z-10 mb-2 overflow-hidden rounded-2xl border border-input bg-popover shadow-xl">
          <div
            className="max-h-72 overflow-y-auto p-1.5"
            ref={slashListRef}
            role="listbox"
            aria-label="Slash commands"
          >
            <div className="px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              {slashHeading ?? "Commands"}
            </div>
            {slashMatches.map((command, index) => (
              <button
                aria-selected={index === highlightedIndex}
                className={cn(
                  "flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm",
                  index === highlightedIndex
                    ? "bg-accent text-accent-foreground"
                    : "text-foreground",
                )}
                data-highlighted={index === highlightedIndex ? "true" : undefined}
                key={command.name}
                onMouseDown={(event) => {
                  // Keep textarea focus; complete on the same gesture.
                  event.preventDefault();
                  completeSlashCommand(command);
                }}
                onMouseMove={() => {
                  if (index !== highlightedIndex) {
                    setSlashIndex(index);
                  }
                }}
                role="option"
                type="button"
              >
                <IconRobot
                  aria-hidden="true"
                  className="size-4 shrink-0 text-muted-foreground"
                  stroke={1.6}
                />
                <span className="shrink-0 font-semibold">/{command.name}</span>
                <span className="truncate text-muted-foreground">
                  {command.description}
                </span>
              </button>
            ))}
          </div>
        </div>
      ) : null}
      <div
        className={cn(
          "ghostex-chat-composer min-w-0 rounded-3xl border border-input bg-card px-4 py-2.5 transition-colors focus-within:border-ring focus-within:ring-[3px] focus-within:ring-ring/20",
          disabled && "opacity-60",
        )}
        data-disabled={disabled ? "true" : undefined}
      >
        {pastedImages.length > 0 || pendingImagePastes > 0 ? (
          <div className="flex flex-wrap items-center gap-2 pb-2">
            {pastedImages.map((image) => (
              <div className="relative" key={image.id}>
                <button
                  aria-label="View pasted image"
                  className="block cursor-zoom-in rounded-lg"
                  disabled={!imageViewer}
                  onClick={() =>
                    imageViewer?.open({
                      alt: "Pasted image",
                      url: image.dataUrl,
                    })
                  }
                  type="button"
                >
                  <img
                    alt="Pasted image"
                    className="h-12 w-12 rounded-lg border border-input object-cover"
                    src={image.dataUrl}
                  />
                </button>
                <button
                  aria-label="Remove image"
                  className="absolute -right-1.5 -top-1.5 flex size-4 items-center justify-center rounded-full border border-input bg-card text-muted-foreground hover:text-foreground"
                  onClick={() => removePastedImage(image)}
                  type="button"
                >
                  <IconX aria-hidden="true" size={10} stroke={2.4} />
                </button>
              </div>
            ))}
            {pendingImagePastes > 0 ? (
              <div
                aria-label="Saving attachment"
                className="flex h-12 w-12 items-center justify-center rounded-lg border border-dashed border-input text-muted-foreground"
              >
                <IconLoader2
                  aria-hidden="true"
                  className="animate-spin"
                  size={16}
                  stroke={2}
                />
              </div>
            ) : null}
          </div>
        ) : null}
        <div className="flex min-w-0 items-end gap-2 pb-1.5">
        {useMonaco ? (
          <SessionChatMonacoInput
            disabled={disabled}
            initialValue={draft}
            onChange={updateDraft}
            onKeyDown={handleKeyDown}
            onLoadFailed={(error) => {
              console.error(
                "[session-chat] Monaco failed to load; using the plain input.",
                error,
              );
              setMonacoFailed(true);
            }}
            onPasteData={processClipboardData}
            placeholder={placeholder ?? "Send a message…"}
            registerApi={(api) => {
              monacoApiRef.current = api;
              if (api && pendingInsertTextRef.current) {
                const pending = pendingInsertTextRef.current;
                pendingInsertTextRef.current = "";
                api.insertText(pending);
              }
              if (api && pendingFocusRef.current) {
                pendingFocusRef.current = false;
                api.focus();
              }
            }}
            theme={theme}
            vsBaseUrl={monacoVsBaseUrl ?? ""}
          />
        ) : (
          <textarea
            className="ghostex-chat-composer-input max-h-40 min-h-6 min-w-0 flex-1 resize-none overflow-y-auto bg-transparent text-sm leading-6 text-foreground outline-none [field-sizing:content] placeholder:text-muted-foreground"
            disabled={disabled}
            onChange={(event) => {
              updateDraft(event.target.value);
            }}
            onKeyDown={(event) => {
              const adapted = reactKeyEventAdapter(event);
              if (adapted.isComposing) {
                if (adapted.key === "Enter") {
                  event.preventDefault();
                }
                return;
              }
              handleKeyDown(adapted);
            }}
            onPaste={(event) => {
              if (processClipboardData(event.clipboardData)) {
                event.preventDefault();
              }
            }}
            placeholder={placeholder ?? "Send a message…"}
            ref={textareaRef}
            rows={1}
            value={draft}
          />
        )}
        </div>
        <div className="flex w-full items-center justify-between gap-2">
          <div className="flex min-w-0 items-center gap-0.5">
            {optionPills}
          </div>
          <div className="ml-auto flex items-center gap-1.5">
            {onPasteImage || onAttachFile || onPickPaths ? (
              <>
                {onPickPaths ? null : (
                  <input
                    className="hidden"
                    multiple
                    onChange={(event) => {
                      const files = Array.from(event.target.files ?? []);
                      // Same input element every time: clear it so re-picking
                      // the same file still fires change.
                      event.target.value = "";
                      const images = files.filter(
                        (file) => isImageFile(file) && onPasteImage !== undefined,
                      );
                      const others = files.filter(
                        (file) => !images.includes(file) && onAttachFile !== undefined,
                      );
                      if (images.length > 0) {
                        consumeImageFiles(images);
                      }
                      if (others.length > 0) {
                        consumeAttachmentFiles(others);
                      }
                    }}
                    ref={fileInputRef}
                    tabIndex={-1}
                    type="file"
                    {...(onAttachFile ? {} : { accept: "image/*" })}
                  />
                )}
                <AppTooltip content="Attach an Image, File, or Folder">
                  <span className="inline-flex">
                    <Button
                      aria-label="Attach an Image, File, or Folder"
                      className="ghostex-chat-footer-control rounded-full"
                      disabled={disabled}
                      onClick={() => {
                        if (onPickPaths) {
                          attachFromNativePicker();
                        } else {
                          fileInputRef.current?.click();
                        }
                      }}
                      size="icon-sm"
                      variant="ghost"
                    >
                      <IconPaperclip aria-hidden="true" stroke={2} />
                    </Button>
                  </span>
                </AppTooltip>
              </>
            ) : null}
            {isWorking ? (
              <Button
                aria-label="Stop the agent"
                className="size-8 rounded-full"
                onClick={() => {
                  onInterrupt();
                }}
                size="icon"
                variant="secondary"
              >
                <IconPlayerStopFilled
                  aria-hidden="true"
                  className="size-3.5"
                  stroke={1.6}
                />
              </Button>
            ) : (
              <Button
                aria-label="Send"
                className="ghostex-chat-send-button size-8 rounded-full"
                disabled={sendDisabled}
                onClick={() => send()}
                size="icon"
              >
                <IconArrowUp aria-hidden="true" className="size-4" stroke={2.2} />
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
});
