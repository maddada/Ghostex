import type { Meta, StoryObj } from '@storybook/react-vite';
import { useMemo, useState, type CSSProperties } from 'react';
import { Button } from '@/packages/components/ui/button';
import {
  DEFAULT_ACCOUNT_POLICY,
  type AccountSwitchProgress,
  type AgentAccountsState,
} from '@/packages/shared/agent-accounts';
import type { GxserverReadSessionChatResult, GxserverSessionChatEvent } from '@/packages/shared/session-chat';
import { SessionChatView } from '../chat/session-chat-view';
import type { SessionChatTransport } from '../chat/session-chat-transport';
import { SessionAccountsPanel } from './session-panel';

function LiveSwitch({ manual = false, narrow = false }: { manual?: boolean; narrow?: boolean }) {
  const [sent, setSent] = useState<string[]>([]);
  const fixture = useMemo(() => {
    const source = manual ? 'manual' : 'automatic';
    let seq = 1;
    let progress: AccountSwitchProgress | null = {
      id: `live-${Date.now()}`,
      provider: 'claude',
      source,
      phase: 'switching',
      fromAccountId: 'source',
      toAccountId: 'target',
      updatedAt: new Date().toISOString(),
    };
    const accounts: AgentAccountsState['accounts'] = [
      {
        id: 'source',
        name: 'source@example.com',
        selector: '71',
        email: 'source@example.com',
        percentages: [100, 82, 96],
      },
      {
        id: 'target',
        name: 'target@example.com',
        selector: '70',
        email: 'target@example.com',
        percentages: [19, 29, 56],
      },
    ].map(({ percentages, ...account }) => ({
      ...account,
      provider: 'claude',
      color: 'neutral',
      eligible: true,
      registered: true,
      sharedHistory: true,
      status: 'ready',
      sessionCount: 1,
      usage: ['fiveHour', 'sevenDay', 'fable'].map((id, i) => ({
        id,
        label: id,
        usedPercent: percentages[i],
        resetsAt: new Date(Date.now() + 8100000).toISOString(),
        ...(i === 2 ? { model: 'Fable' } : {}),
      })),
    }));
    const listeners = new Set<(event: GxserverSessionChatEvent) => void>();
    const read = (): GxserverReadSessionChatResult => ({
      messages: [],
      hasMore: false,
      beforeOffset: 0,
      epoch: 1,
      seq,
      status: 'ready',
      agent: 'claude',
      sessionAgentId: 'claude',
      accountSwitch: progress,
      working: false,
      queue: [],
      screenProbed: true,
    });
    const publish = (phase: AccountSwitchProgress['phase']) => {
      progress = {
        ...progress!,
        phase,
        updatedAt: new Date().toISOString(),
        ...(phase === 'failed' ? { reason: 'The selected account could not be verified.' } : {}),
      };
      seq++;
      for (const onEvent of listeners)
        onEvent({
          type: 'sessionChatState',
          protocolVersion: 1,
          serverId: 'storybook',
          projectId: 'storybook',
          sessionId: 'account-switch',
          epoch: 1,
          seq,
          status: 'ready',
          working: false,
          accountSwitch: progress,
          queue: [],
        });
    };
    const transport: SessionChatTransport = {
      read: async () => read(),
      subscribe: ({ onEvent }) => {
        listeners.add(onEvent);
        return () => {
          listeners.delete(onEvent);
        };
      },
      send: async (text) => {
        setSent((messages) => [...messages, text]);
      },
      answerPrompt: async () => {},
      interrupt: async () => {
        publish('cancelled');
      },
      accounts: async (request) => {
        if (request.operation === 'select') {
          progress = { ...progress!, id: `manual-${Date.now()}`, source: 'manual', toAccountId: request.accountId };
          publish('switching');
        }
        return {
          accounts,
          helpers: [],
          defaults: { claude: DEFAULT_ACCOUNT_POLICY, codex: DEFAULT_ACCOUNT_POLICY },
          defaultAccounts: {},
          session: {
            provider: 'claude',
            accountId: progress?.phase === 'success' || progress?.phase === 'continuing' ? 'target' : 'source',
            policy: DEFAULT_ACCOUNT_POLICY,
            override: null,
            accountSwitch: progress,
          },
        };
      },
    };
    return {
      transport,
      publish,
      restart: () => {
        progress = { ...progress!, id: `live-${Date.now()}`, source };
        publish('switching');
      },
    };
  }, [manual]);
  return (
    <div style={{ height: '100vh', display: 'flex', flexDirection: 'column', background: '#0d0d0d' }}>
      <div style={{ display: 'flex', gap: 8, padding: 12, flexWrap: 'wrap' }}>
        <Button size='sm' variant='outline' onClick={fixture.restart}>
          Start switch
        </Button>
        <Button size='sm' variant='outline' onClick={() => fixture.publish('resuming')}>
          Resume conversation
        </Button>
        {!manual && (
          <Button size='sm' variant='outline' onClick={() => fixture.publish('continuing')}>
            Continue Session
          </Button>
        )}
        <Button size='sm' variant='outline' onClick={() => fixture.publish('success')}>
          Complete
        </Button>
        <Button size='sm' variant='outline' onClick={() => fixture.publish('failed')}>
          Fail
        </Button>
        <span style={{ color: '#888', alignSelf: 'center', fontSize: 12 }}>Sent messages: {sent.length}</span>
      </div>
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          minHeight: 0,
          flex: 1,
          width: '100%',
          maxWidth: narrow ? 420 : 1100,
          alignSelf: 'center',
        }}
      >
        <SessionChatView
          agentLabel='claude'
          transport={fixture.transport}
          sessionKey='storybook:account-switch'
          theme='dark'
          inputBackend='plain'
        />
      </div>
    </div>
  );
}
const meta = {
  title: 'Chat/Account switching live',
  component: LiveSwitch,
  parameters: { layout: 'fullscreen' },
  args: { manual: false, narrow: false },
} satisfies Meta<typeof LiveSwitch>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Automatic: Story = {};
export const Manual: Story = { args: { manual: true } };
export const Narrow: Story = { args: { narrow: true } };

function RecoveryPanelPreview() {
  const [stopped, setStopped] = useState(false);
  const policy = { ...DEFAULT_ACCOUNT_POLICY, enabled: true };
  const data: AgentAccountsState = {
    accounts: [
      {
        id: 'current',
        provider: 'claude',
        selector: '71',
        name: 'x•••1@•••••.•••',
        email: 'x•••1@•••••.•••',
        color: 'neutral',
        eligible: true,
        registered: true,
        sharedHistory: true,
        status: 'ready',
        sessionCount: 1,
        usage: [
          { id: 'fiveHour', label: '5h limit', usedPercent: 55 },
          { id: 'sevenDay', label: '7d limit', usedPercent: 100 },
          { id: 'fable', label: 'Fable', model: 'Fable', usedPercent: 55 },
        ],
      },
    ],
    helpers: [],
    defaults: { claude: policy, codex: DEFAULT_ACCOUNT_POLICY },
    defaultAccounts: {},
    session: {
      provider: 'claude',
      accountId: 'current',
      policy,
      override: null,
      ...(stopped
        ? {}
        : {
            recovery: {
              status: 'resumed' as const,
              reason: 'Continuation sent on the selected account.',
              attempt: 0,
              updatedAt: new Date().toISOString(),
            },
          }),
    },
  };
  return (
    <div className='dark' style={{ padding: 16, minHeight: '100vh', background: '#0d0d0d' }}>
      <div
        className='gx-account-submenu'
        style={
          {
            margin: '0 auto',
            width: '100%',
            maxWidth: 420,
            border: '1px solid var(--border)',
            borderRadius: 12,
            '--available-height': 'calc(100vh - 32px)',
          } as CSSProperties
        }
      >
        <SessionAccountsPanel
          data={data}
          error=''
          busy={false}
          close={() => {}}
          request={async (request) => {
            if (request.operation === 'stopRecovery') setStopped(true);
            return true;
          }}
        />
      </div>
    </div>
  );
}
export const RecoveryStatus: Story = { render: () => <RecoveryPanelPreview /> };
