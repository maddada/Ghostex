import * as React from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import type { SessionChatMessage } from "../../shared/session-chat";
import { writeSessionChatCodeWrapDefault } from "./session-chat-code-wrap";
import { SessionChatHostLinksProvider } from "./session-chat-links";
import { SessionChatMessageList } from "./session-chat-message-list";

/*
 * Showcase for the fenced-code header: the filename a fence names, and the
 * three actions beside it — wrap lines, open the named file, copy.
 *
 * Open only appears when both halves of the question say yes: the fence has to
 * name something that really is a path (session-chat-file-paths.ts decides, and
 * it says no to `Release v1.2.3` and to a fence that names only a language),
 * and the host has to own an editor. The bottom row shows both answers side by
 * side at the ~385px a real transcript column is wide, which is also where a
 * long filename has to give way to three buttons.
 *
 * Everything here is mock transcript data — no gxserver, no session, no host
 * bridge — so the story runs anywhere Storybook does and exercises exactly the
 * renderer the real chat uses (SessionChatMessageList -> SessionChatMarkdown ->
 * session-chat-code-fence-meta.ts).
 */

const fence = (info: string, lines: readonly string[]): string =>
  ["```" + info, ...lines, "```"].join("\n");

/** ```ts title="x.ts" — the attribute form, quoted. */
const TITLE_ATTRIBUTE_FENCE = fence('ts title="packages/core-ui/chat/session-chat-code-wrap.ts"', [
  "export function readWrapDefault(): boolean {",
  '  return storage()?.getItem(KEY) === "1";',
  "}",
]);

/** ```json file=x — the short attribute form, unquoted. */
const FILE_ATTRIBUTE_FENCE = fence("json file=package.json", [
  "{",
  '  "name": "ghostex",',
  '  "scripts": { "storybook": "storybook dev" }',
  "}",
]);

/** ```md filename=x — and a markdown name, which earns the markdown glyph. */
const FILENAME_ATTRIBUTE_FENCE = fence("md filename=README.md", [
  "# Ghostex",
  "",
  "- `gx f` searches prompt history",
]);

/** ```sh path/to/file.ext — the bare token form, no attribute at all. */
const BARE_TOKEN_FENCE = fence("bash tooling/build-mobile-find.mjs", [
  "set -euo pipefail",
  "bun build apps/mobile/views/find/index.tsx",
]);

/** No meta: the header keeps showing the language, exactly as it always has. */
const LANGUAGE_ONLY_FENCE = fence("rust", [
  "pub fn title(meta: &str) -> Option<&str> {",
  "    meta.split_whitespace().next()",
  "}",
]);

/** Meta that names no file: still the language, not `showLineNumbers`. */
const NON_TITLE_META_FENCE = fence("css showLineNumbers {2,4-6}", [
  '.codeblock[data-wrap="true"] pre {',
  "  white-space: pre-wrap;",
  "}",
]);

/**
 * A title that is a title and not a path. The header shows it, because the
 * agent asked for it — but there is nothing here an editor could open, so the
 * open action stays away.
 */
const NON_PATH_TITLE_FENCE = fence('text title="Release v1.2.3"', [
  "- chat: fenced blocks name the file they came from",
  "- chat: and open it, when it is one",
]);

/** A language we ship no grammar for — the header is still a header. */
const UNSUPPORTED_LANGUAGE_FENCE = fence("brainfuck-9000 notes/whiteboard.txt", [
  "no grammar ships for this one, so the body stays plain.",
]);

/** Lines far past any pane width, which is what the wrap toggle is for. */
const LONG_LINE_FENCE = fence("bash deploy/rollout.sh", [
  "#!/usr/bin/env bash",
  'curl -sSf "https://gxserver.example.internal/api/sessions?project=/Users/madda/dev/_active/Ghostex&include=transcript,queue,notices&limit=200" | jq -r ".sessions[] | select(.state == \\"working\\") | .id"',
  "",
  'export GHOSTEX_LAUNCH_PLAN="eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzZXNzaW9uIjoiZ2hvc3RleC1kZW1vLXNlc3Npb24tMDAxIiwicHJvamVjdCI6Ii9Vc2Vycy9tYWRkYS9kZXYvX2FjdGl2ZS9HaG9zdGV4In0"',
]);

const HEADER_MARKDOWN = [
  "Four ways of naming the same thing — the header reads all of them:",
  "",
  TITLE_ATTRIBUTE_FENCE,
  "",
  FILE_ATTRIBUTE_FENCE,
  "",
  FILENAME_ATTRIBUTE_FENCE,
  "",
  BARE_TOKEN_FENCE,
  "",
  "No file named, no filename invented — the language stays:",
  "",
  LANGUAGE_ONLY_FENCE,
  "",
  NON_TITLE_META_FENCE,
  "",
  NON_PATH_TITLE_FENCE,
  "",
  "An unsupported language still gets the whole header:",
  "",
  UNSUPPORTED_LANGUAGE_FENCE,
].join("\n");

/**
 * The bottom row's transcript, sized for a 385px column: one path deep enough
 * that the header has to elide it, one bare filename, one title that is not a
 * path, and one fence that names only a language.
 */
const NARROW_MARKDOWN = [
  "The header at the width a transcript actually has:",
  "",
  fence(
    'tsx title="apps/web/src/chat/session-chat-queued-prompts-button.tsx:118"',
    [
      "export function QueuedPromptsButton({ count }: Props) {",
      "  return <Button size=\"icon-xs\">{count}</Button>;",
      "}",
    ],
  ),
  "",
  fence("json package.json", ['  "name": "ghostex",']),
  "",
  NON_PATH_TITLE_FENCE,
  "",
  LANGUAGE_ONLY_FENCE,
].join("\n");

/*
 * The "before" column renders the same transcript with the meta cut off every
 * fence, which is what the previous header could show: the language and Copy.
 * The wrap action is hidden there by PREVIEW_STYLES below.
 */
function stripFenceMeta(markdown: string): string {
  return markdown.replace(/^```(\S+)[^\n]*$/gm, "```$1");
}

/*
 * One assistant turn and no user bubble: every pane is here to show fences, and
 * a prompt above them would push the first one off a short panel.
 */
function transcript(markdown: string): SessionChatMessage[] {
  return [
    {
      blocks: [{ text: markdown, type: "text" }],
      id: "assistant-1",
      role: "assistant",
      source: "transcript",
      timestamp: 2_000,
    },
  ];
}

const AFTER_MESSAGES = transcript(HEADER_MARKDOWN);
const BEFORE_MESSAGES = transcript(stripFenceMeta(HEADER_MARKDOWN));

const WRAP_MARKDOWN = [
  "One command, one base64 blob, both far wider than the pane.",
  "",
  LONG_LINE_FENCE,
].join("\n");

const WRAP_MESSAGES = transcript(WRAP_MARKDOWN);
const NARROW_MESSAGES = transcript(NARROW_MARKDOWN);

const PREVIEW_STYLES = `
  [data-chat-header-preview="before"] .ghostex-chat-markdown-codeblock-actions
    [aria-pressed] {
    display: none;
  }
`;

function ChatPane({
  before = false,
  children,
  theme,
  withEditorSurface = true,
}: {
  before?: boolean;
  children: React.ReactNode;
  theme: "dark" | "light";
  withEditorSurface?: boolean;
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
        data-chat-header-preview={before ? "before" : undefined}
        data-chat-theme={theme}
      >
        {children}
      </div>
    </SessionChatHostLinksProvider>
  );
}

function PaneLabel({ children }: { children: React.ReactNode }) {
  return (
    <div className="px-3 py-1.5 font-mono text-[11px] uppercase tracking-wide text-white/60">
      {children}
    </div>
  );
}

function StaticList({
  before,
  messages,
  theme,
  withEditorSurface,
}: {
  before?: boolean;
  messages: SessionChatMessage[];
  theme: "dark" | "light";
  withEditorSurface?: boolean;
}) {
  return (
    <ChatPane before={before} theme={theme} withEditorSurface={withEditorSurface}>
      <SessionChatMessageList
        hasMore={false}
        isWorking={false}
        loadingEarlier={false}
        messages={messages}
        onLoadEarlier={() => undefined}
        verboseMode={false}
      />
    </ChatPane>
  );
}

/**
 * `narrow` pins the pane to 385px, the width of a real transcript column in the
 * sidebar. It is the width that decides whether a long filename and three
 * actions can share one header row, so the open action has to be judged there
 * rather than at Storybook's much wider default.
 */
function Pane({
  before,
  label,
  messages,
  narrow = false,
  theme,
  withEditorSurface,
}: {
  before?: boolean;
  label: string;
  messages: SessionChatMessage[];
  narrow?: boolean;
  theme: "dark" | "light";
  withEditorSurface?: boolean;
}) {
  return (
    <div
      className={`flex min-h-0 flex-col overflow-hidden rounded-lg border border-white/10 ${
        narrow ? "flex-none" : "flex-1"
      }`}
      // An inline width, not a Tailwind arbitrary value: the sidebar builds its
      // CSS ahead of time (shadcn.generated.css), so `w-[385px]` would compile
      // to nothing and the pane would silently be whatever width was left.
      style={narrow ? { width: "385px" } : undefined}
    >
      <PaneLabel>{label}</PaneLabel>
      <StaticList
        before={before}
        messages={messages}
        theme={theme}
        withEditorSurface={withEditorSurface}
      />
    </div>
  );
}

/*
 * A block reads the remembered wrap default once, while it renders. To show
 * both states side by side the two panes therefore have to mount one after the
 * other, each with the default it is meant to demonstrate already written —
 * mounting them together would have both read whichever value was written last.
 */
function WrapComparison({ theme }: { theme: "dark" | "light" }) {
  const [mounted, setMounted] = React.useState(0);

  React.useEffect(() => {
    if (mounted > 1) {
      return;
    }
    writeSessionChatCodeWrapDefault(mounted === 1);
    setMounted(mounted + 1);
  }, [mounted]);

  return (
    <div className="flex min-h-0 flex-1 gap-2">
      {mounted > 0 ? (
        <Pane label="wrap off — scrolls sideways" messages={WRAP_MESSAGES} theme={theme} />
      ) : null}
      {mounted > 1 ? (
        <Pane label="wrap on — every character on screen" messages={WRAP_MESSAGES} theme={theme} />
      ) : null}
    </div>
  );
}

function SessionChatCodeHeaderStory({ theme }: { theme: "dark" | "light" }) {
  /*
   * The panel owns the remembered wrap default while it is up. Seeding it here
   * — during the parent's render, before any block reads it — is what keeps the
   * top row unwrapped no matter which state the last visit to this story left
   * behind; the effect hands it back on the way out so the other chat stories
   * are not left wrapped.
   */
  React.useState(() => writeSessionChatCodeWrapDefault(false));
  React.useEffect(() => () => writeSessionChatCodeWrapDefault(false), []);

  return (
    <>
      <style>{PREVIEW_STYLES}</style>
      {/* The min-height is inline for the same reason the narrow pane's width
          is: `min-h-[66rem]` is not in the pre-built sidebar CSS, so it would
          compile to nothing and the three rows would crush each other on a
          short window. */}
      <div
        className="flex h-screen flex-col gap-2 bg-[#0a0a0a] p-2"
        style={{ minHeight: "66rem" }}
      >
        <div className="flex min-h-0 flex-[3] gap-2">
          <Pane
            label="after — filename + wrap, open, copy"
            messages={AFTER_MESSAGES}
            theme={theme}
          />
          <Pane
            before
            label="before — language + copy only"
            messages={BEFORE_MESSAGES}
            theme={theme}
          />
        </div>
        <div className="flex min-h-0 flex-[2] gap-2">
          <WrapComparison theme={theme} />
        </div>
        <div className="flex min-h-0 flex-[3] gap-2">
          <Pane
            label="385px — three actions, host with an editor"
            messages={NARROW_MESSAGES}
            narrow
            theme={theme}
          />
          <Pane
            label="385px — web/phone, no editor to open into"
            messages={NARROW_MESSAGES}
            narrow
            theme={theme}
            withEditorSurface={false}
          />
          <div className="flex-1" />
        </div>
      </div>
    </>
  );
}

const meta = {
  argTypes: {
    theme: { control: "inline-radio", options: ["dark", "light"] },
  },
  component: SessionChatCodeHeaderStory,
  parameters: { layout: "fullscreen" },
  title: "Chat/Code block header",
} satisfies Meta<typeof SessionChatCodeHeaderStory>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Dark: Story = { args: { theme: "dark" } };

export const Light: Story = { args: { theme: "light" } };
