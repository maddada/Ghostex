import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { SessionChatTheme } from '@/packages/shared/session-chat';
import type { SessionChatComposerInputApi, SessionChatComposerKeyEvent } from './session-chat-composer';
import { SessionChatLexicalInput } from './session-chat-lexical-input';
import { useSessionChatReferenceInteractions } from './use-session-chat-reference-interactions';
import './session-chat-answer-input.css';
import {
  clipboardImageFiles,
  linkedImageReferenceHrefs,
  nextImageReferenceIndex,
  saveSessionChatImageFile,
  type PastedImagePreview,
  type SaveSessionChatImage,
} from './session-chat-image-attachments';
import { SessionChatAttachmentPreviews } from './session-chat-attachment-previews';
import { sessionChatImageTargetForHref, useSessionChatImageViewer } from './session-chat-image-viewer';

/**
 * CDXC:Clipboard 2026-09-15 DECISION:
 * User: pasting images in question answers must insert numbered image references and show thumbnails exactly like the composer.
 * Uploads update their original question's saved draft even if its input unmounts while the image is saving.
 * User: answer references must render as the same editable pills as the composer.
 */
export function SessionChatAnswerInput({
  value,
  onUpdate,
  onPasteImage,
  onPendingChange,
  onKeyDown,
  className,
  disabled,
  placeholder = '',
  theme = 'dark',
  'aria-label': ariaLabel = 'Your answer',
  'aria-describedby': ariaDescribedBy,
}: {
  value: string;
  className?: string;
  disabled?: boolean;
  placeholder?: string;
  theme?: SessionChatTheme;
  'aria-label'?: string;
  'aria-describedby'?: string;
  onKeyDown: (event: SessionChatComposerKeyEvent) => void;
  onUpdate: (update: (text: string) => string) => void;
  onPasteImage?: SaveSessionChatImage;
  onPendingChange: (pending: boolean) => void;
}) {
  const imageViewer = useSessionChatImageViewer();
  const inputRef = useRef<SessionChatComposerInputApi | null>(null);
  const referenceInteractions = useSessionChatReferenceInteractions(value);
  const caretRef = useRef<number | null>(null);
  const [previews, setPreviews] = useState<Record<string, string>>({});
  const [pending, setPending] = useState(0);
  const pendingRef = useRef(0);
  const [error, setError] = useState('');
  const paths = [...new Set(linkedImageReferenceHrefs(value))];
  const images: PastedImagePreview[] = paths.flatMap((path) =>
    previews[path] ? [{ id: path, path, dataUrl: previews[path] }] : []
  );

  useLayoutEffect(() => {
    const caret = caretRef.current;
    caretRef.current = null;
    const input = inputRef.current;
    if (input && input.getValue() !== value) {
      input.applyValue(value, caret ?? input.getSelection().end);
    }
  }, [value]);

  useEffect(() => {
    let disposed = false;
    for (const path of linkedImageReferenceHrefs(value)) {
      if (previews[path]) continue;
      void imageViewer
        ?.resolve(sessionChatImageTargetForHref(path))
        ?.then((dataUrl) => {
          if (!disposed) setPreviews((current) => ({ ...current, [path]: dataUrl }));
        })
        .catch(() => {
          // The saved reference remains usable when its preview cannot be loaded.
        });
    }
    return () => {
      disposed = true;
    };
  }, [imageViewer, value, previews]);

  return (
    <div className='min-w-0 flex-1'>
      <SessionChatAttachmentPreviews
        images={images}
        text={value}
        pending={pending}
        disabled={disabled}
        activeImagePath={referenceInteractions.hoveredImagePath}
        onRemove={(image) => {
          const escaped = image.path.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
          onUpdate((text) => text.replace(new RegExp(`\\s?\\[Image #\\d+·?\\]\\(${escaped}\\) ?`), ''));
        }}
      />
      <div
        className={`ghostex-chat-answer-editor ${className ?? ''}`}
        onClick={referenceInteractions.clickReference}
        onDoubleClick={referenceInteractions.cancelImageOpen}
        onMouseOver={(event) => referenceInteractions.hoverReference(event.target)}
        onMouseLeave={() => referenceInteractions.hoverReference(null)}
      >
        <SessionChatLexicalInput
          initialValue={value}
          registerApi={(api) => {
            inputRef.current = api;
          }}
          onCaretChange={() => {}}
          onChange={(text) => onUpdate(() => text)}
          onKeyDown={onKeyDown}
          placeholder={placeholder}
          fillHeight={false}
          theme={theme}
          ariaLabel={ariaLabel}
          ariaDescribedBy={ariaDescribedBy}
          readOnly={disabled}
          onPasteData={(data) => {
            if (!onPasteImage || disabled) return false;
            const files = clipboardImageFiles(data);
            if (!files.length) return false;
            const save = onPasteImage;
            let original = inputRef.current?.getValue() ?? value;
            let { start, end } = inputRef.current?.getSelection() ?? { start: original.length, end: original.length };
            pendingRef.current += files.length;
            setPending(pendingRef.current);
            onPendingChange(true);
            setError('');
            void (async () => {
              for (const file of files) {
                try {
                  const { path, dataUrl } = await saveSessionChatImageFile(file, save);
                  onUpdate((text) => {
                    const position = text === original ? start : text.length;
                    const finish = text === original ? end : text.length;
                    const reference = `[Image #${nextImageReferenceIndex(text)}](${path})`;
                    const inserted = `${position > 0 && !/\s/.test(text[position - 1] ?? '') ? ' ' : ''}${reference} `;
                    const next = text.slice(0, position) + inserted + text.slice(finish);
                    original = next;
                    start = end = position + inserted.length;
                    caretRef.current = start;
                    return next;
                  });
                  setPreviews((current) => ({ ...current, [path]: dataUrl }));
                } catch (reason) {
                  setError(
                    reason instanceof Error ? reason.message : 'Could not save the image. Please paste it again.'
                  );
                } finally {
                  pendingRef.current -= 1;
                  setPending(pendingRef.current);
                  onPendingChange(pendingRef.current > 0);
                }
              }
            })();
            return true;
          }}
        />
      </div>
      {error ? (
        <p className='text-destructive' role='alert'>
          {error}
        </p>
      ) : null}
    </div>
  );
}
