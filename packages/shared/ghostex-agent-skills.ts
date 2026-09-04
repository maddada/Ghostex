export type BundledGhostexAgentSkillId =
  | 'cli'
  | 'browserUse'
  | 'embeddedBrowserUse'
  | 'computerUse'
  | 'fable56Orchestration'
  | 'manageBeads'
  | 'generateTitle'
  | 'moveCodexSession';

export type BundledGhostexAgentSkillTier = 'recommended' | 'optional';

export type BundledGhostexAgentSkill = {
  command: string;
  description: string;
  id: BundledGhostexAgentSkillId;
  name: string;
  /**
   * Skills that drive the real machine or a real browser go through Trycua, so
   * the install surfaces group them under one shared Trycua install step
   * instead of letting agents discover the missing driver at the moment they
   * try to use it.
   */
  requiresCuaDriver?: boolean;
  /**
   * CDXC:AgentSkills 2026-08-24:
   * Some bundled skills stay installable through the CLI (and keep working when
   * already installed) but are deliberately absent from every app surface:
   * onboarding, Settings > Integrations, and settings search. Fable 5.6
   * Orchestration remains optional; the project-board skill is recommended.
   */
  hiddenFromUi?: boolean;
  skillName: string;
  tier: BundledGhostexAgentSkillTier;
};

/**
 * CDXC:Extensions 2026-08-24:
 * User-facing surfaces say "Trycua", never the `trycua/cua` repository slug or
 * an internal component name, so the prerequisite reads as one product the user
 * installs once.
 */
export const GHOSTEX_TRYCUA_PRODUCT_NAME = 'Trycua';

/**
 * CDXC:AgentSkills 2026-05-31-09:18:
 * Bundled Ghostex skills must be visible as individual user-installed items in
 * first launch and Settings. Keep the product copy and install commands in one
 * shared catalog so onboarding, settings, and status checks describe the same
 * bundled skills without hiding them behind CLI installation.
 *
 * CDXC:AgentSkills 2026-06-26-13:24:
 * Bundle the Codex session-move guidance as its own installable skill so first
 * launch and Settings can install it with the app's other agent-facing skills.
 *
 * CDXC:ProjectBoard 2026-08-24:
 * The Project Board beads skill shipped in the bundle with no way to install it,
 * so agents never learned to put the session they are working in on the card.
 * It belongs in this catalog like every other bundled skill.
 */
export const BUNDLED_GHOSTEX_AGENT_SKILLS: readonly BundledGhostexAgentSkill[] = [
  {
    command: 'ghostex cli install-skill',
    description:
      'The entry point for everything Ghostex: teaches agents help-first `ghostex` CLI discovery for sessions, orchestration, automations, projects, quick actions, chat queues, prompt history, and diagnostics.',
    id: 'cli',
    name: 'Ghostex CLI',
    skillName: 'ghostex-cli',
    tier: 'recommended',
  },
  {
    command: 'ghostex computer-use install-skill',
    description:
      'Let agents control your machine: click, type, and see the screen in native apps. Runs through Trycua, and macOS asks for Accessibility and Screen Recording permissions.',
    id: 'computerUse',
    name: 'Ghostex Computer Use',
    requiresCuaDriver: true,
    skillName: 'ghostex-computer-use',
    tier: 'recommended',
  },
  {
    command: 'ghostex browser-use install-skill',
    description:
      'Let agents control your browser: open pages, click, fill forms, and read what is on screen in supported external browsers.',
    id: 'browserUse',
    name: 'Ghostex Browser Use',
    requiresCuaDriver: true,
    skillName: 'ghostex-browser-use',
    tier: 'recommended',
  },
  {
    command: 'ghostex browser install-skill',
    description:
      'Let agents control the browser panes built into Ghostex: read console logs, capture screenshots, and interact with pages.',
    id: 'embeddedBrowserUse',
    name: 'Ghostex Embedded Browser Use',
    skillName: 'ghostex-embedded-browser-use',
    tier: 'recommended',
  },
  {
    command: 'ghostex fable-5.6-orchestration install-skill',
    description:
      'Use Claude Code Fable to orchestrate GPT 5.6 Sol sub-agents, then verify with Fable. A mix of the smartest model out there with the best implementer out there, for the best cost to performance.',
    id: 'fable56Orchestration',
    name: 'Ghostex Fable 5.6 Orchestration',
    skillName: 'ghostex-fable-56-orchestration',
    tier: 'optional',
  },
  {
    command: 'ghostex generate-title install-skill',
    description:
      'Teaches agents how to generate concise Ghostex session titles and submit the rename command in the current session.',
    hiddenFromUi: true,
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
    hiddenFromUi: true,
    id: 'moveCodexSession',
    name: 'Ghostex Move Codex Session',
    skillName: 'ghostex-move-codex-session',
    tier: 'optional',
  },
];

/** The bundled skills that app surfaces (onboarding, Settings, search) may show. */
export const VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS: readonly BundledGhostexAgentSkill[] =
  BUNDLED_GHOSTEX_AGENT_SKILLS.filter((skill) => skill.hiddenFromUi !== true);
