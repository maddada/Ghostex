import type { Meta, StoryObj } from '@storybook/react-vite';
import { useRef, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { GxserverRpcError } from '@/packages/shared/gxserver-rpc-error';
import { SessionChatComposer, type SessionChatComposerHandle } from './session-chat-composer';

function ComposerStatusStory({
  scenario,
  theme,
  inputBackend,
}: {
  scenario: 'panels' | 'incoming' | 'error' | 'notReady' | 'blocked' | 'compacting' | 'idle';
  theme: 'dark' | 'light';
  inputBackend: 'lexical' | 'plain';
}) {
  const composer = useRef<SessionChatComposerHandle>(null);
  const transcript = useRef<HTMLDivElement>(null);
  const [sessionKey] = useState(() => `composer-status-story:${crypto.randomUUID()}`);
  const [notice, setNotice] = useState('');
  const [collapsed, setCollapsed] = useState(false);
  const [detectedAt] = useState(() => new Date().toISOString());
  return (
    <div
      className='ghostex-session-chat-scope flex h-screen flex-col gap-3 bg-background p-4 text-foreground'
      data-chat-theme={theme}
    >
      <div className='mx-auto flex w-full max-w-3xl flex-wrap items-center gap-2 text-xs'>
        <Button variant='outline' onClick={() => composer.current?.appendText('My local draft')}>
          Fill composer
        </Button>
        <Button
          variant='outline'
          onClick={() => {
            composer.current?.appendText('Keep this local draft');
            void composer.current
              ?.receiveDraftHandoff({ content: 'Incoming draft from another device' })
              .catch((error: unknown) => setNotice(String(error)));
          }}
        >
          Show incoming draft
        </Button>
        <span role='status'>
          {notice || `Scroll up to collapse; use More actions to maximize. ${collapsed ? 'Collapsed' : 'Expanded'}.`}
        </span>
      </div>
      <div className='mx-auto min-h-0 w-full max-w-3xl flex-1' ref={transcript}>
        <div className='h-full overflow-y-auto' data-slot='message-scroller-viewport'>
          {Array.from({ length: 30 }, (_, index) => (
            <p className='py-3 text-sm text-muted-foreground' key={index}>
              Progress update {index + 1}: continuing the implementation.
            </p>
          ))}
        </div>
      </div>
      <div className='mx-auto w-full max-w-3xl'>
        <SessionChatComposer
          ref={composer}
          sessionKey={sessionKey}
          theme={theme}
          inputBackend={inputBackend}
          isWorking={scenario !== 'idle'}
          workingStatus={{
            working: scenario !== 'idle',
            activity:
              scenario === 'compacting'
                ? { kind: 'compacting', label: 'Compacting conversation', percent: 42, detectedAt }
                : null,
          }}
          agentTasks={
            scenario === 'panels'
              ? { tasks: [{ id: '1', subject: 'Review the composer states', status: 'in_progress' }] }
              : null
          }
          agentFleet={
            scenario === 'panels'
              ? { detectedAt, agents: [{ name: 'explore', task: 'Checking browser interactions', elapsedSeconds: 8 }] }
              : null
          }
          draftSync={{ clientId: 'storybook', canSync: true, synced: null, push: async () => {} }}
          sendBlockedReason={scenario === 'blocked' ? 'Input is held by another device.' : null}
          onSend={async () => {
            if (scenario === 'error') throw new Error('Connection interrupted. Your draft is kept.');
            if (scenario === 'notReady')
              throw new GxserverRpcError('composerNotReady', 'The agent is waiting for setup to finish.', '/api/sendSessionChatMessage');
            setNotice('Message sent.');
          }}
          onInterrupt={() => setNotice('Stop requested.')}
          transcriptRef={transcript}
          scrollCollapseEnabled
          onScrollCollapsedChange={setCollapsed}
        />
      </div>
    </div>
  );
}

const meta = {
  title: 'Chat/Composer Status Placement',
  component: ComposerStatusStory,
  parameters: { layout: 'fullscreen' },
  args: { scenario: 'panels', theme: 'dark', inputBackend: 'lexical' },
  argTypes: {
    scenario: {
      control: 'select',
      options: ['panels', 'incoming', 'error', 'notReady', 'blocked', 'compacting', 'idle'],
    },
    theme: { control: 'inline-radio', options: ['dark', 'light'] },
    inputBackend: { control: 'inline-radio', options: ['lexical', 'plain'] },
  },
} satisfies Meta<typeof ComposerStatusStory>;
export default meta;
type Story = StoryObj<typeof meta>;
export const AgentPanels: Story = {};
export const IncomingDraft: Story = { args: { scenario: 'incoming' } };
export const SendError: Story = { args: { scenario: 'error' } };
export const ComposerNotReady: Story = { args: { scenario: 'notReady' } };
export const BlockedSend: Story = { args: { scenario: 'blocked' } };
export const Compacting: Story = { args: { scenario: 'compacting' } };
export const Idle: Story = { args: { scenario: 'idle' } };
export const PlainInput: Story = { args: { inputBackend: 'plain' } };
export const Light: Story = { args: { theme: 'light' } };
