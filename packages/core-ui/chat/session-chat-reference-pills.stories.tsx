import type { Meta, StoryObj } from '@storybook/react-vite';
import { useRef, useState } from 'react';
import { expect, fireEvent, waitFor } from 'storybook/test';
import { SessionChatComposer, type SessionChatComposerHandle } from './session-chat-composer';

const SESSION_KEY = 'storybook-reference-pills';
const DRAFT = [
  '[Image #1](/Users/madda/.local/share/ghostex/i/1787798799853.png)',
  '[Image #2](/Users/madda/.local/share/ghostex/i/1787802689390.png)',
  '[File #1](/Users/madda/dev/_active/Ghostex/packages/core-ui/chat/session-chat-monaco-input.tsx:491)',
  '[Folder #1](/Users/madda/dev/_active/Ghostex/packages/core-ui/chat)',
  '[$ghostex-browser-use](/Users/madda/.agents/skills/ghostex-browser-use/SKILL.md)',
].join(' ');

function SessionChatReferencePillsStory() {
  const composerRef = useRef<SessionChatComposerHandle | null>(null);
  const [sentMessages, setSentMessages] = useState<string[]>([]);
  const [sessionKey] = useState(() => {
    window.localStorage.setItem(`ghostex.sessionChat.draft.${SESSION_KEY}`, DRAFT);
    return SESSION_KEY;
  });

  return (
    <div className='ghostex-session-chat-scope flex h-screen flex-col justify-end bg-background p-6 text-foreground'>
      <button
        data-testid='insert-reference-draft'
        hidden
        onClick={() => composerRef.current?.insertTypedText(DRAFT)}
        type='button'
      />
      <output data-testid='sent-reference-drafts' hidden>
        {JSON.stringify(sentMessages)}
      </output>
      <div className='mx-auto' style={{ width: 220 }}>
        <SessionChatComposer
          isWorking={false}
          monacoVsBaseUrl='/monaco/vs'
          onInterrupt={() => undefined}
          onSend={(message) => setSentMessages((current) => [...current, message])}
          ref={composerRef}
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
      expect(canvasElement.querySelectorAll('.ghostex-chat-reference-pill')).toHaveLength(5);
    });

    for (const pill of canvasElement.querySelectorAll<HTMLElement>('.ghostex-chat-reference-pill')) {
      expect(pill.getBoundingClientRect().width).toBeGreaterThan(0);
      expect(pill.getClientRects()).toHaveLength(1);
      expect(pill.title.startsWith('/Users/madda/')).toBe(true);
    }

    for (const token of canvasElement.querySelectorAll<HTMLElement>('.ghostex-chat-composer-reference-source')) {
      expect(token.getBoundingClientRect().width).toBeLessThan(0.5);
    }
    expect(canvasElement.querySelector('.view-lines')?.textContent).not.toMatch(/[\ue000-\uf8ff]/u);

    const sentDrafts = canvasElement.querySelector<HTMLOutputElement>('[data-testid="sent-reference-drafts"]');
    const sendButton = canvasElement.querySelector<HTMLButtonElement>('[aria-label="Send"]');
    if (!sentDrafts || !sendButton) {
      throw new Error('The reference-pill Storybook harness did not render its send controls.');
    }
    fireEvent.click(sendButton);
    await waitFor(() => {
      expect(canvasElement.querySelectorAll('.ghostex-chat-reference-pill')).toHaveLength(0);
      expect(sentDrafts.textContent).toBe(JSON.stringify([DRAFT]));
    });

    const insertButton = canvasElement.querySelector<HTMLButtonElement>('[data-testid="insert-reference-draft"]');
    if (!insertButton) {
      throw new Error('The reference-pill Storybook harness did not render its insertion control.');
    }
    fireEvent.click(insertButton);
    await waitFor(() => {
      expect(canvasElement.querySelectorAll('.ghostex-chat-reference-pill')).toHaveLength(5);
    });

    const secondSendButton = canvasElement.querySelector<HTMLButtonElement>('[aria-label="Send"]');
    if (!secondSendButton) {
      throw new Error('The reference-pill Storybook harness did not restore its send control.');
    }
    fireEvent.click(secondSendButton);
    await waitFor(() => {
      expect(canvasElement.querySelectorAll('.ghostex-chat-reference-pill')).toHaveLength(0);
      expect(sentDrafts.textContent).toBe(JSON.stringify([DRAFT, DRAFT]));
    });

    fireEvent.click(insertButton);
    await waitFor(() => {
      expect(canvasElement.querySelectorAll('.ghostex-chat-reference-pill')).toHaveLength(5);
    });
  },
};
