// Composer autocomplete triggers: "$skill" mentions and "@path" file mentions.
//
// Detection is caret-relative, not draft-relative: the token under the caret is
// the one being typed, so the pickers open when a mention is written anywhere in
// the draft (mid-sentence, on an indented line, before already-typed text)
// instead of only when the draft ends with the token.
//
// Token boundary is whitespace only, mirroring how the agent CLIs parse these
// mentions: a token that starts with "$"/"@" right after whitespace (or at the
// start of the draft) is a mention, anything else — "cost$5", "user@host" — is
// ordinary text.

import type { SessionChatSkill } from '../../shared/session-chat';

export type SessionChatComposerTriggerKind = 'path' | 'skill';

export interface SessionChatComposerTrigger {
  kind: SessionChatComposerTriggerKind;
  /** Token text after the "$"/"@" sigil. */
  query: string;
  /** Draft offset of the sigil. */
  start: number;
  /** Draft offset just past the token (the caret). */
  end: number;
}

function clampCaret(text: string, caret: number): number {
  if (!Number.isFinite(caret)) {
    return text.length;
  }
  return Math.max(0, Math.min(text.length, Math.floor(caret)));
}

export function detectSessionChatComposerTrigger(text: string, caret: number): SessionChatComposerTrigger | null {
  const cursor = clampCaret(text, caret);
  let index = cursor - 1;
  while (index >= 0 && !/\s/.test(text[index] ?? '')) {
    index -= 1;
  }
  const start = index + 1;
  const token = text.slice(start, cursor);
  if (token.startsWith('$')) {
    return { end: cursor, kind: 'skill', query: token.slice(1), start };
  }
  if (token.startsWith('@')) {
    return { end: cursor, kind: 'path', query: token.slice(1), start };
  }
  return null;
}

export function filterSessionChatSkills(skills: readonly SessionChatSkill[], query: string): SessionChatSkill[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (normalized === '') {
    return [...skills];
  }
  return skills.filter((skill) => skill.name.toLocaleLowerCase().includes(normalized));
}

export function sessionChatDisplaySkillDirectoryPath(path: string): string {
  return path.replace(/^\/Users\/[^/]+\//, '~/');
}

/**
 * Markdown-linked skill mention: the label carries the "$name" the agent reads,
 * and the destination is the skill's SKILL.md — the thing a reader clicking the
 * mention wants open in the editor. The folder is not a valid destination: the
 * host's file-link route only opens files.
 */
export function linkedSessionChatSkillMention(skill: SessionChatSkill): string {
  const label = skill.name.replace(/[\\\[\]]/g, '\\$&');
  return `[$${label}](${markdownLinkDestination(skill.skillFilePath)})`;
}

/**
 * A markdown link destination: bare with the delimiters escaped, or angle-
 * bracketed when the path carries whitespace a bare destination would end at.
 */
function markdownLinkDestination(path: string): string {
  if (/\s/.test(path)) {
    return `<${path.replace(/[\\<>]/g, '\\$&')}>`;
  }
  return path.replace(/[\\()]/g, '\\$&');
}

const SIMPLE_MENTION_PATH_PATTERN = /^[^\s@"\\]+$/;

/**
 * "@path" as the agent CLIs parse it: bare when the path has no whitespace or
 * quoting hazard, quoted otherwise.
 */
export function sessionChatFileMention(path: string): string {
  if (SIMPLE_MENTION_PATH_PATTERN.test(path)) {
    return `@${path}`;
  }
  return `@"${path.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`;
}

export function sessionChatFileBasename(path: string): string {
  const separator = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return separator >= 0 ? path.slice(separator + 1) : path;
}

export function sessionChatFileDirectory(path: string): string {
  const separator = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return separator >= 0 ? path.slice(0, separator) : '';
}

const FILE_MATCH_LIMIT = 60;

/**
 * Ranks project-relative paths for the "@" picker: basename prefix first, then
 * basename substring, then anywhere in the path. Ties break on the shortest
 * path so top-level files outrank deeply nested namesakes.
 */
export function filterSessionChatFiles(files: readonly string[], query: string): string[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (normalized === '') {
    return files.slice(0, FILE_MATCH_LIMIT);
  }
  const scored: { path: string; score: number }[] = [];
  for (const path of files) {
    const lowered = path.toLocaleLowerCase();
    const basename = sessionChatFileBasename(lowered);
    const score = basename.startsWith(normalized)
      ? 0
      : basename.includes(normalized)
        ? 1
        : lowered.includes(normalized)
          ? 2
          : 3;
    if (score < 3) {
      scored.push({ path, score });
    }
  }
  scored.sort(
    (left, right) =>
      left.score - right.score || left.path.length - right.path.length || left.path.localeCompare(right.path)
  );
  return scored.slice(0, FILE_MATCH_LIMIT).map((entry) => entry.path);
}
