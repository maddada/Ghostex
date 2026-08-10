import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

/**
 * CDXC:WorktreeRename 2026-08-09-18:40:
 * The rename modal renders inside the native app-modal child window, which this
 * repo has no DOM harness for, so its contract is asserted against source text
 * the way `native/sidebar/tasks-placeholder-source.test.ts` does. What matters
 * here is not that the file compiles — typecheck already proves that — but that
 * the three things the daemon depends on stay true: the draft carries the flags
 * the runtime computed, submit sends the shared contract message, and Rename is
 * genuinely disabled when the rename cannot run.
 */

const worktreeRenameModalSource = readFileSync(
  new URL("./worktree-rename-modal.tsx", import.meta.url),
  "utf8",
);

function sourceBetween(start: string, end: string): string {
  const startIndex = worktreeRenameModalSource.indexOf(start);
  const endIndex = worktreeRenameModalSource.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return worktreeRenameModalSource.slice(startIndex, endIndex);
}

describe("worktree rename modal draft", () => {
  test("carries the flags the runtime resolved before the modal opened", () => {
    const draftType = sourceBetween("export type WorktreeRenameModalDraft = {", "};");

    expect(draftType).toContain("renameBranchDefault: boolean");
    expect(draftType).toContain("blockingReason?: string");
    expect(draftType).toContain("warnings?: readonly string[]");
    expect(draftType).toContain("currentName: string");
    expect(draftType).toContain("parentFolderName: string");
  });
});

describe("worktree rename modal submit", () => {
  test("sends the shared confirmRenameWorktree fields and nothing else", () => {
    const submit = sourceBetween("const submitRename = (", "};");

    expect(submit).toContain("onRename(draft.projectId, { name: trimmedName, renameBranch })");
    expect(submit).toContain("if (!canSubmit)");
    expect(worktreeRenameModalSource).not.toContain("destinationPath");
  });

  test("disables Rename for a blocking reason and for an invalid name", () => {
    const footer = sourceBetween("<DialogFooter", "</DialogFooter>");

    expect(footer).toContain("disabled={!canSubmit}");
    expect(worktreeRenameModalSource).toContain("const canSubmit = !submitError && !draft.blockingReason");
    expect(worktreeRenameModalSource).toContain("worktreeRenameNameError(name)");
  });

  test("previews the folder slug and the verbatim branch separately", () => {
    /*
     * CDXC:WorktreeRename 2026-08-09-18:40:
     * The folder and the branch are deliberately different strings for the same
     * typed name, so the preview must not collapse them into one line.
     */
    const preview = sourceBetween("worktree-rename-preview", "</FieldDescription>");

    expect(preview).toContain("Folder:");
    expect(preview).toContain("{nextFolderName");
    expect(preview).toContain("Branch:");
    expect(preview).toContain("{trimmedName");
    expect(worktreeRenameModalSource).toContain("worktreeRenameFolderSlug(name)");
  });
});
