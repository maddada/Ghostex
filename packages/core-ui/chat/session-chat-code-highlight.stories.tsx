import * as React from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import type { SessionChatMessage } from "../../shared/session-chat";
import { SessionChatMessageList } from "./session-chat-message-list";

/*
 * Showcase for Shiki syntax highlighting in Session Chat fenced code blocks.
 *
 * Everything here is mock transcript data — no gxserver, no session, no host
 * bridge — so the story runs anywhere Storybook does and exercises exactly the
 * renderer the real chat uses (SessionChatMessageList -> SessionChatMarkdown ->
 * session-chat-code-highlight.ts).
 */

const fence = (info: string, lines: readonly string[]): string =>
  ["```" + info, ...lines, "```"].join("\n");

const TYPESCRIPT_FENCE = fence("ts", [
  "// Resolves a fence info string to a grammar we actually ship.",
  "export function resolveLanguage(info: string | null): string | null {",
  '  if (typeof info !== "string") return null;',
  "  const normalized = info.trim().toLowerCase();",
  "  return LANGUAGE_ALIASES[normalized] ?? null;",
  "}",
]);

const TSX_FENCE = fence("tsx", [
  'import { Suspense, use } from "react";',
  "",
  "export function CodeBlock({ code, lang }: { code: string; lang: string }) {",
  "  const core = use(highlighterFor(lang));",
  "  return (",
  '    <Suspense fallback={<pre>{code}</pre>}>',
  '      <div className="shiki-host" data-lang={lang}>',
  "        {core.codeToHtml(code, { lang })}",
  "      </div>",
  "    </Suspense>",
  "  );",
  "}",
]);

const RUST_FENCE = fence("rust", [
  "#[derive(Debug, Clone)]",
  "pub struct PromptHit {",
  "    pub score: f32,",
  "    pub prompt: String,",
  "}",
  "",
  "impl PromptHit {",
  "    pub fn is_strong(&self) -> bool {",
  "        self.score > 0.75 && !self.prompt.is_empty()",
  "    }",
  "}",
]);

const CSS_FENCE = fence("css", [
  '[data-chat-theme="dark"] .ghostex-chat-markdown-shiki .shiki span {',
  "  color: var(--shiki-dark);",
  "}",
  "",
  ".ghostex-chat-markdown pre code {",
  "  background: transparent;",
  "  font-size: 0.8125rem; /* one notch under the prose around it */",
  "}",
]);

const BASH_FENCE = fence("bash", [
  "#!/usr/bin/env bash",
  "set -euo pipefail",
  "",
  'for target in gpui ghostex-web mobile; do',
  '  echo "building ${target}…"',
  '  bun run "build:${target}" || exit 1',
  "done",
]);

const JSON_FENCE = fence("json", [
  "{",
  '  "language": "typescript",',
  '  "cache": { "maxEntries": 500, "maxMemoryBytes": 52428800 },',
  '  "themes": ["github-light-default", "github-dark-default"],',
  '  "streaming": false',
  "}",
]);

const UNKNOWN_FENCE = fence("brainfuck-9000", [
  "this fence claims a language we ship no grammar for,",
  "so it must render exactly as it did before Shiki existed",
  "-- plain, readable, and definitely not an error.",
]);

const NO_LANGUAGE_FENCE = fence("", [
  "$ gx f --agents claude",
  "  3 matches in 2 projects",
  "  (no info string at all: still a plain block)",
]);

const STORY_MESSAGES: SessionChatMessage[] = [
  {
    blocks: [
      {
        text: "Add syntax highlighting to chat code fences. Show me every language you cover, plus the ones you don't.",
        type: "text",
      },
    ],
    id: "user-1",
    role: "user",
    source: "transcript",
    timestamp: 1_000,
  },
  {
    blocks: [
      {
        text: [
          "Done. The renderer resolves a fence's info string through `resolveSessionChatCodeLanguage`, loads that grammar with a dynamic `import()`, and keeps the plain `<pre>` on screen until it lands — so prose like this paragraph, inline code such as `sessionChatHighlightCacheKey(code, language)`, and the block below all stay on the same rhythm.",
          "",
          TYPESCRIPT_FENCE,
          "",
          "A `tsx` fence exercises the JSX grammar, which is a different chunk again:",
          "",
          TSX_FENCE,
          "",
          "Rust, CSS, shell and JSON all come from their own lazily-loaded chunks:",
          "",
          RUST_FENCE,
          "",
          CSS_FENCE,
          "",
          BASH_FENCE,
          "",
          JSON_FENCE,
          "",
          "Two fences deliberately stay monochrome. The first names a language we ship no grammar for; the second has no info string at all. Neither is an error, and neither may take the message down with it:",
          "",
          UNKNOWN_FENCE,
          "",
          NO_LANGUAGE_FENCE,
        ].join("\n"),
        type: "text",
      },
    ],
    id: "assistant-1",
    role: "assistant",
    source: "transcript",
    timestamp: 2_000,
  },
];

/*
 * The "before" column is the same tree with token colours neutralised, which is
 * the honest comparison: highlighting changes colour only. If the two columns
 * ever differ in height, padding, font size or wrapping, that is the
 * regression this panel exists to catch.
 */
const PLAIN_PREVIEW_STYLES = `
  [data-chat-highlight-preview="plain"] .ghostex-chat-markdown-shiki .shiki span {
    color: inherit !important;
  }
`;

function ChatPane({
  children,
  plain = false,
  theme,
}: {
  children: React.ReactNode;
  plain?: boolean;
  theme: "dark" | "light";
}) {
  return (
    <div
      className="ghostex-session-chat-scope flex min-h-0 flex-1 flex-col bg-background text-foreground"
      data-chat-highlight-preview={plain ? "plain" : undefined}
      data-chat-theme={theme}
    >
      {children}
    </div>
  );
}

function PaneLabel({ children }: { children: React.ReactNode }) {
  return (
    <div className="px-3 py-1.5 font-mono text-[11px] uppercase tracking-wide text-white/60">
      {children}
    </div>
  );
}

function StaticList({ plain, theme }: { plain?: boolean; theme: "dark" | "light" }) {
  return (
    <ChatPane plain={plain} theme={theme}>
      <SessionChatMessageList
        hasMore={false}
        isWorking={false}
        loadingEarlier={false}
        messages={STORY_MESSAGES}
        onLoadEarlier={() => undefined}
        verboseMode={false}
      />
    </ChatPane>
  );
}

const STREAMING_FENCE_LINES = [
  "pub fn highlight(code: &str, lang: &Language) -> Vec<Token> {",
  "    let mut tokens = Vec::new();",
  "    for line in code.lines() {",
  "        tokens.extend(lang.tokenize(line));",
  "    }",
  "    tokens",
  "}",
];

const STREAMING_PREFIX = [
  "Streaming a fence in, one chunk at a time. The block always shows the newest",
  "characters — highlighting is deferred, so a tokenize pass that the next chunk",
  "interrupts is thrown away instead of being repeated for every chunk, and",
  "nothing half-written is ever written to the highlight cache.",
  "",
  "```rust",
].join("\n");

const STREAMING_BODY = STREAMING_FENCE_LINES.join("\n") + "\n```";
const STREAMING_STEP_CHARS = 2;
const STREAMING_TICK_MS = 25;
const STREAMING_RESTART_MS = 2_000;

/**
 * Types the fence out character by character with `isWorking` held true, which
 * is what marks the newest row as still streaming in SessionChatMessageList.
 */
function StreamingList({ theme }: { theme: "dark" | "light" }) {
  const [revealed, setRevealed] = React.useState(0);

  React.useEffect(() => {
    if (revealed >= STREAMING_BODY.length) {
      const restart = window.setTimeout(() => setRevealed(0), STREAMING_RESTART_MS);
      return () => window.clearTimeout(restart);
    }
    const tick = window.setTimeout(() => {
      setRevealed((value) =>
        Math.min(value + STREAMING_STEP_CHARS, STREAMING_BODY.length),
      );
    }, STREAMING_TICK_MS);
    return () => window.clearTimeout(tick);
  }, [revealed]);

  const messages = React.useMemo<SessionChatMessage[]>(
    () => [
      {
        blocks: [{ text: "Stream me a rust fence.", type: "text" }],
        id: "streaming-user",
        role: "user",
        source: "transcript",
        timestamp: 1_000,
      },
      {
        blocks: [
          {
            text: `${STREAMING_PREFIX}\n${STREAMING_BODY.slice(0, revealed)}`,
            type: "text",
          },
        ],
        id: "streaming-assistant",
        role: "assistant",
        source: "hook",
        timestamp: 2_000,
      },
    ],
    [revealed],
  );

  return (
    <ChatPane theme={theme}>
      <SessionChatMessageList
        hasMore={false}
        isWorking
        loadingEarlier={false}
        messages={messages}
        onLoadEarlier={() => undefined}
        verboseMode={false}
      />
    </ChatPane>
  );
}

function SessionChatCodeHighlightStory({ theme }: { theme: "dark" | "light" }) {
  return (
    <>
      <style>{PLAIN_PREVIEW_STYLES}</style>
      <div className="flex h-screen min-h-[46rem] flex-col gap-2 bg-[#0a0a0a] p-2">
        <div className="flex min-h-0 flex-[3] gap-2">
          <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-white/10">
            <PaneLabel>after — shiki highlighted</PaneLabel>
            <StaticList theme={theme} />
          </div>
          <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-white/10">
            <PaneLabel>before — plain (token colours neutralised)</PaneLabel>
            <StaticList plain theme={theme} />
          </div>
        </div>
        <div className="flex min-h-0 flex-[2] flex-col overflow-hidden rounded-lg border border-white/10">
          <PaneLabel>streaming — fence still being appended to</PaneLabel>
          <StreamingList theme={theme} />
        </div>
      </div>
    </>
  );
}

const meta = {
  argTypes: {
    theme: { control: "inline-radio", options: ["dark", "light"] },
  },
  component: SessionChatCodeHighlightStory,
  parameters: { layout: "fullscreen" },
  title: "Chat/Code highlighting",
} satisfies Meta<typeof SessionChatCodeHighlightStory>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Dark: Story = { args: { theme: "dark" } };

export const Light: Story = { args: { theme: "light" } };
