import { describe, expect, test } from "vite-plus/test";
import { normalizeSidebarPinnedPrompts } from "./sidebar-pinned-prompts";

describe("normalizeSidebarPinnedPrompts", () => {
  test("should keep multiline prompt content intact", () => {
    expect(
      normalizeSidebarPinnedPrompts([
        {
          content: "Line one\nLine two",
          createdAt: "2026-04-16T06:00:00.000Z",
          promptId: "prompt-1",
          title: "Multiline note",
          updatedAt: "2026-04-16T07:00:00.000Z",
        },
      ]),
    ).toEqual([
      {
        content: "Line one\nLine two",
        createdAt: "2026-04-16T06:00:00.000Z",
        promptId: "prompt-1",
        title: "Multiline note",
        updatedAt: "2026-04-16T07:00:00.000Z",
      },
    ]);
  });

  test("should sort prompts by newest updatedAt first", () => {
    expect(
      normalizeSidebarPinnedPrompts([
        {
          content: "Older",
          createdAt: "2026-04-16T06:00:00.000Z",
          promptId: "prompt-1",
          title: "Older",
          updatedAt: "2026-04-16T07:00:00.000Z",
        },
        {
          content: "Newer",
          createdAt: "2026-04-16T06:30:00.000Z",
          promptId: "prompt-2",
          title: "Newer",
          updatedAt: "2026-04-16T08:00:00.000Z",
        },
      ]).map((prompt) => prompt.promptId),
    ).toEqual(["prompt-2", "prompt-1"]);
  });

  test("should derive a title from the first non-empty content line when title is missing", () => {
    expect(
      normalizeSidebarPinnedPrompts([
        {
          content: "\n\nFirst real line\nSecond line",
          createdAt: "2026-04-16T06:00:00.000Z",
          promptId: "prompt-1",
          updatedAt: "2026-04-16T07:00:00.000Z",
        },
      ])[0]?.title,
    ).toBe("First real line");
  });
});
