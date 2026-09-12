import type { Meta, StoryObj } from '@storybook/react-vite';
import { normalizeSessionCardHoverButtons } from '@/packages/shared/session-card-hover-actions';
import { useLayoutEffect, useMemo, useState } from 'react';
import { DragDropProvider } from '@dnd-kit/react';
import { Button } from '@/packages/components/ui/button';
import type { GxserverSessionChatEvent, SessionChatMessage } from '@/packages/shared/session-chat';
import { SessionChatView } from './session-chat-view';
import type { SessionChatTransport } from './session-chat-transport';
import { pendingSessionChatAsyncQuestions } from './session-chat-async-questions-state';
import { SortableSessionCard } from '../sortable-session-card';
import { useSidebarStore } from '../sidebar-store';
import type { SidebarSessionItem } from '@/packages/shared/session-grid-contract';

function createPreviewTransport(onPendingChange: (count: number) => void, working: boolean): SessionChatTransport {
  const listeners = new Set<(event: GxserverSessionChatEvent) => void>();
  let seq = 1;
  const dismissed = new Set<string>();
  const messages: SessionChatMessage[] = [
    {
      id: 'request',
      role: 'user',
      source: 'transcript',
      timestamp: 1,
      blocks: [{ type: 'text', text: 'Build the settings page. Ask me about choices while you keep working.' }],
    },
    {
      id: 'questions',
      role: 'assistant',
      source: 'transcript',
      timestamp: 2,
      blocks: [{ type: 'text', text: 'I am building the layout. A few preferences will help me finish the details.' }],
      asyncQuestions: [
        {
          title: 'Which theme should the settings page use?',
          options: ['Dark (Recommended)', 'Light', 'Follow system'],
        },
        { title: 'How should changes be saved?', options: ['Save automatically', 'Use a Save button'] },
        { title: 'What should the heading above the settings be?' },
      ],
    },
    {
      id: 'progress',
      role: 'assistant',
      source: 'transcript',
      timestamp: 3,
      blocks: [{ type: 'text', text: 'I am continuing with keyboard navigation while you choose.' }],
    },
  ];
  return {
    read: async () => ({
      messages: [...messages],
      status: working ? 'working' : 'ready',
      working,
      agent: 'codex',
      agentSessionId: 'storybook-async-questions',
      screenProbed: true,
      hasMore: false,
      beforeOffset: 0,
      epoch: 1,
      seq,
    }),
    subscribe: ({ onEvent }) => {
      listeners.add(onEvent);
      return () => listeners.delete(onEvent);
    },
    send: async (text) => {
      const message: SessionChatMessage = {
        id: `reply-${++seq}`,
        role: 'user',
        source: 'transcript',
        timestamp: Date.now(),
        blocks: [{ type: 'text', text }],
      };
      messages.push(message);
      onPendingChange(
        pendingSessionChatAsyncQuestions(messages).filter((question) => !dismissed.has(question.key)).length
      );
      for (const onEvent of listeners)
        onEvent({
          type: 'sessionChatAppended',
          protocolVersion: 1,
          serverId: 'storybook',
          projectId: 'storybook',
          sessionId: 'async-questions',
          epoch: 1,
          seq,
          messages: [message],
        });
    },
    answerPrompt: async (params) => {
      if (params.kind === 'dismissAsyncQuestion' && params.questionId) {
        dismissed.add(params.questionId);
        onPendingChange(
          pendingSessionChatAsyncQuestions(messages).filter((question) => !dismissed.has(question.key)).length
        );
      }
    },
    interrupt: async () => {},
  };
}

function SidebarQuestionPreview({ count, working }: { count: number; working: boolean }) {
  const [focused, setFocused] = useState(true);
  const [sessionId] = useState(() => `storybook-async-sidebar:${crypto.randomUUID()}`);
  const ready = useSidebarStore((state) => Boolean(state.sessionsById[sessionId]));
  useLayoutEffect(() => {
    const session: SidebarSessionItem = {
      sessionId,
      alias: 'Codex settings page',
      displayTitle: 'Codex settings page',
      agentIcon: 'codex',
      agentName: 'codex',
      activity: working ? 'working' : 'idle',
      pendingQuestionCount: count,
      isFocused: focused,
      isVisible: true,
      isRunning: true,
      isLive: true,
      lifecycleState: 'running',
      sessionKind: 'terminal',
      row: 0,
      column: 0,
      shortcutLabel: '',
    };
    useSidebarStore.setState((state) => ({ sessionsById: { ...state.sessionsById, [sessionId]: session } }));
  }, [count, working, focused, sessionId]);
  useLayoutEffect(() => {
    return () => {
      useSidebarStore.setState((state) => {
        const { [sessionId]: _preview, ...sessionsById } = state.sessionsById;
        return { sessionsById };
      });
    };
  }, [sessionId]);
  return (
    <div style={{ display: 'grid', gap: 8 }}>
      <div style={{ display: 'flex', gap: 12, alignItems: 'center', fontSize: 12, color: '#bbb' }}>
        <span>
          {working ? 'Working' : 'Finished'} · {count} unanswered
        </span>
        <Button size='sm' variant='outline' onClick={() => setFocused((value) => !value)}>
          {focused ? 'Unfocus sidebar row' : 'Focus sidebar row'}
        </Button>
      </div>
      <div
        className='sidebar-reference-layout'
        data-reference-sidebar='true'
        style={{ width: 320, maxWidth: '100%', background: '#202020', padding: 8 }}
      >
        <DragDropProvider>
          {ready ? (
            <SortableSessionCard
              sessionId={sessionId}
              groupId='async-preview'
              index={0}
              dragDisabled
              vscode={{ postMessage: () => {} }}
              sessionCardSettings={{
                enableSessionParking: false,
                hideBrowserFaviconUntilHover: false,
                hideSessionAgentIconUntilHover: false,
                hoverButtons: normalizeSessionCardHoverButtons(['close']),
                renameSessionOnDoubleClick: false,
                showDebugSessionNumbers: false,
                showLastActiveTime: false,
                showSessionCommandCopyActions: false,
                showSessionDetailsCopyAction: false,
                showTagMenuWhenParking: false,
              }}
            />
          ) : null}
        </DragDropProvider>
      </div>
    </div>
  );
}

function AsyncQuestionsStory({
  theme,
  paneWidth,
  paneHeight,
  working,
}: {
  theme: 'dark' | 'light';
  paneWidth: number;
  paneHeight: number;
  working: boolean;
}) {
  const [revision, setRevision] = useState(0);
  const [identity] = useState(() => crypto.randomUUID());
  const [pendingCount, setPendingCount] = useState(3);
  const transport = useMemo(() => createPreviewTransport(setPendingCount, working), [revision, working]);
  useLayoutEffect(() => setPendingCount(3), [revision, working]);
  return (
    <div style={{ minHeight: '100vh', padding: 20, background: '#161616' }}>
      <div style={{ width: paneWidth, maxWidth: '100%', margin: '0 auto', display: 'grid', gap: 12 }}>
        <Button variant='outline' onClick={() => setRevision((value) => value + 1)}>
          Reset questions
        </Button>
        <SidebarQuestionPreview count={pendingCount} working={working} />
        <div style={{ height: paneHeight, minHeight: 0, overflow: 'hidden', border: '1px solid #444' }}>
          <SessionChatView
            key={`${revision}:${working}`}
            sessionKey={`storybook-async:${identity}:${revision}:${working}`}
            sessionTitle='Codex questions while working'
            agentLabel='codex'
            inputBackend='lexical'
            transport={transport}
            theme={theme}
            working={working}
          />
        </div>
      </div>
    </div>
  );
}

const meta = {
  title: 'Chat/Codex Async Questions',
  component: AsyncQuestionsStory,
  parameters: { layout: 'fullscreen' },
  args: { theme: 'dark', paneWidth: 800, paneHeight: 780, working: true },
  argTypes: {
    theme: { control: 'inline-radio', options: ['dark', 'light'] },
    paneWidth: { control: { type: 'range', min: 320, max: 1200, step: 10 } },
    paneHeight: { control: { type: 'range', min: 400, max: 1000, step: 10 } },
  },
} satisfies Meta<typeof AsyncQuestionsStory>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Working: Story = {};
export const Light: Story = { args: { theme: 'light' } };
export const ShortPane: Story = { args: { paneWidth: 390, paneHeight: 500 } };
export const FinishedWithQuestions: Story = { args: { working: false } };
