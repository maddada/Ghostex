import { DEFAULT_ghostex_SETTINGS } from "./defaults";
import { type SessionTitleGenerationAgent } from "./types";

export const SESSION_TITLE_GENERATION_AGENT_OPTIONS: ReadonlyArray<{
  label: string;
  value: SessionTitleGenerationAgent;
}> = [
  { label: "Codex", value: "codex" },
  { label: "Cursor CLI", value: "cursor" },
  { label: "Claude", value: "claude" },
  { label: "Grok Build", value: "grok" },
  { label: "Custom", value: "custom" },
];
export const SESSION_TITLE_GENERATION_PROMPT_PLACEHOLDER = "<title generation prompt>";

export function normalizeSessionTitleGenerationAgent(
  value: string | undefined,
): SessionTitleGenerationAgent {
  return value === "cursor" || value === "claude" || value === "grok" || value === "custom"
    ? value
    : DEFAULT_ghostex_SETTINGS.sessionTitleGenerationAgent;
}

export function normalizeCustomSessionTitleGenerationCommand(value: string | undefined): string {
  return (value ?? "").trim().slice(0, 240);
}

export function getSessionTitleGenerationCommandPreview(
  agent: SessionTitleGenerationAgent,
  options: { command?: string } = {},
): string {
  const command = readSessionTitleGenerationPreviewCommand(agent, options.command);
  const prompt = SESSION_TITLE_GENERATION_PROMPT_PLACEHOLDER;
  switch (agent) {
    case "codex":
      /*
      CDXC:SessionTitleSettings 2026-06-07-01:57:
      Settings must preview the same internal Codex title-generation command gxserver runs. Include `--ephemeral` so users see that generated titles do not create restorable Codex sessions.
      */
      return createSessionTitleGenerationHereDocPreview(
        `${command} exec --ephemeral --skip-git-repo-check -m gpt-5.6-luna -c 'model_reasoning_effort="low"'`,
        prompt,
      );
    case "cursor":
      return `${command} --print --yolo --trust --model cursor-grok-4.5-low --output-format text '${prompt}'`;
    case "claude":
      return createSessionTitleGenerationHereDocPreview(
        `${command} -p --model haiku --effort low`,
        prompt,
      );
    case "grok":
      return `${command} --model grok-4.5 --reasoning-effort low --output-format plain --no-alt-screen --no-plan --no-subagents --disable-web-search --max-turns 1 --single '${prompt}'`;
    case "custom":
      return createSessionTitleGenerationHereDocPreview(command, prompt);
  }
}

function readSessionTitleGenerationPreviewCommand(
  agent: SessionTitleGenerationAgent,
  command: string | undefined,
): string {
  const configured = command?.trim();
  if (configured) {
    return configured;
  }
  switch (agent) {
    case "codex":
      return "codex";
    case "cursor":
      return "cursor-agent";
    case "claude":
      return "claude";
    case "grok":
      return "grok";
    case "custom":
      return "<custom command>";
  }
}

function createSessionTitleGenerationHereDocPreview(command: string, prompt: string): string {
  return `${command} <<'PROMPT'\n${prompt}\nPROMPT`;
}
