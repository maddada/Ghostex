import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const sessionRenameModalSource = readFileSync(
  new URL("../../../packages/core-ui/session-rename-modal.tsx", import.meta.url),
  "utf8",
);

describe("session rename modal focus source", () => {
  test("retries full-name selection across native child-window focus", () => {
    /*
    CDXC:SidebarRename 2026-06-15-01:27:
    Rename Session is presented from a hidden native child-window host, so initial React focus can run before the visible window becomes key. Source coverage keeps the focus/select request tied to native window focus and guarded by user interaction.
    */
    expect(sessionRenameModalSource).toContain("userInteractedAfterOpenRef.current = false;");
    expect(sessionRenameModalSource).toContain("input.focus({ preventScroll: true });");
    expect(sessionRenameModalSource).toContain("input.setSelectionRange(0, input.value.length);");
    expect(sessionRenameModalSource).not.toContain("input.select();");
    expect(sessionRenameModalSource).toContain(
      "const retryDelaysMs = [0, 16, 50, 100, 250, 500, 1000, 1600, 2400];",
    );
    expect(sessionRenameModalSource).toContain('window.addEventListener("focus", handleWindowFocus);');
    expect(sessionRenameModalSource).toContain(
      'window.removeEventListener("focus", handleWindowFocus);',
    );
    expect(sessionRenameModalSource).toContain("onKeyDownCapture={markUserInteractedAfterOpen}");
    expect(sessionRenameModalSource).toContain("onPointerDownCapture={markUserInteractedAfterOpen}");
  });
});
