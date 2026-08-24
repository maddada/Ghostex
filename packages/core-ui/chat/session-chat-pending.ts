// Optimistic pending sends, slash-command markers, and the /clear boundary
// (upstream chat spec §10.3 port). Pending echoes render identically to real user turns so
// replacement by the real transcript turn causes no visible state change.

import type { SessionChatAppCommand, SessionChatMessage } from '../../shared/session-chat';
import { parseSessionChatCommandEnvelope } from './session-chat-command-envelope';

export const SESSION_CHAT_PENDING_SEND_LIMIT = 8;
export const SESSION_CHAT_COMMAND_MARKER_LIMIT = 8;

// Claude records an attached image prompt as "[Image #1] prompt" (§11.8);
// normalization strips that marker so an optimistic echo matches its
// transcript twin.
const IMAGE_PROMPT_MARKER = /^\[Image #\d+\]\s*/;

// Kill-key bytes a TUI occasionally swallows into the submitted prompt as
// literal text: the send path's Ctrl-U/Ctrl-K clear burst can coalesce into
// the paste frame's stdin chunk and get recorded at the head of the message.
// Never typeable content, so both sides drop all C0 controls except \t/\n/\r
// (real text) and ESC (a bare strip would leave dangling ANSI fragments).
const LEAKED_CONTROL_CHARS = /[\u0000-\u0008\u000b\u000c\u000e-\u001a\u001c-\u001f\u007f]/g;

// A skill mention is typed as `[$name](path)`, but the harness owns the
// destination: Codex resolves symlinked skill roots, so the echo and its
// transcript twin can disagree on the path while meaning the same mention.
// Only the `$name` label is identity. Destinations follow
// linkedSessionChatSkillMention: bare with \-escaped delimiters, or
// angle-bracketed when the path carries whitespace.
const SKILL_MENTION_LINK = /\[(\$(?:[^\]\\\n]|\\.)+)\]\((?:<(?:[^>\\]|\\.)*>|(?:[^)\s\\]|\\.)*)\)/g;

const SKILL_CHIP_LINE = /^Skill: (.+)$/;

/**
 * Daemons that predate the chip-drop decode Codex's skill content chip into a
 * "Skill: name" text block the composer never typed. Drop such a line when the
 * text already carries its `$name` mention, so the echo still matches across
 * that version skew.
 */
function stripSkillChipLines(text: string): string {
  if (!text.includes('Skill: ')) {
    return text;
  }
  const lines = text.split('\n');
  const kept = lines.filter((line, index) => {
    const name = SKILL_CHIP_LINE.exec(line.trim())?.[1];
    if (!name) {
      return true;
    }
    const rest = lines.filter((_, other) => other !== index).join('\n');
    return !rest.includes(`$${name}`);
  });
  return kept.length === lines.length ? text : kept.join('\n');
}

export interface SessionChatPendingSend {
  id: string;
  text: string;
  imagePaths?: readonly string[];
  sentAt: number;
  /** Last authoritative message id when the send was issued; null = none. */
  afterMessageId?: string | null;
  afterMessageTimestamp?: number | null;
  /** 1-based among identical sends sharing a boundary. */
  matchingOccurrence?: number;
  matchingAfterTimestamp?: number;
}

export interface SessionChatCommandMarker {
  id: string;
  command: string;
  sentAt: number;
  /**
   * Row text override. Keystroke dispatches ("Sent Shift+Tab (mode cycle)")
   * are not slash commands, so "Ran /x" would read wrong.
   */
  label?: string;
  /**
   * Compaction records the transcript already held when this marker was
   * created. A `/compact` marker retires once that count grows, which is a
   * COUNT and not a timestamp comparison on purpose: a remote session's
   * transcript timestamps come off the remote host's clock, so any skew there
   * would either retire the marker on sight or strand it forever.
   */
  compactionRecordsBefore?: number;
}

let pendingSendCounter = 0;

export function nextSessionChatPendingSendId(now: number = Date.now()): string {
  pendingSendCounter += 1;
  return `${now}-${pendingSendCounter}`;
}

export function isSessionChatPendingMessageId(id: string): boolean {
  return id.startsWith('pending:');
}

export function isSessionChatCommandMarkerId(id: string): boolean {
  return id.startsWith('command:');
}

// --- Content keys / normalization -------------------------------------------

export function stripSessionChatImagePromptMarker(text: string): string {
  return text.replace(IMAGE_PROMPT_MARKER, '');
}

export function normalizeSessionChatPendingText(text: string): string {
  return stripSkillChipLines(
    stripSessionChatImagePromptMarker(text.replace(LEAKED_CONTROL_CHARS, '')).replace(SKILL_MENTION_LINK, '$1')
  )
    .trim()
    .replace(/\s+/g, ' ');
}

export function sessionChatPendingContentKey(entry: { text: string; imagePaths?: readonly string[] }): string {
  const normalized = normalizeSessionChatPendingText(entry.text);
  if (normalized) {
    return `text:${normalized}`;
  }
  const paths = entry.imagePaths?.filter(Boolean) ?? [];
  return paths.length ? `images:${JSON.stringify(paths)}` : 'empty';
}

export function sessionChatPendingMatchKey(entry: SessionChatPendingSend): string {
  return `${String(entry.afterMessageId)}\0${sessionChatPendingContentKey(entry)}`;
}

// --- Boundary filtering ------------------------------------------------------

function messageIsAfterPendingTimestamp(message: SessionChatMessage, pending: SessionChatPendingSend): boolean {
  if (message.timestamp === null) {
    // Some transcripts (Grok) never carry timestamps; excluding them would
    // strand a rank-pinned bubble at the list tail forever.
    return true;
  }
  const boundary = pending.matchingAfterTimestamp ?? pending.afterMessageTimestamp ?? pending.sentAt;
  return pending.afterMessageTimestamp == null
    ? message.timestamp >= boundary // local send time: no existing record ⇒ inclusive
    : message.timestamp > boundary; // transcript-clock boundary describes an EXISTING msg ⇒ exclusive
}

export function messagesAfterPendingBoundary(
  messages: readonly SessionChatMessage[],
  pending: SessionChatPendingSend
): readonly SessionChatMessage[] {
  if (pending.afterMessageId === undefined) {
    return messages;
  }
  if (pending.afterMessageId === null) {
    return messages.filter((message) => messageIsAfterPendingTimestamp(message, pending));
  }
  const index = messages.findIndex((message) => message.id === pending.afterMessageId);
  if (index >= 0) {
    return messages.slice(index + 1);
  }
  // A bounded read can page the boundary out — fall back to send time, NOT an
  // arbitrary older prompt.
  return messages.filter((message) => messageIsAfterPendingTimestamp(message, pending));
}

// --- Counting modes over user messages ---------------------------------------

function userMessageContentKey(message: SessionChatMessage): string {
  const text = message.blocks
    .filter((block) => block.type === 'text')
    .map((block) => block.text)
    .join('\n');
  const imagePaths = message.blocks
    .filter((block) => block.type === 'image-ref')
    .map((block) => block.path ?? block.url ?? '')
    .filter(Boolean);
  return sessionChatPendingContentKey({ imagePaths, text });
}

/** ALL user messages, counted by content key. */
export function matchingSessionChatUserContentCounts(messages: readonly SessionChatMessage[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const message of messages) {
    if (message.role !== 'user') {
      continue;
    }
    const key = userMessageContentKey(message);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return counts;
}

/** Only user texts that have a LATER NON-USER turn. */
export function advancedSessionChatUserContentCounts(messages: readonly SessionChatMessage[]): Map<string, number> {
  const counts = new Map<string, number>();
  let waiting: string[] = [];
  for (const message of messages) {
    if (message.role === 'user') {
      waiting.push(userMessageContentKey(message));
      continue;
    }
    for (const key of waiting) {
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    waiting = [];
  }
  return counts;
}

function userTexts(messages: readonly SessionChatMessage[], advanced: boolean): string[] {
  const texts: string[] = [];
  let waiting: string[] = [];
  for (const message of messages) {
    if (message.role === 'user') {
      const text = normalizeSessionChatPendingText(
        message.blocks
          .filter((block) => block.type === 'text')
          .map((block) => block.text)
          .join('\n')
      );
      if (advanced) {
        waiting.push(text);
      } else {
        texts.push(text);
      }
      continue;
    }
    if (advanced) {
      texts.push(...waiting);
      waiting = [];
    }
  }
  return texts;
}

export function matchingSessionChatUserTexts(messages: readonly SessionChatMessage[]): string[] {
  return userTexts(messages, false);
}

export function advancedSessionChatUserTexts(messages: readonly SessionChatMessage[]): string[] {
  return userTexts(messages, true);
}

// --- Rapid-send glue handling -------------------------------------------------

export function countLeadingPendingTextsGluedToUserText(pendingTexts: readonly string[], userText: string): number {
  let combined = '';
  for (let i = 0; i < pendingTexts.length; i += 1) {
    const piece = pendingTexts[i];
    if (!piece) {
      return 0;
    }
    combined += piece;
    if (combined === userText) {
      return i + 1;
    }
    if (!userText.startsWith(combined)) {
      return 0;
    }
  }
  return 0;
}

export function selectPendingIndicesRepresentedByUserTexts(
  pending: readonly SessionChatPendingSend[],
  userTextList: readonly string[]
): Set<number> {
  const represented = new Set<number>();
  if (pending.length < 2 || userTextList.length === 0) {
    return represented;
  }
  let remaining = pending.map((entry, index) => ({
    index,
    text: normalizeSessionChatPendingText(entry.text),
  }));
  for (const userText of userTextList) {
    const gluedCount = countLeadingPendingTextsGluedToUserText(
      remaining.map((entry) => entry.text),
      userText
    );
    if (gluedCount < 2) {
      // 1 is an exact match — leave it to occurrence counting.
      continue;
    }
    for (const entry of remaining.slice(0, gluedCount)) {
      represented.add(entry.index);
    }
    remaining = remaining.slice(gluedCount);
  }
  return represented;
}

// --- Prune / visibility -------------------------------------------------------

function filterPendingSends(
  pending: readonly SessionChatPendingSend[],
  messages: readonly SessionChatMessage[],
  counts: (messages: readonly SessionChatMessage[]) => Map<string, number>,
  texts: (messages: readonly SessionChatMessage[]) => string[]
): readonly SessionChatPendingSend[] {
  const consumed = new Map<string, number>();
  const exactKeep: boolean[] = pending.map((entry) => {
    const contentKey = sessionChatPendingContentKey(entry);
    const matchKey = sessionChatPendingMatchKey(entry);
    const available = counts(messagesAfterPendingBoundary(messages, entry)).get(contentKey) ?? 0;
    const used = consumed.get(matchKey) ?? 0;
    const occurrence = entry.matchingOccurrence ?? used + 1;
    consumed.set(matchKey, Math.max(used, occurrence));
    return occurrence > available;
  });
  const stillOpen = pending.filter((_, index) => exactKeep[index]);
  const gluedRepresented = selectPendingIndicesRepresentedByUserTexts(stillOpen, texts(messages));
  const embeddedRepresented = new Set<number>();
  stillOpen.forEach((entry, index) => {
    const pendingText = normalizeSessionChatPendingText(entry.text);
    // Codex's steering bundle separator is preserved in the optimistic text,
    // while the authoritative turn can prepend an already-staged input. That
    // makes the pending text an exact suffix rather than an exact whole-turn
    // match. Only apply this rule to an actual steering bundle so ordinary
    // suffixes ("fun" in "jokes are fun") cannot consume an echo.
    if (!pendingText.includes(' --- ')) {
      return;
    }
    const represented = texts(messagesAfterPendingBoundary(messages, entry)).some(
      (userText) => userText !== pendingText && userText.endsWith(pendingText)
    );
    if (represented) {
      embeddedRepresented.add(index);
    }
  });
  let openIndex = -1;
  const next = pending.filter((_, index) => {
    if (!exactKeep[index]) {
      return false;
    }
    openIndex += 1;
    return !gluedRepresented.has(openIndex) && !embeddedRepresented.has(openIndex);
  });
  return next.length === pending.length ? pending : next;
}

/**
 * Prune rule (drop the echo): keep the echo through the user-only transcript
 * phase — prune only once an assistant/other turn has landed after the
 * matching user text. Otherwise a first turn flashes the empty state before
 * the assistant reply arrives.
 */
export function pruneSessionChatPendingSends(
  pending: readonly SessionChatPendingSend[],
  messages: readonly SessionChatMessage[]
): readonly SessionChatPendingSend[] {
  return filterPendingSends(pending, messages, advancedSessionChatUserContentCounts, advancedSessionChatUserTexts);
}

/**
 * Visibility rule (hide the echo): identical structure but counts ALL user
 * messages, so an echo is hidden as soon as the transcript carries its user
 * row, even before the reply lands.
 */
export function visibleSessionChatPendingSends(
  pending: readonly SessionChatPendingSend[],
  messages: readonly SessionChatMessage[]
): readonly SessionChatPendingSend[] {
  return filterPendingSends(pending, messages, matchingSessionChatUserContentCounts, matchingSessionChatUserTexts);
}

// --- Occurrence assignment on append -----------------------------------------

export function assignSessionChatPendingOccurrence(
  existing: readonly SessionChatPendingSend[],
  entry: SessionChatPendingSend
): SessionChatPendingSend {
  const entryKey = sessionChatPendingMatchKey(entry);
  const matching = existing.filter((candidate) => sessionChatPendingMatchKey(candidate) === entryKey);
  if (matching.length === 0) {
    return entry;
  }
  let previousOccurrence = 0;
  matching.forEach((candidate, index) => {
    previousOccurrence = Math.max(previousOccurrence, candidate.matchingOccurrence ?? index + 1);
  });
  const first = matching[0];
  return {
    ...entry,
    matchingAfterTimestamp: first?.matchingAfterTimestamp ?? first?.afterMessageTimestamp ?? first?.sentAt,
    // Pruning an earlier echo must not let a later identical send reuse the
    // same transcript occurrence.
    matchingOccurrence: previousOccurrence + 1,
  };
}

// --- Rendering pending as messages -------------------------------------------

export function sessionChatPendingSendsAsMessages(pending: readonly SessionChatPendingSend[]): SessionChatMessage[] {
  return pending.map((entry) => ({
    blocks: [
      ...(entry.imagePaths ?? []).map((path) => ({
        path,
        type: 'image-ref' as const,
      })),
      ...(entry.text.trim() ? [{ text: entry.text, type: 'text' as const }] : []),
    ],
    id: `pending:${entry.id}`,
    role: 'user' as const,
    // Lowest priority: the real transcript turn always supersedes.
    source: 'client' as const,
    timestamp: entry.sentAt,
  }));
}

// --- Slash-command markers ----------------------------------------------------

export function appendSessionChatCommandMarker(
  markers: readonly SessionChatCommandMarker[],
  command: string,
  sentAt: number = Date.now(),
  label?: string,
  compactionRecordsBefore?: number
): readonly SessionChatCommandMarker[] {
  const next = [
    ...markers,
    {
      command,
      id: nextSessionChatPendingSendId(sentAt),
      sentAt,
      ...(label ? { label } : {}),
      ...(compactionRecordsBefore === undefined ? {} : { compactionRecordsBefore }),
    },
  ];
  return next.length > SESSION_CHAT_COMMAND_MARKER_LIMIT
    ? next.slice(next.length - SESSION_CHAT_COMMAND_MARKER_LIMIT)
    : next;
}

function commandMarkerName(command: string): string {
  return command.trim().toLowerCase().split(/\s+/, 1)[0] ?? '';
}

export function sessionChatCommandMarkersAsMessages(
  markers: readonly SessionChatCommandMarker[],
  /**
   * Compactions the authoritative transcript records NOW, so a `/compact`
   * marker can retire the moment its own compaction lands.
   */
  compactionRecords = 0
): SessionChatMessage[] {
  return markers.flatMap((marker) => {
    const commandName = commandMarkerName(marker.command);
    if (commandName === '/model' || commandName === '/effort') {
      // Not typed: these are what the model and effort pills dispatch. The
      // configuration they produce gets one authoritative status row, and a
      // "Ran /model" above it would narrate the implementation of a click.
      return [];
    }
    /*
     * `/compact` IS typed, and until it finishes there is nothing else to see:
     * Claude hides its command record and Codex writes none at all, so
     * dropping this row left a minutes-long compaction looking like the chat
     * had ignored the send. It retires against the agent's own completion row
     * (`isSessionChatCompactionRecord`) rather than living forever, because
     * markers render after the transcript — an un-retired one would end up
     * below the "Compaction completed" / "Context compacted" row it precedes.
     */
    if (commandName === '/compact' && compactionRecords > (marker.compactionRecordsBefore ?? 0)) {
      return [];
    }
    return [
      {
        // Text deliberately avoids harness noise prefixes so the noise filter
        // keeps it.
        blocks: [{ text: marker.label ?? `Ran ${marker.command}`, type: 'text' as const }],
        id: `command:${marker.id}`,
        role: 'system' as const,
        source: 'client' as const,
        timestamp: marker.sentAt,
      },
    ];
  });
}

/*
CDXC:SessionChatAppCommands 2026-08-23:
Commands GHOSTEX typed into the agent (auto-title `/rename`, a fork's
provisional title). They reuse the marker lane rather than getting a surface of
their own — it is the same fact, "a command went to the terminal", and only the
sender differs — but they say so, because "Ran /rename Fix parser" reads as
something the user did and would be the second most confusing thing on screen
after no row at all.

Retired against the agent's OWN record of the same command: Claude Code writes a
`<command-name>` envelope for everything it intercepts, so rendering both would
double the row. Codex writes nothing, which is the whole reason these exist.
*/
export function sessionChatAppCommandsAsMessages(
  commands: readonly SessionChatAppCommand[],
  transcript: readonly SessionChatMessage[]
): SessionChatMessage[] {
  const recorded = new Set(
    transcript.flatMap((message) => {
      const envelope = parseSessionChatCommandEnvelope(
        message.blocks.map((block) => (block.type === 'text' ? block.text : '')).join('\n')
      );
      if (!envelope) {
        return [];
      }
      return [normalizeSessionChatPendingText(`${envelope.name} ${envelope.args}`)];
    })
  );
  return commands.flatMap((entry) => {
    if (recorded.has(normalizeSessionChatPendingText(entry.command))) {
      return [];
    }
    return [
      {
        blocks: [{ text: `Ghostex sent ${entry.command}`, type: 'text' as const }],
        id: `app-command:${entry.id}`,
        role: 'system' as const,
        source: 'client' as const,
        timestamp: Date.parse(entry.sentAt) || null,
      },
    ];
  });
}

export function isSessionChatClearCommand(command: string): boolean {
  return command.trim().toLowerCase().split(/\s+/)[0] === '/clear';
}

/**
 * /clear mutates the TUI/transcript ASYNCHRONOUSLY — hide the current
 * transcript immediately so the UI reflects the command before the agent
 * writes a replacement session.
 */
export function applySessionChatCommandMarkerBoundaries(
  messages: readonly SessionChatMessage[],
  markers: readonly SessionChatCommandMarker[]
): readonly SessionChatMessage[] {
  let clearSentAt: number | null = null;
  for (const marker of markers) {
    if (isSessionChatClearCommand(marker.command)) {
      clearSentAt = clearSentAt === null ? marker.sentAt : Math.max(clearSentAt, marker.sentAt);
    }
  }
  if (clearSentAt === null) {
    return messages;
  }
  const boundary = clearSentAt;
  return messages.filter((message) => message.timestamp !== null && message.timestamp > boundary);
}
