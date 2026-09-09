import type { Meta, StoryObj } from '@storybook/react-vite';
import { SessionChatActivityRow } from './session-chat-activity-row';

const meta = {
  title: 'Chat/Compaction',
  component: SessionChatActivityRow,
  parameters: { layout: 'fullscreen' },
  decorators: [
    (Story) => (
      <div
        className='ghostex-session-chat-scope dark min-h-screen bg-background p-6 text-foreground'
        data-chat-theme='dark'
      >
        <div className='mx-auto w-full max-w-2xl'>
          <Story />
        </div>
      </div>
    ),
  ],
} satisfies Meta<typeof SessionChatActivityRow>;

export default meta;
type Story = StoryObj<typeof meta>;

export const ClaudeCode: Story = {
  args: {
    activity: {
      kind: 'compacting',
      label: 'Compacting conversation',
      percent: 26,
      detectedAt: new Date().toISOString(),
      elapsedSeconds: 26,
    },
  },
};
