// CDXC:SessionChatRewind 2026-09-02:
// The confirmation in front of `/api/rewindSessionChat`. Rewinding drives the
// agent's own terminal dialog (Claude: `/rewind`, pick the prompt, "Restore
// conversation"), so it is not undoable from here and it is not cancellable
// once the daemon starts typing: the dialog therefore disables BOTH buttons
// while the call is in flight instead of offering a Cancel that could leave the
// terminal half-way through its own picker.
//
// It renders as a plain in-page dialog (the shared shadcn Dialog, portaled into
// this page), which is what the chat's Save-to-Markdown dialog does: the chat
// runs inside CEF on desktop and in a browser tab on web, and the same markup
// has to work in both, so no native child window is involved.

import { IconLoader2 } from '@tabler/icons-react';
import { useEffect, useState, type FormEvent } from 'react';
import { Button } from '@/packages/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/packages/components/ui/dialog';
import { cn } from '@/packages/components/utils';
import type { SessionChatTheme } from '@/packages/shared/session-chat';
import type { GxserverRewindSessionChatResult } from '@/packages/shared/gxserver-protocol';

export type RewindSessionChatToMessage = (params: { messageId: string }) => Promise<GxserverRewindSessionChatResult>;

export interface SessionChatRewindRequest {
  messageId: string;
  /**
   * The prompt's own copy text. The dialog quotes its first lines so the reader
   * confirms the right row, and the whole of it is what the composer is handed
   * back after a successful rewind.
   */
  prompt: string;
}

const PREVIEW_LINE_LIMIT = 3;

/** The first few lines of the prompt, so the quote stays a glance and not a re-read. */
function previewLines(preview: string): { lines: string[]; truncated: boolean } {
  const lines = preview.replace(/\s+$/u, '').split(/\r?\n/u);
  return {
    lines: lines.slice(0, PREVIEW_LINE_LIMIT),
    truncated: lines.length > PREVIEW_LINE_LIMIT,
  };
}

export function SessionChatRewindDialog({
  onOpenChange,
  onRewound,
  request,
  rewind,
  theme,
}: {
  onOpenChange: (open: boolean) => void;
  /**
   * The rewind landed: the prompt it rewound to, verbatim, so the surface can
   * put it back in the composer. Called once the dialog is closing, never on a
   * refusal.
   */
  onRewound?: (prompt: string) => void;
  /** The row being rewound to; null closes the dialog. */
  request: SessionChatRewindRequest | null;
  rewind: RewindSessionChatToMessage;
  theme: SessionChatTheme;
}) {
  const [error, setError] = useState<string | null>(null);
  const [rewinding, setRewinding] = useState(false);
  const open = request !== null;

  useEffect(() => {
    if (!open) {
      return;
    }
    setError(null);
    setRewinding(false);
  }, [open, request?.messageId]);

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (request === null || rewinding) {
      return;
    }
    setRewinding(true);
    setError(null);
    try {
      await rewind({ messageId: request.messageId });
      // The daemon re-snapshots the chat stream itself, so the rewound rows
      // leave the list on their own frame. Nothing is pruned here.
      onOpenChange(false);
      /*
      The prompt that was rewound away is the one the reader is about to
      rewrite, so it goes straight back into the composer. Handing it over
      while this dialog is still on screen is deliberate: the surface focuses
      the composer, and a modal popup that sees focus move outside it on the
      way out leaves it there instead of pulling it back to the trigger.
      */
      onRewound?.(request.prompt);
    } catch (failure) {
      // The daemon's own sentence names what it verified on the screen and why
      // it stopped, which is the only useful thing to show for a refusal.
      setError(failure instanceof Error ? failure.message : 'The conversation could not be rewound.');
      setRewinding(false);
    }
  };

  const preview = previewLines(request?.prompt ?? '');

  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!rewinding || nextOpen) {
          onOpenChange(nextOpen);
        }
      }}
      open={open}
    >
      <DialogContent
        className={cn(
          'ghostex-session-chat-popup w-full max-w-md rounded-xl font-sans [--radius:0.625rem]',
          theme === 'dark' && 'dark'
        )}
      >
        <form className='flex flex-col gap-6' onSubmit={(event) => void submit(event)}>
          <DialogHeader>
            <DialogTitle>Rewind conversation</DialogTitle>
            <DialogDescription>Restore the conversation to the point before you sent this message?</DialogDescription>
          </DialogHeader>
          <div className='flex flex-col gap-4'>
            {preview.lines.length > 0 ? (
              <blockquote className='min-w-0 border-l-2 border-border pl-3 text-sm leading-relaxed break-words whitespace-pre-wrap text-muted-foreground'>
                {preview.lines.join('\n')}
                {preview.truncated ? '\n…' : ''}
              </blockquote>
            ) : null}
            <p className='text-xs leading-relaxed text-muted-foreground'>
              We only rewind using "Restore conversation" in the Chat View currently. Switch to Terminal View and use
              /rewind to resume using another option.
            </p>
            {error !== null ? (
              <p className='text-xs leading-relaxed text-destructive' role='alert'>
                {error}
              </p>
            ) : null}
          </div>
          <DialogFooter>
            <Button disabled={rewinding} onClick={() => onOpenChange(false)} type='button' variant='outline'>
              Cancel
            </Button>
            <Button autoFocus disabled={rewinding} type='submit' variant='outline'>
              {rewinding ? <IconLoader2 className='animate-spin' data-icon='inline-start' /> : null}
              {rewinding ? 'Rewinding' : 'Rewind'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
