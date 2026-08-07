import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "vitest";

import { buildAgentWorkPrompt } from "./project-board-shared";

/*
 * CDXC:BoardStartWork 2026-08-07:
 * gxserver's /api/startBoardWork owns a Rust port of buildAgentWorkPrompt so
 * `ghostex board start-work` sends the same canonical bead work prompt the
 * board sends. There must be exactly one prompt shape: this test renders the
 * TypeScript template with Rust-style placeholders and asserts every line is
 * present verbatim in the Rust template, so the two copies cannot drift apart
 * silently.
 */
describe("board start-work prompt parity", () => {
  test("the Rust work-prompt template matches buildAgentWorkPrompt line for line", () => {
    // Rust string literals escape double quotes, so unescape them before
    // comparing raw prompt lines against the template source.
    const rustSource = readFileSync(
      join(__dirname, "..", "..", "gxserver-rs", "src", "board_start_work.rs"),
      "utf8",
    ).replaceAll('\\"', '"');
    const prompt = buildAgentWorkPrompt({
      boardStatus: "todo",
      displayId: "{display_id}",
      id: "{bead_id}",
      status: "open",
      title: "{title}",
    });
    for (const line of prompt.split("\n")) {
      if (!line || line === "No prompt provided.") {
        // The default-description line is asserted separately below; blank
        // separator lines carry no template content.
        continue;
      }
      expect(rustSource).toContain(line);
    }
    expect(rustSource).toContain("No prompt provided.");
  });
});
