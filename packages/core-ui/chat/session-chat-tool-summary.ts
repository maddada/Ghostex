// Tool input previews and run summaries (upstream chat spec §11.5 port).

import type { SessionChatBlock } from '../../shared/session-chat';

export const SESSION_CHAT_MAX_PREVIEW_LENGTH = 80;
export const SESSION_CHAT_MAX_PREVIEW_STRING_INPUT = 160;
export const SESSION_CHAT_MAX_PREVIEW_COLLECTION_ITEMS = 8;
export const SESSION_CHAT_MAX_PREVIEW_DEPTH = 2;
export const SESSION_CHAT_MAX_TOOL_RUN_SUMMARY_PARTS = 3;
export const SESSION_CHAT_MAX_COMMAND_PREVIEW_LINES = 3;

function boundedPreviewValue(value: unknown, depth: number, seen: WeakSet<object>): unknown {
  if (typeof value === 'string') {
    return value.length > SESSION_CHAT_MAX_PREVIEW_STRING_INPUT
      ? `${value.slice(0, SESSION_CHAT_MAX_PREVIEW_STRING_INPUT)}…`
      : value;
  }
  if (typeof value !== 'object' || value === null) {
    return value;
  }
  if (seen.has(value)) {
    return '[circular]';
  }
  if (depth >= SESSION_CHAT_MAX_PREVIEW_DEPTH) {
    return '[…]';
  }
  seen.add(value);
  if (Array.isArray(value)) {
    const bounded = value
      .slice(0, SESSION_CHAT_MAX_PREVIEW_COLLECTION_ITEMS)
      .map((item) => boundedPreviewValue(item, depth + 1, seen));
    if (value.length > SESSION_CHAT_MAX_PREVIEW_COLLECTION_ITEMS) {
      bounded.push('…');
    }
    return bounded;
  }
  const record = value as Record<string, unknown>;
  const out: Record<string, unknown> = {};
  const keys = Object.keys(record);
  for (const key of keys.slice(0, SESSION_CHAT_MAX_PREVIEW_COLLECTION_ITEMS)) {
    out[key] = boundedPreviewValue(record[key], depth + 1, seen);
  }
  if (keys.length > SESSION_CHAT_MAX_PREVIEW_COLLECTION_ITEMS) {
    out['…'] = '…';
  }
  return out;
}

function toRawPreview(input: unknown): string {
  if (input === null || input === undefined) {
    return '';
  }
  if (typeof input === 'string') {
    return input;
  }
  if (typeof input !== 'object') {
    return String(input);
  }
  try {
    return JSON.stringify(boundedPreviewValue(input, 0, new WeakSet())) ?? '';
  } catch {
    return '';
  }
}

export function summarizeSessionChatToolInput(input: unknown): string {
  const collapsed = toRawPreview(input).replace(/\s+/g, ' ').trim();
  return collapsed.length <= SESSION_CHAT_MAX_PREVIEW_LENGTH
    ? collapsed
    : `${collapsed.slice(0, SESSION_CHAT_MAX_PREVIEW_LENGTH - 1)}…`;
}

function sessionChatCommandText(input: unknown): string {
  if (typeof input === 'string') {
    const trimmed = input.trim();
    if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
      try {
        return sessionChatCommandText(JSON.parse(trimmed));
      } catch {
        // Freeform Codex exec input is JavaScript, not necessarily JSON.
      }
    }
    const embeddedCommand = /(?:\bcmd|["']cmd["']|\bcommand|["']command["'])\s*:\s*("(?:\\.|[^"\\])*")/su.exec(
      input
    )?.[1];
    if (embeddedCommand) {
      try {
        const parsed = JSON.parse(embeddedCommand);
        if (typeof parsed === 'string') {
          return parsed;
        }
      } catch {
        // Keep the freeform source as the honest preview when it is not JSON.
      }
    }
    return input;
  }
  if (typeof input !== 'object' || input === null) {
    return input === undefined || input === null ? '' : String(input);
  }
  const record = input as Record<string, unknown>;
  const command = record.command ?? record.cmd ?? record.script;
  return typeof command === 'string' ? command : toRawPreview(input);
}

/** First three non-empty command lines, flattened into the compact tool row. */
export function summarizeSessionChatCommandInput(input: unknown): string {
  return sessionChatCommandText(input)
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(0, SESSION_CHAT_MAX_COMMAND_PREVIEW_LINES)
    .join(' ');
}

/** Full detail for the expanded view. */
export function formatSessionChatToolInput(input: unknown): string {
  if (input === null || input === undefined) {
    return '';
  }
  if (typeof input === 'string') {
    return input;
  }
  if (typeof input === 'number' || typeof input === 'boolean') {
    return String(input);
  }
  try {
    return JSON.stringify(input, null, 2) ?? '';
  } catch {
    return '';
  }
}

function nonEmptyString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

export function sessionChatToolFilePath(input: unknown): string | null {
  if (typeof input !== 'object' || input === null) {
    return null;
  }
  const record = input as Record<string, unknown>;
  return (
    nonEmptyString(record.file_path) ??
    nonEmptyString(record.filePath) ??
    nonEmptyString(record.path) ??
    nonEmptyString(record.notebook_path)
  );
}

export function briefSessionChatToolArg(input: unknown): string {
  if (typeof input === 'object' && input !== null) {
    const record = input as Record<string, unknown>;
    const path = sessionChatToolFilePath(input);
    if (path) {
      const segments = path.split(/[\\/]/).filter((segment) => segment.length > 0);
      return segments.at(-1) ?? path;
    }
    const command = record.command ?? record.cmd ?? record.query ?? record.pattern;
    if (typeof command === 'string') {
      return summarizeSessionChatToolInput(command).slice(0, 28);
    }
  }
  return summarizeSessionChatToolInput(input).slice(0, 28);
}

export function summarizeSessionChatToolRun(blocks: readonly SessionChatBlock[]): string {
  const parts: string[] = [];
  for (const block of blocks) {
    if (block.type !== 'tool-call') {
      continue;
    }
    const name = block.name.trim();
    if (!name) {
      continue;
    }
    const detail = briefSessionChatToolArg(block.input);
    parts.push(detail ? `${name} ${detail}` : name);
    if (parts.length >= SESSION_CHAT_MAX_TOOL_RUN_SUMMARY_PARTS) {
      break;
    }
  }
  return parts.join('  ·  ');
}

export function countSessionChatToolCalls(blocks: readonly SessionChatBlock[]): number {
  return blocks.filter((block) => block.type === 'tool-call').length;
}
