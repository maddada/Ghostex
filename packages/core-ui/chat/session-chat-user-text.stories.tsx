import type { Meta, StoryObj } from "@storybook/react-vite";
import type { SessionChatMessage } from "../../shared/session-chat";
import { SessionChatMessageList } from "./session-chat-message-list";

/*
 * What a user's turn is, and what it is not.
 *
 * A user turn arrives as text somebody typed into a composer that submits on
 * Enter, so every newline in it is a line they chose to end. CommonMark reads
 * that text as a document instead: a single newline inside a paragraph is a
 * space, and "lazy continuation" lets an unprefixed line join the block above
 * it. Rendered that way, `> quoted` on one line and an ordinary sentence on the
 * next come out as one quote.
 *
 * The bubble used to keep the typed newlines with `white-space: pre-wrap`,
 * which also painted the structural `"\n"` text nodes react-markdown puts
 * BETWEEN block children — so the numbered list below rendered "1." on one line
 * and its sentence on the next, and the quotes stood blank lines taller than
 * their own text. Both repros are here, both are real transcript text.
 *
 * Typed newlines are hard breaks now and a quote ends where the author stopped
 * typing `>` (SessionChatMarkdown's `chatText` mode, session-chat-user-text.ts).
 * The agent's answer in the same transcript is NOT read that way — it is real
 * markdown, and the pane below it shows the same two texts rendered as an agent
 * would have written them, which is where lazy continuation is correct.
 *
 * Everything here is mock transcript data — no gxserver, no session, no host
 * bridge — so it runs anywhere Storybook does while exercising exactly the
 * renderer the real chat uses.
 */

/** Repro 1: two quoted lines, each answered by an unquoted one. */
const QUOTE_TURN = [
  "> Two bits of accidental damage worth fixing",
  "fix these with subagent",
  "",
  "> If you don't care about herdr 0.8.x features right now",
  "lets delete the tui2 fully. i dont want it anymore. it has served its",
  "purpose.",
  "",
  "what other questions do you have here / what else do we plan to do?",
].join("\n");

/** Repro 2: a numbered list with a blank line between the items. */
const LIST_TURN = [
  "1. we already changed it to launch main ghostex app",
  "",
  "2. delete it full on fam. i dont want it at all anymore.",
  "",
  "3. delete it as soon as spec is done, no need to keep it around after that.",
].join("\n");

/** The same two shapes an agent would write, to show GFM is untouched. */
const AGENT_TURN = [
  "Read back the way an agent's answer is read:",
  "",
  "> a quoted line",
  "continued lazily, which is correct in a document",
  "",
  "1. first",
  "2. second",
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

const MESSAGES: SessionChatMessage[] = [
  turn("user-1", "user", QUOTE_TURN, 1_000),
  turn(
    "assistant-1",
    "assistant",
    "Both quotes end where you stopped typing `>`, and the lines under them are your lines.",
    2_000,
  ),
  turn("user-2", "user", LIST_TURN, 3_000),
  turn("assistant-2", "assistant", AGENT_TURN, 4_000),
];

const STORY_STYLES = `
  .chat-user-text-story {
    background: #050505;
    display: flex;
    flex-direction: column;
    height: 100vh;
    min-height: 0;
  }
  .chat-user-text-story__surface {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
  }
`;

const meta: Meta = {
  title: "Session Chat/User text",
  parameters: { layout: "fullscreen" },
};

export default meta;

/** The two reported repros, in a transcript, on the chat's own page colour. */
export const TypedText: StoryObj = {
  render: () => (
    <div className="chat-user-text-story">
      <style>{STORY_STYLES}</style>
      <div
        className="ghostex-session-chat-scope chat-user-text-story__surface"
        data-chat-theme="dark"
      >
        <SessionChatMessageList
          hasMore={false}
          isWorking={false}
          loadingEarlier={false}
          messages={MESSAGES}
          onLoadEarlier={() => undefined}
        />
      </div>
    </div>
  ),
};
