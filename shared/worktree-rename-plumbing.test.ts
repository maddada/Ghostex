import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

/**
 * CDXC:WorktreeRename 2026-08-09-18:40:
 * `tsconfig.json` covers `src/`, `shared/`, `sidebar/`, `native/sidebar/` and
 * `mobile-chat/` — NOT `gpui/`, and there is no `gpui/tsconfig.json` either. So
 * every edit to `gpui/sidebar/gxserver-runtime.ts` compiles clean no matter what
 * it says, and `gpui/src/main.rs` cannot be cargo-checked in a reasonable time
 * because its `build.rs` builds GhosttyKit via Zig plus CEF. Repo policy also
 * forbids tests inside `gpui/`.
 *
 * That leaves the rename feature's longest chain unverified by anything, and its
 * failure mode is silent: a field missing from the bridge allowlist is stripped
 * without an error, and a command type missing from the dispatch list simply
 * never reaches the allowlist at all. This test reads those files as text and
 * asserts the hops exist, following the precedent set by
 * `shared/gpui-hotkey-defaults-parity.test.ts`.
 */

const gpuiMainSource = readFileSync(new URL("../gpui/src/main.rs", import.meta.url), "utf8");
const gpuiRuntimeSource = readFileSync(
  new URL("../gpui/sidebar/gxserver-runtime.ts", import.meta.url),
  "utf8",
);
const modalHostSource = readFileSync(
  new URL("../native/sidebar/modal-host.tsx", import.meta.url),
  "utf8",
);

function sourceBetweenIn(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("gpui/src/main.rs rename bridge", () => {
  test("forwards the rename confirmation's name, projectId, and renameBranch", () => {
    /*
     * The string loop and the boolean block are separate code paths in
     * `forward_gpui_worktree_modal_command_to_sidebar`. A boolean listed only in
     * `allowed_string_fields` is silently dropped, which would turn every
     * "also rename the branch" tick into a folder-only rename with no error.
     */
    const allowlist = sourceBetweenIn(
      gpuiMainSource,
      "fn forward_gpui_worktree_modal_command_to_sidebar",
      "let Some(sidebar) = self.sidebar.clone()",
    );

    expect(allowlist).toContain('"confirmRenameWorktree" => &["projectId", "name"]');
    expect(allowlist).toContain('command_type == "confirmRenameWorktree"');
    expect(allowlist).toContain('"renameBranch"');
  });

  test("dispatches confirmRenameWorktree to the worktree forwarder", () => {
    /*
     * This fixed list gates the forwarder. Without the new type here, hop 8
     * above is never reached and the modal's Rename button does nothing at all.
     */
    const dispatch = sourceBetweenIn(
      gpuiMainSource,
      '"requestProjectWorktrees"\n            | "createProjectWorktree"',
      "forward_gpui_worktree_modal_command_to_sidebar(command_type, command, cx);",
    );

    expect(dispatch).toContain('"confirmRenameWorktree"');
  });

  test("registers the renameWorktree app-modal kind", () => {
    expect(gpuiMainSource).toContain('"renameWorktree" => Some(Self::RenameWorktree)');
    expect(gpuiMainSource).toContain('Self::RenameWorktree => "renameWorktree"');
    expect(gpuiMainSource).toContain('Self::RenameWorktree => "Ghostex Rename Worktree"');
  });
});

describe("gpui/sidebar/gxserver-runtime.ts rename handlers", () => {
  test("handles both rename messages", () => {
    expect(gpuiRuntimeSource).toContain('case "promptRenameWorktreeForGroup":');
    expect(gpuiRuntimeSource).toContain('case "confirmRenameWorktree":');
    expect(gpuiRuntimeSource).toContain("private async promptRenameWorktreeForGroup(");
    expect(gpuiRuntimeSource).toContain("private async confirmRenameWorktree(");
  });

  test("calls the single rename endpoint rather than orchestrating git itself", () => {
    /*
     * Rollback for a failed move lives in gxserver, not here: a renderer that
     * reloads mid-rename must not be the only thing that can undo a half-applied
     * branch rename.
     */
    const confirm = sourceBetweenIn(
      gpuiRuntimeSource,
      "private async confirmRenameWorktree(",
      "private async promptDeleteRemoteWorktreeForGroup(",
    );

    expect(confirm).toContain('"/api/renameWorktreeProject"');
    expect(confirm).not.toContain('action: "move"');
    expect(confirm).not.toContain('action: "renameBranch"');
  });

  test("routes rename errors around the slash-stripping worktree error filter", () => {
    /*
     * `gpuiWorktreeUserVisibleErrorMessage` drops any message containing "/",
     * which is every rename refusal that names a branch. The rename flow needs
     * its own reader or the user gets a generic failure instead of
     * `Branch "feat/x" already exists.`
     */
    expect(gpuiRuntimeSource).toContain("function gpuiWorktreeRenameUserVisibleErrorMessage(");
    const confirm = sourceBetweenIn(
      gpuiRuntimeSource,
      "private async confirmRenameWorktree(",
      "private async promptDeleteRemoteWorktreeForGroup(",
    );
    expect(confirm).toContain("gpuiWorktreeRenameUserVisibleErrorMessage(error)");
    expect(confirm).not.toContain("gpuiWorktreeUserVisibleErrorMessage(error)");
  });

  test("translates an out-of-date daemon into something the user can act on", () => {
    /*
     * Verified live against a stale daemon: it answers
     * `notFound: "No gxserver endpoint for POST /api/renameWorktreeProject."`.
     * gxserver has more than one phrasing for an unroutable path, so the match
     * is on the ENDPOINT PATH, not on a sentence — an earlier version of this
     * guard keyed off one phrasing and silently failed to fire against the real
     * daemon. An error naming this route is always the daemon being older than
     * the app, never anything the user did.
     */
    const reader = sourceBetweenIn(
      gpuiRuntimeSource,
      "function gpuiWorktreeRenameUserVisibleErrorMessage(",
      "\n}",
    );

    expect(reader).toContain('message.includes("/api/renameWorktreeProject")');
    expect(reader).toContain("Quit Ghostex fully, reopen it, and try again.");
  });
});

describe("native/sidebar/modal-host.tsx rename modal", () => {
  test("registers the modal kind, its fit-height selector, and its open arm", () => {
    expect(modalHostSource).toContain('renameWorktree: ".worktree-rename-modal-shadcn"');
    expect(modalHostSource).toContain('message.modal === "renameWorktree"');
    expect(modalHostSource).toContain("worktreeRenameDraft?: WorktreeRenameModalDraft");
    expect(modalHostSource).toContain('type: "confirmRenameWorktree"');
  });
});
