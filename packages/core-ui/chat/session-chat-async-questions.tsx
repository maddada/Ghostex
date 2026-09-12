import { IconChevronDown, IconChevronLeft, IconChevronRight } from '@tabler/icons-react';
import { useId, useMemo, useRef, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import type { SessionChatMessage } from '@/packages/shared/session-chat';
import { SessionChatChoiceRows } from './session-chat-choice-rows';
import { SessionQuestionIndicator } from '../session-question-indicator';
import { pendingSessionChatAsyncQuestions, sessionChatAsyncAnswerPrefix } from './session-chat-async-questions-state';
import './session-chat-async-questions.css';

interface AnswerDraft {
  selected: number;
  custom: string;
}

/**
 * CDXC:SessionChat 2026-09-12 DECISION:
 * User: Codex questions asked while it is still working appear in a new component above the chat composer.
 * The composer keeps its draft and remains usable; choosing a suggested answer requires an explicit send.
 */
export function SessionChatAsyncQuestions({
  messages,
  canSend,
  working,
  onSend,
  onDismiss,
  sessionKey,
}: {
  messages: readonly SessionChatMessage[];
  canSend: boolean;
  working: boolean;
  onSend: (text: string) => Promise<void>;
  onDismiss: (questionId: string) => Promise<void>;
  sessionKey?: string;
}) {
  const storageKey = sessionKey ? `ghostex:async-questions:${sessionKey}` : null;
  const [retired, setRetired] = useState<string[]>(() => {
    if (!storageKey) return [];
    try {
      const value: unknown = JSON.parse(localStorage.getItem(storageKey) ?? '[]');
      return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
    } catch {
      return [];
    }
  });
  const pending = useMemo(
    () => pendingSessionChatAsyncQuestions(messages).filter((question) => !retired.includes(question.key)),
    [messages, retired]
  );
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Record<string, AnswerDraft>>({});
  const [collapsed, setCollapsed] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const submittingRef = useRef(false);
  const panelId = useId();
  const index = Math.max(
    0,
    pending.findIndex((question) => question.key === activeKey)
  );
  const question = pending[index];
  if (!question) return null;
  const draft = drafts[question.key] ?? { selected: 0, custom: '' };
  const answer = draft.custom.trim() || question.options?.[draft.selected] || '';
  const disabled = !canSend || submitting;

  const retire = (key: string): void => {
    setRetired((current) => {
      const next = [...current, key].slice(-1000);
      if (storageKey) {
        try {
          localStorage.setItem(storageKey, JSON.stringify(next));
        } catch {
          /* The mounted view still retains accepted answers. */
        }
      }
      return next;
    });
  };
  const submit = async (): Promise<void> => {
    if (disabled || submittingRef.current || !answer.trim()) return;
    submittingRef.current = true;
    setSubmitting(true);
    setError(null);
    try {
      await onSend(`${sessionChatAsyncAnswerPrefix(question.title)}${answer.trim()}`);
      retire(question.key);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Could not send your answer. Please try again.');
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };
  const skip = async (): Promise<void> => {
    if (disabled || submittingRef.current) return;
    submittingRef.current = true;
    setSubmitting(true);
    setError(null);
    try {
      await onDismiss(question.key);
      retire(question.key);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Could not skip this question. Please try again.');
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };

  return (
    <section
      className='ghostex-chat-async-questions'
      aria-label='Questions from Codex'
      data-chat-async-questions='true'
    >
      <span className='sr-only' role='status'>
        {pending.length} unanswered question{pending.length === 1 ? '' : 's'} from Codex.
        {working ? ' The agent is still working.' : ''}
      </span>
      <button
        className='ghostex-chat-async-questions-header'
        data-slot='async-questions-header'
        type='button'
        aria-expanded={!collapsed}
        aria-controls={panelId}
        onClick={() => setCollapsed((value) => !value)}
      >
        <SessionQuestionIndicator working={working} />
        <span className='font-medium'>Question{pending.length > 1 ? 's' : ''} from Codex</span>
        <span className='min-w-0 flex-1 text-muted-foreground'>{working ? 'Still working' : 'Reply when ready'}</span>
        <span className='text-muted-foreground'>
          {index + 1}/{pending.length}
        </span>
        {collapsed ? <IconChevronRight size={16} /> : <IconChevronDown size={16} />}
      </button>
      {!collapsed ? (
        <div key={question.key} id={panelId} className='ghostex-chat-async-questions-body'>
          <p className='whitespace-pre-wrap' id={`${panelId}-question`}>
            {question.title}
          </p>
          {question.options?.length ? (
            <SessionChatChoiceRows
              options={question.options.map((label) => ({ label }))}
              selected={draft.custom.trim() ? [] : [draft.selected]}
              readOnly={disabled}
              onSelect={(selected) =>
                setDrafts((current) => ({ ...current, [question.key]: { selected, custom: '' } }))
              }
            />
          ) : null}
          <textarea
            className='ghostex-chat-async-questions-answer'
            rows={2}
            aria-label='Your answer'
            aria-describedby={`${panelId}-question`}
            placeholder={question.options?.length ? 'Or write your own answer…' : 'Write your answer…'}
            disabled={submitting}
            value={draft.custom}
            // CDXC:SessionChat 2026-09-12 DECISION: User: Enter sends a question answer; Shift+Enter inserts a newline.
            onKeyDown={(event) => {
              if (event.key !== 'Enter' || event.shiftKey || event.nativeEvent.isComposing || event.keyCode === 229)
                return;
              event.preventDefault();
              event.stopPropagation();
              if (!event.repeat) void submit();
            }}
            onChange={(event) =>
              setDrafts((current) => ({ ...current, [question.key]: { ...draft, custom: event.target.value } }))
            }
          />
          {error ? (
            <p className='text-destructive' role='alert'>
              {error}
            </p>
          ) : null}
          {!canSend ? (
            <p className='text-muted-foreground' role='status'>
              Answers are unavailable while this chat is read-only or disconnected.
            </p>
          ) : null}
          <div className='ghostex-chat-async-questions-actions'>
            {pending.length > 1 ? (
              <>
                <Button
                  aria-label='Previous question'
                  size='icon-sm'
                  variant='ghost'
                  disabled={submitting || index === 0}
                  onClick={() => {
                    setActiveKey(pending[index - 1]!.key);
                    setError(null);
                  }}
                >
                  <IconChevronLeft size={16} />
                </Button>
                <Button
                  aria-label='Next question'
                  size='icon-sm'
                  variant='ghost'
                  disabled={submitting || index === pending.length - 1}
                  onClick={() => {
                    setActiveKey(pending[index + 1]!.key);
                    setError(null);
                  }}
                >
                  <IconChevronRight size={16} />
                </Button>
              </>
            ) : null}
            <Button className='ml-auto' size='sm' variant='ghost' disabled={disabled} onClick={() => void skip()}>
              Skip
            </Button>
            <Button
              size='sm'
              variant='outline'
              data-chat-answer-control=''
              disabled={disabled || !answer.trim()}
              onClick={() => void submit()}
            >
              {submitting ? 'Sending…' : 'Send answer'}
            </Button>
          </div>
        </div>
      ) : null}
    </section>
  );
}
