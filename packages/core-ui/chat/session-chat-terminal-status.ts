/*
CDXC:SessionChatTerminalActivity 2026-09-02:
Claude's `⏺` status rows as transient chat history, and how the transcript
retires them. The rows exist because the same text can take a while to reach
the JSONL transcript, so the terminal is the earliest place the chat can read
it from. They are only ever a stand-in: the moment the transcript carries the
same turn, the transient row must go, or the reader sees the sentence twice.

Exact text equality used to be the only retirement rule, and it failed in three
ordinary cases, each of which left a permanent near-duplicate row:

  - the terminal wraps at its width, so a captured status was a cut prefix of
    the transcript's sentence;
  - the terminal renders markdown while the transcript stores it, so `**bold**`
    and bold never compared equal;
  - a `⏺` row also announces each tool call by its description, which the
    transcript keeps inside the tool-call input rather than as prose.

The rules here compare on a rendered-text normalization, accept a prefix as the
same status (a status is a snapshot of a sentence still being painted), and
retire tool descriptions against the transcript's tool-call blocks. Rows that
still could not be reconciled are cleared when the NEXT turn starts, never when
the current one ends: a turn's transcript may still be catching up at that
moment, and clearing then would make the text vanish and reappear.
*/

import type { SessionChatMessage, SessionChatTerminalActivity } from '../../shared/session-chat';

const CLAUDE_TERMINAL_STATUS_KIND = 'claude-status';
const TERMINAL_STATUS_ID_PREFIX = 'terminal-status:';

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

/** A `⏺ Read(path)`-shaped status: the tool name and whatever Claude showed of its arguments. */
function toolHeader(text: string): { name: string; args: string } | null {
  const match = /^([A-Za-z][\w-]*)\((.*)\)$/su.exec(text);
  return match ? { name: match[1], args: sessionChatTerminalStatusText(match[2]) } : null;
}

interface TranscriptEvidence {
  texts: string[];
  toolInputs: string[];
}

function transcriptEvidence(transcript: readonly SessionChatMessage[]): TranscriptEvidence {
  const evidence: TranscriptEvidence = { texts: [], toolInputs: [] };
  for (const message of transcript) {
    if (message.source !== 'transcript') {
      continue;
    }
    const text = messageText(message);
    if (text) {
      evidence.texts.push(text);
    }
    for (const block of message.blocks) {
      if (block.type !== 'tool-call') {
        continue;
      }
      const input = block.input;
      if (input && typeof input === 'object') {
        const description = (input as { description?: unknown }).description;
        if (typeof description === 'string' && description.trim()) {
          evidence.texts.push(sessionChatTerminalStatusText(description));
        }
      }
      try {
        evidence.toolInputs.push(sessionChatTerminalStatusText(JSON.stringify(input) ?? ''));
      } catch {
        // A non-serializable input simply contributes no argument evidence.
      }
    }
  }
  return evidence;
}

/** Enough of a tool argument to be a meaningful match rather than a stray word. */
const TOOL_ARGS_MATCH_MIN_LENGTH = 8;

function isReconciled(text: string, evidence: TranscriptEvidence): boolean {
  if (evidence.texts.some((candidate) => candidate === text || candidate.startsWith(text))) {
    return true;
  }
  /*
  A tool header is matched on its arguments only, never on the tool name: an
  earlier Bash call in the transcript says nothing about whether THIS one has
  landed, and the name-only rule would retire the in-flight call precisely
  while the transcript is behind. Short arguments (`Bash(ls)`) stay until the
  next turn starts rather than risk matching a stray substring.
  */
  const header = toolHeader(text);
  return (
    header !== null &&
    header.args.length >= TOOL_ARGS_MATCH_MIN_LENGTH &&
    evidence.toolInputs.some((input) => input.includes(header.args))
  );
}

/**
 * When the transcript last saw a user prompt. Everything the terminal painted
 * before that prompt belongs to a turn whose transcript is complete by now.
 */
function latestTranscriptPromptTimestamp(transcript: readonly SessionChatMessage[]): number | null {
  let latest: number | null = null;
  for (const message of transcript) {
    if (message.source !== 'transcript' || message.role !== 'user' || message.timestamp === null) {
      continue;
    }
    if (latest === null || message.timestamp > latest) {
      latest = message.timestamp;
    }
  }
  return latest;
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
  const promptTimestamp = latestTranscriptPromptTimestamp(transcript);
  return statuses.filter((status) => {
    if (promptTimestamp !== null && status.timestamp !== null && status.timestamp < promptTimestamp) {
      return false;
    }
    return !isReconciled(messageText(status), evidence);
  });
}
