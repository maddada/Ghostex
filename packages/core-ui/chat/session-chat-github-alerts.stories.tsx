import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ReactNode } from "react";
import type { SessionChatMessage } from "../../shared/session-chat";
import { SessionChatMessageList } from "./session-chat-message-list";

/*
 * Showcase for GitHub-style alerts in Session Chat markdown.
 *
 * Everything here is mock transcript data — no gxserver, no session, no host
 * bridge — so the story runs anywhere Storybook does and exercises exactly the
 * renderer the real chat uses (SessionChatMessageList -> SessionChatMarkdown ->
 * session-chat-github-alerts.ts).
 */

const ALERT_BODY = [
  "Here is the release checklist, in the shape agents actually write it.",
  "",
  "> [!NOTE]",
  "> The remote gxserver ships inside the app bundle, so a remote always lags",
  "> the client it is talking to.",
  "",
  "> [!tip]",
  "> Markers are matched case-insensitively — this one was written `[!tip]`.",
  "",
  "> [!IMPORTANT]",
  "> Capability-gate every new selector before you call it against a remote.",
  "",
  "> [!WARNING]",
  "> `release:verify --skip-android` still uploads the macOS artifact.",
  "",
  "> [!CAUTION]",
  "> Never run `git reset --hard` in this checkout: other agents keep",
  "> uncommitted work here.",
  "",
  "An alert's body is ordinary markdown, so everything that works in a turn",
  "works inside one:",
  "",
  "> [!IMPORTANT]",
  "> Before tagging a build, walk the three active targets:",
  ">",
  "> 1. **gpui** — `bun run build`, then check the titlebar renders natively.",
  "> 2. **web** — `bun run web:typecheck`; the [Agents workspace](https://example.invalid/agents) is the smoke test.",
  "> 3. **mobile** — Android only; iOS lives in the deprecated checkout.",
  ">",
  "> ```bash",
  "> bun run release:preflight",
  '> bun run release:verify --skip-android',
  "> ```",
  ">",
  "> A failure in any one of them blocks the tag.",
  "",
  "A quote that is *not* an alert has to keep rendering exactly as it always",
  "did — muted, grey rule, no icon:",
  "",
  "> The Swift macOS app was removed on 2026-08-20. What is left under",
  "> `native/sidebar/` is compiled into gpui.",
  "",
  "And GitHub's own rule holds: a marker sharing its line with other text is",
  "not an alert either.",
  "",
  "> [!NOTE] this one has an aside on the marker line, so it stays a quote.",
].join("\n");

const STORY_MESSAGES: SessionChatMessage[] = [
  {
    blocks: [
      {
        text: "Give me the release checklist, and lean on GitHub alerts for the parts I must not miss.",
        type: "text",
      },
    ],
    id: "user-1",
    role: "user",
    source: "transcript",
    timestamp: 1_000,
  },
  {
    blocks: [{ text: ALERT_BODY, type: "text" }],
    id: "assistant-1",
    role: "assistant",
    source: "transcript",
    timestamp: 2_000,
  },
];

/*
 * The "before" column is the same transcript with every marker defused, which
 * is precisely what the old renderer put on screen: remark-gfm has no idea what
 * `[!WARNING]` means, so the marker survived as literal text inside a muted
 * quote.
 *
 * Defusing it takes a zero-width space rather than a backslash escape, because
 * escapes are resolved while parsing — `> \[!NOTE\]` reaches the plugin as the
 * text `[!NOTE]` and would be picked up as an alert again. A zero-width space
 * renders as nothing, so the column shows exactly the glyphs the old renderer
 * showed, without a second copy of the renderer to keep in step.
 */
const BEFORE_MESSAGES: SessionChatMessage[] = STORY_MESSAGES.map((message) => ({
  ...message,
  blocks: message.blocks.map((block) =>
    block.type === "text"
      ? {
          ...block,
          text: block.text.replace(
            /^(> *)\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]/gim,
            "$1[​!$2]",
          ),
        }
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
  );
}

function PaneLabel({ children }: { children: ReactNode }) {
  return (
    <div className="px-3 py-1.5 font-mono text-[11px] uppercase tracking-wide text-white/60">
      {children}
    </div>
  );
}

function SessionChatGithubAlertsStory({ theme }: { theme: "dark" | "light" }) {
  return (
    <div className="flex h-screen min-h-[46rem] gap-2 bg-[#0a0a0a] p-2">
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-white/10">
        <PaneLabel>after — rendered alerts</PaneLabel>
        <ChatPane messages={STORY_MESSAGES} theme={theme} />
      </div>
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-white/10">
        <PaneLabel>before — markers left as literal text</PaneLabel>
        <ChatPane messages={BEFORE_MESSAGES} theme={theme} />
      </div>
    </div>
  );
}

const meta = {
  argTypes: {
    theme: { control: "inline-radio", options: ["dark", "light"] },
  },
  component: SessionChatGithubAlertsStory,
  parameters: { layout: "fullscreen" },
  title: "Chat/GitHub alerts",
} satisfies Meta<typeof SessionChatGithubAlertsStory>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Dark: Story = { args: { theme: "dark" } };

export const Light: Story = { args: { theme: "light" } };
