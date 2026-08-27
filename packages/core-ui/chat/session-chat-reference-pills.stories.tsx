import type { Meta, StoryObj } from '@storybook/react-vite';
import { useState } from 'react';
import { expect, waitFor } from 'storybook/test';
import { SessionChatComposer } from './session-chat-composer';

const SESSION_KEY = 'storybook-reference-pills';
const DRAFT = [
  '[Image #1](/Users/madda/.local/share/ghostex/i/1787798799853.png)',
  '[File #1](/Users/madda/dev/_active/Ghostex/packages/core-ui/chat/session-chat-monaco-input.tsx:491)',
  '[Folder #1](/Users/madda/dev/_active/Ghostex/packages/core-ui/chat)',
  '[$ghostex-browser-use](/Users/madda/.agents/skills/ghostex-browser-use/SKILL.md)',
].join(' ');

function SessionChatReferencePillsStory() {
  const [sessionKey] = useState(() => {
    window.localStorage.setItem(`ghostex.sessionChat.draft.${SESSION_KEY}`, DRAFT);
    return SESSION_KEY;
  });

  return (
    <div className='ghostex-session-chat-scope flex h-screen flex-col justify-end bg-background p-6 text-foreground'>
      <div className='mx-auto' style={{ width: 220 }}>
        <SessionChatComposer
          isWorking={false}
          monacoVsBaseUrl='/monaco/vs'
          onInterrupt={() => undefined}
          onSend={() => undefined}
          sendOnEnter
          sessionKey={sessionKey}
          theme='dark'
        />
      </div>
    </div>
  );
}

const meta = {
  component: SessionChatReferencePillsStory,
  parameters: { layout: 'fullscreen' },
  title: 'Chat/Composer reference pills',
} satisfies Meta<typeof SessionChatReferencePillsStory>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Monaco: Story = {
  play: async ({ canvasElement }) => {
    await waitFor(() => {
      expect(canvasElement.querySelectorAll('.ghostex-chat-reference-pill')).toHaveLength(4);
    });

    for (const pill of canvasElement.querySelectorAll<HTMLElement>('.ghostex-chat-reference-pill')) {
      expect(pill.getBoundingClientRect().width).toBeGreaterThan(0);
      expect(pill.getClientRects()).toHaveLength(1);
      expect(pill.title.startsWith('/Users/madda/')).toBe(true);
    }
  },
};
