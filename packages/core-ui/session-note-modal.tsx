import { useCallback, useEffect, useId, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { Field, FieldDescription, FieldGroup, FieldLabel } from '@/packages/components/ui/field';
import { Textarea } from '@/packages/components/ui/textarea';
import {
  AppModalButton,
  AppModalDescription,
  AppModalFooter,
  AppModalForm,
  AppModalHeader,
  AppModalShell,
  AppModalTitle,
} from './app-modal-shell';

export type SessionNoteModalProps = {
  initialNote: string;
  isOpen: boolean;
  onCancel: () => void;
  /** Receives the trimmed note. An empty string is the explicit CLEAR. */
  onConfirm: (note: string) => void;
  sessionTitle?: string;
};

/**
 * CDXC:SessionNotes 2026-08-24:
 * "What to do next in this thread". The note is filed against the session's
 * provider conversation id, not the Ghostex session, so it survives closing the
 * row and resuming the same agent conversation later — which is why this dialog
 * only ever reports the text and lets the host resolve which conversation it
 * belongs to.
 *
 * It reuses Rename Session's compact modal chrome (`session-rename-*`) the same
 * way Clone Repository does: the native child-window sizing, fit-height, and
 * footer rules are already expressed there, and a second copy of them would
 * drift. Only the note textarea's own resting height is new.
 */
export function SessionNoteModal({ initialNote, isOpen, onCancel, onConfirm, sessionTitle }: SessionNoteModalProps) {
  const [note, setNote] = useState(initialNote);
  const inputId = useId();
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const userInteractedAfterOpenRef = useRef(false);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    userInteractedAfterOpenRef.current = false;
    setNote(initialNote);
  }, [initialNote, isOpen]);

  /**
   * CDXC:SessionNotes 2026-08-24:
   * Same focus contract as Rename Session: own the initial focus through the
   * dialog's `initialFocus` hook (returning false so base-ui does not re-focus
   * afterwards) and keep re-requesting it across the native child window's
   * focus boundary until the user actually interacts. Unlike Rename, the caret
   * goes to the END of the existing note instead of selecting it — a note is
   * appended to far more often than it is replaced.
   */
  const focusInput = useCallback(() => {
    const input = inputRef.current;
    if (input) {
      input.focus({ preventScroll: true });
      input.setSelectionRange(input.value.length, input.value.length);
    }
    return false as const;
  }, []);

  const markUserInteractedAfterOpen = useCallback(() => {
    userInteractedAfterOpenRef.current = true;
  }, []);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const focusUnlessUserInteracted = () => {
      if (userInteractedAfterOpenRef.current) {
        return;
      }
      focusInput();
    };
    const retryDelaysMs = [0, 16, 50, 100, 250, 500, 1000, 1600, 2400];
    const timeoutIds = retryDelaysMs.map((delayMs) => window.setTimeout(focusUnlessUserInteracted, delayMs));
    const animationFrame = window.requestAnimationFrame(focusUnlessUserInteracted);
    const windowFocusTimeoutIds: number[] = [];
    const windowFocusAnimationFrames: number[] = [];
    const handleWindowFocus = () => {
      windowFocusTimeoutIds.push(window.setTimeout(focusUnlessUserInteracted, 0));
      windowFocusAnimationFrames.push(window.requestAnimationFrame(focusUnlessUserInteracted));
    };

    window.addEventListener('focus', handleWindowFocus);
    return () => {
      window.cancelAnimationFrame(animationFrame);
      timeoutIds.forEach((timeoutId) => window.clearTimeout(timeoutId));
      windowFocusTimeoutIds.forEach((timeoutId) => window.clearTimeout(timeoutId));
      windowFocusAnimationFrames.forEach((frameId) => window.cancelAnimationFrame(frameId));
      window.removeEventListener('focus', handleWindowFocus);
    };
  }, [focusInput, initialNote, isOpen]);

  if (!isOpen) {
    return null;
  }

  const trimmedNote = note.trim();
  const hasExistingNote = initialNote.trim().length > 0;
  const submitNote = () => {
    onConfirm(trimmedNote);
  };

  const handleInputKeyDown = (event: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Escape') {
      /*
       * Escape is a CANCEL, not a save: the note is a deliberate write, so a
       * dismissed dialog must leave the stored note exactly as it was.
       */
      event.preventDefault();
      event.stopPropagation();
      onCancel();
      return;
    }
    /*
     * Enter inserts a newline — notes are multi-line by design — so the
     * keyboard submit is the platform's "commit this form" chord instead.
     */
    if (event.key !== 'Enter' || event.nativeEvent.isComposing || !(event.metaKey || event.ctrlKey)) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    submitNote();
  };

  return (
    <AppModalShell
      className='session-rename-modal-shadcn session-note-modal-shadcn'
      initialFocus={focusInput}
      isOpen={isOpen}
      onClose={onCancel}
    >
      <AppModalForm
        className='session-rename-form session-note-form'
        onKeyDownCapture={markUserInteractedAfterOpen}
        onPointerDownCapture={markUserInteractedAfterOpen}
        onSubmit={(event) => {
          event.preventDefault();
          submitNote();
        }}
      >
        <AppModalHeader>
          <AppModalTitle>Session Note</AppModalTitle>
          <AppModalDescription>
            {sessionTitle
              ? `What to do next in “${sessionTitle}”. The note stays with this agent conversation.`
              : 'What to do next here. The note stays with this agent conversation.'}
          </AppModalDescription>
        </AppModalHeader>
        <FieldGroup className='session-rename-field-group'>
          <Field>
            <FieldLabel htmlFor={inputId}>Note</FieldLabel>
            <Textarea
              aria-label='Session Note'
              className='session-rename-textarea session-note-textarea'
              id={inputId}
              onChange={(event) => setNote(event.currentTarget.value)}
              onKeyDown={handleInputKeyDown}
              placeholder='What to pick up when you come back…'
              ref={inputRef}
              value={note}
            />
            <FieldDescription>
              {hasExistingNote ? 'Save an empty note to clear it.' : 'Press ⌘/Ctrl + Enter to save.'}
            </FieldDescription>
          </Field>
        </FieldGroup>
        <AppModalFooter>
          <AppModalButton onClick={onCancel} type='button'>
            Cancel
          </AppModalButton>
          <AppModalButton onClick={submitNote} type='button'>
            {trimmedNote.length === 0 && hasExistingNote ? 'Clear Note' : 'Save'}
          </AppModalButton>
        </AppModalFooter>
      </AppModalForm>
    </AppModalShell>
  );
}
