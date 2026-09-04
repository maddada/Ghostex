/*
CDXC:AgentScreenDetection 2026-09-03:
Claude's `⏺` status rows as transient chat history, and how the transcript
retires them. The rows exist because the same text can take a while to reach
the JSONL transcript, so the terminal is the earliest place the chat can read
it from. They are only ever a stand-in: the moment the transcript carries the
same turn, the transient row must go, or the reader sees the sentence twice.

Two structural facts make retirement deterministic without guessing at
Claude's wording:

  - gxserver publishes a status as the message's FIRST PARAGRAPH, re-joined
    from its wrapped rows and stopped at the first blank row (a tool row, a
    collapsed "Ran 6 shell commands" summary and a second paragraph all sit
    under the bullet with the same indent, so nothing past the blank can be
    trusted to be prose). The label is therefore always a prefix of the
    transcript's text once markdown decoration and whitespace are normalized
    away, and a prefix match retires it.
  - Claude Code appends its transcript in order. A status was painted before
    gxserver sampled it, so its own transcript row is older than any row the
    agent produced after that sample. Once the transcript holds a row newer
    than the sample and the status still has no match, its row can never
    arrive: it was a tool call whose bullet was read before the `⎿` gutter
    appeared, or a line Claude repainted. That bounds every row's life to
    "until the agent's next transcript entry" and needs no knowledge of what
    the row was.

Tool-call rows (`claude-tool`) never become reasoning history: the transcript
writes the call with its result, and the screen paints the description in a
different form ("Reading …" for "Read …") than the transcript stores. They are
the pending tool row instead, below.
*/

import type { SessionChatMessage, SessionChatTerminalActivity } from '../../shared/session-chat';

const CLAUDE_TERMINAL_STATUS_KIND = 'claude-status';
const CLAUDE_TERMINAL_TOOL_KIND = 'claude-tool';
const TERMINAL_STATUS_ID_PREFIX = 'terminal-status:';
const TERMINAL_TOOL_ID_PREFIX = 'terminal-tool:';

/*
CDXC:SessionChatTerminalActivity 2026-09-04 DECISION:
User: show the live tool card at the very bottom of the chat transcript
instead of above the chat box, where it replaced the animated working spinner
and text; keep the last card until a newer status replaces it or no Claude
tool has been on screen for 5 seconds, because a card that came and went
every second kept pushing the chat up and back down.
*/
export const SESSION_CHAT_TERMINAL_TOOL_HOLD_MS = 5_000;

/**
 * Text as the terminal would paint it: markdown decoration gone, whitespace
 * collapsed, and any trailing ellipsis dropped so a status Claude itself
 * truncated still counts as a prefix of the sentence it was cut from.
 */
export function sessionChatTerminalStatusText(text: string): string {
  return text
    .replace(/[*`_~]/g, '')
    .replace(/\s+/g, ' ')
    .replace(/(?:…|\.{3})\s*$/u, '')
    .trim();
}

function messageText(message: SessionChatMessage): string {
  return sessionChatTerminalStatusText(
    message.blocks
      .filter((block) => block.type === 'text')
      .map((block) => (block.type === 'text' ? block.text : ''))
      .join('\n\n')
  );
}

export function sessionChatTerminalStatusMessage(activity: SessionChatTerminalActivity): SessionChatMessage | null {
  const text = activity.label.trim();
  if (activity.kind !== CLAUDE_TERMINAL_STATUS_KIND || !text) {
    return null;
  }
  const timestamp = Date.parse(activity.detectedAt);
  return {
    id: `${TERMINAL_STATUS_ID_PREFIX}${activity.detectedAt}`,
    role: 'reasoning',
    blocks: [{ type: 'text', text }],
    timestamp: Number.isNaN(timestamp) ? Date.now() : timestamp,
    source: 'hook',
  };
}

/**
 * One row per status, however many probes it took to paint. A later sample
 * that extends an earlier one is the same sentence with more of it visible, so
 * it replaces that row's text in place and keeps the row's id and position; a
 * sample that is a prefix of a row already held brings nothing new.
 */
export function mergeSessionChatTerminalStatus(
  current: readonly SessionChatMessage[],
  transient: SessionChatMessage
): readonly SessionChatMessage[] {
  const text = messageText(transient);
  if (!text) {
    return current;
  }
  for (let index = current.length - 1; index >= 0; index -= 1) {
    const existing = current[index];
    const existingText = messageText(existing);
    if (existingText.startsWith(text)) {
      return current;
    }
    if (text.startsWith(existingText)) {
      const next = current.slice();
      next[index] = { ...existing, blocks: transient.blocks };
      return next;
    }
  }
  return [...current, transient];
}

interface TranscriptEvidence {
  /** Normalized text of every transcript turn. */
  texts: string[];
  /** The newest transcript row's timestamp, whatever its role. */
  latestTimestamp: number | null;
}

function transcriptEvidence(transcript: readonly SessionChatMessage[]): TranscriptEvidence {
  const evidence: TranscriptEvidence = { texts: [], latestTimestamp: null };
  for (const message of transcript) {
    if (message.source !== 'transcript') {
      continue;
    }
    const text = messageText(message);
    if (text) {
      evidence.texts.push(text);
    }
    if (
      message.timestamp !== null &&
      (evidence.latestTimestamp === null || message.timestamp > evidence.latestTimestamp)
    ) {
      evidence.latestTimestamp = message.timestamp;
    }
  }
  return evidence;
}

/** The transient rows the transcript has not caught up with yet. */
export function unreconciledSessionChatTerminalStatuses(
  statuses: readonly SessionChatMessage[],
  transcript: readonly SessionChatMessage[]
): SessionChatMessage[] {
  if (statuses.length === 0) {
    return [];
  }
  const evidence = transcriptEvidence(transcript);
  return statuses.filter((status) => {
    if (evidence.latestTimestamp !== null && status.timestamp !== null && status.timestamp < evidence.latestTimestamp) {
      return false;
    }
    const text = messageText(status);
    return !evidence.texts.some((candidate) => candidate === text || candidate.startsWith(text));
  });
}

/** The pending tool row: one `claude-tool` activity as a transcript row. */
export function sessionChatTerminalToolMessage(activity: SessionChatTerminalActivity): SessionChatMessage | null {
  const text = activity.label.trim();
  if (activity.kind !== CLAUDE_TERMINAL_TOOL_KIND || !text) {
    return null;
  }
  const timestamp = Date.parse(activity.detectedAt);
  return {
    id: `${TERMINAL_TOOL_ID_PREFIX}${activity.detectedAt}`,
    role: 'system',
    blocks: [{ type: 'text', text }],
    timestamp: Number.isNaN(timestamp) ? Date.now() : timestamp,
    source: 'hook',
  };
}

export function isSessionChatTerminalToolMessage(message: SessionChatMessage): boolean {
  return message.id.startsWith(TERMINAL_TOOL_ID_PREFIX);
}

/** The activity card a pending tool row renders as. */
export function sessionChatTerminalToolActivity(message: SessionChatMessage): SessionChatTerminalActivity {
  return {
    kind: CLAUDE_TERMINAL_TOOL_KIND,
    label: message.blocks
      .filter((block) => block.type === 'text')
      .map((block) => (block.type === 'text' ? block.text : ''))
      .join(' ')
      .trim(),
    detectedAt: message.id.slice(TERMINAL_TOOL_ID_PREFIX.length),
  };
}

/** Two samples of the same painted tool row. */
export function sameSessionChatTerminalTool(current: SessionChatMessage, next: SessionChatMessage): boolean {
  return messageText(current) === messageText(next);
}

/**
 * A tool's bullet can be read before its `⎿` gutter has been painted, in
 * which case it arrived as a status. The moment the same row is recognised as
 * a tool, that status row is its twin and goes.
 */
export function withoutSessionChatTerminalStatus(
  current: readonly SessionChatMessage[],
  tool: SessionChatMessage
): readonly SessionChatMessage[] {
  const text = messageText(tool);
  const next = current.filter((status) => {
    const statusText = messageText(status);
    return statusText !== text && !text.startsWith(statusText);
  });
  return next.length === current.length ? current : next;
}

/** Enough of a tool argument to be a meaningful match rather than a stray word. */
const TOOL_ARGS_MATCH_MIN_LENGTH = 8;

/**
 * The part of a painted tool row that Claude copies verbatim from the call:
 * the arguments of a `Read(path)` header, or everything after the leading
 * verb of a description ("Reading /tmp/web-3.png", "Finding files handling
 * AskUserQuestion"). The verb itself is the one word Claude rewrites.
 */
function toolArguments(text: string): string {
  const header = /^[A-Za-z][\w-]*\((.*)\)$/su.exec(text);
  if (header) {
    return sessionChatTerminalStatusText(header[1]);
  }
  const space = text.indexOf(' ');
  return space < 0 ? '' : text.slice(space + 1).trim();
}

function stringLeaves(value: unknown, out: string[]): void {
  if (typeof value === 'string') {
    if (value.trim()) {
      out.push(sessionChatTerminalStatusText(value));
    }
  } else if (Array.isArray(value)) {
    for (const item of value) {
      stringLeaves(item, out);
    }
  } else if (value && typeof value === 'object') {
    for (const item of Object.values(value)) {
      stringLeaves(item, out);
    }
  }
}

/**
 * The transcript has caught up with the pending tool: either a tool call
 * carries its arguments (the call landed, however its verb was painted), or
 * the transcript holds a row newer than the sample, which the append-only
 * transcript could only have written after the call itself.
 */
export function sessionChatTerminalToolLanded(
  tool: SessionChatMessage,
  transcript: readonly SessionChatMessage[]
): boolean {
  const args = toolArguments(messageText(tool));
  const leaves: string[] = [];
  for (const message of transcript) {
    if (message.source !== 'transcript') {
      continue;
    }
    if (tool.timestamp !== null && message.timestamp !== null && message.timestamp > tool.timestamp) {
      return true;
    }
    for (const block of message.blocks) {
      if (block.type === 'tool-call') {
        stringLeaves(block.input, leaves);
      }
    }
  }
  return args.length >= TOOL_ARGS_MATCH_MIN_LENGTH && leaves.some((leaf) => leaf.includes(args));
}
