import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

/**
 * CDXC:Worktrees 2026-08-09-18:40:
 * `tsconfig.json` covers `packages/core-ui/assets/`, `packages/shared/`,
 * `packages/core-ui/`, `apps/desktop/views/` and `apps/mobile/views/chat/` — NOT
 * `apps/desktop/`, and there is no `apps/desktop/tsconfig.json` either. So
 * every edit to `apps/desktop/sidebar/gxserver-runtime.ts` (deleted 2026-09-25) compiled clean
 * no matter what it said, and `apps/desktop/src/main.rs` cannot be cargo-checked in a reasonable time
 * because its `build.rs` builds GhosttyKit via Zig plus CEF. Repo policy also
 * forbids tests inside `apps/desktop/`.
 *
 * That leaves the rename feature's longest chain unverified by anything, and its
 * failure mode is silent: a field missing from the bridge allowlist is stripped
 * without an error, and a command type missing from the dispatch list simply
 * never reaches the allowlist at all. This test reads those files as text and
 * asserts the hops exist, following the precedent set by
 * `packages/shared/gpui-hotkey-defaults-parity.test.ts`.
 *
 * CDXC:RepoStructure 2026-08-22:
 * `gxserver-runtime.ts` became a folder (itself deleted on 2026-09-25; see the note below). The three hops this file used to find
 * in one text blob live in three different modules, so each read is aimed at the
 * module that owns its hop: the sidebar-message dispatch in `core.ts`, the two
 * rename handlers in `worktrees.ts`, and the error reader in
 * `helpers/worktrees.ts`. The handlers lost their `private` keyword in the move
 * (a method copied onto the prototype from another module cannot be `private`)
 * and gained an explicit `this` parameter, so the literal markers below match
 * the new form. Nothing else about what is asserted changed.
 *
 * There is also a real typecheck over this tree now — `apps/desktop/tsconfig.json`,
 * run by `bun run desktop:typecheck` — but it is not a substitute for this file:
 * it cannot see the Rust bridge or the modal host, and a missing dispatch arm is
 * still valid TypeScript.
 *
 * CDXC:RepoStructure 2026-08-22:
 * `apps/desktop/src/main.rs` was cut down to a 756-line crate root; the three
 * Rust hops below moved into `apps/desktop/src/app/**` (docs/2026-08-22/repo-
 * restructure/SPLITS.md C1 and the app-modal-kind split). The allowlist +
 * dispatch-gate pair stayed contiguous with each other because both landed in
 * `sidebar_dispatch.rs` and `delayed_send.rs` respectively (each read is a
 * single `sourceBetweenIn` inside one file, same as before); only the
 * `Self::RenameWorktree` app-modal-kind literals, which never had a contiguity
 * assumption between them (three independent `toContain`s), moved to
 * `app/model/types1.rs`. What each test verifies is unchanged.
 *
 * CDXC:RepoStructure 2026-08-23:
 * `app/model/types1.rs..types6.rs` (the C1 wave-3 chunk split) were
 * re-clustered into descriptively named domain modules per their
 * FOLLOW-UPS.md note. `GpuiAppModalKind` (and so the three `Self::
 * RenameWorktree` literals) landed in `app/model/app_modal_kind.rs`; nothing
 * else about what is asserted changed.
 */

/*
CDXC:Worktrees 2026-09-25 WHY:
The app runtime port (family F5) moved the rename flow out of the QuickJS runtime: the dialog's
command allowlist is `gx_store/git/modal_commands.rs`, the two handlers are
`gx_store/git/worktree_rename.rs`, and the error reader is gx-core's `git_menu/worktree.rs`. Each read
below is aimed at the file that owns its hop now; what each test asserts is unchanged.
*/
const gpuiModalCommandsSource = readFileSync(
  new URL('../../apps/desktop/src/app/gx_store/git/modal_commands.rs', import.meta.url),
  'utf8'
);
const gpuiDelayedSendSource = readFileSync(
  new URL('../../apps/desktop/src/app/delayed_send.rs', import.meta.url),
  'utf8'
);
const gpuiModalKindSource = readFileSync(
  new URL('../../apps/desktop/src/app/model/app_modal_kind.rs', import.meta.url),
  'utf8'
);
const gpuiSidebarGitActionsSource = readFileSync(
  new URL('../../apps/desktop/src/app/gx_store/git/actions.rs', import.meta.url),
  'utf8'
);
const gpuiWorktreeRenameSource = readFileSync(
  new URL('../../apps/desktop/src/app/gx_store/git/worktree_rename.rs', import.meta.url),
  'utf8'
);
const gxCoreWorktreeSource = readFileSync(
  new URL('../../packages/gx-core/src/git_menu/worktree.rs', import.meta.url),
  'utf8'
);
const modalHostSource = readFileSync(new URL('../../apps/desktop/views/modal-host.tsx', import.meta.url), 'utf8');

function sourceBetweenIn(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe('gpui/src/main.rs rename bridge', () => {
  test("forwards the rename confirmation's name, projectId, and renameBranch", () => {
    /*
     * The string copy and the boolean copy are separate calls in
     * `forward_gpui_worktree_modal_command_to_sidebar`. A boolean listed only among the strings is
     * silently dropped, which would turn every "also rename the branch" tick into a folder-only
     * rename with no error.
     */
    const allowlist = sourceBetweenIn(
      gpuiModalCommandsSource,
      'fn forward_gpui_worktree_modal_command_to_sidebar',
      'fn forward_gpui_git_commit_modal_command_to_sidebar'
    );

    expect(allowlist).toContain('copy_strings(command, &mut message, &["projectId", "name"]);');
    expect(allowlist).toContain('copy_bools(command, &mut message, &["renameBranch"]);');
    expect(allowlist).toContain('flag("renameBranch")');
  });

  test('dispatches confirmRenameWorktree to the worktree forwarder', () => {
    /*
     * This fixed list gates the forwarder. Without the new type here, the hop above is never
     * reached and the modal's Rename button does nothing at all.
     */
    const dispatch = sourceBetweenIn(
      gpuiDelayedSendSource,
      '"requestProjectWorktrees"\n            | "createProjectWorktree"',
      'forward_gpui_worktree_modal_command_to_sidebar(command_type, command, cx);'
    );

    expect(dispatch).toContain('"confirmRenameWorktree"');
  });

  test('registers the renameWorktree app-modal kind', () => {
    expect(gpuiModalKindSource).toContain('"renameWorktree" => Some(Self::RenameWorktree)');
    expect(gpuiModalKindSource).toContain('Self::RenameWorktree => "renameWorktree"');
    expect(gpuiModalKindSource).toContain('Self::RenameWorktree => "Ghostex Rename Worktree"');
  });
});

describe('gx_store/git rename handlers', () => {
  test('handles both rename messages', () => {
    expect(gpuiSidebarGitActionsSource).toContain('Some("promptRenameWorktreeForGroup") => {');
    expect(gpuiModalCommandsSource).toContain('"confirmRenameWorktree" => {');
    expect(gpuiWorktreeRenameSource).toContain('pub(crate) fn git_prompt_rename_worktree(');
    expect(gpuiWorktreeRenameSource).toContain('pub(crate) fn git_confirm_rename_worktree(');
  });

  test('calls the single rename endpoint rather than orchestrating git itself', () => {
    /*
     * Rollback for a failed move lives in gxserver, not here: a client that goes away mid-rename
     * must not be the only thing that can undo a half-applied branch rename.
     */
    const confirm = sourceBetweenIn(gpuiWorktreeRenameSource, 'pub(crate) fn git_confirm_rename_worktree(', '\n    }\n}');

    expect(confirm).toContain('"/api/renameWorktreeProject"');
    expect(confirm).not.toContain('"move"');
    expect(confirm).not.toContain('"action": "renameBranch"');
  });

  test('routes rename errors around the slash-stripping worktree error filter', () => {
    /*
     * `worktree_user_visible_error` drops any message containing "/", which is every rename refusal
     * that names a branch. The rename flow needs its own reader or the user gets a generic failure
     * instead of `Branch 'feat/x' already exists.`
     */
    expect(gxCoreWorktreeSource).toContain('pub fn worktree_rename_user_visible_error(');
    const confirm = sourceBetweenIn(gpuiWorktreeRenameSource, 'pub(crate) fn git_confirm_rename_worktree(', '\n    }\n}');
    expect(confirm).toContain('worktree_rename_user_visible_error(&error.message)');
    expect(confirm).not.toContain('worktree_user_visible_error(');
  });

  test('translates an out-of-date daemon into something the user can act on', () => {
    /*
     * Verified live against a stale daemon: it answers
     * `notFound: 'No gxserver endpoint for POST /api/renameWorktreeProject.'`. The match is on the
     * ENDPOINT PATH, not on a sentence: an error naming this route is always the daemon being older
     * than the app, never anything the user did.
     */
    const reader = sourceBetweenIn(gxCoreWorktreeSource, 'pub fn worktree_rename_user_visible_error(', '\n}');

    expect(reader).toContain('message.contains("/api/renameWorktreeProject")');
    expect(reader).toContain('Quit Ghostex fully, reopen it, and try again.');
  });
});

describe('native/sidebar/modal-host.tsx rename modal', () => {
  test('registers the modal kind, its fit-height selector, and its open arm', () => {
    expect(modalHostSource).toContain('renameWorktree: ".worktree-rename-modal-shadcn"');
    expect(modalHostSource).toContain('message.modal === "renameWorktree"');
    expect(modalHostSource).toContain('worktreeRenameDraft?: WorktreeRenameModalDraft');
    expect(modalHostSource).toContain('type: "confirmRenameWorktree"');
  });
});
