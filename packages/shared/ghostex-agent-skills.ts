export type BundledGhostexAgentSkillId =
  | 'browserUse'
  | 'embeddedBrowserUse'
  | 'computerUse'
  | 'agentOrchestration'
  | 'fable56Orchestration'
  | 'findPrevSession'
  | 'generateTitle'
  | 'manageBeads'
  | 'moveCodexSession';

export type BundledGhostexAgentSkillTier = 'recommended' | 'optional';

export type BundledGhostexAgentSkill = {
  command: string;
  description: string;
  id: BundledGhostexAgentSkillId;
  name: string;
  /**
   * Skills that drive the real machine or a real browser go through Cua Driver
   * (https://github.com/trycua/cua), so the install surfaces offer that
   * one-time setup next to the skill instead of letting agents discover the
   * missing driver at the moment they try to use it.
   */
  requiresCuaDriver?: boolean;
  skillName: string;
  tier: BundledGhostexAgentSkillTier;
};

export const GHOSTEX_CUA_PROJECT_URL = 'https://github.com/trycua/cua';

/**
 * CDXC:AgentSkills 2026-05-31-09:18:
 * Bundled Ghostex skills must be visible as individual user-installed items in
 * first launch and Settings. Keep the product copy and install commands in one
 * shared catalog so onboarding, settings, and status checks describe the same
 * bundled skills without hiding them behind CLI installation.
 *
 * CDXC:CodexSessionMove 2026-06-26-13:24:
 * Bundle the Codex session-move guidance as its own installable skill so first
 * launch and Settings can install it with the app's other agent-facing skills.
 *
 * CDXC:BoardAssociateSession 2026-08-24:
 * The Project Board beads skill shipped in the bundle with no way to install it,
 * so agents never learned to put the session they are working in on the card.
 * It belongs in this catalog like every other bundled skill.
 */
export const BUNDLED_GHOSTEX_AGENT_SKILLS: readonly BundledGhostexAgentSkill[] = [
  {
    command: 'ghostex computer-use install-skill',
    description:
      'Teaches agents the Ghostex-named workflow for native macOS app automation through Cua Driver, including Accessibility and Screen Recording requirements.',
    id: 'computerUse',
    name: 'Ghostex Computer Use',
    requiresCuaDriver: true,
    skillName: 'ghostex-computer-use',
    tier: 'recommended',
  },
  {
    command: 'ghostex browser-use install-skill',
    description:
      "Teaches agents to inspect and operate supported external browser pages through Cua Driver's typed browser tools.",
    id: 'browserUse',
    name: 'Ghostex Browser Use',
    requiresCuaDriver: true,
    skillName: 'ghostex-browser-use',
    tier: 'recommended',
  },
  {
    command: 'ghostex browser install-skill',
    description:
      'Teaches agents to inspect Ghostex embedded browser panes, read console logs, capture screenshots, and interact with pages through the embedded browser MCP server.',
    id: 'embeddedBrowserUse',
    name: 'Ghostex Embedded Browser Use',
    skillName: 'ghostex-embedded-browser-use',
    tier: 'recommended',
  },
  {
    command: 'ghostex agent-orchestration install-skill',
    description:
      'Teaches agents to coordinate Ghostex sessions through supported CLI commands for creating panes, sending messages, reading output, and checking status.',
    id: 'agentOrchestration',
    name: 'Ghostex Agent Orchestration',
    skillName: 'ghostex-agent-orchestration',
    tier: 'optional',
  },
  {
    command: 'ghostex fable-5.6-orchestration install-skill',
    description:
      'Teaches agents a plan-implement-verify pipeline over Ghostex panes: plan inline with Fable, launch a Codex gpt-5.6 worker pane per phase, then verify with a Fable pane and spawn fixers until verification passes.',
    id: 'fable56Orchestration',
    name: 'Ghostex Fable 5.6 Orchestration',
    skillName: 'ghostex-fable-5.6-orchestration',
    tier: 'optional',
  },
  {
    command: 'ghostex find-prev-session install-skill',
    description:
      "Teaches agents to find, inspect, resume, or fork previous Claude Code, Codex, Pi, OpenCode, Cursor Agent, and Grok sessions with Ghostex's bundled Zehn search.",
    id: 'findPrevSession',
    name: 'Ghostex Find Previous Session',
    skillName: 'ghostex-find-prev-session',
    tier: 'optional',
  },
  {
    command: 'ghostex generate-title install-skill',
    description:
      'Teaches agents how to generate concise Ghostex session titles and submit the rename command in the current session.',
    id: 'generateTitle',
    name: 'Ghostex Auto Rename Session',
    skillName: 'ghostex-auto-rename-session',
    tier: 'optional',
  },
  {
    command: 'ghostex board install-skill',
    description:
      "Teaches agents to work a project board bead: move it through the board's statuses, comment progress on it, and link the session they are working in to the card so the board shows who has it.",
    id: 'manageBeads',
    name: 'Ghostex Project Board Beads',
    skillName: 'ghostex-manage-beads',
    tier: 'recommended',
  },
  {
    command: 'ghostex move-codex-session install-skill',
    description:
      'Teaches agents how to fork a Codex conversation into another folder with the correct session id, target root, and optional full-access mode.',
    id: 'moveCodexSession',
    name: 'Ghostex Move Codex Session',
    skillName: 'ghostex-move-codex-session',
    tier: 'optional',
  },
];
