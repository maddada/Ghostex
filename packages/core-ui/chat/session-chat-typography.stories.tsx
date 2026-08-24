import type { Meta, StoryObj } from '@storybook/react-vite';
import type { ReactNode } from 'react';
import type { SessionChatMessage } from '../../shared/session-chat';
import { SessionChatMessageList } from './session-chat-message-list';

/*
 * Showcase for the four small typographic deltas the chat.css port had missed:
 * footnotes, inline images, the ordered-list marker gutter, and task-list
 * checkbox alignment.
 *
 * Everything here is mock transcript data — no gxserver, no session, no host
 * bridge — so it runs anywhere Storybook does while exercising exactly the
 * renderer the real chat uses (SessionChatMessageList -> SessionChatMarkdown).
 *
 * Each row is a pair: the pane on the left is what ships now, the pane on the
 * right is the state before this work, reverted with story-local CSS (see
 * PREVIEW_STYLES) rather than a second renderer, so the two panes differ only
 * by the rules that changed.
 *
 * There is deliberately no fifth row for external links. The reference we
 * ported from puts a favicon chip beside them, fetched per domain from
 * google.com/s2/favicons — which would tell a third party every host an agent
 * mentions in a transcript. The chat already shows the full URL in a tooltip,
 * and nothing in this repo can produce a favicon for an arbitrary domain
 * locally, so that one was not taken.
 */

/** A GFM footnote block: refs in the prose, definitions in the trailing section. */
const FOOTNOTES = [
  'Remote attach has two failure modes, and they look identical from the',
  'sidebar: a carrier that dies[^carrier], and a scoped id that does not',
  'match[^scope].',
  '',
  '[^carrier]: The SSH carrier is an ordinary session, so a filter that drops it',
  '    drops the tunnel underneath.',
  '[^scope]: The scoped id is the workspace key.',
].join('\n');

/*
 * Real files behind real URLs rather than inline `data:` ones: react-markdown's
 * default urlTransform keeps only http, https, mailto, xmpp and irc, so a data
 * URL written in markdown never reaches the <img> at all. `?no-inline` is what
 * keeps these three — all under Vite's 4 KB inline limit — from being turned
 * into exactly such a data URL by the bundler.
 */
const CLAUDE_ICON = new URL('../assets/claude.svg?no-inline', import.meta.url).href;
const CODEX_ICON = new URL('../assets/codex.svg?no-inline', import.meta.url).href;
const GEMINI_ICON = new URL('../assets/gemini.svg?no-inline', import.meta.url).href;

const INLINE_IMAGES = [
  'The three agents that took a turn on it:',
  '',
  `![claude](${CLAUDE_ICON}) claude, `,
  `![codex](${CODEX_ICON}) codex, and `,
  `![gemini](${GEMINI_ICON}) gemini.`,
  '',
  'That is one paragraph and one sentence, so it belongs on one line — which is',
  'also the shape of the badge row an agent pastes out of a README.',
].join('\n');

/**
 * An ordered list whose last marker is three digits. Written with `start: 96`
 * so the story stays short — the renderer sizes the gutter from the *last*
 * marker, which is what decides whether the leading digit is clipped.
 */
const LONG_ORDERED_LIST = [
  'Picking up at the tail of the migration checklist:',
  '',
  ...[
    'Drain the queue.',
    'Stop the daemon.',
    'Swap the binary.',
    'Wait for the handshake.',
    'Re-run resume lookup.',
    'Compare the census.',
    'Re-arm the sidebar.',
    'Clear the rollback marker.',
    'Post the result.',
  ].map((text, index) => `${96 + index}. ${text}`),
  '',
  'Nested under one of them, the numbering starts over and so does the gutter:',
  '',
  '1. Outer',
  '   1. Inner, which must not inherit the widened gutter',
  '   2. Inner',
].join('\n');

/**
 * A task list at the top level and a second one nested inside an ordered list
 * whose markers are three digits wide — the case where a checkbox pulled by a
 * constant, or by an inherited gutter, would land somewhere other than the text
 * edge of the list it is actually in.
 */
const TASK_LIST = [
  'Where the release stands:',
  '',
  '- [x] Bundle the remote daemon',
  '- [x] Gate the new selectors',
  '- [ ] Ship the token probe',
  '',
  'And the same list nested inside a three-digit ordered list:',
  '',
  '104. Verify the checklist tail',
  "     - [x] On the nested list's own text edge",
  '     - [ ] A second row to read the alignment against',
  '105. Sign off',
].join('\n');

function assistantTurn(id: string, text: string): SessionChatMessage {
  return {
    blocks: [{ text, type: 'text' }],
    id,
    role: 'assistant',
    source: 'transcript',
    timestamp: 1_000,
  };
}

const FOOTNOTE_MESSAGES = [assistantTurn('footnotes', FOOTNOTES)];
const IMAGE_MESSAGES = [assistantTurn('images', INLINE_IMAGES)];
const ORDERED_MESSAGES = [assistantTurn('ordered', LONG_ORDERED_LIST)];
const TASK_MESSAGES = [assistantTurn('tasks', TASK_LIST)];

/*
 * The "before" column is the same transcript with each of the four rules put
 * back the way it was:
 *
 *  - footnotes had no rules at all, so the definitions rendered as an ordinary
 *    trailing ordered list at body size and the refs as bare inline text (they
 *    were also inert — the link renderer classified "#user-content-fn-1" as an
 *    unopenable href and dropped the anchor, which no stylesheet can undo, so
 *    that half of the delta only shows by clicking the "after" pane's ref);
 *  - `img` inherited the Tailwind preflight's `display: block`;
 *  - the ordered-list gutter was a flat 1.25rem for every list;
 *  - the task-list checkbox was pulled by a hardcoded -1.1rem.
 */
const PREVIEW_STYLES = `
  /* Story scaffolding, applied to both panes so the comparison stays honest:
     the agent icons standing in for badge images are viewBox-only SVGs with no
     intrinsic size, so an <img> would render them at the 300px default. Height
     is not what either pane is demonstrating. */
  .ghostex-chat-typography-story .ghostex-chat-markdown img {
    height: 1.15em;
    width: auto;
  }
  [data-chat-typography-preview="before"] .ghostex-chat-markdown section[data-footnotes] {
    border-top: none;
    color: inherit;
    font-size: inherit;
    margin-top: 0.65rem;
    padding-top: 0;
  }
  [data-chat-typography-preview="before"] .ghostex-chat-markdown section[data-footnotes] ol {
    margin: 0.65rem 0;
  }
  [data-chat-typography-preview="before"]
    .ghostex-chat-markdown :is(a[data-footnote-ref], a[data-footnote-backref]) {
    border-radius: 0;
    display: inline;
    font-size: inherit;
    font-weight: inherit;
    min-width: 0;
  }
  [data-chat-typography-preview="before"]
    .ghostex-chat-markdown img:not(.ghostex-chat-inline-image) {
    display: block;
  }
  [data-chat-typography-preview="before"] .ghostex-chat-markdown ol {
    --chat-list-gutter: 1.25rem !important;
  }
  [data-chat-typography-preview="before"] .ghostex-chat-markdown input[type="checkbox"] {
    margin-left: -1.1rem;
  }
`;

function ChatPane({
  before = false,
  messages,
  theme,
}: {
  before?: boolean;
  messages: SessionChatMessage[];
  theme: 'dark' | 'light';
}) {
  return (
    <div
      className='ghostex-session-chat-scope flex min-h-0 flex-1 flex-col bg-background text-foreground'
      data-chat-theme={theme}
      data-chat-typography-preview={before ? 'before' : undefined}
    >
      <SessionChatMessageList
        hasMore={false}
        isWorking={false}
        loadingEarlier={false}
        messages={messages}
        onLoadEarlier={() => undefined}
        verboseMode={false}
      />
    </div>
  );
}

/*
 * Panes are pinned to a sidebar-ish width rather than left to fill the window.
 * The transcript column in the real chat is narrow, and half of what these
 * deltas are about — a marker that escapes its gutter, a badge row that has to
 * fit on one line — only reads at the width the reader actually has.
 */
function Pane({ children, label }: { children: ReactNode; label: string }) {
  return (
    <div
      className='flex min-h-0 flex-none flex-col overflow-hidden rounded-lg border border-white/10'
      style={{ width: '21rem' }}
    >
      <div className='px-3 py-1.5 font-mono text-[11px] uppercase tracking-wide text-white/60'>{label}</div>
      {children}
    </div>
  );
}

function SessionChatTypographyStory({ theme }: { theme: 'dark' | 'light' }) {
  const rows: {
    height: string;
    labels: [string, string];
    messages: SessionChatMessage[];
    title: string;
  }[] = [
    {
      height: '22rem',
      labels: ['after — ruled, muted, chip refs', 'before — unstyled trailing list'],
      messages: FOOTNOTE_MESSAGES,
      title: '1. Footnotes',
    },
    {
      height: '23rem',
      labels: ['after — inline-block, one row', 'before — preflight block, stacked'],
      messages: IMAGE_MESSAGES,
      title: '2. Inline images',
    },
    {
      height: '34rem',
      labels: ['after — gutter fits the 3-digit marker', 'before — marker escapes the text edge'],
      messages: ORDERED_MESSAGES,
      title: '3. Ordered-list gutter',
    },
    {
      height: '23rem',
      labels: ['after — pulled the real gutter', 'before — hardcoded -1.1rem'],
      messages: TASK_MESSAGES,
      title: '4. Task-list checkboxes',
    },
  ];
  return (
    <div className='ghostex-chat-typography-story flex flex-col gap-3 bg-[#0a0a0a] p-2' style={{ minHeight: '100vh' }}>
      <style>{PREVIEW_STYLES}</style>
      {rows.map((row) => (
        <div className='flex flex-col gap-1' key={row.title}>
          <div className='px-1 font-mono text-[11px] uppercase tracking-wide text-white/60'>{row.title}</div>
          <div className='flex gap-2' style={{ height: row.height }}>
            <Pane label={row.labels[0]}>
              <ChatPane messages={row.messages} theme={theme} />
            </Pane>
            <Pane label={row.labels[1]}>
              <ChatPane before messages={row.messages} theme={theme} />
            </Pane>
          </div>
        </div>
      ))}
    </div>
  );
}

const meta = {
  argTypes: { theme: { control: 'inline-radio', options: ['dark', 'light'] } },
  component: SessionChatTypographyStory,
  parameters: { layout: 'fullscreen' },
  title: 'Chat/Typography',
} satisfies Meta<typeof SessionChatTypographyStory>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Dark: Story = { args: { theme: 'dark' } };

export const Light: Story = { args: { theme: 'light' } };
