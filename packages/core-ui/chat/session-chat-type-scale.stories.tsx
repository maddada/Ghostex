import type { Meta, StoryObj } from '@storybook/react-vite';
import type { SessionChatMessage } from '../../shared/session-chat';
import { SessionChatMessageList } from './session-chat-message-list';

/*
 * CDXC:SessionChatOneSize 2026-08-22:
 * The transcript's type scale — one size, and what it replaced.
 *
 * Every line of reading content is now 0.875rem/14px at 1.625: the answer, the
 * user's message, the reasoning rows, the tool-run headings, the work toggles,
 * a table cell, a footnote. The lanes are told apart by COLOUR and weight, not
 * by size. Headings stay stepped, because a long answer needs structure.
 *
 * Code is the single exception and is expressed as a ratio (`--chat-code-size`,
 * 0.9em) rather than a second tier. A monospace face set at the same nominal
 * size as the sans around it reads visibly larger — the mono x-height at 14px
 * is 7.66px against DM Sans's 7.06px — so matching the number is exactly what
 * makes the two look mismatched. Being relative, a chip in a sentence and a
 * fenced block in a work row both track whatever they sit in.
 *
 * The "before" pane restores what shipped: four absolute tiers (14 / 13 / 12 /
 * 11) and two different leadings for the same 14px text. It is reverted with
 * story-local CSS in BEFORE_STYLES rather than by a second renderer, so the two
 * panes differ only by the rules that changed.
 *
 * The pattern to look at is the reasoning → tool run → answer alternation near
 * the end: three objects under one shared bullet that used to be three
 * different sizes.
 *
 * Everything here is mock transcript data — no gxserver, no session, no host
 * bridge — so it runs anywhere Storybook does while exercising exactly the
 * renderer the real chat uses.
 *
 * The story's own scaffolding is plain CSS, not tailwind utilities: the
 * generated sheet only carries the classes the app already uses, so a story
 * that reached for a new one would render unstyled until someone rebuilt it.
 */

const USER_TURN = [
  'the model/effort pills take a second to show up when I open chat, and the',
  'type in here looks bold/smaller/bigger at random. can you make the',
  'transcript read as one thing?',
].join('\n');

/** An answer: prose, with the headings, chips and code a real one uses. */
const ANSWER = [
  'The transcript was running on four absolute sizes.',
  '',
  '## What it runs on now',
  '',
  'One. Every line of reading content is `14px / 1.625` — this answer, your',
  'message, a reasoning paragraph, a tool-run heading, a table cell.',
  '',
  '### Why the old scale read as random',
  '',
  '- `.ghostex-chat-markdown` never declared a line height, so the answer took',
  '  `20px` while your bubble took `22.75px` — the same text, two rhythms',
  '- a `13px` tier held the reasoning rows and the completed-work toggle,',
  '  belonging to neither prose nor the work rows',
  '- reasoning and answers alternate under the *same* bullet, so a 1px',
  '  difference between them read as a mistake rather than a level',
  '',
  'Lanes are told apart by colour now, not size. Code is the one exception, and',
  'it is a ratio rather than a tier:',
  '',
  '```css',
  '.ghostex-session-chat-scope {',
  '  --chat-code-size: 0.9em;',
  '}',
  '```',
  '',
  "> Monospace set at the sans's own size reads larger — 7.66px of x-height",
  '> against 7.06px — so matching the number is what makes them look mismatched.',
].join('\n');

/*
 * The tail is deliberately the reasoning/tool/answer alternation rather than
 * the long answer above: the transcript auto-scrolls to its end, and this
 * alternation under one shared bullet is the exact pattern the scale was
 * failing on.
 */
const REASONING_ONE = [
  '**Planning the type audit**',
  '',
  'Two separate complaints. The pills are a latency problem and the type is a',
  'scale problem, so I will measure the live surface before changing anything.',
].join('\n');

const REASONING_TWO = [
  '**Reading the computed styles back**',
  '',
  "The answer's paragraphs come back at 20px leading and the user's bubble at",
  '22.75px, which is the same 14px text set two ways.',
].join('\n');

const REASONING_THREE = [
  '**Deciding what the reasoning lane is**',
  '',
  'Reasoning renders as bulleted markdown in the same flow as the answer, under',
  'the same bullet — so it is the same kind of object and takes the same size.',
  'Colour is what separates the lanes.',
].join('\n');

function turn(id: string, role: SessionChatMessage['role'], text: string, timestamp: number): SessionChatMessage {
  return {
    blocks: [{ text, type: 'text' }],
    id,
    role,
    source: 'transcript',
    timestamp,
  };
}

function toolRun(id: string, name: string, input: unknown, output: string, timestamp: number): SessionChatMessage {
  return {
    blocks: [
      { input, name, type: 'tool-call' },
      { output, type: 'tool-result' },
    ],
    id,
    role: 'tool',
    source: 'transcript',
    timestamp,
  };
}

const MESSAGES: SessionChatMessage[] = [
  turn('user-1', 'user', USER_TURN, 1_000),
  turn('assistant-1', 'assistant', ANSWER, 2_000),
  turn('reasoning-1', 'reasoning', REASONING_ONE, 3_000),
  toolRun(
    'tool-1',
    'Bash',
    { command: "rg -n 'font-size' packages/core-ui/styles/chat.css" },
    [
      'packages/core-ui/styles/chat.css:322:  font-size: 0.8125rem;',
      'packages/core-ui/styles/chat.css:461:  font-size: 0.8125rem;',
      'packages/core-ui/styles/chat.css:550:  font-size: 0.75rem;',
      'packages/core-ui/styles/chat.css:642:  font-size: 0.6875rem;',
      'packages/core-ui/styles/chat.css:1273: font-size: 0.75rem;',
    ].join('\n'),
    4_000
  ),
  turn(
    'assistant-2',
    'assistant',
    'Four absolute tiers in one sheet. Measuring the live surface before I touch it.',
    5_000
  ),
  turn('reasoning-2', 'reasoning', REASONING_TWO, 6_000),
  toolRun(
    'tool-2',
    'Read',
    { file_path: 'packages/core-ui/styles/chat.css', limit: 40, offset: 1069 },
    [
      'agent md p:      14px   / lh 20px    / w 400',
      'user bubble p:   14px   / lh 22.75px / w 400',
      'thinking row:    13px   / lh 19.5px  / w 400',
      'work trigger:    12px   / lh 20px    / w 400',
      'inline code:     12px   mono',
      'fenced code:     13px   mono',
    ].join('\n'),
    7_000
  ),
  turn('assistant-3', 'assistant', 'Confirmed: the same 14px text was being set at two different leadings.', 8_000),
  turn('reasoning-3', 'reasoning', REASONING_THREE, 9_000),
  turn(
    'assistant-4',
    'assistant',
    'One size now. Every bullet above is the same 14px — only the colour moves.',
    10_000
  ),
];

/*
 * The "before" pane, reverted rule by rule to what shipped:
 *
 *  - the markdown root declared no line height, so prose fell back to the
 *    `text-sm` utility's own 20px, while the user's bubble kept 1.625 from its
 *    own wrapper — the same 14px text at two rhythms;
 *  - the reasoning lane and the completed-work toggle sat at 0.8125rem (13px),
 *    a tier that belonged to neither prose nor the work lane;
 *  - the work rows sat at 0.75rem (12px) and their detail at 0.6875rem (11px);
 *  - code was pinned to absolute sizes of its own — 0.75rem inline, 0.8125rem
 *    fenced — instead of tracking the text around it.
 */
const STORY_STYLES = `
  .chat-type-scale-story {
    background: #050505;
    display: flex;
    flex-direction: column;
    height: 100vh;
    min-height: 0;
  }
  .chat-type-scale-story__panes {
    display: flex;
    flex: 1 1 auto;
    min-height: 0;
  }
  .chat-type-scale-story__pane {
    display: flex;
    flex: 1 1 0;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }
  .chat-type-scale-story__pane + .chat-type-scale-story__pane {
    border-left: 1px solid #262626;
  }
  .chat-type-scale-story__label {
    color: #737373;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.14em;
    padding: 10px 14px;
    text-transform: uppercase;
  }
  .chat-type-scale-story__surface {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
  }

  [data-chat-type-scale="before"] .ghostex-chat-markdown {
    line-height: 1.25rem;
  }
  [data-chat-type-scale="before"] .ghostex-chat-user-bubble .ghostex-chat-markdown {
    line-height: 1.625;
  }
  [data-chat-type-scale="before"] .ghostex-chat-thinking-row {
    font-size: 0.8125rem;
    line-height: 1.5;
  }
  [data-chat-type-scale="before"] .ghostex-chat-completed-work-trigger {
    font-size: 0.8125rem;
    line-height: 1.25rem;
  }
  [data-chat-type-scale="before"] :is(
      .ghostex-chat-work-trigger,
      .ghostex-chat-tool-run-toggle
    ) {
    font-size: 0.75rem;
    line-height: 1.25rem;
  }
  [data-chat-type-scale="before"] :is(
      .ghostex-chat-work-detail .ghostex-chat-markdown,
      .ghostex-chat-tool-body,
      .ghostex-chat-tool-body-label,
      .ghostex-chat-diff
    ) {
    font-size: 0.6875rem;
  }
  [data-chat-type-scale="before"] .ghostex-chat-markdown :not(pre) > code {
    font-size: 0.75rem;
  }
  [data-chat-type-scale="before"] .ghostex-chat-markdown pre code {
    font-size: 0.8125rem;
  }
  [data-chat-type-scale="before"] .ghostex-chat-markdown table {
    font-size: 0.8125rem;
  }
`;

function ChatPane({ before = false, label }: { before?: boolean; label: string }) {
  return (
    <div className='chat-type-scale-story__pane'>
      <div className='chat-type-scale-story__label'>{label}</div>
      <div
        className='ghostex-session-chat-scope chat-type-scale-story__surface'
        data-chat-theme='dark'
        data-chat-type-scale={before ? 'before' : undefined}
      >
        {/* Verbose so the completed-work section starts open: the reasoning
            rows and tool runs — two of the three lanes — live inside it, and
            they are the point of the comparison. */}
        <SessionChatMessageList
          hasMore={false}
          isWorking={false}
          loadingEarlier={false}
          messages={MESSAGES}
          onLoadEarlier={() => undefined}
          verboseMode
        />
      </div>
    </div>
  );
}

const meta: Meta = {
  title: 'Session Chat/Type scale',
  parameters: { layout: 'fullscreen' },
};

export default meta;

/** The two panes side by side, same transcript, same renderer. */
export const BeforeAndAfter: StoryObj = {
  render: () => (
    <div className='chat-type-scale-story'>
      <style>{STORY_STYLES}</style>
      <div className='chat-type-scale-story__panes'>
        <ChatPane before label='before — four absolute tiers, two rhythms' />
        <ChatPane label='after — one size, code at 0.9em' />
      </div>
    </div>
  ),
};

/** The shipped scale alone, full width, on the transcript's own page colour. */
export const Shipped: StoryObj = {
  render: () => (
    <div className='chat-type-scale-story'>
      <style>{STORY_STYLES}</style>
      <div className='chat-type-scale-story__panes'>
        <ChatPane label='shipped' />
      </div>
    </div>
  ),
};
