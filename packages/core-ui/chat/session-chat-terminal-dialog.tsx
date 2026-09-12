import { useEffect, useRef, useState } from 'react';
import { IconX } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { Input } from '@/packages/components/ui/input';
import { Textarea } from '@/packages/components/ui/textarea';
import { useSessionChatHostLinks } from './session-chat-links';
import type { GxserverAnswerSessionChatPromptParams, SessionChatTerminalDialog } from '@/packages/shared/session-chat';

type DialogAnswer = Omit<GxserverAnswerSessionChatPromptParams, 'projectId' | 'sessionId'>;
const ACTION_LABELS: Record<string, string> = {
  up: '↑ Previous',
  down: '↓ Next',
  left: '← Left',
  right: 'Right →',
  pageUp: 'Page up',
  pageDown: 'Page down',
  home: 'First',
  end: 'Last',
  tab: 'Next field',
  toggle: 'Toggle selected',
  confirm: 'Confirm',
  cancel: 'Back / Cancel',
  sessionOnly: 'Use for this session',
  sort: 'Change sort',
  reset: 'Reset to auto',
  day: 'Day view',
  week: 'Week view',
  projects: 'Toggle all projects',
  branch: 'Toggle current branch',
};

/** The agent owns the choices and settings; this card mirrors its current dialog. */
export function SessionChatTerminalDialogCard({
  dialog,
  canSend,
  onAnswer,
  controlsOnly = false,
}: {
  dialog: SessionChatTerminalDialog;
  canSend: boolean;
  controlsOnly?: boolean;
  onAnswer: (answer: DialogAnswer) => Promise<void>;
}) {
  const hostLinks = useSessionChatHostLinks();
  const [text, setText] = useState(dialog.inputValue);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef(false);
  useEffect(() => setText(dialog.inputValue), [dialog.inputValue]);
  const run = async (params: Partial<DialogAnswer>): Promise<void> => {
    if (!canSend || inFlight.current) return;
    inFlight.current = true;
    setPending(true);
    setError(null);
    try {
      await onAnswer({ kind: 'terminalDialog', dialogId: dialog.id, ...params });
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : 'The dialog changed. Review the current options and try again.'
      );
    } finally {
      inFlight.current = false;
      setPending(false);
    }
  };
  const disabled = !canSend || pending;
  /**
   * CDXC:SessionChat 2026-09-06 DECISION:
   * User: the Codex transcript card should match the notices above the composer, offer a visible "Restore chat" button, and omit terminal keyboard commands because they do not work in chat.
   * User: title it "Restore terminal to chat view" and make the body and button font sizes match the header.
   */
  if (dialog.id === 'codex-transcript-pager') {
    return (
      /* CDXC:SessionChat 2026-09-07 DECISION: User: Restore chat uses the same 20px outer padding and additional 10px row spacing as the other notice cards. */
      <section
        aria-label={dialog.title}
        className='flex min-w-0 flex-col gap-[10px] p-[20px]'
        data-slot='terminal-dialog'
      >
        <h3 className='ghostex-chat-card-title text-sm leading-snug font-medium text-foreground'>{dialog.title}</h3>
        <p className='mt-1 whitespace-pre-line break-words text-sm leading-snug text-muted-foreground'>{dialog.body}</p>
        <div className='mt-3 flex flex-wrap items-center gap-2'>
          <Button disabled={disabled} onClick={() => void run({ dialogAction: 'cancel' })} size='sm' variant='outline'>
            {pending ? 'Restoring chat…' : 'Restore chat'}
          </Button>
        </div>
        {!canSend ? (
          <p className='mt-2 text-[11px] leading-snug text-muted-foreground'>Input is held by another device.</p>
        ) : null}
        {error ? (
          <p role='alert' className='mt-2 text-[11px] leading-snug text-destructive/80'>
            {error}
          </p>
        ) : null}
      </section>
    );
  }
  const submitLabel =
    dialog.title === 'Ready to code?'
      ? 'Request changes'
      : dialog.title.startsWith('Tell us more (')
        ? 'Send feedback'
        : dialog.title === 'Custom review instructions'
          ? 'Start review'
          : dialog.title === 'Add marketplace'
            ? 'Add marketplace'
            : dialog.footer.includes('Enter to continue')
              ? 'Continue'
              : dialog.footer.includes('Enter to add')
                ? 'Add directory'
                : dialog.footer.includes('submit')
                  ? 'Submit'
                  : 'Save';
  const multilineInput =
    dialog.input === 'text' &&
    (dialog.title.startsWith('Tell us more (') ||
      dialog.title === 'Custom review instructions' ||
      dialog.title === 'Submit feedback / bug report');
  /**
   * CDXC:SessionChat 2026-09-08 DECISION:
   * User: always show the exit action at the bottom beside the other buttons for /usage and similar agent dialogs, so leaving them never requires switching to the terminal.
   */
  const visibleActions = dialog.actions.filter((action) => dialog.input !== 'text' || action !== 'confirm');
  const cancelLabel = dialog.footer.toLowerCase().includes('esc to clear')
    ? 'Clear / Back'
    : dialog.footer.includes('go back')
      ? 'Back'
      : dialog.footer.includes('close') || dialog.footer.includes('q to quit')
        ? 'Close'
        : 'Cancel';
  return (
    <section aria-label={dialog.title} className='grid min-w-0 gap-3 p-4' data-slot='terminal-dialog'>
      {!controlsOnly ? (
        <div className='flex items-center justify-between gap-3'>
          <h3 className='ghostex-chat-card-title text-sm font-medium'>{dialog.title}</h3>
          {/* CDXC:SessionChat 2026-09-06 DECISION: User: top-right Cancel/Close controls use the same X button as the existing question cards. */}
          <Button
            className='ghostex-chat-card-dismiss'
            aria-label={cancelLabel}
            disabled={disabled}
            onClick={() => void run({ dialogAction: 'cancel' })}
            size='icon-xs'
            variant='outline'
          >
            <IconX aria-hidden='true' stroke={2} />
          </Button>
        </div>
      ) : null}
      {dialog.body && !controlsOnly ? (
        <pre className='max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-muted/30 p-3 text-xs leading-relaxed'>
          {dialog.body.split(/(https?:\/\/[^\s<>]+)/g).map((part, index) =>
            /^https?:\/\//.test(part) ? (
              <a
                key={index}
                href={part}
                target='_blank'
                rel='noreferrer'
                className='underline underline-offset-2'
                onClick={(event) => {
                  if (!hostLinks?.openUrl) return;
                  event.preventDefault();
                  hostLinks.openUrl(part, { external: event.shiftKey });
                }}
              >
                {part}
              </a>
            ) : (
              part
            )
          )}
        </pre>
      ) : null}
      {dialog.input === 'key' ? (
        <Button
          disabled={disabled}
          variant='outline'
          onKeyDown={(event) => {
            if (['Control', 'Alt', 'Shift', 'Meta'].includes(event.key)) return;
            event.preventDefault();
            event.stopPropagation();
            void run({
              dialogAction: 'key',
              text: event.key,
              keyModifiers:
                Number(event.shiftKey) +
                2 * Number(event.altKey) +
                4 * Number(event.ctrlKey) +
                8 * Number(event.metaKey),
            });
          }}
        >
          Focus here, then press the new shortcut
        </Button>
      ) : null}
      {dialog.input === 'text' || dialog.input === 'search' ? (
        <form
          className='ghostex-chat-card-input-row flex gap-2'
          onSubmit={(event) => {
            event.preventDefault();
            void run({ dialogAction: dialog.input === 'text' ? 'submit' : 'text', text });
          }}
        >
          {multilineInput ? (
            <Textarea
              aria-label={dialog.title}
              placeholder='Enter text…'
              maxLength={8192}
              value={text}
              onChange={(event) => setText(event.target.value)}
              disabled={disabled}
              rows={2}
            />
          ) : (
            <Input
              aria-label={dialog.input === 'search' ? 'Search options' : dialog.title}
              placeholder={dialog.input === 'search' ? 'Search options…' : 'Enter text…'}
              maxLength={dialog.input === 'search' ? 512 : 8192}
              value={text}
              onChange={(event) => setText(event.target.value)}
              disabled={disabled}
            />
          )}
          {multilineInput ? (
            /* CDXC:SessionChat 2026-09-07 DECISION: User: textarea cards place their footer hint on the left of the submit button in the same row below the input. */
            <div className='flex items-center justify-between gap-3'>
              <p className='ghostex-chat-card-hint min-w-0 text-xs text-muted-foreground'>{dialog.footer}</p>
              <Button type='submit' disabled={disabled} size='sm' variant='outline'>
                {submitLabel}
              </Button>
            </div>
          ) : (
            <Button type='submit' disabled={disabled} size='sm' variant='outline'>
              {dialog.input === 'search' ? 'Search' : submitLabel}
            </Button>
          )}
        </form>
      ) : null}
      {visibleActions.length > 0 ? (
        <div className='flex flex-wrap gap-2'>
          {visibleActions.map((action) => (
            <Button
              key={action}
              disabled={disabled}
              size='sm'
              variant='outline'
              onClick={() => void run({ dialogAction: action })}
            >
              {action === 'cancel'
                ? cancelLabel
                : action === 'confirm' && dialog.footer.includes('set as default')
                  ? 'Set as default'
                  : (ACTION_LABELS[action] ?? action)}
            </Button>
          ))}
        </div>
      ) : null}
      {!multilineInput ? <p className='ghostex-chat-card-hint text-xs text-muted-foreground'>{dialog.footer}</p> : null}
      {!canSend ? <p className='text-xs text-muted-foreground'>Input is currently controlled elsewhere.</p> : null}
      {error ? (
        <p role='alert' className='text-xs text-destructive'>
          {error}
        </p>
      ) : null}
    </section>
  );
}
