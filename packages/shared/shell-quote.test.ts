import { describe, expect, test } from "vitest";
import { quoteShellDoubleArg } from "./shell-quote";

describe("quoteShellDoubleArg", () => {
  test("should wrap values in double quotes", () => {
    expect(quoteShellDoubleArg("my-session-id")).toBe('"my-session-id"');
    expect(quoteShellDoubleArg("Fix auth flow")).toBe('"Fix auth flow"');
  });

  test("should escape double quotes and shell expansions", () => {
    expect(quoteShellDoubleArg('say "hi"')).toBe('"say \\"hi\\""');
    expect(quoteShellDoubleArg("price $5")).toBe('"price \\$5"');
  });
});
