import { describe, expect, test } from "vitest";
import { trimPromptEditorTrailingSpaces } from "./prompt-editor-text";

describe("trimPromptEditorTrailingSpaces", () => {
  test("removes spaces and tabs before line endings and end of text", () => {
    expect(trimPromptEditorTrailingSpaces("first  \nsecond\t \nthird   ")).toBe(
      "first\nsecond\nthird",
    );
  });

  test("preserves indentation, internal spacing, blank lines, and CRLF endings", () => {
    expect(trimPromptEditorTrailingSpaces("  first value  \r\n\tsecond value\r\n\r\n")).toBe(
      "  first value\r\n\tsecond value\r\n\r\n",
    );
  });

  test("removes trailing spaces before old Mac carriage returns", () => {
    expect(trimPromptEditorTrailingSpaces("first  \rsecond\t \r")).toBe("first\rsecond\r");
  });
});
