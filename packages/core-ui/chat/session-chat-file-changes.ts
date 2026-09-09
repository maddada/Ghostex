import type { SessionChatToolCallBlock, SessionChatToolResultBlock } from '../../shared/session-chat';
import type { SessionChatDiffLine } from './session-chat-diff';
import { pairSessionChatToolBlocks } from './session-chat-tool-fold';
import { sessionChatToolFilePath } from './session-chat-tool-summary';

export interface SessionChatFileChange {
  path: string;
  action: 'Write' | 'Edit' | 'Delete';
  lines: SessionChatDiffLine[];
  result?: SessionChatToolResultBlock;
}

function codeLines(text: string, kind: SessionChatDiffLine['kind']): SessionChatDiffLine[] {
  const lines = text.replace(/\r\n/g, '\n').split('\n');
  if (lines.at(-1) === '') lines.pop();
  return lines.map((text) => ({ kind, text }));
}

function patchChanges(patch: string): SessionChatFileChange[] {
  if (!patch.trimStart().startsWith('*** Begin Patch')) return [];
  const changes: SessionChatFileChange[] = [];
  let current: SessionChatFileChange | undefined;
  for (const line of patch.split(/\r?\n/)) {
    const header = /^\*\*\* (Add|Update|Delete) File: (.+)$/.exec(line);
    if (header) {
      current = {
        path: header[2]!,
        action: header[1] === 'Add' ? 'Write' : header[1] === 'Delete' ? 'Delete' : 'Edit',
        lines: [],
      };
      changes.push(current);
    } else if (current && line.startsWith('*** Move to: ')) {
      current.path = line.slice('*** Move to: '.length);
    } else if (current && /^[+\- ]/.test(line)) {
      current.lines.push({ kind: line[0] === '+' ? 'add' : line[0] === '-' ? 'del' : 'context', text: line.slice(1) });
    } else if (current && line.startsWith('@@')) {
      current.lines.push({ kind: 'meta', text: line });
    } else if (line === '*** End Patch') {
      current = undefined;
    }
  }
  return changes;
}

function fileChanges(call: SessionChatToolCallBlock): SessionChatFileChange[] {
  const name = call.name.split('.').at(-1)?.toLowerCase();
  let input = call.input;
  if (typeof input === 'string' && input.trimStart().startsWith('{')) {
    try {
      input = JSON.parse(input);
    } catch {
      /* A streaming JSON argument may still be incomplete. */
    }
  }
  if (name === 'apply_patch') {
    const patch =
      typeof input === 'string'
        ? input
        : input && typeof input === 'object'
          ? (input as Record<string, unknown>).patch
          : null;
    return typeof patch === 'string' ? patchChanges(patch) : [];
  }
  // Codex's exec wrapper records literal apply_patch arguments inside JavaScript.
  // Decode only JSON string literals; never execute transcript source.
  if (name === 'exec' && typeof input === 'string') {
    return [...input.matchAll(/\btools\.apply_patch\(\s*("(?:\\.|[^"\\])*")\s*\)/g)].flatMap((match) => {
      try {
        return patchChanges(JSON.parse(match[1]!));
      } catch {
        return [];
      }
    });
  }
  if (!['write', 'edit', 'multiedit', 'str_replace'].includes(name ?? '') || !input || typeof input !== 'object')
    return [];
  const record = input as Record<string, unknown>;
  const path = sessionChatToolFilePath(input);
  if (!path) return [];
  const edits = name === 'multiedit' && Array.isArray(record.edits) ? record.edits : [record];
  const lines: SessionChatDiffLine[] = [];
  for (const edit of edits) {
    if (!edit || typeof edit !== 'object') continue;
    const value = edit as Record<string, unknown>;
    const before = value.old_string ?? value.oldString ?? value.old;
    const after = value.new_string ?? value.newString ?? value.new ?? value.content ?? value.file_text;
    if (typeof after !== 'string') return [];
    if (typeof before === 'string') lines.push(...codeLines(before, 'del'));
    lines.push(...codeLines(after, 'add'));
  }
  return [{ path, action: name === 'write' ? 'Write' : 'Edit', lines }];
}

/** CDXC:SessionChat 2026-09-09 DECISION:
 * User: show file writes and code edits at the top level, outside other tools, with the filename above a seven-line preview that expands and collapses on click.
 * Pair before extracting so removing a write cannot attach its result to the next tool.
 */
export function splitSessionChatFileChanges(
  blocks: readonly (SessionChatToolCallBlock | SessionChatToolResultBlock)[]
) {
  const tools: (SessionChatToolCallBlock | SessionChatToolResultBlock)[] = [];
  const changes: SessionChatFileChange[] = [];
  for (const pair of pairSessionChatToolBlocks(blocks)) {
    const files = pair.call ? fileChanges(pair.call) : [];
    changes.push(...files.map((file) => ({ ...file, result: pair.result })));
    // An exec wrapper can also run unrelated commands, so retain its raw activity.
    if (files.length === 0 || pair.call?.name.split('.').at(-1) === 'exec') {
      if (pair.call) tools.push(pair.call);
      if (pair.result) tools.push(pair.result);
    }
  }
  return { tools, changes };
}
