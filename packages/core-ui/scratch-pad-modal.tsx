import { createPortal } from "react-dom";
import { useEffect, useRef, useState, type ClipboardEvent as ReactClipboardEvent } from "react";
import { trimPromptEditorTrailingSpaces } from "../shared/prompt-editor-text";
import { useSidebarStore } from "./sidebar-store";

export type ScratchPadModalProps = {
  isOpen: boolean;
  onClose: () => void;
  onDebug?: (event: string, details?: Record<string, unknown>) => void;
  onSave: (content: string) => void;
};

export function ScratchPadModal({ isOpen, onClose, onDebug, onSave }: ScratchPadModalProps) {
  const content = useSidebarStore((state) => state.scratchPadContent);
  const [draftContent, setDraftContent] = useState(content);
  const lastSavedContentRef = useRef(content);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const wasOpenRef = useRef(false);

  /**
   * CDXC:ScratchPadFocus 2026-04-28-05:21
   * Scratch Pad typing failures must be traceable without recording note text.
   * Log modal, textarea, and document focus boundaries with content length and
   * selection metadata so terminal-first-responder steals can be correlated with
   * native-terminal-focus-debug.log after a user repro.
   */
  const postDebug = (event: string, details?: Record<string, unknown>) => {
    onDebug?.(`scratchPadFocus.${event}`, {
      activeElement: describeScratchPadElement(document.activeElement),
      documentHasFocus: document.hasFocus(),
      hidden: document.hidden,
      isOpen,
      visibilityState: document.visibilityState,
      ...details,
    });
  };

  /**
   * CDXC:ScratchPad 2026-05-28-07:47:
   * Scratch Pad is freeform text, but saved and pasted note content should not
   * preserve invisible spaces at line ends. Normalize at paste/save boundaries
   * instead of changing every keystroke while the user is composing text.
   */
  const flushDraft = () => {
    const normalizedContent = trimPromptEditorTrailingSpaces(draftContent);
    if (normalizedContent !== draftContent) {
      setDraftContent(normalizedContent);
    }
    if (normalizedContent === lastSavedContentRef.current) {
      postDebug("flushDraft.skipped", {
        reason: "unchanged",
        valueLength: normalizedContent.length,
      });
      return;
    }

    lastSavedContentRef.current = normalizedContent;
    postDebug("flushDraft.saved", {
      valueLength: normalizedContent.length,
    });
    onSave(normalizedContent);
  };

  const closeModal = () => {
    flushDraft();
    onClose();
  };

  useEffect(() => {
    if (!isOpen) {
      if (wasOpenRef.current) {
        postDebug("modal.closed", {
          contentLength: content.length,
          draftLength: draftContent.length,
        });
      }
      setDraftContent(content);
      lastSavedContentRef.current = content;
      wasOpenRef.current = false;
      return;
    }

    if (!wasOpenRef.current) {
      setDraftContent(content);
      lastSavedContentRef.current = content;
      wasOpenRef.current = true;
      postDebug("modal.opened", {
        contentLength: content.length,
      });
    }
  }, [content, isOpen]);

  useEffect(() => {
    const normalizedContent = trimPromptEditorTrailingSpaces(draftContent);
    if (!isOpen || normalizedContent === lastSavedContentRef.current) {
      return;
    }

    const timeoutId = window.setTimeout(() => {
      lastSavedContentRef.current = normalizedContent;
      onSave(normalizedContent);
    }, 180);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [draftContent, isOpen, onSave]);

  const insertDraftText = (text: string) => {
    const textarea = textareaRef.current;
    if (!textarea) {
      setDraftContent((current) => `${current}${text}`);
      return;
    }
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    const nextContent = `${draftContent.slice(0, start)}${text}${draftContent.slice(end)}`;
    setDraftContent(nextContent);
    window.requestAnimationFrame(() => {
      textarea.focus();
      const nextSelection = start + text.length;
      textarea.setSelectionRange(nextSelection, nextSelection);
    });
  };

  const handlePaste = (event: ReactClipboardEvent<HTMLTextAreaElement>) => {
    const pastedText = event.clipboardData.getData("text/plain");
    const trimmedText = trimPromptEditorTrailingSpaces(pastedText);
    if (!pastedText || trimmedText === pastedText) {
      return;
    }
    event.preventDefault();
    insertDraftText(trimmedText);
  };

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      postDebug("document.keydown", {
        code: event.code,
        key: summarizeScratchPadKey(event),
        target: describeScratchPadElement(event.target),
      });

      if (event.key === "Escape") {
        closeModal();
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [draftContent, isOpen, onClose, onSave]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const timeoutId = window.setTimeout(() => {
      const textarea = textareaRef.current;
      if (!textarea) {
        postDebug("textarea.initialFocusSkipped", {
          reason: "missingTextarea",
        });
        return;
      }

      postDebug("textarea.initialFocus.requested", {
        textareaMatchesActiveElement: document.activeElement === textarea,
        valueLength: textarea.value.length,
      });
      textarea.focus();
      const selectionIndex = textarea.value.length;
      textarea.setSelectionRange(selectionIndex, selectionIndex);
      postDebug("textarea.initialFocus.completed", {
        selectionEnd: textarea.selectionEnd,
        selectionStart: textarea.selectionStart,
        textareaMatchesActiveElement: document.activeElement === textarea,
        valueLength: textarea.value.length,
      });
    }, 0);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const handleWindowFocus = () => {
      postDebug("window.focus");
    };
    const handleWindowBlur = () => {
      postDebug("window.blur");
    };
    const handleVisibilityChange = () => {
      postDebug("document.visibilityChange");
    };
    const handleFocusIn = (event: FocusEvent) => {
      postDebug("document.focusin", {
        target: describeScratchPadElement(event.target),
      });
    };
    const handleFocusOut = (event: FocusEvent) => {
      postDebug("document.focusout", {
        relatedTarget: describeScratchPadElement(event.relatedTarget),
        target: describeScratchPadElement(event.target),
      });
    };

    window.addEventListener("focus", handleWindowFocus);
    window.addEventListener("blur", handleWindowBlur);
    document.addEventListener("visibilitychange", handleVisibilityChange);
    document.addEventListener("focusin", handleFocusIn);
    document.addEventListener("focusout", handleFocusOut);
    return () => {
      window.removeEventListener("focus", handleWindowFocus);
      window.removeEventListener("blur", handleWindowBlur);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
      document.removeEventListener("focusin", handleFocusIn);
      document.removeEventListener("focusout", handleFocusOut);
    };
  }, [isOpen, onDebug]);

  if (!isOpen) {
    return null;
  }

  return createPortal(
    <div className="confirm-modal-root scroll-mask-y" role="presentation">
      <button className="confirm-modal-backdrop" onClick={closeModal} type="button" />
      <div
        aria-labelledby="scratch-pad-modal-title"
        aria-modal="true"
        className="confirm-modal scratch-pad-modal scroll-mask-y"
        role="dialog"
      >
        <div className="confirm-modal-header">
          <div className="confirm-modal-title" id="scratch-pad-modal-title">
            Scratch Pad
          </div>
        </div>
        <div className="scratch-pad-modal-body">
          <textarea
            aria-label="Scratch pad"
            className="scratch-pad-textarea"
            onBlur={(event) => {
              postDebug("textarea.blur", {
                relatedTarget: describeScratchPadElement(event.relatedTarget),
                selectionEnd: event.currentTarget.selectionEnd,
                selectionStart: event.currentTarget.selectionStart,
                valueLength: event.currentTarget.value.length,
              });
              flushDraft();
            }}
            onChange={(event) => {
              postDebug("textarea.change", {
                selectionEnd: event.currentTarget.selectionEnd,
                selectionStart: event.currentTarget.selectionStart,
                valueLength: event.currentTarget.value.length,
              });
              setDraftContent(event.target.value);
            }}
            onFocus={(event) => {
              postDebug("textarea.focus", {
                selectionEnd: event.currentTarget.selectionEnd,
                selectionStart: event.currentTarget.selectionStart,
                valueLength: event.currentTarget.value.length,
              });
            }}
            onInput={(event) => {
              postDebug("textarea.input", {
                selectionEnd: event.currentTarget.selectionEnd,
                selectionStart: event.currentTarget.selectionStart,
                valueLength: event.currentTarget.value.length,
              });
            }}
            onPaste={handlePaste}
            placeholder="Workspace notes that autosave as you type."
            ref={textareaRef}
            spellCheck={false}
            value={draftContent}
          />
        </div>
      </div>
    </div>,
    document.body,
  );
}

function describeScratchPadElement(value: EventTarget | Element | null): string {
  if (!(value instanceof Element)) {
    return value === null ? "null" : "non-element";
  }

  const tagName = value.tagName.toLowerCase();
  const role = value.getAttribute("role");
  const ariaLabel = value.getAttribute("aria-label");
  const className =
    typeof value.className === "string"
      ? value.className
          .split(/\s+/)
          .filter(Boolean)
          .slice(0, 3)
          .join(".")
      : "";
  return [tagName, role ? `role=${role}` : "", ariaLabel ? `aria=${ariaLabel}` : "", className]
    .filter(Boolean)
    .join(" ");
}

function summarizeScratchPadKey(event: KeyboardEvent): string {
  if (event.key.length === 1) {
    return "printable";
  }
  return event.key;
}
