import type { Meta, StoryObj } from "@storybook/react-vite";
import type { CSSProperties, ReactNode } from "react";
import type { SessionChatMessage } from "../../shared/session-chat";
import { SessionChatMessageList } from "./session-chat-message-list";

/*
 * Showcase for body-vs-heading contrast in the Session Chat transcript.
 *
 * Everything here is mock transcript data — no gxserver, no session, no host
 * bridge — so the story runs anywhere Storybook does and exercises exactly the
 * renderer the real chat uses (SessionChatMessageList -> SessionChatMarkdown),
 * styled by the real packages/core-ui/styles/chat.css.
 *
 * The transcript deliberately puts every surface that could end up
 * double-muted into one screenshot: the answer's prose and headings, bullet and
 * numbered lists, inline and fenced code, a blockquote (already muted), a
 * GitHub alert (deliberately NOT muted), a user bubble (left at full strength),
 * and a reasoning row plus a tool run (already muted).
 */

const ANSWER = [
  "# Release checklist",
  "",
  "The three active targets ship from one tag, in the order below. A failure in",
  "any one of them blocks the tag.",
  "",
  "## What the pipeline builds",
  "",
  "The desktop app is the only target that signs, so it is the only one that can",
  "fail late — run `bun run release:preflight` first.",
  "",
  "- **gpui** — the desktop app. Rust shell plus the CEF React surfaces.",
  "- **web** — a static build of the shared sidebar, talking to `gxserver`.",
  "- **mobile** — React Native, Android only. iOS lives in the retired checkout.",
  "",
  "### Running it",
  "",
  "1. Tag the commit and let the dispatcher decide BUILD / SKIP / REUSE.",
  "2. Wait for the provenance assets; a reused artifact still publishes one.",
  "3. Verify with `bun run release:verify`, then publish the appcast.",
  "",
  "> The remote `gxserver` ships inside the app bundle, so a remote always lags",
  "> the client it is talking to. Capability-gate every new selector.",
  "",
  "> [!WARNING]",
  "> `release:verify --skip-android` still uploads the macOS artifact. If you",
  "> are only smoke-testing, stop before the verify step entirely.",
  "",
  "The dispatcher reads the plan file it wrote on the previous run:",
  "",
  "```ts",
  "export function planForTag(tag: string): ReleasePlan {",
  "  const previous = readProvenance(tag);",
  "  if (previous?.digest === currentDigest()) {",
  '    return { kind: "reuse", from: previous.runId };',
  "  }",
  '  return { kind: "build", targets: ACTIVE_TARGETS };',
  "}",
  "```",
  "",
  "###### Note",
  "",
  "A reused build keeps the original run's timestamps, so the appcast date and",
  "the binary's build date will not agree.",
].join("\n");

const STORY_MESSAGES: SessionChatMessage[] = [
  {
    blocks: [
      {
        text: "Walk me through the release checklist — headings, the commands, and whatever I am likely to get wrong.",
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
        text: "Pulling the three active targets out of the release scripts, then checking what the dispatcher does with a reused artifact.",
        type: "text",
      },
    ],
    id: "reasoning-1",
    role: "reasoning",
    source: "transcript",
    timestamp: 2_000,
  },
  {
    blocks: [
      {
        input: { cmd: "rg -n 'release:' package.json" },
        name: "exec",
        type: "tool-call",
      },
      {
        output: "12 matches across the release:* scripts.",
        type: "tool-result",
      },
    ],
    id: "tool-1",
    role: "tool",
    source: "transcript",
    timestamp: 3_000,
  },
  {
    blocks: [{ text: ANSWER, type: "text" }],
    id: "assistant-1",
    role: "assistant",
    source: "transcript",
    timestamp: 4_000,
  },
];

/*
 * The "before" column is the same transcript with the prose colour reverted to
 * plain --foreground, which is exactly what the transcript did before this
 * change: every element in an answer rendered at full strength, so a heading
 * weighed the same as the sentence under it.
 *
 * A variable override rather than a second copy of the stylesheet, so the two
 * columns can never drift apart on anything except the one value under test.
 */
const BEFORE_STYLE = {
  "--chat-prose-foreground": "var(--foreground)",
} as CSSProperties;

function ChatPane({
  style,
  theme,
}: {
  style?: CSSProperties;
  theme: "dark" | "light";
}) {
  return (
    <div
      className="ghostex-session-chat-scope flex min-h-0 flex-1 flex-col bg-background text-foreground"
      data-chat-theme={theme}
      style={style}
    >
      {/* Verbose keeps the completed-work disclosure open, so the reasoning row
          and the tool run stay on screen beside the answer — two of the
          surfaces this change must not double-mute. */}
      <SessionChatMessageList
        hasMore={false}
        isWorking={false}
        loadingEarlier={false}
        messages={STORY_MESSAGES}
        onLoadEarlier={() => undefined}
        verboseMode
      />
    </div>
  );
}

function PaneLabel({ children }: { children: ReactNode }) {
  return (
    <div className="px-3 py-1.5 font-mono text-[11px] uppercase tracking-wide text-white/60">
      {children}
    </div>
  );
}

function SessionChatProseHierarchyStory({ theme }: { theme: "dark" | "light" }) {
  return (
    <div className="flex h-screen min-h-[46rem] gap-2 bg-[#0a0a0a] p-2">
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-white/10">
        <PaneLabel>after — prose steps down, headings stay full</PaneLabel>
        <ChatPane theme={theme} />
      </div>
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-white/10">
        <PaneLabel>before — everything at full --foreground</PaneLabel>
        <ChatPane style={BEFORE_STYLE} theme={theme} />
      </div>
    </div>
  );
}

const meta = {
  argTypes: {
    theme: { control: "inline-radio", options: ["dark", "light"] },
  },
  component: SessionChatProseHierarchyStory,
  parameters: { layout: "fullscreen" },
  title: "Chat/Prose hierarchy",
} satisfies Meta<typeof SessionChatProseHierarchyStory>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Dark: Story = { args: { theme: "dark" } };

export const Light: Story = { args: { theme: "light" } };
