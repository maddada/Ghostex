import type { Meta, StoryObj } from "@storybook/react-vite";
import type { SessionChatMessage } from "../../shared/session-chat";
import { SessionChatMessageList } from "./session-chat-message-list";

/*
 * CDXC:SessionChatTypeScale 2026-08-22:
 * What the transcript's type scale is, and what it was.
 *
 * The chat runs on THREE sizes and nothing else:
 *
 *   14px / 1.625   prose — an answer, a user message, a reasoning paragraph
 *   12px / 20px    the work lane — tool rows, group toggles, run headings
 *   11px / 1.625   detail under a work row — command output, diffs, tool bodies
 *
 * It used to run on four, with the leading left to whatever each host happened
 * to inherit. The two panes differ ONLY by the rules that changed — the "before"
 * pane is reverted with story-local CSS in BEFORE_STYLES rather than by a second
 * renderer, so the comparison stays honest:
 *
 *   - the answer's paragraphs took `text-sm`'s own 20px leading while the
 *     user's bubble had 1.625, so the two sides of one conversation set the
 *     same 14px text at different rhythms;
 *   - a 13px tier sat between prose and the work lane holding the reasoning
 *     rows and the completed-work toggle — close enough to both to read as a
 *     mistake rather than a level. Reasoning and answer rows alternate and wear
 *     the same bullet, so that near-miss is what made the transcript look like
 *     it changed its mind about size from block to block.
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
  "the model/effort pills take a second to show up when I open chat, and the",
  "type in here looks bold/smaller/bigger at random. can you make the",
  "transcript read as one thing?",
].join("\n");

/** An answer: the prose lane, with the headings, chips and code it really uses. */
const ANSWER = [
  "The transcript was running on four sizes, not three.",
  "",
  "## What the lanes are",
  "",
  "Prose is `14px / 1.625` — an answer, your message, a reasoning paragraph.",
  "The work lane is `12px / 20px`, and detail under a work row is `11px`.",
  "",
  "### Where it went wrong",
  "",
  "- `.ghostex-chat-markdown` never declared a line height, so each host",
  "  inherited its own",
  "- a 13px tier held the reasoning rows and the completed-work toggle",
  "- both of those alternate with 14px prose under the *same* bullet",
  "",
  "The leading is declared once on the markdown root now, so a size is chosen",
  "per lane and a rhythm is not:",
  "",
  "```css",
  ".ghostex-chat-markdown {",
  "  line-height: 1.625;",
  "}",
  "```",
  "",
  "> A near-miss between two lanes reads as a mistake. A real gap reads as",
  "> hierarchy.",
].join("\n");

/*
 * The tail is deliberately the reasoning/tool/answer alternation rather than
 * the long answer above: the transcript auto-scrolls to its end, and this
 * alternation under one shared bullet is the exact pattern the scale was
 * failing on.
 */
const REASONING_ONE = [
  "**Planning the type audit**",
  "",
  "Two separate complaints. The pills are a latency problem and the type is a",
  "scale problem, so I will measure the live surface before changing anything.",
].join("\n");

const REASONING_TWO = [
  "**Reading the computed styles back**",
  "",
  "The answer's paragraphs come back at 20px leading and the user's bubble at",
  "22.75px, which is the same 14px text set two ways.",
].join("\n");

const REASONING_THREE = [
  "**Deciding what the reasoning lane is**",
  "",
  "Reasoning is the same kind of thing as a tool row — something done on the",
  "way to the answer — so it belongs in the work lane, not in a tier of its own.",
].join("\n");

function turn(
  id: string,
  role: SessionChatMessage["role"],
  text: string,
  timestamp: number,
): SessionChatMessage {
  return {
    blocks: [{ text, type: "text" }],
    id,
    role,
    source: "transcript",
    timestamp,
  };
}

function toolRun(
  id: string,
  name: string,
  input: unknown,
  output: string,
  timestamp: number,
): SessionChatMessage {
  return {
    blocks: [
      { input, name, type: "tool-call" },
      { output, type: "tool-result" },
    ],
    id,
    role: "tool",
    source: "transcript",
    timestamp,
  };
}

const MESSAGES: SessionChatMessage[] = [
  turn("user-1", "user", USER_TURN, 1_000),
  turn("assistant-1", "assistant", ANSWER, 2_000),
  turn("reasoning-1", "reasoning", REASONING_ONE, 3_000),
  toolRun(
    "tool-1",
    "Bash",
    { command: "rg -n 'font-size' sidebar/styles/chat.css" },
    [
      "sidebar/styles/chat.css:290:  font-size: 0.8125rem;",
      "sidebar/styles/chat.css:427:  font-size: 0.8125rem;",
      "sidebar/styles/chat.css:516:  font-size: 0.75rem;",
      "sidebar/styles/chat.css:608:  font-size: 0.6875rem;",
    ].join("\n"),
    4_000,
  ),
  turn(
    "assistant-2",
    "assistant",
    "Four sizes in one sheet. Measuring the live surface before I touch it.",
    5_000,
  ),
  turn("reasoning-2", "reasoning", REASONING_TWO, 6_000),
  toolRun(
    "tool-2",
    "Read",
    { file_path: "sidebar/styles/chat.css", limit: 40, offset: 1069 },
    [
      "agent md p:      14px / lh 20px    / w 400",
      "user bubble p:   14px / lh 22.75px / w 400",
      "thinking row:    13px / lh 19.5px  / w 400",
      "work trigger:    12px / lh 20px    / w 400",
      "completed work:  13px / lh 20px    / w 500",
    ].join("\n"),
    7_000,
  ),
  turn(
    "assistant-3",
    "assistant",
    "Confirmed: the same 14px text is set at two different leadings.",
    8_000,
  ),
  turn("reasoning-3", "reasoning", REASONING_THREE, 9_000),
  turn(
    "assistant-4",
    "assistant",
    "Three lanes now, and every bullet you see below belongs to one of them.",
    10_000,
  ),
];

/*
 * The "before" pane, reverted rule by rule:
 *
 *  - the markdown root declared no line height, so prose fell back to the
 *    `text-sm` utility's own 20px (the user's bubble kept 1.625 from its own
 *    wrapper, which is exactly the mismatch);
 *  - the reasoning lane and the completed-work toggle sat at 0.8125rem.
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
  }
`;

function ChatPane({ before = false, label }: { before?: boolean; label: string }) {
  return (
    <div className="chat-type-scale-story__pane">
      <div className="chat-type-scale-story__label">{label}</div>
      <div
        className="ghostex-session-chat-scope chat-type-scale-story__surface"
        data-chat-theme="dark"
        data-chat-type-scale={before ? "before" : undefined}
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
  title: "Session Chat/Type scale",
  parameters: { layout: "fullscreen" },
};

export default meta;

/** The two panes side by side, same transcript, same renderer. */
export const BeforeAndAfter: StoryObj = {
  render: () => (
    <div className="chat-type-scale-story">
      <style>{STORY_STYLES}</style>
      <div className="chat-type-scale-story__panes">
        <ChatPane before label="before — four sizes, two rhythms" />
        <ChatPane label="after — three sizes, one rhythm" />
      </div>
    </div>
  ),
};

/** The shipped scale alone, full width, on the transcript's own page colour. */
export const Shipped: StoryObj = {
  render: () => (
    <div className="chat-type-scale-story">
      <style>{STORY_STYLES}</style>
      <div className="chat-type-scale-story__panes">
        <ChatPane label="shipped" />
      </div>
    </div>
  ),
};
