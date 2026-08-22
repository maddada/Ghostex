import { getVisiblePrimaryTitle, normalizeTerminalTitle } from "./session-grid-contract";

/**
 * CDXC:NativeOnlyCleanup 2026-05-05-02:22
 * Native ghostex owns active terminal/session behavior. Keep first-prompt
 * title helpers in shared code so the retired VS Code extension tree can be
 * removed without changing native Ghostty session naming.
 */

export type FirstPromptAutoRenameStrategy =
  | "sendBareRenameCommand"
  | "generateTitleAndRename"
  | "generateTitleAndName";
export type FirstPromptAutoRenameDecisionReason =
  | "alreadyAutoNamed"
  | "alreadyPending"
  | "eligible"
  | "emptyPrompt"
  | "metaPrompt"
  | "nonGenericCurrentTitle"
  | "slashCommand"
  | "unsupportedAgent";

export type FirstPromptAutoRenameDecision = {
  normalizedPrompt?: string;
  reason: FirstPromptAutoRenameDecisionReason;
  shouldAutoName: boolean;
  strategy?: FirstPromptAutoRenameStrategy;
};

const META_PROMPT_PREFIXES = [
  "<command",
  "<environment_context",
  "<permissions instructions>",
  "<user_instructions>",
  "<INSTRUCTIONS>",
  "<collaboration_mode>",
  "<app-context>",
  "<turn_aborted>",
  "<ide_opened_file>",
  "<local-",
  "[Tool Result]",
  "Caveat:",
] as const;

const GENERIC_SESSION_TITLES_BY_AGENT = new Map<string, ReadonlySet<string>>([
  ["claude", new Set(["claude", "claude code", "claude session"])],
  ["codex", new Set(["codex", "openai codex", "codex cli"])],
  /**
   * CDXC:CursorCLI 2026-05-19-15:35:
   * Cursor CLI sessions start with placeholder names until the CLI publishes a
   * real terminal title. Treat those placeholders as generic so terminal-title
   * sync can persist the CLI-provided name without fighting first-prompt rename.
   */
  ["cursor", new Set(["cursor", "cursor agent", "cursor cli", "cursor-agent"])],
  /**
   * CDXC:AntigravityCLI 2026-05-19-18:45:
   * Antigravity CLI keeps the short `agy` terminal title while running. Treat
   * that placeholder as generic so richer auto titles can sync when available.
   */
  ["antigravity", new Set(["agy", "antigravity", "antigravity cli"])],
  ["gemini", new Set(["gemini"])],
  ["opencode", new Set(["opencode", "open code"])],
  ["pi", new Set(["pi", "π"])],
]);
const LEADING_PROMPT_FILLER_PATTERN =
  /^(?:(?:please|kindly|hey|hi|hello)\s+|(?:can|could|would|will)\s+you\s+|(?:can|could|would)\s+we\s+|help\s+me\s+|i\s+need(?:\s+you)?\s+to\s+|i\s+need\s+|how\s+do\s+i\s+|how\s+does\s+|is\s+there\s+(?:any\s+)?way\s+to\s+)+/iu;
const LEADING_SLASH_COMMAND_LINE_PATTERN =
  /(?:^|\r?\n)[ \t]*\/[a-z][\w-]*(?=\s|$|[).,:;!?'"`])/iu;
const MAX_SLASH_COMMAND_AUTO_RENAME_BLOCK_LENGTH = 50;

export function resolveFirstPromptAutoRenameStrategy(
  agentName: string | undefined,
): FirstPromptAutoRenameStrategy | undefined {
  const normalizedAgentName = agentName?.trim().toLowerCase();
  if (normalizedAgentName === "claude" || normalizedAgentName === "claude code") {
    /**
     * CDXC:SessionTitleSync 2026-06-12-07:08:
     * Claude Code can leave newly working sessions at the generic `Claude Code`
     * title. Send a bare `/rename` for unrenamed Claude sessions because
     * Claude can generate the title itself from the active conversation.
     */
    return "sendBareRenameCommand";
  }

  if (normalizedAgentName === "codex") {
    return "generateTitleAndRename";
  }

  if (
    normalizedAgentName === "cursor" ||
    normalizedAgentName === "cursor agent" ||
    normalizedAgentName === "cursor cli" ||
    normalizedAgentName === "cursor-agent"
  ) {
    /**
     * CDXC:SessionTitleSync 2026-05-30-05:44:
     * Cursor Agent already names its sessions automatically. Do not run
     * Ghostex first-prompt auto-title generation for Cursor sessions.
     */
    return undefined;
  }

  if (normalizedAgentName === "pi" || normalizedAgentName === "π") {
    /**
     * CDXC:PiAgent 2026-05-08-09:42
     * Pi's CLI names sessions with `/name <title>` instead of Codex's
     * `/rename <title>`. Keep the generation policy shared while letting the
     * native sender choose Pi's command syntax.
     */
    return "generateTitleAndName";
  }

  return undefined;
}

export function isGenericAgentSessionTitle(
  agentName: string | undefined,
  title: string | undefined,
): boolean {
  /**
   * CDXC:SessionTitleSync 2026-04-28-03:49
   * First-prompt auto-title is allowed only while the session is effectively
   * untitled. Placeholder creation names and path-like shell titles are not
   * persisted names, but user/terminal/generated meaningful titles must block
   * generation so hooks cannot overwrite established session titles.
   */
  if (title !== undefined && !getVisiblePrimaryTitle(title)) {
    return true;
  }

  const normalizedTitle = normalizeTerminalTitle(title)?.toLowerCase();
  if (!normalizedTitle) {
    return true;
  }

  const genericTitles = GENERIC_SESSION_TITLES_BY_AGENT.get(agentName?.trim().toLowerCase() ?? "");
  return genericTitles ? genericTitles.has(normalizedTitle) : false;
}

export function shouldAutoNameSessionFromFirstPrompt(input: {
  agentName: string | undefined;
  currentTitle: string | undefined;
  hasAutoTitleFromFirstPrompt?: boolean;
  pendingFirstPromptAutoRenamePrompt?: string;
  prompt: string | undefined;
}): boolean {
  return explainFirstPromptAutoRenameDecision(input).shouldAutoName;
}

export function getCurrentTitleForFirstPromptAutoRename(input: {
  agentName: string | undefined;
  pendingPrompt: string | undefined;
  persistedTitle: string | undefined;
  protectStoredTitleFromAutomation?: boolean;
  sessionTitle: string | undefined;
  terminalTitle: string | undefined;
}): string | undefined {
  /**
   * CDXC:SessionTitleSync 2026-05-08-16:23
   * First-prompt auto-rename may claim a terminal-auto title only while the
   * session is still effectively generic, such as `Codex Session`. A meaningful
   * Codex-provided terminal title is already the desired session name and must
   * block `/rename <generated title>` from being sent later for the same prompt.
   */
  const shouldClaimGenericCurrentTitle =
    Boolean(input.pendingPrompt?.trim()) &&
    input.protectStoredTitleFromAutomation !== true &&
    isGenericAgentSessionTitle(input.agentName, input.sessionTitle);
  return shouldClaimGenericCurrentTitle
    ? undefined
    : input.persistedTitle || input.sessionTitle || input.terminalTitle;
}

export function explainFirstPromptAutoRenameDecision(input: {
  agentName: string | undefined;
  currentTitle: string | undefined;
  hasAutoTitleFromFirstPrompt?: boolean;
  pendingFirstPromptAutoRenamePrompt?: string;
  prompt: string | undefined;
}): FirstPromptAutoRenameDecision {
  const strategy = resolveFirstPromptAutoRenameStrategy(input.agentName);
  if (!strategy) {
    return {
      reason: "unsupportedAgent",
      shouldAutoName: false,
    };
  }

  if (input.hasAutoTitleFromFirstPrompt) {
    return {
      reason: "alreadyAutoNamed",
      shouldAutoName: false,
      strategy,
    };
  }

  if (input.pendingFirstPromptAutoRenamePrompt?.trim()) {
    return {
      reason: "alreadyPending",
      shouldAutoName: false,
      strategy,
    };
  }

  if (!input.prompt?.trim()) {
    return {
      reason: "emptyPrompt",
      shouldAutoName: false,
      strategy,
    };
  }

  const normalizedPrompt = normalizePrompt(input.prompt);
  if (!normalizedPrompt) {
    return {
      reason: "emptyPrompt",
      shouldAutoName: false,
      strategy,
    };
  }

  if (isMetaPrompt(normalizedPrompt)) {
    return {
      normalizedPrompt,
      reason: "metaPrompt",
      shouldAutoName: false,
      strategy,
    };
  }

  if (
    normalizedPrompt.length <= MAX_SLASH_COMMAND_AUTO_RENAME_BLOCK_LENGTH &&
    containsLeadingSlashCommandLine(input.prompt)
  ) {
    return {
      normalizedPrompt,
      reason: "slashCommand",
      shouldAutoName: false,
      strategy,
    };
  }

  if (!isGenericAgentSessionTitle(input.agentName, input.currentTitle)) {
    return {
      normalizedPrompt,
      reason: "nonGenericCurrentTitle",
      shouldAutoName: false,
      strategy,
    };
  }

  return {
    normalizedPrompt,
    reason: "eligible",
    shouldAutoName: true,
    strategy,
  };
}

function normalizePrompt(prompt: string): string | undefined {
  const normalizedPrompt = prompt.replace(/\s+/g, " ").trim();
  if (!normalizedPrompt) {
    return undefined;
  }

  const strippedPrompt = normalizedPrompt.replace(LEADING_PROMPT_FILLER_PATTERN, "").trim();
  const cleanedPrompt = (strippedPrompt || normalizedPrompt).replace(/[.?!:;,]+$/g, "").trim();
  return cleanedPrompt || undefined;
}

function containsLeadingSlashCommandLine(prompt: string): boolean {
  /**
   * CDXC:SessionTitleSync 2026-05-30-05:18:
   * First-prompt auto-title should still run when a normal request discusses
   * slash commands, such as asking Ghostex to send `/rename <title>`.
   * Suppress only short slash-command invocations that start a line. Longer
   * text is a user request worth naming even if it begins with a slash command.
   */
  return LEADING_SLASH_COMMAND_LINE_PATTERN.test(prompt);
}

function isMetaPrompt(prompt: string): boolean {
  if (prompt.startsWith("# AGENTS")) {
    return true;
  }

  if (prompt.includes("tool_use_id")) {
    return true;
  }

  return META_PROMPT_PREFIXES.some((prefix) => prompt.startsWith(prefix));
}
