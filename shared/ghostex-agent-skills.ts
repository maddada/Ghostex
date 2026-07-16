export type BundledGhostexAgentSkillId =
  | "browserUse"
  | "computerUse"
  | "agentOrchestration"
  | "fable56Orchestration"
  | "generateTitle"
  | "moveCodexSession";

export type BundledGhostexAgentSkill = {
  command: string;
  description: string;
  id: BundledGhostexAgentSkillId;
  name: string;
  skillName: string;
};

/**
 * CDXC:AgentSkills 2026-05-31-09:18:
 * Bundled Ghostex skills must be visible as individual user-installed items in
 * first launch and Settings. Keep the product copy and install commands in one
 * shared catalog so onboarding, settings, and status checks describe the same
 * four bundled skills without hiding them behind CLI installation.
 *
 * CDXC:CodexSessionMove 2026-06-26-13:24:
 * Bundle the Codex session-move guidance as its own installable skill so first
 * launch and Settings can install it with the app's other agent-facing skills.
 */
export const BUNDLED_GHOSTEX_AGENT_SKILLS: readonly BundledGhostexAgentSkill[] = [
  {
    command: "ghostex browser install-skill",
    description:
      "Teaches agents to inspect Ghostex browser panes, read console logs, capture screenshots, and interact with pages through the Ghostex Browser Use MCP server.",
    id: "browserUse",
    name: "Ghostex Browser Use",
    skillName: "ghostex-browser-use",
  },
  {
    command: "ghostex computer-use install-skill",
    description:
      "Teaches agents the Ghostex-named workflow for native macOS app automation through Cua Driver, including Accessibility and Screen Recording requirements.",
    id: "computerUse",
    name: "Ghostex Computer Use",
    skillName: "ghostex-computer-use",
  },
  {
    command: "ghostex agent-orchestration install-skill",
    description:
      "Teaches agents to coordinate Ghostex sessions through supported CLI commands for creating panes, sending messages, reading output, and checking status.",
    id: "agentOrchestration",
    name: "Ghostex Agent Orchestration",
    skillName: "ghostex-agent-orchestration",
  },
  {
    command: "ghostex fable-5.6-orchestration install-skill",
    description:
      "Teaches agents a plan-implement-verify pipeline over Ghostex panes: plan inline with Fable, launch a Codex gpt-5.6 worker pane per phase, then verify with a Fable pane and spawn fixers until verification passes.",
    id: "fable56Orchestration",
    name: "Ghostex Fable 5.6 Orchestration",
    skillName: "ghostex-fable-5.6-orchestration",
  },
  {
    command: "ghostex generate-title install-skill",
    description:
      "Teaches agents how to generate concise Ghostex session titles and submit the rename command in the current session.",
    id: "generateTitle",
    name: "Ghostex Generate Title",
    skillName: "ghostex-generate-title",
  },
  {
    command: "ghostex move-codex-session install-skill",
    description:
      "Teaches agents how to fork a Codex conversation into another folder with the correct session id, target root, and optional full-access mode.",
    id: "moveCodexSession",
    name: "Ghostex Move Codex Session",
    skillName: "ghostex-move-codex-session",
  },
];
