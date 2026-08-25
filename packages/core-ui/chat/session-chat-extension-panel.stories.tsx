import type { Meta, StoryObj } from '@storybook/react-vite';
import { useState } from 'react';
import { SessionChatExtensionPanel, type SessionChatBarExtension } from './session-chat-extension-panel';

const SCRATCHPAD_ICON =
  "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%23d4d4d8' stroke-width='1.8'%3E%3Cpath d='M5 4h14v16H5zM8 8h8M8 12h8M8 16h5'/%3E%3C/svg%3E";
const TIMER_ICON =
  "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%23d4d4d8' stroke-width='1.8'%3E%3Ccircle cx='12' cy='13' r='7'/%3E%3Cpath d='M12 10v4l2 1M9 3h6'/%3E%3C/svg%3E";

const EXTENSIONS: SessionChatBarExtension[] = [
  {
    id: 'session-scratchpad',
    title: 'Session Scratchpad',
    iconUrl: SCRATCHPAD_ICON,
    url: 'https://session-scratchpad.example.invalid/',
  },
  {
    id: 'focus-timer',
    title: 'Focus Timer',
    iconUrl: TIMER_ICON,
    url: 'https://focus-timer.example.invalid/',
  },
];

function SessionChatExtensionPanelStory({ initialMinimized }: { initialMinimized: boolean }) {
  const [activeExtensionId, setActiveExtensionId] = useState(EXTENSIONS[0].id);
  const [minimized, setMinimized] = useState(initialMinimized);
  const [open, setOpen] = useState(true);

  return (
    <div
      className='ghostex-session-chat-scope dark flex h-screen flex-col justify-end bg-background p-4 text-foreground'
      data-chat-theme='dark'
    >
      <div className='mx-auto grid w-full max-w-3xl gap-2'>
        <div className='ghostex-chat-composer rounded-3xl border border-input bg-card px-4 py-3 text-sm text-muted-foreground'>
          Composer sits immediately above this panel.
        </div>
        {open ? (
          <SessionChatExtensionPanel
            activeExtensionId={activeExtensionId}
            extensions={EXTENSIONS}
            minimized={minimized}
            onActiveExtensionChange={setActiveExtensionId}
            onBridgeRequest={async () => null}
            onClose={() => setOpen(false)}
            onMinimizedChange={setMinimized}
          />
        ) : (
          <button className='text-xs text-muted-foreground' onClick={() => setOpen(true)} type='button'>
            Show chat extension
          </button>
        )}
      </div>
    </div>
  );
}

const meta = {
  args: { initialMinimized: false },
  component: SessionChatExtensionPanelStory,
  parameters: { layout: 'fullscreen' },
  title: 'Chat/Extension panel',
} satisfies Meta<typeof SessionChatExtensionPanelStory>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Expanded: Story = {};

export const Minimized: Story = { args: { initialMinimized: true } };
