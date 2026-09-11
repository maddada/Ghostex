/*
CDXC:AgentScreenDetection 2026-09-11 DECISION:
User: while Claude Code streams a long reply, show the text as it comes in
from the terminal (chunks every second are enough) and switch to the
transcript's message the moment it is saved to the JSONL.

gxserver publishes the `⏺` block Claude is painting as a `agent-stream`
activity whose `text` is the whole message so far (see
server/src/session_chat_terminal_activity.rs). The client shows it as the
synthetic streaming assistant bubble (session-chat-streaming.ts) and retires
it on transcript evidence:

  - the transcript carries an assistant row whose text contains the stream's
    first paragraph (`label`). A prefix is not enough because a stream whose
    bullet scrolled off before its first probe starts mid-message; the
    terminal wraps at spaces, so the words of any painted paragraph are a
    substring of the saved text once markdown decoration is normalized away.
    Only the newest few assistant rows are consulted, so an older turn that
    happened to open with the same words cannot hide a live stream.
  - once gxserver stops publishing it (the block is no longer the newest
    thing on screen, or the agent went idle): any transcript row newer than
    the stream's start. The row it was waiting for has either landed or will
    never come (an interruption leaves only the interrupt marker), so a held
    card without a match by then is stale.
  - the block turned out to be a tool call: its `⎿` gutter painted a probe
    later and the same row arrived as `claude-tool`.
  - the newest transcript row is a tool call still waiting for its result.
    Claude cannot stream text while a tool executes, so whatever `⏺` row the
    screen shows then is the tool's own row (Claude repaints it with and
    without its gutter) or an older message; neither is a message being
    written.
*/

import type { SessionChatMessage, SessionChatTerminalActivity } from '../../shared/session-chat';
import { sessionChatTerminalStatusText } from './session-chat-terminal-status';

const AGENT_TERMINAL_STREAM_KIND = 'agent-stream';

/**
 * Newest transcript assistant rows a stream is matched against. Wide enough
 * that a tool-heavy turn (one row per call) cannot push the text a stale
 * on-screen block belongs to out of range, which would show it twice.
 */
const STREAM_MATCH_ASSISTANT_ROWS = 25;

export interface SessionChatTerminalStream {
  /** `detectedAt` of the run: stable for one message across probes. */
  id: string;
  /** Epoch ms of `id`. */
  startedAt: number;
  /** The message as painted so far. */
  text: string;
  /** Normalized first paragraph, the transcript match key. */
  key: string;
  /** True while the newest frame still carried the stream. */
  live: boolean;
}

export function sessionChatTerminalStreamFromActivity(
  activity: SessionChatTerminalActivity
): SessionChatTerminalStream | null {
  const text = activity.text?.trim() ?? '';
  if (activity.kind !== AGENT_TERMINAL_STREAM_KIND || !text) {
    return null;
  }
  const startedAt = Date.parse(activity.detectedAt);
  return {
    id: activity.detectedAt,
    startedAt: Number.isNaN(startedAt) ? Date.now() : startedAt,
    text,
    key: sessionChatTerminalStatusText(activity.label),
    live: true,
  };
}

function assistantText(message: SessionChatMessage): string {
  return sessionChatTerminalStatusText(
    message.blocks
      .filter((block) => block.type === 'text')
      .map((block) => (block.type === 'text' ? block.text : ''))
      .join('\n\n')
  );
}

/** True once the transcript makes the stream redundant (see the header). */
export function sessionChatTerminalStreamRetired(
  stream: SessionChatTerminalStream,
  transcript: readonly SessionChatMessage[]
): boolean {
  let assistantRowsSeen = 0;
  let latestTimestamp: number | null = null;
  let newestRow = true;
  for (let index = transcript.length - 1; index >= 0; index -= 1) {
    const message = transcript[index];
    if (message.source !== 'transcript') {
      continue;
    }
    if (newestRow) {
      newestRow = false;
      if (
        message.role === 'assistant' &&
        message.blocks.some((block) => block.type === 'tool-call') &&
        !message.blocks.some((block) => block.type === 'text' && block.text.trim().length > 0)
      ) {
        return true;
      }
    }
    if (message.timestamp !== null && (latestTimestamp === null || message.timestamp > latestTimestamp)) {
      latestTimestamp = message.timestamp;
    }
    if (message.role === 'assistant' && assistantRowsSeen < STREAM_MATCH_ASSISTANT_ROWS) {
      assistantRowsSeen += 1;
      if (stream.key && assistantText(message).includes(stream.key)) {
        return true;
      }
    }
  }
  return !stream.live && latestTimestamp !== null && latestTimestamp > stream.startedAt;
}

/** The same painted row read as a tool once its gutter appeared. */
export function sessionChatTerminalStreamIsTool(stream: SessionChatTerminalStream, tool: SessionChatMessage): boolean {
  const toolText = sessionChatTerminalStatusText(
    tool.blocks
      .filter((block) => block.type === 'text')
      .map((block) => (block.type === 'text' ? block.text : ''))
      .join(' ')
  );
  return toolText.length > 0 && (toolText === stream.key || (stream.key.length > 0 && toolText.startsWith(stream.key)));
}
