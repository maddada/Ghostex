import type { Meta, StoryObj } from '@storybook/react-vite';
import { useState } from 'react';
import type { SessionChatQueuedPrompt } from '../../shared/session-chat';
import { SessionChatQueueRows } from './session-chat-queue-rows';
import { moveSessionChatQueueRow } from './session-chat-queue';

/*
 * The Ghostex prompt-queue strip (plan 016 §4), standing in for the composer it
 * normally lives inside.
 *
 * Everything is local state — no gxserver, no transport, no daemon — so the
 * strip can be seen and DRAGGED before any host wires the endpoints. That is
 * the point: reorder, delete, retry and send-now all mutate the array here the
 * same way an authoritative queue from the server replaces it in the real
 * composer.
 *
 * READ THIS TWICE: these rows are NOT SessionChatMessage.queued. That flag is
 * the agent CLI's own internal queue and renders inside the transcript. These
 * are prompts the agent has never seen.
 */

const AT = '2026-08-21T10:00:00.000Z';

function prompt(id: string, text: string, extra: Partial<SessionChatQueuedPrompt> = {}): SessionChatQueuedPrompt {
  return { createdAt: AT, id, state: 'queued', text, updatedAt: AT, ...extra };
}

const SEED: SessionChatQueuedPrompt[] = [
  prompt('1', 'Run the release preflight and tell me what fails.'),
  prompt(
    '2',
    '\n\n# Sweep the sidebar\n\nThen check every session card still reorders under touch, because the last dnd-kit change moved the activation constraints and nothing covers that path.'
  ),
  prompt('3', 'Commit the fix with a message that names the bead.', {
    state: 'sending',
  }),
  prompt('4', 'Open a PR against main and paste the typecheck output.', {
    errorMessage: "The agent's login expired.",
    state: 'failed',
  }),
  prompt('5', 'Summarise what changed in three bullets.'),
  prompt('6', 'Then go quiet until I ask for something else.'),
];

function SessionChatQueueStripStory({ disabled, theme }: { disabled: boolean; theme: 'dark' | 'light' }) {
  const [prompts, setPrompts] = useState<SessionChatQueuedPrompt[]>(SEED);

  return (
    <div
      className='ghostex-session-chat-scope flex h-screen flex-col justify-end bg-background p-4 text-foreground'
      data-chat-theme={theme}
    >
      <div className='mx-auto w-full max-w-3xl'>
        {/* The real container: the strip sits inside it, directly above the
            input, and never in the transcript. */}
        <div className='ghostex-chat-composer min-w-0 rounded-3xl border border-input bg-card px-4 py-2.5'>
          <SessionChatQueueRows
            disabled={disabled}
            onDelete={(row) => {
              setPrompts((current) => current.filter((entry) => entry.id !== row.id));
            }}
            onEdit={(row) => {
              setPrompts((current) => current.filter((entry) => entry.id !== row.id));
            }}
            onReorder={(promptIds) => {
              setPrompts((current) => {
                let next = [...current];
                promptIds.forEach((id, target) => {
                  const from = next.findIndex((entry) => entry.id === id);
                  if (from >= 0) {
                    next = moveSessionChatQueueRow(next, from, target);
                  }
                });
                return next;
              });
            }}
            onRetry={(row) => {
              setPrompts((current) =>
                current.map((entry) => (entry.id === row.id ? prompt(entry.id, entry.text) : entry))
              );
            }}
            onSendNow={(row) => {
              setPrompts((current) => current.filter((entry) => entry.id !== row.id));
            }}
            prompts={prompts}
          />
          <div className='pb-1.5 text-sm leading-6 text-muted-foreground'>Send a message to the agent…</div>
        </div>
      </div>
    </div>
  );
}

const meta = {
  args: { disabled: false, theme: 'dark' },
  argTypes: {
    theme: { control: 'inline-radio', options: ['dark', 'light'] },
  },
  component: SessionChatQueueStripStory,
  parameters: { layout: 'fullscreen' },
  title: 'Chat/Prompt queue strip',
} satisfies Meta<typeof SessionChatQueueStripStory>;

export default meta;

type Story = StoryObj<typeof meta>;

/** Six rows: one multi-line, one being delivered, one failed. Caps at five. */
export const Dark: Story = { args: { theme: 'dark' } };

export const Light: Story = { args: { theme: 'light' } };

/** Input held by another device: rows stay readable, controls go inert. */
export const Disabled: Story = { args: { disabled: true } };
