import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ReactNode } from "react";
import type { SessionChatMessage } from "../../shared/session-chat";
import { SessionChatHostLinksProvider } from "./session-chat-links";
import { SessionChatMessageList } from "./session-chat-message-list";

/*
 * Showcase for file-path chips in Session Chat markdown.
 *
 * Everything here is mock transcript data — no gxserver, no session, no host
 * bridge — so the story runs anywhere Storybook does and exercises exactly the
 * renderer the real chat uses (SessionChatMessageList -> SessionChatMarkdown ->
 * session-chat-file-paths.ts).
 *
 * The only host-shaped thing in it is `openFile`: a chip exists because the
 * host has an editor to open a file in, so the "after" column supplies one and
 * the "before" column does not. That is not a story trick — it is the real
 * contract, and the before column is precisely what the web app and the phone
 * still render today.
 */

const PATH_SHAPES = [
  "Here is where each piece of that lives.",
  "",
  "- Bare repo-relative path: `apps/desktop/src/main.rs`",
  "- With a line: `packages/core-ui/styles/chat.css:913`",
  "- With a line and a column: `apps/desktop/src/cef/shell.rs:42:8`",
  "- Deeply nested: `server/src/ghostex_cli/actions.rs:117`",
  "- Absolute: `/Users/madda/dev/_active/Ghostex/AGENTS.md`",
  "- Home-relative: `~/.claude/settings.json`",
  "- Explicitly relative: `./scripts/build-mobile-find.mjs`",
  "- A parent hop: `../../shared/session-chat.ts:88`",
  "- A dotfile in a folder: `apps/desktop/.cargo/config.toml`",
  "- Conventional extensionless names, but only with a line: `Makefile:12`",
  "- Windows, for the remote case: `C:\\repo\\gxserver\\service.rs:9`",
  "",
  "In running prose they behave the same way: the chat body lives in",
  "`packages/core-ui/chat/session-chat-view.tsx`, the markdown renderer under it is",
  "`packages/core-ui/chat/session-chat-markdown.tsx:463`, and the styles both of them",
  "read come from `packages/core-ui/styles/chat.css`.",
].join("\n");

const SAME_BASENAMES = [
  "There are two of each of these, and the chip shows the whole path, so the",
  "difference is on screen rather than one hover away:",
  "",
  "1. `packages/core-ui/styles/chat.css:913` is the shared sidebar stylesheet.",
  "2. `apps/web/src/styles/chat.css` is the web app's own copy.",
  "",
  "Same for the two mains and the two mounts:",
  "",
  "- `apps/desktop/src/main.rs` vs `server/src/main.rs`",
  "- `apps/desktop/sidebar/chat-main.tsx:586` vs `apps/mobile/views/chat/chat-main.tsx:141`",
].join("\n");

/*
 * The overflow panel. A transcript column is around 385px and a real path is
 * routinely longer than that, so this is where the layout has to prove itself:
 * the filename and the coordinates stay whole and the directories in the middle
 * are what give way. Two long chips in one sentence is the case that decides
 * whether a paragraph of them still reads as prose.
 */
const LONG_PATHS = [
  "These are longer than the column, one per line:",
  "",
  "- `server/src/session_chat_terminal_activity.rs:1284:37`",
  "- `/Users/madda/dev/_active/Ghostex/packages/core-ui/chat/session-chat-terminal-notice-card.tsx:212`",
  "- `node_modules/@tabler/icons-react/dist/esm/tabler-icons-react.mjs`",
  "- `apps/web/src/connections/gxserver-client-presentation-cache.ts:77:14`",
  "",
  "And here they are in running prose, several to a paragraph: the notice card",
  "in `packages/core-ui/chat/session-chat-terminal-notice-card.tsx:212` reads its screen",
  "classifier from `server/src/session_chat_terminal_activity.rs:1284:37`,",
  "which the CLI also calls from `server/src/ghostex_cli/sessions.rs:940`,",
  "and all three agree with `packages/shared/gxserver-presentation-sidebar-projection.ts`.",
  "Short ones — `src/main.rs`, `AGENTS.md` — sit between them unchanged.",
].join("\n");

/*
 * A table caps each cell at min(24rem, 60cqw) and, collapsed, ellipsizes the
 * cell itself. A chip inside one has to shrink to the cell rather than be
 * clipped by it, so this is the second half of the overflow story.
 */
const TABLE_WITH_PATHS = [
  "| Surface | Entry point | Styles |",
  "| --- | --- | --- |",
  "| gpui | `apps/desktop/sidebar/chat-main.tsx:586` | `packages/core-ui/styles/chat.css:913` |",
  "| web | `apps/web/src/connections/gxserver-client-presentation-cache.ts:77:14` | `apps/web/src/styles/chat.css` |",
  "| mobile | `apps/mobile/views/chat/session-chat-main.tsx:141` | `packages/core-ui/styles/chat.css` |",
  "",
  "Collapsed, the cell ellipsis and the chip's own truncation have to agree.",
  "Expanded, the cells wrap and the chips get their full width back.",
].join("\n");

const NOT_PATHS = [
  "None of the inline code below is a file reference, and none of it may turn",
  "into a chip. This is the panel that matters: a chip that opens nothing is",
  "worse than a grey span.",
  "",
  "**Commands and flags.** Run `npm install`, then `bun run build`. Pass",
  "`--flag`, `-g '!.dependencies/ghostty/**'`, or `--skip-android`. Kill it with `^g`, and",
  "note that `Ctrl+T` is reserved by the browser.",
  "",
  "**Method calls and identifiers.** `Array.map`, `foo.bar()`, `React.useState`,",
  "`node.meta`, `window.getSelection`, `array[0]`, `key=value`, `$HOME`,",
  "`<T>`, `IconFileCode`.",
  "",
  "**Slashed things that are not paths.** `origin/main`, `feature/my-branch`,",
  "`and/or`, `client/server`, `A/B`, `24/7`, `text/plain`, `application/json`,",
  "`image/svg+xml`, `@tabler/icons-react`, `@types/node`, `src/utils`,",
  "`hooks/useX`, `s/foo/bar/g`, `src/**/*.ts`, `release/v1.2`.",
  "",
  "**Colons that are not coordinates.** `node:fs`, `release:verify`,",
  "`mailto:me@example.com`, `vscode://file/x`, `localhost:3000`,",
  "`127.0.0.1:8080`, `12:30`, `10:30:45`, `TODO:12`, `C:12`.",
  "",
  "**Hosts, versions, and dates.** `example.com/index.html`, `https://x.com/a.html`,",
  "`1.2.3`, `v1.2`, `p99.9`, `gpt-5.6-sol`, `2026-08-20`.",
  "",
  "**Bare filenames.** A file named in passing is still just a name: `README.md`,",
  "`package.json`, `AGENTS.md`, `Makefile`, `chat.css`. Without a directory or a",
  "line number, an agent is talking *about* the file, not pointing at it.",
  "",
  "**Fenced code is untouched.** Highlighting owns this block, and every path",
  "inside it stays a path, not a chip:",
  "",
  "```ts",
  'import { SessionChatMarkdown } from "./core-ui/chat/session-chat-markdown";',
  '// see packages/core-ui/styles/chat.css:913 for the chip styles',
  "```",
  "",
  "And a path inside a real markdown link keeps the link's own behaviour:",
  "[session-chat-links.tsx](packages/core-ui/chat/session-chat-links.tsx).",
].join("\n");

function userTurn(id: string, text: string, timestamp: number): SessionChatMessage {
  return {
    blocks: [{ text, type: "text" }],
    id,
    role: "user",
    source: "transcript",
    timestamp,
  };
}

function assistantTurn(
  id: string,
  text: string,
  timestamp: number,
): SessionChatMessage {
  return {
    blocks: [{ text, type: "text" }],
    id,
    role: "assistant",
    source: "transcript",
    timestamp,
  };
}

const STORY_MESSAGES: SessionChatMessage[] = [
  userTurn("user-1", "Where did the chat-polish work actually land?", 1_000),
  assistantTurn("assistant-1", PATH_SHAPES, 2_000),
  userTurn("user-2", "Two of those have the same filename — which is which?", 3_000),
  assistantTurn("assistant-2", SAME_BASENAMES, 4_000),
  userTurn("user-3", "Some of our paths are very long. What happens then?", 5_000),
  assistantTurn("assistant-3", LONG_PATHS, 6_000),
  userTurn("user-4", "Put the long ones in a table too.", 7_000),
  assistantTurn("assistant-4", TABLE_WITH_PATHS, 8_000),
  userTurn("user-5", "And make sure ordinary inline code is left alone.", 9_000),
  assistantTurn("assistant-5", NOT_PATHS, 10_000),
];

function ChatPane({
  theme,
  withEditorSurface,
}: {
  theme: "dark" | "light";
  withEditorSurface: boolean;
}) {
  return (
    <SessionChatHostLinksProvider
      {...(withEditorSurface
        ? {
            links: {
              openFile: (path, position) => {
                // Stands in for the host's editor: gpui opens Docs or Code,
                // resolving a relative path against the active project root.
                // eslint-disable-next-line no-console
                console.log("openFile", path, position ?? "(no position)");
              },
            },
          }
        : {})}
    >
      <div
        className="ghostex-session-chat-scope flex min-h-0 flex-1 flex-col bg-background text-foreground"
        data-chat-theme={theme}
      >
        <SessionChatMessageList
          hasMore={false}
          isWorking={false}
          loadingEarlier={false}
          messages={STORY_MESSAGES}
          onLoadEarlier={() => undefined}
          verboseMode={false}
        />
      </div>
    </SessionChatHostLinksProvider>
  );
}

function PaneLabel({ children }: { children: ReactNode }) {
  return (
    <div className="px-3 py-1.5 font-mono text-[11px] uppercase tracking-wide text-white/60">
      {children}
    </div>
  );
}

function Pane({
  children,
  label,
  width,
}: {
  children: ReactNode;
  label: string;
  width?: number;
}) {
  return (
    <div
      className="flex min-h-0 flex-col overflow-hidden rounded-lg border border-white/10"
      style={width === undefined ? { flex: "1 1 0" } : { flex: "none", width }}
    >
      <PaneLabel>{label}</PaneLabel>
      {children}
    </div>
  );
}

/**
 * The narrow pane is the one that matters. A real Ghostex transcript column is
 * around 385px, which is narrower than several of the paths in this transcript,
 * so it is the only pane where the chip's truncation actually runs. The wide
 * pane beside it shows the same chips with room to spare.
 */
const TRANSCRIPT_COLUMN_WIDTH = 385;

function SessionChatFileChipsStory({ theme }: { theme: "dark" | "light" }) {
  return (
    <div className="flex h-screen min-h-[46rem] gap-2 bg-[#0a0a0a] p-2">
      <Pane
        label="after — 385px, the real transcript width"
        width={TRANSCRIPT_COLUMN_WIDTH}
      >
        <ChatPane theme={theme} withEditorSurface />
      </Pane>
      <Pane label="after — wide, nothing to truncate">
        <ChatPane theme={theme} withEditorSurface />
      </Pane>
      <Pane label="before — no editor surface, so every span stays inline code">
        <ChatPane theme={theme} withEditorSurface={false} />
      </Pane>
    </div>
  );
}

const meta = {
  argTypes: {
    theme: { control: "inline-radio", options: ["dark", "light"] },
  },
  component: SessionChatFileChipsStory,
  parameters: { layout: "fullscreen" },
  title: "Chat/File path chips",
} satisfies Meta<typeof SessionChatFileChipsStory>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Dark: Story = { args: { theme: "dark" } };

export const Light: Story = { args: { theme: "light" } };
