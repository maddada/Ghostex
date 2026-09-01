// Interactive prompt card (upstream chat spec §2.6 / §8.7 lifecycle, adapted
// to the normalized SessionChatInteractivePrompt wire shape). The live prompt
// status lingers after answering (the agent emits a post-tool event carrying
// the same prompt), so the card hides by CONTENT KEY until a genuinely
// different prompt arrives; the dismissed key resets whenever the prompt
// clears so an identical follow-up shows again.
//
// Two states beyond "answerable":
//   - delivery failed → the card stays with an inline notice pointing at the
//     terminal, because the keystrokes never reached the TUI;
//   - input is held elsewhere (canSend false) → the card renders READ-ONLY
//     instead of vanishing, so the question is still visible with a hint to
//     answer it in the terminal.
//
// Layout: the card takes the composer's place while a prompt is live, so it
// wears the composer's surface (same radius, border, and dark fill) with the
// question panel stacked on top of a composer-shaped answer row. One question
// at a time behind a collapsible header with an "n/total" counter, options as
// full-width rows carrying their 1-9 shortcut key, and a free-text answer in
// the bottom row next to the send button.

import { IconArrowLeft, IconChevronRight, IconTerminal2, IconX } from '@tabler/icons-react';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type {
  GxserverAnswerSessionChatPromptParams,
  SessionChatInteractivePrompt,
  SessionChatQuestionSelection,
} from '../../shared/session-chat';
import { cn } from '@/packages/components/utils';
import { Button } from '../../components/ui/button';
import { SessionChatChoiceRows } from './session-chat-choice-rows';

export function sessionChatCardDismissKey(prompt: SessionChatInteractivePrompt | null): string | null {
  if (!prompt) {
    return null;
  }
  if (prompt.kind === 'question') {
    return `question:${prompt.questions.length}:${prompt.questions[0]?.question ?? ''}`;
  }
  const title = `Allow ${prompt.tool}?`;
  return `approval:${title}:${prompt.summary ?? ''}`;
}

const DELIVERY_FAILED_NOTICE = "Couldn't deliver the answer — switch to Terminal View to answer there.";
const READ_ONLY_NOTICE = 'Switch to Terminal to answer';

export interface SessionChatInteractiveCardProps {
  prompt: SessionChatInteractivePrompt | null;
  canSend: boolean;
  onAnswer: (params: Omit<GxserverAnswerSessionChatPromptParams, 'projectId' | 'sessionId'>) => Promise<void>;
  /** Cancel/close: dismisses the card and interrupts the agent prompt (ESC). */
  onInterrupt: () => void;
  /** The question card replaces the composer while showing. */
  onShowingQuestionChange?: (showing: boolean) => void;
  /**
   * Reports whether the card is on screen at all — question, approval or plan.
   * The parent needs that to keep the new-session welcome, a centered overlay
   * over the same column, from painting through the card.
   */
  onShowingChange?: (showing: boolean) => void;
  /** Host switch-back, offered by the read-only and delivery-failed notices. */
  onSwitchToTerminal?: () => void;
}

interface DraftAnswer {
  indices: number[];
  other: string;
}

/** Composer-shaped surface: the card stands in for the composer while live. */
function CardShell({ children, kind }: { children?: React.ReactNode; kind: string }) {
  return (
    <div
      className='ghostex-chat-question-card min-w-0 overflow-hidden rounded-3xl border border-input bg-card'
      data-kind={kind}
    >
      {children}
    </div>
  );
}

/** The panel half of the card: everything above the answer row. */
function CardPanel({ children }: { children: React.ReactNode }) {
  return <div className='border-b border-border/65 bg-muted/20'>{children}</div>;
}

/**
 * Section label + trailing controls. The label row is the collapse trigger when
 * `onToggleCollapsed` is given; a collapsed panel echoes the question next to
 * the label so the header still says what is being asked.
 */
function CardHeader({
  collapsed,
  collapsedSummary,
  counter,
  label,
  onDismiss,
  onToggleCollapsed,
}: {
  collapsed?: boolean;
  collapsedSummary?: string;
  counter?: string;
  label: string;
  onDismiss?: () => void;
  onToggleCollapsed?: () => void;
}) {
  const labelRow = (
    <>
      <span
        className={cn(
          'text-[11px] font-semibold tracking-widest text-muted-foreground uppercase',
          onToggleCollapsed && 'group-hover/header:text-foreground'
        )}
      >
        {label}
      </span>
      {counter ? (
        <span className='flex h-5 shrink-0 items-center rounded-md bg-muted/60 px-1.5 text-[10px] font-medium text-muted-foreground tabular-nums'>
          {counter}
        </span>
      ) : null}
      {collapsed && collapsedSummary ? (
        <span className='min-w-0 flex-1 truncate text-xs text-muted-foreground'>{collapsedSummary}</span>
      ) : null}
    </>
  );

  return (
    <div className='flex items-center gap-1 px-2.5 py-2.5'>
      {onToggleCollapsed ? (
        <button
          className='group/header flex min-w-0 flex-1 items-center gap-3 rounded-lg px-2.5 py-1.5 text-left outline-none transition-colors duration-150'
          // The sidebar's legacy `button:where(:not([data-slot]))` base paints a
          // 1px app border on every bare button; naming the slot opts these
          // custom rows out so their Tailwind borders/fills are the only ones.
          data-slot='session-chat-question-header'
          onClick={onToggleCollapsed}
          title={collapsed ? 'Show the question and its options' : 'Hide the question and its options'}
          type='button'
        >
          {labelRow}
          {/* Control tier, like every other expander in the chat. */}
          <IconChevronRight
            aria-hidden='true'
            className={cn(
              'ghostex-chat-disclosure-chevron ml-auto text-muted-foreground group-hover/header:text-foreground',
              !collapsed && 'is-open'
            )}
          />
        </button>
      ) : (
        <div className='flex min-w-0 flex-1 items-center gap-3 px-2.5 py-1.5'>{labelRow}</div>
      )}
      {onDismiss ? (
        <Button aria-label='Dismiss' onClick={onDismiss} size='icon-xs' variant='ghost'>
          <IconX aria-hidden='true' stroke={2} />
        </Button>
      ) : null}
    </div>
  );
}

function CardNotice({
  onSwitchToTerminal,
  text,
  tone,
}: {
  onSwitchToTerminal?: () => void;
  text: string;
  tone: 'destructive' | 'muted';
}) {
  return (
    <div
      className={cn(
        'flex items-center gap-2 text-[11px]',
        tone === 'destructive' ? 'text-destructive/80' : 'text-muted-foreground'
      )}
      role='status'
    >
      <span className='min-w-0 flex-1 leading-snug'>{text}</span>
      {onSwitchToTerminal ? (
        <Button onClick={onSwitchToTerminal} size='xs' variant='outline'>
          <IconTerminal2 aria-hidden='true' stroke={2} />
          Terminal
        </Button>
      ) : null}
    </div>
  );
}

/** Composer-shaped bottom row: notices, free text, and the primary action. */
function CardActionRow({ children, notice }: { children: React.ReactNode; notice?: React.ReactNode }) {
  return (
    <div className='grid gap-2 px-4 py-2.5'>
      {notice}
      <div className='flex items-center gap-2'>{children}</div>
    </div>
  );
}

export function SessionChatInteractiveCard({
  canSend,
  onAnswer,
  onInterrupt,
  onShowingChange,
  onShowingQuestionChange,
  onSwitchToTerminal,
  prompt,
}: SessionChatInteractiveCardProps) {
  const [dismissedKey, setDismissedKey] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<DraftAnswer[]>([]);
  const [activeQuestion, setActiveQuestion] = useState(0);
  const [collapsed, setCollapsed] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [deliveryFailed, setDeliveryFailed] = useState(false);
  const submittingRef = useRef(false);

  const cardKey = sessionChatCardDismissKey(prompt);
  const promptContentKey = prompt === null ? null : JSON.stringify(prompt);
  const showing = prompt !== null && cardKey !== dismissedKey;
  const showingQuestion = showing && prompt?.kind === 'question';
  const readOnly = !canSend;

  const questions = prompt?.kind === 'question' ? prompt.questions : [];
  const questionIndex = Math.min(activeQuestion, Math.max(questions.length - 1, 0));
  const question = questions[questionIndex];

  // Reset the dismissed key whenever the prompt clears so an identical
  // follow-up prompt shows again.
  useEffect(() => {
    if (prompt === null) {
      setDismissedKey(null);
    }
  }, [prompt]);

  // Fresh drafts per prompt content; cancel a stale in-flight submit gate
  // during commit so an old answer can't act on a new prompt.
  useLayoutEffect(() => {
    submittingRef.current = false;
    setSubmitting(false);
    setDeliveryFailed(false);
    setActiveQuestion(0);
    setCollapsed(false);
    if (prompt?.kind === 'question') {
      setDrafts(prompt.questions.map(() => ({ indices: [], other: '' })));
    } else {
      setDrafts([]);
    }
  }, [canSend, promptContentKey]);

  useEffect(() => {
    onShowingQuestionChange?.(showingQuestion === true);
  }, [onShowingQuestionChange, showingQuestion]);

  useEffect(() => {
    onShowingChange?.(showing);
    return () => onShowingChange?.(false);
  }, [onShowingChange, showing]);

  const submitAnswer = useCallback(
    (params: Omit<GxserverAnswerSessionChatPromptParams, 'projectId' | 'sessionId'>): void => {
      if (submittingRef.current || readOnly) {
        return;
      }
      submittingRef.current = true;
      setSubmitting(true);
      setDeliveryFailed(false);
      const keyAtSubmit = cardKey;
      void onAnswer(params)
        .then(() => {
          setDismissedKey(keyAtSubmit);
        })
        .catch(() => {
          // The keystrokes never reached the TUI: keep the card and say so.
          setDeliveryFailed(true);
        })
        .finally(() => {
          submittingRef.current = false;
          setSubmitting(false);
        });
    },
    [cardKey, onAnswer, readOnly]
  );

  const submitQuestions = useCallback(
    (answerDrafts: DraftAnswer[]): void => {
      const selections: SessionChatQuestionSelection[] = answerDrafts.map((entry) => ({
        indices: entry.indices,
        ...(entry.other.trim() ? { other: entry.other.trim() } : {}),
      }));
      submitAnswer({ kind: 'question', selections });
    },
    [submitAnswer]
  );

  const selectOption = useCallback(
    (optionIndex: number): void => {
      if (!question || readOnly || submitting) {
        return;
      }
      const nextDrafts = drafts.map((entry, index) => {
        if (index !== questionIndex) {
          return entry;
        }
        if (question.multiSelect) {
          const selected = entry.indices.includes(optionIndex);
          return {
            ...entry,
            indices: selected
              ? entry.indices.filter((value) => value !== optionIndex)
              : [...entry.indices, optionIndex].sort((a, b) => a - b),
          };
        }
        return { ...entry, indices: [optionIndex] };
      });
      setDrafts(nextDrafts);

      if (question.multiSelect) {
        return;
      }
      if (questionIndex >= questions.length - 1) {
        submitQuestions(nextDrafts);
      } else {
        setActiveQuestion(questionIndex + 1);
      }
    },
    [drafts, question, questionIndex, questions.length, readOnly, submitQuestions, submitting]
  );

  // Number keys 1-9 pick the matching option while focus sits outside an
  // editable field. A collapsed panel opts out: the numbers it refers to are
  // not on screen.
  useEffect(() => {
    if (!question || readOnly || submitting || collapsed || !showingQuestion) {
      return;
    }
    const handler = (event: KeyboardEvent): void => {
      if (event.metaKey || event.ctrlKey || event.altKey) {
        return;
      }
      const target = event.target;
      if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) {
        return;
      }
      if (target instanceof HTMLElement && target.closest('[contenteditable]:not([contenteditable="false"])')) {
        return;
      }
      const digit = Number.parseInt(event.key, 10);
      if (Number.isNaN(digit) || digit < 1 || digit > 9) {
        return;
      }
      const optionIndex = digit - 1;
      if (optionIndex >= question.options.length) {
        return;
      }
      event.preventDefault();
      selectOption(optionIndex);
    };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, [collapsed, question, readOnly, selectOption, showingQuestion, submitting]);

  if (!showing || !prompt) {
    return null;
  }

  const dismiss = (): void => {
    setDismissedKey(cardKey);
    onInterrupt();
  };

  const notice = deliveryFailed ? (
    <CardNotice
      text={DELIVERY_FAILED_NOTICE}
      tone='destructive'
      {...(onSwitchToTerminal ? { onSwitchToTerminal } : {})}
    />
  ) : readOnly ? (
    <CardNotice text={READ_ONLY_NOTICE} tone='muted' {...(onSwitchToTerminal ? { onSwitchToTerminal } : {})} />
  ) : null;

  if (prompt.kind === 'approval') {
    return (
      <CardShell kind='approval'>
        <CardPanel>
          <CardHeader label='Pending approval' {...(readOnly ? {} : { onDismiss: dismiss })} />
          <div className='min-w-0 px-4 pt-1 pb-3.5 sm:px-5'>
            <p className='text-sm text-foreground/90'>{`Allow ${prompt.tool}?`}</p>
            {prompt.summary ? (
              <div className='mt-3 min-w-0 rounded-lg border border-border/65 bg-background/70 p-3'>
                <pre className='max-h-40 min-w-0 overflow-auto font-mono text-xs leading-relaxed whitespace-pre-wrap text-foreground [overflow-wrap:anywhere]'>
                  {prompt.summary}
                </pre>
              </div>
            ) : null}
          </div>
        </CardPanel>
        <CardActionRow {...(notice ? { notice } : {})}>
          <div className='ml-auto flex items-center gap-2'>
            <Button
              data-chat-answer-control=''
              disabled={submitting || readOnly}
              onClick={() => {
                submitAnswer({ approvalSend: '', kind: 'approval' });
              }}
              size='sm'
              variant='outline'
            >
              Deny
            </Button>
            <Button
              data-chat-answer-control=''
              disabled={submitting || readOnly}
              onClick={() => {
                submitAnswer({ approvalSend: '1', kind: 'approval' });
              }}
              size='sm'
            >
              Allow
            </Button>
          </div>
        </CardActionRow>
      </CardShell>
    );
  }

  const draft = drafts[questionIndex] ?? { indices: [], other: '' };
  const isLastQuestion = questionIndex >= questions.length - 1;
  const customAnswerActive = draft.other.trim().length > 0;

  const questionAnswered = (index: number): boolean => {
    const entry = drafts[index];
    return entry !== undefined && (entry.indices.length > 0 || entry.other.trim().length > 0);
  };

  const hasAnswer = drafts.some((entry) => entry.indices.length > 0 || entry.other.trim().length > 0);

  const advance = (): void => {
    if (readOnly || submitting) {
      return;
    }
    if (isLastQuestion) {
      if (hasAnswer) {
        submitQuestions(drafts);
      }
      return;
    }
    setActiveQuestion(questionIndex + 1);
  };

  // Trailing button cycles Skip → Next → Send answer → Sending… (§2.6).
  // Single-select options advance immediately, including submitting the final
  // question; multi-select questions keep the explicit trailing action.
  const trailingLabel = submitting
    ? 'Sending…'
    : isLastQuestion
      ? 'Send answer'
      : questionAnswered(questionIndex)
        ? 'Next'
        : 'Skip';

  return (
    <CardShell kind='question'>
      <CardPanel>
        <CardHeader
          collapsed={collapsed}
          label={question?.header ?? (questions.length === 1 ? 'Question' : 'Questions')}
          onToggleCollapsed={() => setCollapsed((value) => !value)}
          {...(question ? { collapsedSummary: question.question } : {})}
          {...(questions.length > 1 ? { counter: `${questionIndex + 1}/${questions.length}` } : {})}
          {...(readOnly ? {} : { onDismiss: dismiss })}
        />
        {question && !collapsed ? (
          <div className='px-4 pt-1 pb-3 sm:px-5'>
            <p className='text-sm text-foreground/90'>{question.question}</p>
            {question.multiSelect ? (
              <p className='mt-1 text-xs text-muted-foreground'>Select one or more options.</p>
            ) : null}
            <div className='mt-3'>
              <SessionChatChoiceRows
                onSelect={selectOption}
                options={question.options}
                readOnly={readOnly}
                selected={customAnswerActive ? [] : draft.indices}
                showShortcuts
              />
            </div>
          </div>
        ) : null}
      </CardPanel>
      <CardActionRow {...(notice ? { notice } : {})}>
        {questionIndex > 0 ? (
          <Button
            aria-label='Previous question'
            disabled={submitting}
            onClick={() => setActiveQuestion(questionIndex - 1)}
            size='icon-sm'
            variant='ghost'
          >
            <IconArrowLeft aria-hidden='true' stroke={2} />
          </Button>
        ) : null}
        {question?.allowCustom === false ? (
          // The asking tool takes no free-text answer (Pi's cursor_ask_question
          // with allowCustom: false), so only the options are offered.
          <div aria-hidden='true' className='min-w-0 flex-1' />
        ) : (
          <input
            className='min-w-0 flex-1 bg-transparent text-sm leading-6 text-foreground outline-none placeholder:text-muted-foreground disabled:cursor-default'
            disabled={readOnly}
            onChange={(event) => {
              const value = event.target.value;
              setDrafts((current) =>
                current.map((entry, index) => (index === questionIndex ? { ...entry, other: value } : entry))
              );
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.shiftKey) {
                event.preventDefault();
                advance();
              }
            }}
            placeholder='Write a custom answer…'
            type='text'
            value={draft.other}
          />
        )}
        <Button
          className='min-w-24'
          data-chat-answer-control=''
          disabled={readOnly || submitting || (isLastQuestion && !hasAnswer)}
          onClick={advance}
          size='sm'
        >
          {trailingLabel}
        </Button>
      </CardActionRow>
    </CardShell>
  );
}
