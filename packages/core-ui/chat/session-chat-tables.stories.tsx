import type { Meta, StoryObj } from '@storybook/react-vite';
import { useEffect, useRef, type ReactNode } from 'react';
import type { SessionChatMessage } from '../../shared/session-chat';
import { SessionChatHostLinksProvider } from './session-chat-links';
import { SessionChatMessageList } from './session-chat-message-list';

/*
 * Showcase for table chrome in Session Chat markdown.
 *
 * Everything here is mock transcript data — no gxserver, no session, no host
 * bridge — so the story runs anywhere Storybook does and exercises exactly the
 * renderer the real chat uses (SessionChatMessageList -> SessionChatMarkdown ->
 * MarkdownTable -> session-chat-table-clipboard.ts).
 *
 * The one host-shaped thing in it is `openFile`, because a file-path chip only
 * exists on a host that has an editor to open a file in. The "rich cells" pane
 * supplies one so a chip can be seen surviving inside a table cell.
 */

/** Eight columns: far wider than the transcript column, so the wrapper scrolls. */
const WIDE_TABLE = [
  'The release matrix, which is wider than any transcript column:',
  '',
  '| Surface | Language | Entry point | Bundler | Ships | Signed | Auto-update | Owner |',
  '| --- | --- | --- | ---: | :---: | :---: | --- | --- |',
  '| gpui desktop | Rust + React | `apps/desktop/src/main.rs` | vite | macOS, Linux | yes | Sparkle | desktop |',
  '| Web app | TypeScript | `apps/web/src/main.tsx` | vite | static bundle | n/a | page reload | web |',
  '| Mobile | TypeScript | `mobile/index.js` | metro | Android | yes | Play Store | mobile |',
  '| gxserver | Rust | `server/src/main.rs` | cargo | daemon | no | bundled | server |',
].join('\n');

/** Cells with far more prose than a column can hold: the collapse cap's reason. */
const LONG_CELL_TABLE = [
  'The three things that were wrong, at the length an agent writes them:',
  '',
  '| Symptom | What was happening |',
  '| --- | --- |',
  '| Columns did not line up | The stylesheet put `display: block` on the table itself to get horizontal overflow, which throws away the table layout algorithm with it, so every row sized its own cells and the header stopped describing the column under it. |',
  '| One cell decided the shape | A single essay-length cell — the kind an agent writes when it explains a failure inline instead of in prose, stack frame and path included — owned its column until every other column was off-screen and unreachable. |',
  '| Expanding threw you off | Lifting the cap re-runs the column algorithm against newly wrapped text, so every column lands somewhere else and the row you were reading moves out from under you. |',
].join('\n');

/** The common case: nothing to expand, nothing off-screen, so no toggle. */
const SMALL_TABLE = [
  'Two by two, which is most of the tables an agent writes:',
  '',
  '| Setting | Value |',
  '| --- | --- |',
  '| Theme | dark |',
  '| Wrap | off |',
  '',
  'No expand toggle appears — there is nothing clipped to expand.',
].join('\n');

/** Inline code, a link, and a file-path chip, all inside cells. */
const RICH_TABLE = [
  'Cell contents keep every inline behaviour they have in prose:',
  '',
  '| Kind | Cell | Copies as |',
  '| --- | --- | --- |',
  '| Inline code | Run `bun run build` before `release:verify` | backticked, unchanged |',
  '| Web link | [remark-gfm](https://github.com/remarkjs/remark-gfm) | `[label](href)` |',
  '| File-path chip | `packages/core-ui/chat/session-chat-markdown.tsx:463` | the full path, not the short chip label |',
  '| Emphasis | **bold**, *italic*, ~~struck~~ | with their markers |',
  '| A pipe | a \\| b | escaped |',
].join('\n');

function assistantTurn(id: string, text: string, timestamp: number): SessionChatMessage {
  return {
    blocks: [{ text, type: 'text' }],
    id,
    role: 'assistant',
    source: 'transcript',
    timestamp,
  };
}

const WIDE_MESSAGES = [assistantTurn('wide', WIDE_TABLE, 1_000)];
const LONG_MESSAGES = [assistantTurn('long', LONG_CELL_TABLE, 1_000)];
const SMALL_MESSAGES = [assistantTurn('small', SMALL_TABLE, 1_000)];
const RICH_MESSAGES = [assistantTurn('rich', RICH_TABLE, 1_000)];

/*
 * The "before" column is the same transcript with the table styling reverted to
 * exactly what shipped before this work: `display: block` on the table itself,
 * carrying its own overflow, with no cell cap and no actions.
 *
 * What that costs is visible side by side. A block table is not a table box, so
 * `width: 100%` stops reaching the rows and the columns size themselves to
 * their longest content — one essay-length cell then owns the table and pushes
 * every other column out of reach, with no way to ask for less.
 */
const PREVIEW_STYLES = `
  [data-chat-table-preview="before"] .ghostex-chat-markdown-table-actions {
    display: none;
  }
  [data-chat-table-preview="before"] .ghostex-chat-markdown-table-scroll {
    overflow-x: visible;
  }
  [data-chat-table-preview="before"] .ghostex-chat-markdown table {
    display: block;
    font-size: inherit;
    overflow-wrap: anywhere;
    overflow-x: auto;
    word-break: break-word;
  }
  [data-chat-table-preview="before"] .ghostex-chat-markdown thead th,
  [data-chat-table-preview="before"] .ghostex-chat-markdown tbody td {
    border-bottom: 1px solid var(--border);
    vertical-align: baseline;
  }
  [data-chat-table-preview="before"]
    .ghostex-chat-markdown-table[data-expanded="false"] :is(th, td) {
    max-width: none;
    overflow: visible;
    text-overflow: clip;
    white-space: normal;
  }
`;

function ChatPane({
  before = false,
  messages,
  theme,
  withEditorSurface = false,
}: {
  before?: boolean;
  messages: SessionChatMessage[];
  theme: 'dark' | 'light';
  withEditorSurface?: boolean;
}) {
  return (
    <SessionChatHostLinksProvider
      {...(withEditorSurface
        ? {
            links: {
              openFile: (path, position) => {
                // eslint-disable-next-line no-console
                console.log('openFile', path, position ?? '(no position)');
              },
            },
          }
        : {})}
    >
      <div
        className='ghostex-session-chat-scope flex min-h-0 flex-1 flex-col bg-background text-foreground'
        data-chat-table-preview={before ? 'before' : undefined}
        data-chat-theme={theme}
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
    </SessionChatHostLinksProvider>
  );
}

/*
 * Presses the pane's real expand toggle once on mount rather than faking the
 * expanded state with story CSS — the column pinning only happens on that
 * click, so a faked pane would not show what expanding actually looks like.
 */
function AutoExpanded({ children }: { children: ReactNode }) {
  const ref = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      ref.current?.querySelector<HTMLButtonElement>('button[aria-label="Expand cells"]')?.click();
    });
    return () => window.cancelAnimationFrame(frame);
  }, []);
  return (
    <div className='flex min-h-0 flex-1 flex-col' ref={ref}>
      {children}
    </div>
  );
}

function PaneLabel({ children }: { children: ReactNode }) {
  return <div className='px-3 py-1.5 font-mono text-[11px] uppercase tracking-wide text-white/60'>{children}</div>;
}

function Pane({ children, label }: { children: ReactNode; label: string }) {
  return (
    <div className='flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-white/10'>
      <PaneLabel>{label}</PaneLabel>
      {children}
    </div>
  );
}

function SessionChatTablesStory({ narrow = false, theme }: { narrow?: boolean; theme: 'dark' | 'light' }) {
  return (
    <div className='flex h-screen min-h-[46rem] flex-col gap-2 bg-[#0a0a0a] p-2'>
      <style>{PREVIEW_STYLES}</style>
      <div className='flex min-h-0 flex-1 gap-2'>
        <Pane label='after — capped, ellipsized, real table layout'>
          <ChatPane messages={LONG_MESSAGES} theme={theme} />
        </Pane>
        <Pane label='before — display:block, uncapped cells, no chrome'>
          <ChatPane before messages={LONG_MESSAGES} theme={theme} />
        </Pane>
      </div>
      <div className='flex min-h-0 flex-1 gap-2'>
        <Pane label='expanded — cells wrap, columns hold'>
          <AutoExpanded>
            <ChatPane messages={LONG_MESSAGES} theme={theme} />
          </AutoExpanded>
        </Pane>
        <Pane label='wide — the wrapper scrolls'>
          <ChatPane messages={WIDE_MESSAGES} theme={theme} />
        </Pane>
        {narrow ? null : (
          <Pane label='2x2 — chrome stays quiet'>
            <ChatPane messages={SMALL_MESSAGES} theme={theme} />
          </Pane>
        )}
        <Pane label='rich — code, link, path chip'>
          <ChatPane messages={RICH_MESSAGES} theme={theme} withEditorSurface />
        </Pane>
      </div>
    </div>
  );
}

const meta = {
  argTypes: {
    narrow: { control: 'boolean' },
    theme: { control: 'inline-radio', options: ['dark', 'light'] },
  },
  component: SessionChatTablesStory,
  parameters: { layout: 'fullscreen' },
  title: 'Chat/Tables',
} satisfies Meta<typeof SessionChatTablesStory>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Dark: Story = { args: { theme: 'dark' } };

export const Light: Story = { args: { theme: 'light' } };

/**
 * Three panes instead of four, for viewing in a narrow window: the transcript
 * column in the real sidebar is not wide, and the cell cap follows it.
 */
export const NarrowDark: Story = { args: { narrow: true, theme: 'dark' } };
