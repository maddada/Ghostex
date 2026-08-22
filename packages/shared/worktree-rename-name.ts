/*
CDXC:WorktreeRename 2026-08-09-18:40:
Renaming a worktree types ONE name that becomes two different things: the git
branch (verbatim, so `feat/kanban-assignee` survives) and the sibling folder
suffix (slugged, because `/` cannot appear in a directory name). Both rules live
here, in `packages/shared/`, so the sidebar modal and the gxserver-facing runtime
validate the same way and `tsconfig.json` actually typechecks them.

The character policy is gxserver's own `is_allowed_git_ref`
(`server/src/typed_operations/values.rs`) plus the three shapes git rejects that
the allowlist misses (a component starting with `.`, a component ending in
`.lock`, a trailing `.`) and a length cap. Keeping it identical to the daemon's
rule means a name the field accepts is a name `git branch -m` accepts, instead of
the field passing something the daemon then refuses.
*/

const WORKTREE_RENAME_NAME_MAX_CHARS = 200;
const WORKTREE_RENAME_FOLDER_SLUG_MAX_CHARS = 48;

export const WORKTREE_RENAME_NAME_CHARACTER_ERROR =
  "Use letters, numbers, and . _ / - only, starting with a letter or number.";
export const WORKTREE_RENAME_NAME_SEPARATOR_ERROR =
  'Names cannot contain "..", "//", or end with "/".';
export const WORKTREE_RENAME_NAME_TOO_LONG_ERROR = "Name is too long (200 characters max).";

export function normalizeWorktreeRenameName(value: string): string {
  return value.trim();
}

export function worktreeRenameNameError(value: string): string | undefined {
  const name = normalizeWorktreeRenameName(value);
  if (name.length > WORKTREE_RENAME_NAME_MAX_CHARS) {
    return WORKTREE_RENAME_NAME_TOO_LONG_ERROR;
  }
  /*
   * CDXC:WorktreeRename 2026-08-09-18:40:
   * An empty name is an error here, unlike the create flow's optional branch
   * field where empty means "let gxserver pick". There is nothing to rename to.
   */
  if (!/^[A-Za-z0-9]/.test(name)) {
    return WORKTREE_RENAME_NAME_CHARACTER_ERROR;
  }
  if (!/^[A-Za-z0-9._/-]*$/.test(name)) {
    return WORKTREE_RENAME_NAME_CHARACTER_ERROR;
  }
  if (name.endsWith(".")) {
    return WORKTREE_RENAME_NAME_CHARACTER_ERROR;
  }
  const components = name.split("/");
  if (components.some((component) => component.startsWith(".") || component.endsWith(".lock"))) {
    return WORKTREE_RENAME_NAME_CHARACTER_ERROR;
  }
  if (name.includes("..") || name.includes("//") || name.endsWith("/")) {
    return WORKTREE_RENAME_NAME_SEPARATOR_ERROR;
  }
  return undefined;
}

/**
 * The folder suffix for a typed name: `feat/kanban-assignee` becomes
 * `feat-kanban-assignee`. Case is preserved on purpose — the existing worktree
 * slugifiers lowercase because they slug a sentence, while this slugs a name the
 * user typed and expects back.
 */
export function worktreeRenameFolderSlug(value: string): string {
  const collapsed = normalizeWorktreeRenameName(value)
    .replace(/[^A-Za-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "");
  if (collapsed.length <= WORKTREE_RENAME_FOLDER_SLUG_MAX_CHARS) {
    return collapsed;
  }
  const cut = collapsed.slice(0, WORKTREE_RENAME_FOLDER_SLUG_MAX_CHARS);
  const boundary = cut.lastIndexOf("-");
  return (boundary > 0 ? cut.slice(0, boundary) : cut).replace(/-+$/, "");
}
