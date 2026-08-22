import { describe, expect, test } from "vite-plus/test";
import {
  DEFAULT_GIT_TEXT_GENERATION_PROVIDER,
  normalizeGitTextGenerationProvider,
} from "./git-text-generation-provider";

describe("normalizeGitTextGenerationProvider", () => {
  test("should default to codex", () => {
    expect(normalizeGitTextGenerationProvider(undefined)).toBe(
      DEFAULT_GIT_TEXT_GENERATION_PROVIDER,
    );
  });

  test("should keep supported providers", () => {
    expect(normalizeGitTextGenerationProvider("codex")).toBe("codex");
    expect(normalizeGitTextGenerationProvider("claude")).toBe("claude");
    expect(normalizeGitTextGenerationProvider("custom")).toBe("custom");
  });

  test("should fall back for unsupported values", () => {
    expect(normalizeGitTextGenerationProvider("cursor")).toBe(DEFAULT_GIT_TEXT_GENERATION_PROVIDER);
  });
});
