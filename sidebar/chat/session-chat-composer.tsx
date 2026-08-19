// Session chat composer (upstream chat spec §1.1/§11.6 port). Enter sends by
// default, hosts can reserve it for newlines, Escape interrupts, the IME guard swallows
// composition Enter, and ArrowUp/Down recall draft history. Typing a
// line-leading "/" opens the slash-command picker (per-agent catalog):
// ArrowUp/Down highlight, Tab/Enter complete, Enter on an exact match sends,
// Escape dismisses the picker without interrupting. A "$" token opens the same
// picker over the session's skills and an "@" token over the project's files;
// both read the token under the caret (see session-chat-composer-trigger.ts),
// so they open wherever in the draft the mention is being typed. Every picker
// row carries `data-chat-picker-option`, which keeps the highlighted row's
// fill out of the dark chat theme's button flattening (sidebar/styles/chat.css)
// — without it the keyboard selection moves invisibly.
//
// Layout (§1.1): input row, then a footer row — session identity/options on
// the left, with Attach, Maximize and Send/Stop on the right. Styled with
// shadcn tokens to sit under the shadcn chat conversation.
//
// Maximize lifts the whole field onto a centered overlay (see
// `.ghostex-chat-composer-maximized` in sidebar/styles/chat.css) so long
// prompts can be edited without scrolling a 160px-tall input. The field keeps
// its place in the React tree while maximized — only its box changes — so the
// monaco instance, caret, undo stack and pending attachments all survive the
// toggle.

import {
  IconArrowUp,
  IconFile,
  IconLoader2,
  IconMaximize,
  IconMinimize,
  IconPaperclip,
  IconPlayerStopFilled,
  IconRobot,
  IconStackPush,
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
import { Field, FieldError } from "../../components/ui/field";
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
import {
  detectSessionChatComposerTrigger,
  filterSessionChatFiles,
  filterSessionChatSkills,
  linkedSessionChatSkillMention,
  sessionChatFileBasename,
  sessionChatFileDirectory,
  sessionChatFileMention,
} from "./session-chat-composer-trigger";
import { SessionChatMonacoInput } from "./session-chat-monaco-input";
import {
  sessionChatImageTargetForHref,
  useSessionChatImageViewer,
} from "./session-chat-image-viewer";
import type { SessionChatSkill, SessionChatTheme } from "../../shared/session-chat";

export interface SessionChatComposerHandle {
  /** Clear the draft only when it still matches the supplied snapshot. */
  clearDraft: (expected: string) => boolean;
  focus: () => void;
  getDraft: () => string;
  /** Insert text at the caret; returns false when the composer cannot take it. */
  insertTypedText: (text: string) => boolean;
  /**
   * Clipboard payload redirected from the chat background: images become
   * attachments, text lands at the caret. Returns false when the composer
   * cannot take anything the clipboard holds.
   */
  pasteClipboard: (data: DataTransfer) => boolean;
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
  /** Whether plain Enter sends instead of inserting a newline. */
  sendOnEnter?: boolean;
  /** Stable conversation identity used to restore this session's unsent draft. */
  sessionKey?: string;
  placeholder?: string;
  /** Agent slash commands offered by the "/" picker; empty disables it. */
  slashCommands?: readonly SessionChatSlashCommand[];
  /** Section heading shown above the picker rows (usually the agent name). */
  slashHeading?: string;
  /** Skills available to this session's agent, resolved on its machine. */
  skills?: readonly SessionChatSkill[];
  /** Section heading shown above the skill mention rows. */
  skillHeading?: string;
  /**
   * Project-relative file paths offered by the "@" picker, listed on the
   * session's machine. Undefined while the host has not answered yet.
   */
  files?: readonly string[];
  /** Section heading shown above the file mention rows. */
  fileHeading?: string;
  /**
   * Asked once the first "@" token is typed so the host can list the project
   * lazily instead of on every chat mount.
   */
  onRequestFiles?: () => void;
  /** True while the host is listing files for the "@" picker. */
  filesLoading?: boolean;
  onSend: (text: string) => void | Promise<void>;
  onInterrupt: () => void;
  /** Save the current draft for later and clear it after the save succeeds. */
  onStash?: () => void;
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

/** Mentions the two composer pickers so they are discoverable without docs. */
const DEFAULT_SESSION_CHAT_PLACEHOLDER =
  "Send a message to the agent. Enter @ to mention a file and $ to use a skill...";

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
    fileHeading,
    files,
    filesLoading = false,
    isWorking,
    monacoVsBaseUrl,
    onAttachFile,
    onInterrupt,
    onLoadImagePreview,
    onPasteImage,
    onPickPaths,
    onRequestFiles,
    onSend,
    onStash,
    optionPills,
    placeholder,
    sendOnEnter = true,
    sessionKey,
    slashCommands,
    slashHeading,
    skills,
    skillHeading,
    theme = "dark",
  },
  ref,
) {
  const [draft, setDraft] = useState(() => readStoredSessionChatDraft(sessionKey));
  const [history, setHistory] = useState(EMPTY_SESSION_CHAT_COMPOSER_HISTORY);
  const [slashDismissed, setSlashDismissed] = useState(false);
  const [slashIndex, setSlashIndex] = useState(0);
  const [skillDismissed, setSkillDismissed] = useState(false);
  const [skillIndex, setSkillIndex] = useState(0);
  const [fileDismissed, setFileDismissed] = useState(false);
  const [fileIndex, setFileIndex] = useState(0);
  /**
   * Caret offset the pickers read. The draft alone cannot say where the caret
   * is, and a mention is only "being typed" when the caret sits at its end.
   */
  const [caret, setCaret] = useState<number | null>(null);
  const [pastedImages, setPastedImages] = useState<readonly PastedImagePreview[]>([]);
  const [pendingImagePastes, setPendingImagePastes] = useState(0);
  const [monacoFailed, setMonacoFailed] = useState(false);
  const [maximized, setMaximized] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const imageViewer = useSessionChatImageViewer();
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const slashListRef = useRef<HTMLDivElement | null>(null);
  const skillListRef = useRef<HTMLDivElement | null>(null);
  const fileListRef = useRef<HTMLDivElement | null>(null);
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
  const trigger = detectSessionChatComposerTrigger(draft, caret ?? draft.length);
  const skillQuery = trigger?.kind === "skill" ? trigger.query : null;
  const skillMatches = useMemo(
    () =>
      skillQuery !== null && !skillDismissed
        ? filterSessionChatSkills(skills ?? [], skillQuery)
        : [],
    [skillDismissed, skillQuery, skills],
  );
  const skillOpen = skillMatches.length > 0 && !disabled && !slashOpen;
  const highlightedSkillIndex = Math.min(
    skillIndex,
    Math.max(skillMatches.length - 1, 0),
  );
  const fileQuery = trigger?.kind === "path" ? trigger.query : null;
  const fileMatches = useMemo(
    () =>
      fileQuery !== null && !fileDismissed
        ? filterSessionChatFiles(files ?? [], fileQuery)
        : [],
    [fileDismissed, fileQuery, files],
  );
  const filePickerActive =
    fileQuery !== null && !fileDismissed && !disabled && !slashOpen;
  // The picker stays up while the host is still listing so "@" never looks
  // dead on the first use of a session, when nothing is cached yet.
  const fileOpen =
    filePickerActive && (fileMatches.length > 0 || (filesLoading && !files));
  const highlightedFileIndex = Math.min(
    fileIndex,
    Math.max(fileMatches.length - 1, 0),
  );

  // Lazy list: the first "@" of a session asks the host for the project files.
  useEffect(() => {
    if (filePickerActive && files === undefined) {
      onRequestFiles?.();
    }
  }, [filePickerActive, files, onRequestFiles]);

  useEffect(() => {
    if (!slashOpen) {
      return;
    }
    slashListRef.current
      ?.querySelector('[data-highlighted="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [highlightedIndex, slashOpen]);

  useEffect(() => {
    if (!skillOpen) {
      return;
    }
    skillListRef.current
      ?.querySelector('[data-highlighted="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [highlightedSkillIndex, skillOpen]);

  useEffect(() => {
    if (!fileOpen) {
      return;
    }
    fileListRef.current
      ?.querySelector('[data-highlighted="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [highlightedFileIndex, fileOpen]);

  const updateDraft = (next: string, nextCaret?: number): void => {
    const caretOffset = nextCaret ?? next.length;
    writeStoredSessionChatDraft(sessionKey, next);
    setDraft(next);
    setCaret(caretOffset);
    setSendError(null);
    setHistory((current) => resetSessionChatComposerHistoryIndex(current));
    if (sessionChatSlashQuery(next) === null) {
      setSlashDismissed(false);
    }
    // Leaving a token re-arms its picker, so a dismissed mention does not stay
    // dismissed for the next one typed in the same draft.
    const nextTrigger = detectSessionChatComposerTrigger(next, caretOffset);
    if (nextTrigger?.kind !== "skill") {
      setSkillDismissed(false);
    }
    if (nextTrigger?.kind !== "path") {
      setFileDismissed(false);
    }
    setSlashIndex(0);
    setSkillIndex(0);
    setFileIndex(0);
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
      updateDraft(next, start + text.length);
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
      setCaret(0);
      setHistory((value) => resetSessionChatComposerHistoryIndex(value));
      getInputApi()?.applyValue("", 0);
      setSlashDismissed(false);
      setSlashIndex(0);
      setSkillDismissed(false);
      setSkillIndex(0);
      setFileDismissed(false);
      setFileIndex(0);
      setSendError(null);
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
    pasteClipboard: (data: DataTransfer): boolean => {
      if (disabled) {
        return false;
      }
      if (processClipboardData(data)) {
        // Images were consumed as attachments; put the caret back in the
        // input so the redirected paste ends with a ready composer.
        getInputApi()?.focus();
        return true;
      }
      const text = data.getData("text/plain");
      if (text === "") {
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
    setSendError(null);

    // The optimistic transcript echo is created synchronously by onSend.
    // Vacate the composer first so the submit gesture feels immediate and
    // any typing that follows belongs to the next draft.
    writeStoredSessionChatDraft(sessionKey, "");
    draftRef.current = "";
    setDraft("");
    setHistory((value) => resetSessionChatComposerHistoryIndex(value));
    getInputApi()?.applyValue("", 0);
    setCaret(0);
    setSlashDismissed(false);
    setSlashIndex(0);
    setSkillDismissed(false);
    setSkillIndex(0);
    setFileDismissed(false);
    setFileIndex(0);

    const sendRequest = (() => {
      try {
        return Promise.resolve(onSend(text));
      } catch (error) {
        return Promise.reject(error);
      }
    })();
    void sendRequest
      .then(() => {
        setHistory((value) => pushSessionChatComposerHistory(value, text));
      })
      .catch(() => {
        // Do not overwrite a next draft typed while the send was in flight.
        // Put the failed message first so retrying still preserves send order.
        const current = getInputApi()?.getValue() ?? draftRef.current;
        const restored =
          current === "" || current === text ? text : `${text}\n${current}`;
        writeStoredSessionChatDraft(sessionKey, restored);
        draftRef.current = restored;
        setDraft(restored);
        setCaret(restored.length);
        setHistory((value) => resetSessionChatComposerHistoryIndex(value));
        getInputApi()?.applyValue(restored, restored.length);
        getInputApi()?.focus();
        setSendError("Message could not be sent. Your draft was restored.");
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
    updateDraft(next, start + inserted.length);
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
    updateDraft(next, matchIndex);
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
    updateDraft(next, next.length);
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
      setSlashIndex((current) => {
        const currentIndex = Math.min(current, slashMatches.length - 1);
        return (currentIndex + delta + slashMatches.length) % slashMatches.length;
      });
      return true;
    }
    if (event.key === "Tab") {
      event.preventDefault();
      completeSlashCommand(highlighted);
      return true;
    }
    if (sendOnEnter && event.key === "Enter" && !event.shiftKey) {
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

  /** Replaces the token under the caret and leaves the caret just past it. */
  const completeMention = (replacement: string): void => {
    if (!trigger) {
      return;
    }
    const next = `${draft.slice(0, trigger.start)}${replacement}${draft.slice(trigger.end)}`;
    const nextCaret = trigger.start + replacement.length;
    updateDraft(next, nextCaret);
    const api = getInputApi();
    api?.focus();
    api?.applyValue(next, nextCaret);
  };

  const completeSkillMention = (skill: SessionChatSkill): void => {
    completeMention(`${linkedSessionChatSkillMention(skill)} `);
  };

  const completeFileMention = (path: string): void => {
    completeMention(`${sessionChatFileMention(path)} `);
  };

  const handleSkillKeyDown = (event: SessionChatComposerKeyEvent): boolean => {
    if (!skillOpen) {
      return false;
    }
    const highlighted = skillMatches[highlightedSkillIndex];
    if (event.key === "Escape") {
      event.preventDefault();
      setSkillDismissed(true);
      return true;
    }
    if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      const delta = event.key === "ArrowUp" ? -1 : 1;
      setSkillIndex((current) => {
        const currentIndex = Math.min(current, skillMatches.length - 1);
        return (currentIndex + delta + skillMatches.length) % skillMatches.length;
      });
      return true;
    }
    if (
      event.key === "Tab" ||
      (sendOnEnter && event.key === "Enter" && !event.shiftKey)
    ) {
      event.preventDefault();
      completeSkillMention(highlighted);
      return true;
    }
    return false;
  };

  const handleFileKeyDown = (event: SessionChatComposerKeyEvent): boolean => {
    if (!fileOpen) {
      return false;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      setFileDismissed(true);
      return true;
    }
    if (fileMatches.length === 0) {
      // Still listing: swallow nothing but Escape so typing and sending work.
      return false;
    }
    const highlighted = fileMatches[highlightedFileIndex];
    if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      const delta = event.key === "ArrowUp" ? -1 : 1;
      setFileIndex((current) => {
        const currentIndex = Math.min(current, fileMatches.length - 1);
        return (currentIndex + delta + fileMatches.length) % fileMatches.length;
      });
      return true;
    }
    if (
      event.key === "Tab" ||
      (sendOnEnter && event.key === "Enter" && !event.shiftKey)
    ) {
      event.preventDefault();
      if (highlighted !== undefined) {
        completeFileMention(highlighted);
      }
      return true;
    }
    return false;
  };

  const setMaximizedAndFocus = (next: boolean): void => {
    setMaximized(next);
    // The field never leaves the React tree, so the live input element is the
    // same node before and after the toggle and can be refocused right away.
    getInputApi()?.focus();
  };

  const handleKeyDown = (event: SessionChatComposerKeyEvent): void => {
    // IME guard: composition Enter confirms the composition; letting it fall
    // through would submit a partial draft. (The textarea wrapper additionally
    // preventDefaults composition Enter; Monaco manages its own IME.)
    if (event.isComposing) {
      return;
    }
    if (
      handleSkillKeyDown(event) ||
      handleFileKeyDown(event) ||
      handleSlashKeyDown(event)
    ) {
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      // Maximize is an overlay, so Escape closes it first; the next Escape
      // interrupts the agent as usual.
      if (maximized) {
        setMaximizedAndFocus(false);
        return;
      }
      onInterrupt();
      return;
    }
    if (sendOnEnter && event.key === "Enter" && !event.shiftKey) {
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
        setCaret(recalled.draft.length);
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
        setCaret(recalled.draft.length);
        getInputApi()?.applyValue(recalled.draft, recalled.draft.length);
      }
    }
  };

  const sendDisabled = isWorking ? false : disabled || draft.trim() === "";

  return (
    <>
      {maximized ? (
        <div
          aria-hidden="true"
          className="ghostex-chat-composer-backdrop"
          onClick={() => {
            setMaximizedAndFocus(false);
          }}
        />
      ) : null}
      {/* min-w-0 all the way down to the input: this sits in a grid/flex column,
          whose items are min-width:auto by default, so an unbreakable pasted run
          would otherwise widen the composer past the pane and scroll the page. */}
      <Field
        className={cn(
          "relative min-w-0 gap-2",
          maximized && "ghostex-chat-composer-maximized",
        )}
        data-invalid={sendError !== null ? true : undefined}
      >
        {slashOpen ? (
          <div className="ghostex-chat-composer-picker absolute inset-x-0 bottom-full z-10 mb-2 overflow-hidden rounded-2xl border border-input bg-popover shadow-xl">
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
                  data-chat-picker-option="true"
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
        {skillOpen ? (
          <div className="ghostex-chat-composer-picker absolute inset-x-0 bottom-full z-10 mb-2 overflow-hidden rounded-2xl border border-input bg-popover shadow-xl">
            <div
              aria-label="Available skills"
              className="max-h-72 overflow-y-auto p-1.5"
              ref={skillListRef}
              role="listbox"
            >
              <div className="px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                {skillHeading ?? "Skills"}
              </div>
              {skillMatches.map((skill, index) => (
                <button
                  aria-selected={index === highlightedSkillIndex}
                  className={cn(
                    "flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm",
                    index === highlightedSkillIndex
                      ? "bg-accent text-accent-foreground"
                      : "text-foreground",
                  )}
                  data-chat-picker-option="true"
                  data-highlighted={
                    index === highlightedSkillIndex ? "true" : undefined
                  }
                  key={`${skill.name}:${skill.directoryPath}`}
                  onMouseDown={(event) => {
                    event.preventDefault();
                    completeSkillMention(skill);
                  }}
                  onMouseMove={() => {
                    if (index !== highlightedSkillIndex) {
                      setSkillIndex(index);
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
                  <span className="shrink-0 font-semibold">${skill.name}</span>
                  <span className="truncate text-muted-foreground">
                    {skill.directoryPath}
                  </span>
                </button>
              ))}
            </div>
          </div>
        ) : null}
        {fileOpen ? (
          <div className="ghostex-chat-composer-picker absolute inset-x-0 bottom-full z-10 mb-2 overflow-hidden rounded-2xl border border-input bg-popover shadow-xl">
            <div
              aria-label="Project files"
              className="max-h-72 overflow-y-auto p-1.5"
              ref={fileListRef}
              role="listbox"
            >
              <div className="px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                {fileHeading ?? "Files"}
              </div>
              {fileMatches.length === 0 ? (
                <div className="flex items-center gap-2.5 px-3 py-2 text-sm text-muted-foreground">
                  <IconLoader2
                    aria-hidden="true"
                    className="size-4 shrink-0 animate-spin"
                    stroke={2}
                  />
                  Listing project files…
                </div>
              ) : null}
              {fileMatches.map((path, index) => (
                <button
                  aria-selected={index === highlightedFileIndex}
                  className={cn(
                    "flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm",
                    index === highlightedFileIndex
                      ? "bg-accent text-accent-foreground"
                      : "text-foreground",
                  )}
                  data-chat-picker-option="true"
                  data-highlighted={
                    index === highlightedFileIndex ? "true" : undefined
                  }
                  key={path}
                  onMouseDown={(event) => {
                    event.preventDefault();
                    completeFileMention(path);
                  }}
                  onMouseMove={() => {
                    if (index !== highlightedFileIndex) {
                      setFileIndex(index);
                    }
                  }}
                  role="option"
                  type="button"
                >
                  <IconFile
                    aria-hidden="true"
                    className="size-4 shrink-0 text-muted-foreground"
                    stroke={1.6}
                  />
                  <span className="shrink-0 font-semibold">
                    {sessionChatFileBasename(path)}
                  </span>
                  <span className="truncate text-muted-foreground">
                    {sessionChatFileDirectory(path)}
                  </span>
                </button>
              ))}
            </div>
          </div>
        ) : null}
        {sendError ? <FieldError className="px-2">{sendError}</FieldError> : null}
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
          <div className="ghostex-chat-composer-row flex min-w-0 items-end gap-2 pb-1.5">
          {useMonaco ? (
            <SessionChatMonacoInput
              disabled={disabled}
              fillHeight={maximized}
              initialValue={draft}
              onCaretChange={setCaret}
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
              placeholder={placeholder ?? DEFAULT_SESSION_CHAT_PLACEHOLDER}
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
              aria-invalid={sendError !== null}
              onChange={(event) => {
                updateDraft(
                  event.target.value,
                  event.target.selectionStart ?? event.target.value.length,
                );
              }}
              onSelect={(event) => {
                // Caret moves (click, arrows, Home/End) decide which token the
                // pickers read, so they have to reach state too.
                setCaret(event.currentTarget.selectionStart ?? null);
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
              placeholder={placeholder ?? DEFAULT_SESSION_CHAT_PLACEHOLDER}
              ref={textareaRef}
              rows={1}
              value={draft}
            />
          )}
          </div>
          <div className="ghostex-chat-composer-footer flex w-full items-center justify-between gap-2">
            <div className="ghostex-chat-composer-footer-options flex min-w-0 items-center gap-0.5">
              {optionPills}
            </div>
            <div className="ghostex-chat-composer-footer-actions ml-auto flex items-center gap-1.5">
              {onStash ? (
                <AppTooltip content="Stash prompt">
                  <span className="ghostex-chat-stash-control inline-flex">
                    <Button
                      aria-label="Stash prompt"
                      className="ghostex-chat-footer-control rounded-full"
                      disabled={disabled || draft.trim() === ""}
                      onClick={onStash}
                      size="icon-sm"
                      variant="ghost"
                    >
                      <IconStackPush aria-hidden="true" stroke={2} />
                    </Button>
                  </span>
                </AppTooltip>
              ) : null}
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
              <AppTooltip content={maximized ? "Exit maximize" : "Maximize"}>
                <span className="inline-flex">
                  <Button
                    aria-label={maximized ? "Exit maximize" : "Maximize"}
                    aria-pressed={maximized}
                    className="ghostex-chat-footer-control rounded-full"
                    onClick={() => {
                      setMaximizedAndFocus(!maximized);
                    }}
                    size="icon-sm"
                    variant="ghost"
                  >
                    {maximized ? (
                      <IconMinimize aria-hidden="true" stroke={2} />
                    ) : (
                      <IconMaximize aria-hidden="true" stroke={2} />
                    )}
                  </Button>
                </span>
              </AppTooltip>
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
      </Field>
    </>
  );
});
