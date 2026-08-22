export const DEFAULT_GIT_TEXT_GENERATION_PROVIDER = "codex";

export type GitTextGenerationProvider = "claude" | "codex" | "custom";

export type GitTextGenerationSettings = {
  customCommand: string;
  provider: GitTextGenerationProvider;
};

export function normalizeGitTextGenerationProvider(value: unknown): GitTextGenerationProvider {
  if (value === "claude" || value === "custom") {
    return value;
  }

  return DEFAULT_GIT_TEXT_GENERATION_PROVIDER;
}
