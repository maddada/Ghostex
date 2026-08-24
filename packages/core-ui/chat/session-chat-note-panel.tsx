/*
CDXC:SessionAgentNotes 2026-08-24:
The chat-side editor for a session's note ("what to do next / when to come back
here"). It sits in flow directly above the composer rather than in a dialog: the
note is written while reading the conversation, so covering the transcript to
type it would defeat the point.

gxserver files the note under the PROVIDER conversation id, so this panel never
names a session — it just reads and writes a body through the transport. There
is no Save button: the note is saved on blur, on Cmd/Ctrl+Enter, when the panel
closes, and on unmount, because a note the user typed and then navigated away
from is exactly the note they most wanted kept.
*/

import { IconCheck, IconCopy, IconEraser, IconX } from '@tabler/icons-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { AppTooltip } from '../app-tooltip';
import { Button } from '../../components/ui/button';

export interface SessionChatNotePanelProps {
  /** Closes the panel; the caller keeps the open/closed state. */
  onClose: () => void;
  readNote: () => Promise<{ agentSessionId?: string; note?: string }>;
  saveNote: (note: string) => Promise<void>;
}

export function SessionChatNotePanel({ onClose, readNote, saveNote }: SessionChatNotePanelProps) {
  const [value, setValue] = useState('');
  const [copied, setCopied] = useState(false);
  const copiedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const valueRef = useRef('');
  /*
  The last body gxserver is known to hold. Four different events flush this
  panel, so without it a single note would be written on blur, again on close
  and again on unmount; comparing against it makes every extra flush a no-op.
  */
  const savedRef = useRef('');
  const editedRef = useRef(false);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const saveNoteRef = useRef(saveNote);
  saveNoteRef.current = saveNote;

  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  useEffect(() => {
    let active = true;
    void readNote()
      .then((result) => {
        const note = result.note ?? '';
        savedRef.current = note.trim();
        // A read that lands after the user started typing must not overwrite
        // what they wrote.
        if (!active || editedRef.current) {
          return;
        }
        valueRef.current = note;
        setValue(note);
      })
      .catch((error: unknown) => {
        console.error('[session-chat] session note read failed', error);
      });
    return () => {
      active = false;
    };
  }, [readNote]);

  const flushNote = useCallback((): void => {
    const previous = savedRef.current;
    const next = valueRef.current.trim();
    if (next === previous) {
      return;
    }
    savedRef.current = next;
    void saveNoteRef.current(next).catch((error: unknown) => {
      // Put the bookkeeping back so the next blur / close retries the write
      // instead of believing a note that never landed is already stored.
      if (savedRef.current === next) {
        savedRef.current = previous;
      }
      console.error('[session-chat] session note save failed', error);
    });
  }, []);

  // Unmount is the last chance to keep what was typed (session switch, the
  // question card taking the composer's place, the pane closing).
  useEffect(() => () => flushNote(), [flushNote]);

  const closePanel = useCallback((): void => {
    flushNote();
    onClose();
  }, [flushNote, onClose]);

  useEffect(
    () => () => {
      if (copiedTimerRef.current !== null) {
        clearTimeout(copiedTimerRef.current);
      }
    },
    []
  );

  const copyNote = useCallback((): void => {
    const body = valueRef.current;
    if (body.trim() === '') {
      return;
    }
    void navigator.clipboard
      .writeText(body)
      .then(() => {
        setCopied(true);
        if (copiedTimerRef.current !== null) {
          clearTimeout(copiedTimerRef.current);
        }
        copiedTimerRef.current = setTimeout(() => setCopied(false), 1200);
      })
      .catch((error: unknown) => {
        console.error('[session-chat] session note copy failed', error);
      });
  }, []);

  /*
  Clearing keeps the panel open and focused: the most common follow-up to
  "clear" is typing the replacement note, and the flush persists the deletion
  immediately so a crash cannot resurrect the old text.
  */
  const clearNote = useCallback((): void => {
    editedRef.current = true;
    valueRef.current = '';
    setValue('');
    flushNote();
    textareaRef.current?.focus();
  }, [flushNote]);

  const handleKeyDown = useCallback(
    (event: KeyboardEvent<HTMLTextAreaElement>): void => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        closePanel();
        return;
      }
      if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        event.stopPropagation();
        flushNote();
      }
    },
    [closePanel, flushNote]
  );

  return (
    <div className='ghostex-chat-session-note-panel'>
      <div className='ghostex-chat-session-note-header'>
        <span className='ghostex-chat-session-note-title'>Session note</span>
        <div className='ghostex-chat-session-note-actions'>
          <AppTooltip content={copied ? 'Copied' : 'Copy note'}>
            <Button
              aria-label='Copy session note'
              className='ghostex-chat-session-note-copy'
              disabled={value.trim() === ''}
              onClick={copyNote}
              size='icon-xs'
              variant='ghost'
            >
              {copied ? (
                <IconCheck aria-hidden='true' className='size-3.5' stroke={2} />
              ) : (
                <IconCopy aria-hidden='true' className='size-3.5' stroke={2} />
              )}
            </Button>
          </AppTooltip>
          <AppTooltip content='Clear note'>
            <Button
              aria-label='Clear session note'
              className='ghostex-chat-session-note-clear'
              disabled={value.trim() === ''}
              onClick={clearNote}
              size='icon-xs'
              variant='ghost'
            >
              <IconEraser aria-hidden='true' className='size-3.5' stroke={2} />
            </Button>
          </AppTooltip>
          <AppTooltip content='Close'>
            <Button
              aria-label='Close session note'
              className='ghostex-chat-session-note-close'
              onClick={closePanel}
              size='icon-xs'
              variant='ghost'
            >
              <IconX aria-hidden='true' className='size-3.5' stroke={2} />
            </Button>
          </AppTooltip>
        </div>
      </div>
      <textarea
        className='ghostex-chat-session-note-input'
        onBlur={flushNote}
        onChange={(event) => {
          editedRef.current = true;
          valueRef.current = event.target.value;
          setValue(event.target.value);
        }}
        onKeyDown={handleKeyDown}
        placeholder='What’s next in this thread…'
        ref={textareaRef}
        rows={3}
        spellCheck={false}
        value={value}
      />
    </div>
  );
}
