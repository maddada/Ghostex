import { describe, expect, test } from 'vitest';
import {
  WORKTREE_RENAME_NAME_CHARACTER_ERROR,
  WORKTREE_RENAME_NAME_SEPARATOR_ERROR,
  WORKTREE_RENAME_NAME_TOO_LONG_ERROR,
  normalizeWorktreeRenameName,
  worktreeRenameFolderSlug,
  worktreeRenameNameError,
} from './worktree-rename-name';

describe('worktreeRenameNameError', () => {
  test('accepts every shape git accepts for a branch', () => {
    for (const name of ['feat/kanban-assignee', 'x/y/z', 'a-b_c.d', 'ABC/Def', 'release/1.0']) {
      expect(worktreeRenameNameError(name), name).toBeUndefined();
    }
  });

  test("rejects unsupported characters, leading punctuation, and git's reserved component shapes", () => {
    /*
     * CDXC:WorktreeRename 2026-08-09-18:40:
     * `-abc` is the footgun this rule exists for: it passes
     * `git check-ref-format`, so a laxer validator would let it through and
     * `git branch -m -- -abc` would still parse it as a flag. Requiring a
     * leading alphanumeric kills it, and the `.hidden` / `.lock` / trailing-dot
     * component rules cover the shapes git refuses that the character allowlist
     * alone would pass.
     */
    for (const name of [
      '-abc',
      '/feat',
      '.hidden',
      'a b',
      'a~b',
      'a^b',
      'a:b',
      'a?b',
      'a*b',
      'a[b',
      'a\\b',
      'a@{b',
      'ünïcode',
      'feat/.hidden',
      'feat/x.lock',
      'a.',
    ]) {
      expect(worktreeRenameNameError(name), name).toBe(WORKTREE_RENAME_NAME_CHARACTER_ERROR);
    }
  });

  test('treats an empty name as an error rather than a request to auto-name', () => {
    /*
     * CDXC:WorktreeRename 2026-08-09-18:40:
     * The create flow's branch field reads empty as "gxserver picks the name".
     * Rename has no such fallback: there is nothing to rename to, so empty is a
     * hard validation failure and Rename stays disabled.
     */
    expect(worktreeRenameNameError('')).toBe(WORKTREE_RENAME_NAME_CHARACTER_ERROR);
    expect(worktreeRenameNameError('   ')).toBe(WORKTREE_RENAME_NAME_CHARACTER_ERROR);
  });

  test('rejects the separator shapes git refuses', () => {
    for (const name of ['a..b', 'feat//x', 'feat/x/']) {
      expect(worktreeRenameNameError(name), name).toBe(WORKTREE_RENAME_NAME_SEPARATOR_ERROR);
    }
  });

  test('caps the name at 200 characters', () => {
    expect(worktreeRenameNameError('a'.repeat(200))).toBeUndefined();
    expect(worktreeRenameNameError('a'.repeat(201))).toBe(WORKTREE_RENAME_NAME_TOO_LONG_ERROR);
  });

  test('validates the trimmed value', () => {
    expect(normalizeWorktreeRenameName('  feat/x  ')).toBe('feat/x');
    expect(worktreeRenameNameError('  feat/x  ')).toBeUndefined();
  });
});

describe('worktreeRenameFolderSlug', () => {
  test('folds path separators into hyphens without lowercasing', () => {
    /*
     * CDXC:WorktreeRename 2026-08-09-18:40:
     * The branch keeps the typed name verbatim while the folder gets this slug,
     * because `/` cannot appear in a directory name. Case is preserved: the
     * existing worktree slugifiers lowercase because they slug a sentence, and
     * lowercasing a name the user typed here would silently rename their folder
     * to something they did not ask for.
     */
    expect(worktreeRenameFolderSlug('feat/kanban-assignee')).toBe('feat-kanban-assignee');
    expect(worktreeRenameFolderSlug('feat/UI-Polish')).toBe('feat-UI-Polish');
  });

  test('truncates long names on a hyphen boundary and never trails a hyphen', () => {
    const slug = worktreeRenameFolderSlug('rewrite-the-entire-presentation-snapshot-projection-pipeline-for-sidebar');

    expect(slug.length).toBeLessThanOrEqual(48);
    expect(slug.endsWith('-')).toBe(false);
    expect(slug.startsWith('rewrite-the-entire-presentation-snapshot')).toBe(true);
  });
});
