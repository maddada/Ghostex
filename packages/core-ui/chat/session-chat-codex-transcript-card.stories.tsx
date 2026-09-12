import type { Meta, StoryObj } from '@storybook/react-vite';
import { useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import type { SessionChatTerminalNotice } from '@/packages/shared/session-chat';
import { SessionChatTerminalNoticeCard } from './session-chat-terminal-notice-card';
import { SessionChatCardGallery } from './session-chat-card-gallery';

// Keep this fixture's copy in sync with server/src/session_chat_codex_pager.rs.
const NOTICE: SessionChatTerminalNotice = {
  kind: 'codexInputBlocked',
  severity: 'info',
  source: 'screen',
  detectedAt: '2026-09-06T12:00:00.000Z',
  title: 'Restore terminal to chat view',
  dialog: {
    id: 'codex-transcript-pager',
    title: 'Restore terminal to chat view',
    body: "Codex's transcript viewer is open. It closes automatically when chat is active and no connected client is viewing this session's terminal. Closing it manually also closes it for other clients.",
    footer: 'q to quit esc/← to edit prev → to edit next enter to edit message',
    rows: [],
    input: null,
    inputValue: '',
    actions: ['cancel'],
  },
};

function CodexTranscriptCardPreview() {
  const [closed, setClosed] = useState(false);
  return (
    <div
      className='ghostex-session-chat-scope dark flex h-screen items-start justify-center overflow-y-auto bg-background p-6 text-foreground [--radius:0.625rem]'
      data-chat-theme='dark'
    >
      <div className='flex w-full max-w-2xl flex-col gap-4'>
        {closed ? (
          <div className='flex items-center justify-between gap-3 text-sm text-muted-foreground'>
            <p role='status'>Chat restored in this preview.</p>
            <Button onClick={() => setClosed(false)} size='sm' variant='outline'>
              Show card again
            </Button>
          </div>
        ) : (
          <SessionChatTerminalNoticeCard
            notice={NOTICE}
            canSend
            onSendKeys={async () => {}}
            onAnswerDialog={async (answer) => {
              if (answer.dialogAction === 'cancel') setClosed(true);
            }}
          />
        )}
        <SessionChatCardGallery />
      </div>
    </div>
  );
}

const meta = {
  title: 'Chat/Codex Transcript Card',
  component: CodexTranscriptCardPreview,
  parameters: { layout: 'fullscreen' },
} satisfies Meta<typeof CodexTranscriptCardPreview>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Open: Story = {};
