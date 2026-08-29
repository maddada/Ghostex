// "Worked for Xs" fold timing. The fold is the settled reading of a turn, so
// it must NEVER appear while the agent is still producing the response — yet
// three kinds of user-role rows land at the transcript tail mid-response and
// used to close the streaming turn the moment they appeared: a harness-injected
// turn (task notification, local command output), the agent CLI's own queued
// prompt row, and the optimistic echo of a send issued mid-turn. Each of them
// folded live work into a "Worked for" row, yanked the bottom-pinned viewport
// onto it, and unfolded again when the injected row settled. These stories
// simulate every trigger live.

import * as React from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, waitFor, within } from 'storybook/test';
import type { SessionChatMessage } from '../../shared/session-chat';
import { SessionChatMessageList } from './session-chat-message-list';

const WORKED_FOLD_LABEL = /^Worked/;

/** The active response: prompt, thinking, tools, and trailing commentary. */
const ACTIVE_RESPONSE: SessionChatMessage[] = [
  {
    id: 'fold-user-prompt',
    role: 'user',
    blocks: [{ type: 'text', text: 'Please refactor the session grid and report what changed.' }],
    source: 'transcript',
    timestamp: 1_000,
  },
  {
    id: 'fold-reasoning',
    role: 'reasoning',
    blocks: [{ type: 'text', text: 'Scanning the session grid model for the columns to move' }],
    source: 'transcript',
    timestamp: 2_000,
  },
  {
    id: 'fold-tools',
    role: 'tool',
    blocks: [{ type: 'tool-call', name: 'bash', input: 'rg -n "sessionGrid" packages/shared' }],
    source: 'transcript',
    timestamp: 3_000,
  },
  {
    id: 'fold-commentary',
    role: 'assistant',
    blocks: [
      {
        type: 'text',
        text: 'The grid model is in packages/shared — moving the column helpers now.',
      },
    ],
    source: 'transcript',
    timestamp: 4_000,
  },
];

const TASK_NOTIFICATION: SessionChatMessage = {
  id: 'fold-task-notification',
  role: 'user',
  blocks: [
    {
      type: 'text',
      text: '<task-notification><status>completed</status><summary>Background typecheck finished</summary></task-notification>',
    },
  ],
  source: 'transcript',
  timestamp: 5_000,
};

const QUEUED_PROMPT_ROW: SessionChatMessage = {
  id: 'fold-queued-prompt',
  role: 'user',
  blocks: [{ type: 'text', text: 'Also rename the helper file afterwards.' }],
  source: 'transcript',
  timestamp: 6_000,
  queued: true,
};

/** What `sessionChatPendingSendsAsMessages` emits for a send issued mid-turn. */
const MID_TURN_SEND_ECHO: SessionChatMessage = {
  id: 'pending:fold-echo',
  role: 'user',
  blocks: [{ type: 'text', text: 'And drop the unused import while you are in there.' }],
  source: 'client',
  timestamp: 7_000,
  queued: true,
};

function FoldTimingStory({
  isWorking,
  messages,
  summaryMode = false,
}: {
  isWorking: boolean;
  messages: SessionChatMessage[];
  summaryMode?: boolean;
}) {
  return (
    <div
      className='ghostex-session-chat-scope flex h-screen min-h-[34rem] flex-col bg-background text-foreground'
      data-chat-theme='dark'
    >
      <SessionChatMessageList
        hasMore={false}
        isWorking={isWorking}
        loadingEarlier={false}
        messages={messages}
        onLoadEarlier={() => undefined}
        summaryMode={summaryMode}
      />
    </div>
  );
}

const meta = {
  title: 'Chat/Worked-for fold timing',
  component: FoldTimingStory,
  parameters: { layout: 'fullscreen' },
} satisfies Meta<typeof FoldTimingStory>;

export default meta;

type Story = StoryObj<typeof meta>;

function expectNoWorkedFold(canvasElement: HTMLElement): void {
  expect(within(canvasElement).queryByText(WORKED_FOLD_LABEL)).not.toBeInTheDocument();
  // The live rows stay expanded in the flow instead of collapsing.
  expect(within(canvasElement).getByText(/moving the column helpers now/)).toBeVisible();
}

/** Trailing agent commentary alone must not read as a settled turn. */
export const MidRunTrailingAgentMessage: Story = {
  args: { isWorking: true, messages: ACTIVE_RESPONSE },
  play: async ({ canvasElement }) => {
    expectNoWorkedFold(canvasElement);
  },
};

/** A harness-injected task notification lands mid-response. */
export const MidRunTaskNotification: Story = {
  args: { isWorking: true, messages: [...ACTIVE_RESPONSE, TASK_NOTIFICATION] },
  play: async ({ canvasElement }) => {
    expectNoWorkedFold(canvasElement);
    // The notification pill itself still shows.
    expect(within(canvasElement).getByText('Background typecheck finished')).toBeVisible();
  },
};

/** The agent CLI parks a mid-turn prompt in its queue. */
export const MidRunQueuedPromptRow: Story = {
  args: { isWorking: true, messages: [...ACTIVE_RESPONSE, QUEUED_PROMPT_ROW] },
  play: async ({ canvasElement }) => {
    expectNoWorkedFold(canvasElement);
    expect(within(canvasElement).getByText('Queued')).toBeVisible();
  },
};

/** The optimistic echo of a send issued while the agent was working. */
export const MidRunSendEcho: Story = {
  args: { isWorking: true, messages: [...ACTIVE_RESPONSE, MID_TURN_SEND_ECHO] },
  play: async ({ canvasElement }) => {
    expectNoWorkedFold(canvasElement);
  },
};

/** Once the agent settles, the same transcript folds as usual. */
export const SettledTurnFolds: Story = {
  args: { isWorking: false, messages: [...ACTIVE_RESPONSE, TASK_NOTIFICATION] },
  play: async ({ canvasElement }) => {
    expect(within(canvasElement).getByText(WORKED_FOLD_LABEL)).toBeVisible();
    // The settled final reply owns its anchored copy affordance again.
    expect(canvasElement.querySelector('.ghostex-chat-final-action-copy')).not.toBeNull();
  },
};

/** Summary mode holds "Active work" — never a premature "Agent reply". */
export const SummaryModeMidRunHoldsActiveWork: Story = {
  args: {
    isWorking: true,
    messages: [...ACTIVE_RESPONSE, QUEUED_PROMPT_ROW],
    summaryMode: true,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    expect(canvas.getByRole('button', { name: 'Show active work' })).toBeVisible();
    expect(canvas.queryByRole('button', { name: 'Show agent reply' })).not.toBeInTheDocument();
  },
};

/**
 * The live simulation: rows land mid-response one after another while the
 * viewport follows the bottom. No "Worked for" may flash in, and the viewport
 * must stay pinned instead of jumping onto a fold. The transcript is padded so
 * it actually scrolls.
 */
function MidRunInjectionHarness() {
  const [messages, setMessages] = React.useState<SessionChatMessage[]>(() => [
    ...Array.from({ length: 8 }, (_unused, turn): SessionChatMessage[] => [
      {
        id: `fold-history-user-${turn}`,
        role: 'user',
        blocks: [{ type: 'text', text: `Earlier prompt ${turn}: adjust the layout of pane ${turn}.` }],
        source: 'transcript',
        timestamp: turn * 100,
      },
      {
        id: `fold-history-reply-${turn}`,
        role: 'assistant',
        blocks: [
          {
            type: 'text',
            text: `Reply ${turn}: adjusted the pane and verified the layout holds at every width.`,
          },
        ],
        source: 'transcript',
        timestamp: turn * 100 + 50,
      },
    ]).flat(),
    ...ACTIVE_RESPONSE,
  ]);

  React.useEffect(() => {
    const notification = window.setTimeout(() => {
      setMessages((current) => [...current, TASK_NOTIFICATION]);
    }, 250);
    const queued = window.setTimeout(() => {
      setMessages((current) => [...current, QUEUED_PROMPT_ROW]);
    }, 500);
    return () => {
      window.clearTimeout(notification);
      window.clearTimeout(queued);
    };
  }, []);

  return (
    <div
      className='ghostex-session-chat-scope flex h-screen flex-col bg-background text-foreground'
      data-chat-theme='dark'
    >
      <SessionChatMessageList
        hasMore={false}
        isWorking
        loadingEarlier={false}
        messages={messages}
        onLoadEarlier={() => undefined}
      />
    </div>
  );
}

export const MidRunInjectionKeepsViewportPinned: Story = {
  args: { isWorking: true, messages: [] },
  render: () => <MidRunInjectionHarness />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const viewport = canvasElement.querySelector<HTMLElement>('[data-slot="message-scroller-viewport"]');
    expect(viewport).not.toBeNull();
    const scroller = viewport as HTMLElement;

    // The eight settled history turns fold; the active response must not join
    // them when the injected rows land.
    expect(canvas.getAllByText(WORKED_FOLD_LABEL)).toHaveLength(8);
    await waitFor(() => {
      expect(canvas.getByText('Background typecheck finished')).toBeVisible();
      expect(canvas.getByText('Queued')).toBeVisible();
    });
    expect(canvas.getAllByText(WORKED_FOLD_LABEL)).toHaveLength(8);
    expect(canvas.getByText(/moving the column helpers now/)).toBeVisible();

    // The reader was following the live tail; the injections must not have
    // yanked the viewport away from the bottom.
    await waitFor(() => {
      expect(scroller.scrollHeight - scroller.clientHeight - scroller.scrollTop).toBeLessThanOrEqual(1);
    });
  },
};
