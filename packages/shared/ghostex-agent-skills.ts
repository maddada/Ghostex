export type BundledGhostexAgentSkillId =
  | 'cli'
  | 'help'
  | 'browserUse'
  | 'embeddedBrowserUse'
  | 'computerUse'
  | 'agentsOrchestration'
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
   * onboarding, Settings > Integrations, and settings search.
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
  /**
   * CDXC:AgentSkills 2026-09-09 DECISION:
   * User: ship a bundled help skill named ghostex-help that explains the app and configures its settings, installable like the other bundled skills.
   */
  {
    command: 'ghostex guide install-skill',
    description:
      'Let agents explain Ghostex and change its settings for you: ask how a feature works or what a setting does, and the agent answers from the built-in guide and applies the change through the ghostex CLI.',
    id: 'help',
    name: 'Ghostex Help',
    skillName: 'ghostex-help',
    tier: 'recommended',
  },
  {
    command: 'ghostex computer-use install-skill',
    description:
      'Let agents control your machine: click, type, and see the screen in native apps. Runs through Trycua, and your operating system may ask for accessibility and screen recording permissions.',
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
  /**
   * CDXC:AgentSkills 2026-09-19 DECISION:
   * User: add a bundled skill named "Ghostex Agents Orchestration" that just tells the agent to use the ghostex help commands to learn how to launch other agents with specific models and efforts and send and receive messages through the ghostex CLI, and remove the Fable 5.6 orchestration skill while keeping what was useful in it. It supersedes the pinned Fable plan / Codex implement / Fable verify pipeline skill, whose model-independent habits now live in the new skill text.
   */
  {
    command: 'ghostex agents-orchestration install-skill',
    description:
      'Let agents work as a team: teaches agents to launch other agents with the model and effort you ask for, message each other to hand off tasks and coordinate work, read the replies, and check the results, all through the `ghostex` CLI help.',
    id: 'agentsOrchestration',
    name: 'Ghostex Agents Orchestration',
    skillName: 'ghostex-agents-orchestration',
    tier: 'recommended',
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
