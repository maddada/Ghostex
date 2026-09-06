import { useEffect, useRef, useState } from 'react';
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
  return (
    <section aria-label={dialog.title} className='grid min-w-0 gap-3 p-4' data-slot='terminal-dialog'>
      {!controlsOnly ? (
        <div className='flex items-center justify-between gap-3'>
          <h3 className='text-sm font-medium'>{dialog.title}</h3>
          <Button disabled={disabled} onClick={() => void run({ dialogAction: 'cancel' })} size='xs' variant='ghost'>
            {dialog.footer.toLowerCase().includes('esc to clear')
              ? 'Clear / Back'
              : dialog.footer.includes('go back')
                ? 'Back'
                : dialog.footer.includes('close') || dialog.footer.includes('q to quit')
                  ? 'Close'
                  : 'Cancel'}
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
          className='flex gap-2'
          onSubmit={(event) => {
            event.preventDefault();
            void run({ dialogAction: dialog.input === 'text' ? 'submit' : 'text', text });
          }}
        >
          {dialog.input === 'text' &&
          (dialog.title.startsWith('Tell us more (') ||
            dialog.title === 'Custom review instructions' ||
            dialog.title === 'Submit feedback / bug report') ? (
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
          <Button type='submit' disabled={disabled} size='sm' variant='secondary'>
            {dialog.input === 'search' ? 'Search' : submitLabel}
          </Button>
        </form>
      ) : null}
      <div className='flex flex-wrap gap-2'>
        {dialog.actions
          .filter(
            (action) => (controlsOnly || action !== 'cancel') && (dialog.input !== 'text' || action !== 'confirm')
          )
          .map((action) => (
            <Button
              key={action}
              disabled={disabled}
              size='sm'
              variant={action === 'confirm' ? 'default' : 'outline'}
              onClick={() => void run({ dialogAction: action })}
            >
              {action === 'confirm' && dialog.footer.includes('set as default')
                ? 'Set as default'
                : (ACTION_LABELS[action] ?? action)}
            </Button>
          ))}
      </div>
      <p className='text-xs text-muted-foreground'>{dialog.footer}</p>
      {!canSend ? <p className='text-xs text-muted-foreground'>Input is currently controlled elsewhere.</p> : null}
      {error ? (
        <p role='alert' className='text-xs text-destructive'>
          {error}
        </p>
      ) : null}
    </section>
  );
}
