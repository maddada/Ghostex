import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

import { buildAgentWorkPrompt } from "./project-board-shared";

/*
 * CDXC:BoardStartWork 2026-08-07:
 * gxserver's /api/startBoardWork owns a Rust port of buildAgentWorkPrompt so
 * `ghostex board start-work` sends the same canonical bead work prompt the
 * board sends. There must be exactly one prompt shape: this test renders the
 * TypeScript template with Rust-style placeholders and compares it to the
 * ordered Rust template sequence, so the two copies cannot drift silently.
 * This shared module is active GPUI code: gpui/sidebar/kanban-main.tsx loads
 * native/sidebar/tasks-placeholder.tsx, which imports project-board-shared.ts.
 */
describe("board start-work prompt parity", () => {
  test("the Rust work-prompt template matches buildAgentWorkPrompt line for line", () => {
    const rustSource = readFileSync(
      new URL("../../../server/src/board_start_work.rs", import.meta.url),
      "utf8",
    );
    const functionSource = rustSource.slice(
      rustSource.indexOf("pub(crate) fn build_board_bead_work_prompt"),
    );
    const entries = functionSource.match(
      /\n    \[\n(?<entries>[\s\S]*?)\n    \]\n    \.join\("\\n"\)/,
    )?.groups?.entries;
    if (!entries) {
      throw new Error("Could not extract the Rust work-prompt array.");
    }
    const rustPrompt = entries
      .trim()
      .split("\n")
      .map((line) => line.trim().replace(/,$/, ""))
      .map((expression) => {
        if (expression === "String::new()") {
          return "";
        }
        if (expression === "description.to_string()") {
          return "No prompt provided.";
        }
        const literal =
          expression.match(/^format!\(("(?:\\.|[^"\\])*")\)$/)?.[1] ??
          expression.match(/^("(?:\\.|[^"\\])*")\.to_string\(\)$/)?.[1];
        if (!literal) {
          throw new Error(`Unsupported Rust prompt expression: ${expression}`);
        }
        return JSON.parse(literal) as string;
      })
      .join("\n");
    const prompt = buildAgentWorkPrompt({
      boardStatus: "todo",
      displayId: "{display_id}",
      id: "{bead_id}",
      status: "open",
      title: "{title}",
    });
    expect(rustPrompt).toContain("No prompt provided.");
    expect(rustPrompt).toBe(prompt);
  });
});
