// Harness-injected turn classifier (upstream chat spec §9.1 port — tag and
// prefix constants verbatim).
//
// Design law (do not "improve" this): match only tags OBSERVED from
// harnesses, never a broad kebab shape. A real prompt starting with a custom
// <my-element> or a Grok <user_query> envelope is a genuine user turn;
// misclassifying it hides the turn. Note <channel> is only matched in its
// attributed `<channel source=` form — a bare <channel> is a real RSS/XML
// paste.
//
// Second law: a matched turn is SUPPRESSED, not deleted. Everything the
// terminal itself prints (task notifications, local command output, an
// interrupt notice, a continuation summary, messages from other sessions)
// renders as a one-line collapsed marker that expands to the full text —
// deleting them is exactly what reads as "chat is missing messages". Only the
// records the terminal never shows either (system reminders, hook plumbing)
// stay hidden.

import type { SessionChatMessage } from '../../shared/session-chat';
import { parseSessionChatCommandEnvelope } from './session-chat-command-envelope';

const LEADING_TAG_NAME = /^<([a-z][a-z0-9-]*)(?:[\s>]|$)/;
const MARKUP_TAG = /<\/?[a-z][a-z0-9-]*(?:\s[^>]*)?>/gi;

/*
 * /compact's local-command stdout is just a dim "Compacted" wrapped in ANSI
 * style codes; readers deserve "Compaction completed" instead of a generic
 * "Local command output" marker. Deliberately lenient: strip SGR sequences
 * (with or without the ESC byte surviving transcript encoding), fold
 * whitespace, and accept the wording variants harnesses have used.
 */
const ANSI_STYLE_SEQUENCE = /(?:\u001b|\u009b)?\[[0-9;]{1,8}m/g;
const COMPACTION_OUTPUT =
  /^compact(?:ed|ing|ion)\b(?:\s+(?:is\s+)?(?:complete[d]?|done|finished|successful(?:ly)?))?(?:\s*\([^)]*\))?\s*[.!…]*$/i;
const POST_COMPACT_SUCCESS_OUTPUT = /^postcompact\b.*\bcompleted successfully:\s*\{\s*"continue"\s*:\s*true\s*\}\s*$/i;

/** Claude: the row derived from `/compact`'s local-command output. */
const COMPACTION_COMPLETED_LABEL = 'Compaction completed';
/*
 * Codex: the row gxserver decodes from the rollout's `ContextCompaction` thread
 * item (`CONTEXT_COMPACTED_STATUS_TEXT` in server/src/session_chat.rs — keep the
 * two spellings in step). Codex has no compaction output line to parse and no
 * progress screen, so that transcript item is the ONLY evidence a compaction
 * happened. Matched exactly, and only on a transcript-decoded system turn, so a
 * user typing the same words still reads as their own message.
 */
const CONTEXT_COMPACTED_LABEL = 'Context compacted';

function isContextCompactionRecord(message: SessionChatMessage, text: string): boolean {
  return message.role === 'system' && message.source === 'transcript' && text.trim() === CONTEXT_COMPACTED_LABEL;
}

/*
 * Claude Code appends a second line when a `.claude/settings.json` model pin
 * disagrees with the picked model, so this cannot be end-anchored: the trailing
 * sentence is captured as a note and reported as its own status row instead of
 * demoting the whole turn to a raw "Local command output" marker.
 */
const MODEL_DEFAULT_OUTPUT =
  /^set model to\s+(.+?)\s+and saved as your default for new sessions\s*(?:[.!…]+(?=\s|$))?\s*(.*)$/i;
const EFFORT_DEFAULT_OUTPUT = /^set effort level to\s+\S+/i;

/*
 * Claude Code's background-task notifications. The wrapper carries four fields
 * of plumbing (task id, tool-use id, output file) around the one line a reader
 * wants — the summary — plus the status that colours it. Observed statuses in
 * real transcripts: completed, failed, killed, stopped, and an empty status on
 * "Monitor event" rows.
 *
 * Harness turns carry exactly one notification, but a genuine user turn can
 * PASTE several while asking about them; that turn is never classified as
 * harness (the leading-tag law), so parsing every block here is just defence
 * against a harness that starts batching them.
 */
const TASK_NOTIFICATION_BLOCK = /<task-notification>([\s\S]*?)<\/task-notification>/gi;
const TASK_NOTIFICATION_FIELD = (name: string): RegExp => new RegExp(`<${name}>([\\s\\S]*?)</${name}>`, 'i');

export type SessionChatStatusTone = 'ok' | 'error' | 'neutral';

export interface SessionChatStatusRow {
  label: string;
  tone: SessionChatStatusTone;
  /** Short trailing fact ("exit code 0") rendered as its own non-wrapping chip. */
  detail?: string;
}

/**
 * CDXC:SessionChat 2026-09-04 WHY:
 * The harness summary ends in "(exit code N)", and on a wrapped row that
 * parenthetical splits so a lone "0)" lands on the second line. Peeling it
 * off into a chip keeps the number with its words and lets the sentence wrap
 * on its own.
 */
const TRAILING_EXIT_CODE = /\s*\((exit code -?\d+)\)\s*$/i;

function splitStatusSummary(summary: string): { label: string; detail?: string } {
  const match = TRAILING_EXIT_CODE.exec(summary);
  if (!match) {
    return { label: summary };
  }
  return { label: summary.slice(0, match.index).trim(), detail: match[1] };
}

function taskNotificationTone(status: string): SessionChatStatusTone {
  if (status === 'completed') {
    return 'ok';
  }
  if (status === 'failed') {
    return 'error';
  }
  // killed/stopped were not failures, they were halted; an empty status is a
  // Monitor event. Neither deserves a red row.
  return 'neutral';
}

/** One status row per `<task-notification>`, or [] when none parse. */
export function parseSessionChatTaskNotifications(text: string): SessionChatStatusRow[] {
  const rows: SessionChatStatusRow[] = [];
  for (const match of text.matchAll(TASK_NOTIFICATION_BLOCK)) {
    const block = match[1] ?? '';
    const status = (TASK_NOTIFICATION_FIELD('status').exec(block)?.[1] ?? '').trim().toLowerCase();
    const summary = (TASK_NOTIFICATION_FIELD('summary').exec(block)?.[1] ?? '').replace(/\s+/g, ' ').trim();
    if (summary.length === 0 && status.length === 0) {
      continue;
    }
    // The summary already states what ran and how it ended. Without one, the
    // status is all there is to say.
    const { label, detail } = splitStatusSummary(summary.length > 0 ? summary : `Background task ${status}`);
    rows.push({ label, tone: taskNotificationTone(status), ...(detail ? { detail } : {}) });
  }
  return rows;
}

/*
 * A short harness turn reads better as one muted line of prose than as a
 * chevron the reader has to click to learn it said "exit code 0". Long output
 * still collapses: inlining a hundred lines of stdout is what buries the
 * conversation.
 */
const INLINE_BODY_MAX_CHARS = 320;
const INLINE_BODY_MAX_LINES = 4;

function fitsInlineSuppressedTurn(body: string): boolean {
  return body.length > 0 && body.length <= INLINE_BODY_MAX_CHARS && body.split(/\n/).length <= INLINE_BODY_MAX_LINES;
}

/**
 * Terminal styling is not content: a transcript row carries the agent's SGR
 * codes verbatim, and a chat view has no terminal to interpret them, so they
 * must never reach the DOM.
 */
export function stripSessionChatAnsi(text: string): string {
  return text.replace(ANSI_STYLE_SEQUENCE, '');
}

function normalizedSuppressedTurnBody(text: string): string {
  return stripSessionChatAnsi(sessionChatSuppressedTurnBody(text)).replace(/\s+/g, ' ').trim();
}

function isCompactionCommandOutput(text: string): boolean {
  const [completion, ...hookResults] = stripSessionChatAnsi(sessionChatSuppressedTurnBody(text))
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  return (
    completion !== undefined &&
    COMPACTION_OUTPUT.test(completion) &&
    hookResults.every((line) => POST_COMPACT_SUCCESS_OUTPUT.test(line))
  );
}

interface ModelDefaultOutput {
  model: string;
  /** Trailing sentence the harness added, e.g. a settings.json pin warning. */
  note: string | null;
}

function modelSetByCommandOutput(text: string): ModelDefaultOutput | null {
  const match = MODEL_DEFAULT_OUTPUT.exec(normalizedSuppressedTurnBody(text));
  const model = match?.[1]?.trim();
  if (!model) {
    return null;
  }
  const note = (match?.[2] ?? '').trim();
  return { model, note: note.length > 0 ? note : null };
}

/** Harness tags that render as a collapsed, expandable marker. */
const COLLAPSED_TAG_LABELS: Readonly<Record<string, string>> = {
  'agent-message': 'Message from another session',
  'bash-input': 'Local command',
  'bash-stderr': 'Local command output',
  'bash-stdout': 'Local command output',
  'command-args': 'Slash command',
  'command-message': 'Slash command',
  'command-name': 'Slash command',
  'cross-session-message': 'Message from another session',
  'local-command-caveat': 'Local command output',
  'local-command-stderr': 'Local command output',
  'local-command-stdout': 'Local command output',
  'task-notification': 'Task notification',
  'teammate-message': 'Message from another session',
  'user-memory-input': 'Memory note',
};

/** Harness tags the agent's own TUI never prints either — stay hidden. */
const HIDDEN_TAG_NAMES: ReadonlySet<string> = new Set([
  'fork-boilerplate',
  'mcp-polling-update',
  'mcp-resource-update',
  'system-reminder',
  'user-prompt-submit-hook',
]);

const COLLAPSED_PREFIX_LABELS: readonly (readonly [string, string])[] = [
  ['<channel source=', 'Message from another session'],
  ['[request interrupted', 'Interrupted'],
  ['a message arrived from ', 'Message from another session'],
  ['another claude session sent a message', 'Message from another session'],
  ['no response requested.', 'Message from another session'],
  ['caveat: the messages below were generated by the user while running local commands', 'Local command output'],
  ['this session is being continued from a previous conversation', 'Session continued from a previous conversation'],
];

export const SESSION_CHAT_KNOWN_HARNESS_TAG_NAMES: ReadonlySet<string> = new Set([
  ...Object.keys(COLLAPSED_TAG_LABELS),
  ...HIDDEN_TAG_NAMES,
]);

export const SESSION_CHAT_HARNESS_INJECTED_TURN_PREFIXES: readonly string[] = COLLAPSED_PREFIX_LABELS.map(
  ([prefix]) => prefix
);

export function isKnownHarnessInjectedUserTurnText(text: string): boolean {
  return harnessInjectedTurnLabel(text) !== null;
}

/**
 * The marker label for a harness-injected turn, "" when the turn is one the
 * terminal never shows either, or null when it is a genuine user turn.
 */
function harnessInjectedTurnLabel(text: string): string | null {
  const normalized = text.trim().toLowerCase();
  if (normalized.length === 0) {
    return null;
  }
  const tagName = LEADING_TAG_NAME.exec(normalized)?.[1];
  if (tagName) {
    const label = COLLAPSED_TAG_LABELS[tagName];
    if (label) {
      return label;
    }
    if (HIDDEN_TAG_NAMES.has(tagName)) {
      return '';
    }
  }
  for (const [prefix, label] of COLLAPSED_PREFIX_LABELS) {
    if (normalized.startsWith(prefix)) {
      return label;
    }
  }
  return null;
}

export function sessionChatMessageText(message: SessionChatMessage): string {
  return message.blocks
    .filter((block) => block.type === 'text')
    .map((block) => block.text)
    .join('')
    .trim();
}

/** Text left once the harness markup is removed — "" ⇒ nothing to expand. */
export function sessionChatSuppressedTurnBody(text: string): string {
  return text.replace(MARKUP_TAG, '').trim();
}

export type SessionChatSuppressedTurn =
  /** Never surfaced: the agent's own TUI does not print it either. */
  | { kind: 'hidden' }
  /** One-line marker that expands to the full text on click. */
  | { kind: 'collapsed'; label: string }
  /** A polished, non-expandable status row for a completed UI action. */
  | { kind: 'status'; label: string; tone?: SessionChatStatusTone };

export function classifySessionChatSuppressedTurn(message: SessionChatMessage): SessionChatSuppressedTurn | null {
  if (message.role !== 'user' && message.role !== 'system') {
    return null;
  }
  if (message.blocks.some((block) => block.type === 'tool-call' || block.type === 'tool-result')) {
    return null;
  }
  const text = sessionChatMessageText(message);
  if (isContextCompactionRecord(message, text)) {
    // Same completed-action pill Claude's compaction gets, so the seam reads
    // identically whichever CLI drew it.
    return { kind: 'status', label: CONTEXT_COMPACTED_LABEL };
  }
  const label = harnessInjectedTurnLabel(text);
  if (label === null) {
    return null;
  }
  if (label === '' || sessionChatSuppressedTurnBody(text).length === 0) {
    // Hidden class, or pure markup with no readable content.
    return { kind: 'hidden' };
  }
  if (label === 'Local command output' && isCompactionCommandOutput(text)) {
    return { kind: 'status', label: COMPACTION_COMPLETED_LABEL };
  }
  const command = parseSessionChatCommandEnvelope(text);
  if (
    label === 'Slash command' &&
    (command?.name.toLowerCase() === '/model' ||
      command?.name.toLowerCase() === '/effort' ||
      command?.name.toLowerCase() === '/compact')
  ) {
    // The local-command output owns the one user-facing result row.
    return { kind: 'hidden' };
  }
  if (label === 'Local command output') {
    const model = modelSetByCommandOutput(text);
    if (model) {
      return { kind: 'status', label: `Set model to ${model.model}` };
    }
    if (EFFORT_DEFAULT_OUTPUT.test(normalizedSuppressedTurnBody(text))) {
      // Effort is part of the model configuration action. The model result
      // above is the single durable row for the two picker selections.
      return { kind: 'hidden' };
    }
  }
  if (label === 'Task notification') {
    const [first] = parseSessionChatTaskNotifications(text);
    if (first) {
      return { kind: 'status', label: first.label, tone: first.tone };
    }
  }
  return { kind: 'collapsed', label };
}

export function isSessionChatNoiseMessage(message: SessionChatMessage): boolean {
  return classifySessionChatSuppressedTurn(message) !== null;
}

/**
 * The agent's own record that a compaction FINISHED, in either lane Ghostex
 * can see it: Claude's `/compact` output row, and Codex's `ContextCompaction`
 * thread item. The optimistic "Ran /compact" row retires against this — once
 * the agent has said the compaction happened, a client-side "we sent it" row
 * would sit BELOW the result it announced.
 */
export function isSessionChatCompactionRecord(message: SessionChatMessage): boolean {
  const suppressed = classifySessionChatSuppressedTurn(message);
  return (
    suppressed?.kind === 'status' &&
    (suppressed.label === CONTEXT_COMPACTED_LABEL || suppressed.label === COMPACTION_COMPLETED_LABEL)
  );
}

/** How many compactions the transcript has recorded so far. */
export function countSessionChatCompactionRecords(messages: readonly SessionChatMessage[]): number {
  return messages.filter(isSessionChatCompactionRecord).length;
}

/** True only for turns that must not reach the list at all. */
export function isSessionChatHiddenMessage(message: SessionChatMessage): boolean {
  return classifySessionChatSuppressedTurn(message)?.kind === 'hidden';
}

/** The collapsed-marker label, or null when the turn renders normally. */
export function sessionChatSuppressedTurnLabel(message: SessionChatMessage): string | null {
  const suppressed = classifySessionChatSuppressedTurn(message);
  return suppressed?.kind === 'collapsed' || suppressed?.kind === 'status' ? suppressed.label : null;
}

export interface SessionChatSuppressedTurnPresentation {
  /** "inline" is a short "collapsed" turn shown as prose instead of a chevron. */
  kind: 'collapsed' | 'inline' | 'status';
  label: string;
  text: string;
  tone?: SessionChatStatusTone;
  /** Set for task notifications: one row per notification in the turn. */
  statuses?: readonly SessionChatStatusRow[];
}

/** Human-readable label and expandable text for a suppressed harness turn. */
export function sessionChatSuppressedTurnPresentation(
  message: SessionChatMessage
): SessionChatSuppressedTurnPresentation | null {
  const suppressed = classifySessionChatSuppressedTurn(message);
  if (suppressed?.kind !== 'collapsed' && suppressed?.kind !== 'status') {
    return null;
  }

  const rawText = sessionChatMessageText(message);

  if (suppressed.kind === 'status') {
    const statuses = parseSessionChatTaskNotifications(rawText);
    if (statuses.length > 0) {
      return {
        kind: 'status',
        label: suppressed.label,
        text: rawText,
        ...(suppressed.tone ? { tone: suppressed.tone } : {}),
        statuses,
      };
    }
  }

  const command = parseSessionChatCommandEnvelope(rawText);
  const model = modelSetByCommandOutput(rawText);
  let text = stripSessionChatAnsi(rawText);
  if (command?.name.toLowerCase() === '/model') {
    text = command.name;
  } else if (model) {
    text = normalizedSuppressedTurnBody(rawText);
    if (model.note) {
      // The pin warning is a second fact about the same action, not chrome to
      // drop: it gets its own neutral row under the model result.
      return {
        kind: 'status',
        label: suppressed.label,
        text,
        statuses: [
          {
            label: suppressed.label,
            tone: suppressed.kind === 'status' ? (suppressed.tone ?? 'ok') : 'ok',
          },
          { label: model.note, tone: 'neutral' },
        ],
      };
    }
  }

  // Short harness turns read as prose; the body without its markup IS the
  // sentence, so the inline row shows that rather than the raw envelope.
  if (suppressed.kind === 'collapsed') {
    const body = stripSessionChatAnsi(sessionChatSuppressedTurnBody(rawText));
    if (fitsInlineSuppressedTurn(body)) {
      return { kind: 'inline', label: suppressed.label, text: body };
    }
  }

  return {
    kind: suppressed.kind,
    label: suppressed.label,
    text,
    ...(suppressed.kind === 'status' && suppressed.tone ? { tone: suppressed.tone } : {}),
  };
}

export function stripSessionChatNoiseMessages(messages: readonly SessionChatMessage[]): SessionChatMessage[] {
  return messages.filter((message) => !isSessionChatNoiseMessage(message));
}

/** Drop only the never-surfaced turns; collapsed markers stay in the list. */
export function dropSessionChatHiddenMessages(messages: readonly SessionChatMessage[]): SessionChatMessage[] {
  return messages.filter((message) => !isSessionChatHiddenMessage(message));
}
