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

Tool-call rows (`claude-tool`) are temporary pending tool rows, never reasoning
history. The transcript owns the saved calls and their results; the terminal
row fills the gap only until the same activity is available there.
*/

import type { SessionChatMessage, SessionChatTerminalActivity } from '../../shared/session-chat';

const CLAUDE_TERMINAL_STATUS_KIND = 'claude-status';
const CLAUDE_TERMINAL_TOOL_KIND = 'claude-tool';
const TERMINAL_STATUS_ID_PREFIX = 'terminal-status:';
export const SESSION_CHAT_TERMINAL_TOOL_ID_PREFIX = 'terminal-tool:';
const TERMINAL_TOOL_ID_PREFIX = SESSION_CHAT_TERMINAL_TOOL_ID_PREFIX;

/*
CDXC:SessionChat 2026-09-13 DECISION:
User: show one live tool card at the transcript bottom only while work is current and absent from chat, regardless of the selected model.
Completed tool counts belong with the turn's expandable work, not beside the composer or model and effort confirmations.
Keep the five-second anti-flicker hold only during active work; a newer status, a transcript replacement, a blocking prompt, or the end of the turn clears the card immediately.
This supersedes the 2026-09-04 hold that kept completed terminal rows visible after the work ended.
*/
export const SESSION_CHAT_TERMINAL_TOOL_HOLD_MS = 5_000;

/** Claude's count-only completed-tool label, not a description of a running tool. */
export function isSessionChatCompletedToolSummary(text: string): boolean {
  const parts = text
    .trim()
    .replace(/\s+/g, ' ')
    .replace(/[.!]$/, '')
    .split(/,\s*(?:and\s+)?|\s+and\s+/i);
  return (
    parts.length > 0 &&
    parts.every((part) =>
      /^(?:searched for \d+ patterns?|ran \d+ shell commands?|read \d+ files?|listed \d+ (?:directories|directory))$/i.test(
        part
      )
    )
  );
}

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
  if (activity.kind !== CLAUDE_TERMINAL_TOOL_KIND || !text || isSessionChatCompletedToolSummary(text)) {
    return null;
  }
  const timestamp = Date.parse(activity.detectedAt);
  const detail = activity.detail?.trim() ?? '';
  return {
    id: `${TERMINAL_TOOL_ID_PREFIX}${activity.detectedAt}`,
    role: 'system',
    // The painted tool block rides along as a tool-result block, so the text
    // blocks stay the label alone for every comparison below.
    blocks: detail
      ? [
          { type: 'text', text },
          { type: 'tool-result', output: detail },
        ]
      : [{ type: 'text', text }],
    timestamp: Number.isNaN(timestamp) ? Date.now() : timestamp,
    source: 'hook',
  };
}

export function isSessionChatTerminalToolMessage(message: SessionChatMessage): boolean {
  return message.id.startsWith(TERMINAL_TOOL_ID_PREFIX);
}

function terminalToolDetail(message: SessionChatMessage): string {
  const block = message.blocks.find((candidate) => candidate.type === 'tool-result');
  return block?.type === 'tool-result' ? block.output : '';
}

/** A terminal preview is redundant once its call is recorded or the transcript advances beyond it. */
export function sessionChatTerminalToolRetired(
  tool: SessionChatMessage,
  transcript: readonly SessionChatMessage[]
): boolean {
  const label = tool.blocks
    .filter((block) => block.type === 'text')
    .map((block) => (block.type === 'text' ? block.text : ''))
    .join(' ');
  const command = terminalToolDetail(tool)
    .split('\n')
    .map((line) => line.trim())
    .find((line) => line.startsWith('$ '))
    ?.slice(2);
  const matches = (painted: string, saved: string): boolean => {
    const normalized = (value: string): string => value.replace(/\s+/g, ' ').trim();
    const preview = normalized(painted);
    const full = normalized(saved);
    if (!preview || !full) return false;
    return (
      preview === full ||
      (/(?:…|\.{3})$/.test(preview) && full.startsWith(preview.replace(/(?:…|\.{3})$/, '').trimEnd()))
    );
  };
  for (let index = transcript.length - 1; index >= 0; index -= 1) {
    const message = transcript[index];
    if (message.source !== 'transcript') continue;
    if (message.timestamp !== null && tool.timestamp !== null && message.timestamp > tool.timestamp) return true;
    if (message.role === 'user') break;
    let hasCall = false;
    for (const block of message.blocks) {
      if (block.type !== 'tool-call') continue;
      hasCall = true;
      if (typeof block.input !== 'object' || block.input === null) continue;
      const input = block.input as Record<string, unknown>;
      if (typeof input.description === 'string' && matches(label, input.description)) return true;
      const savedCommand = input.command ?? input.cmd;
      if (command && typeof savedCommand === 'string' && matches(command, savedCommand)) return true;
      const argument = input.file_path ?? input.path ?? input.pattern;
      if (
        typeof argument === 'string' &&
        (matches(label, `${block.name}(${argument})`) || matches(label, `${block.name} ${argument}`))
      )
        return true;
    }
    // Only the newest batch can describe the pending tool. Older identical
    // commands may be earlier attempts in the same turn.
    if (!hasCall && message.blocks.some((block) => block.type === 'text' && block.text.trim())) break;
  }
  return false;
}

/** The activity card a pending tool row renders as. */
export function sessionChatTerminalToolActivity(message: SessionChatMessage): SessionChatTerminalActivity {
  const detail = terminalToolDetail(message);
  return {
    kind: CLAUDE_TERMINAL_TOOL_KIND,
    label: message.blocks
      .filter((block) => block.type === 'text')
      .map((block) => (block.type === 'text' ? block.text : ''))
      .join(' ')
      .trim(),
    detectedAt: message.id.slice(TERMINAL_TOOL_ID_PREFIX.length),
    ...(detail ? { detail } : {}),
  };
}

/** Two samples of the same painted tool row, whatever its block shows now. */
export function sameSessionChatTerminalTool(current: SessionChatMessage, next: SessionChatMessage): boolean {
  return messageText(current) === messageText(next);
}

/** The held row with the newer sample's tool block, keeping its identity and position. */
export function withSessionChatTerminalToolDetail(
  current: SessionChatMessage,
  next: SessionChatMessage
): SessionChatMessage {
  return terminalToolDetail(current) === terminalToolDetail(next) ? current : { ...current, blocks: next.blocks };
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
