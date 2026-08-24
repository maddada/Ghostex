/*
 * Status payload builders.
 *
 * Every message here is the REAL shared contract type, built the way
 * packages/core-ui/first-launch-setup-modal.stories.tsx:13-35 builds its fixtures (from
 * DEFAULT_SIDEBAR_AGENTS), so the production modals validate and render them
 * unchanged.
 *
 * Derivation rules mirror:
 * - server/src/agent_hooks/api.rs read_hook_status (per-agent status + detail)
 * - apps/desktop/src/app/helpers/os_cli/cli_status.rs gpui_ghostex_cli_status_message (CLI/skill/cua payload)
 */
import type {
  SidebarAgentHookStatus,
  SidebarAgentHookStatusItem,
  SidebarAgentHookStatusMessage,
  SidebarGhostexCliStatusMessage,
  SidebarHydrateMessage,
} from '@/packages/shared/session-grid-contract';
import { DEFAULT_SIDEBAR_AGENTS } from '@/packages/shared/sidebar-agents';
import { DEFAULT_ghostex_SETTINGS, normalizeghostexSettings } from '@/packages/shared/ghostex-settings';
import { createSidebarStoryMessage, type SidebarStoryArgs } from '@/packages/core-ui/sidebar-story-fixtures';
import { PRIORITY_AGENT_IDS, SIM_AGENT_IDS, type SimAgentId, type SimEnvState } from '../state/types';

const HOOK_STATE_DIRECTORY = '~/.ghostexterm';
const NOTIFY_HOOK_PATH = '~/.ghostexterm/notify-agent-status.js';

const AGENT_CONFIG_PATHS: Partial<Record<SimAgentId, string>> = {
  codex: '~/.codex/config.toml',
  claude: '~/.claude/settings.json',
  opencode: '~/.config/opencode/opencode.json',
  pi: '~/.pi/config.json',
};

export function agentCliCommand(agentId: string): string {
  const agent = DEFAULT_SIDEBAR_AGENTS.find((entry) => entry.agentId === agentId);
  const command = agent?.command ?? agentId;
  return command.split(' ')[0] ?? command;
}

export function agentDisplayName(agentId: string): string {
  return DEFAULT_SIDEBAR_AGENTS.find((entry) => entry.agentId === agentId)?.name ?? agentId;
}

/** server/src/agent_hooks/api.rs read_hook_status:218 */
export function deriveAgentHookStatus(env: SimEnvState, agentId: SimAgentId): SidebarAgentHookStatus {
  const agent = env.agents[agentId];
  if (!agent.cliInstalled) {
    return 'cliMissing';
  }
  if (agent.hookState === 'installed') {
    return 'installed';
  }
  if (agent.hookState === 'outdated') {
    return 'updateRequired';
  }
  return 'missing';
}

/** server/src/agent_hooks/api.rs hook_detail:259 */
function hookDetail(agentId: SimAgentId, status: SidebarAgentHookStatus): string {
  const display = AGENT_CONFIG_PATHS[agentId] ?? `${HOOK_STATE_DIRECTORY}/${agentId}.json`;
  switch (status) {
    case 'cliMissing':
      return `${agentCliCommand(agentId)} was not found on PATH.`;
    case 'installed':
      return `Installed in ${display}`;
    case 'updateRequired':
      return `Run Update Hooks to repair ${display}`;
    default:
      return `Run Install Hooks to write ${display}`;
  }
}

export function agentHookStatusItem(env: SimEnvState, agentId: SimAgentId): SidebarAgentHookStatusItem {
  const status = deriveAgentHookStatus(env, agentId);
  return {
    agentId,
    cliCommand: agentCliCommand(agentId),
    cliInstalled: env.agents[agentId].cliInstalled,
    detail: hookDetail(agentId, status),
    hookInstalled: status === 'installed',
    paths: [AGENT_CONFIG_PATHS[agentId] ?? `${HOOK_STATE_DIRECTORY}/${agentId}.json`],
    status,
  };
}

export function createAgentHookStatusMessage(
  env: SimEnvState,
  agentIds: readonly SimAgentId[]
): SidebarAgentHookStatusMessage {
  return {
    agents: agentIds.map((agentId) => agentHookStatusItem(env, agentId)),
    generatedAt: new Date().toISOString(),
    hookStateDirectory: HOOK_STATE_DIRECTORY,
    notifyHookPath: NOTIFY_HOOK_PATH,
    type: 'agentHookStatus',
  };
}

/**
 * apps/desktop/src/app/helpers/agents_hub/agent_hook_status.rs gpui_ordered_agent_hook_status_agent_ids:20 — priority
 * agents first (codex, claude, opencode, pi), then everything else requested.
 */
export function orderedHookStatusAgentIds(requested: readonly string[] | undefined): SimAgentId[] {
  const requestedIds = (requested && requested.length > 0 ? requested : SIM_AGENT_IDS)
    .map((agentId) => agentId.trim())
    .filter((agentId): agentId is SimAgentId => (SIM_AGENT_IDS as readonly string[]).includes(agentId));
  const seen = new Set<SimAgentId>(requestedIds);
  const ordered = PRIORITY_AGENT_IDS.filter((agentId) => seen.has(agentId));
  for (const agentId of requestedIds) {
    if (!PRIORITY_AGENT_IDS.includes(agentId) && !ordered.includes(agentId)) {
      ordered.push(agentId);
    }
  }
  return ordered;
}

export function createGhostexCliStatusMessage(
  env: SimEnvState,
  detailOverride?: string
): SidebarGhostexCliStatusMessage {
  const skills = env.bundledSkills;
  const installed = env.ghostexCli.installed;
  const detail =
    detailOverride ??
    (installed
      ? env.ghostexCli.gxUsable
        ? 'Ghostex CLI is installed automatically with the app. Use ghostex for the full command or gx for the short alias.'
        : env.ghostexCli.gxBlockedByExistingCommand
          ? 'Another gx command already exists on PATH, so Ghostex did not replace it. Use ghostex for the full command.'
          : 'Ghostex CLI is installed, but the gx alias is not usable yet.'
      : 'Ghostex CLI auto-install did not find a usable ghostex command on PATH.');
  return {
    agentOrchestrationSkillInstalled: skills.agentOrchestration,
    ...(skills.agentOrchestration
      ? { agentOrchestrationSkillPath: '~/agents/skills/ghostex-agent-orchestration/SKILL.md' }
      : {}),
    browserSkillInstalled: skills.browser,
    ...(skills.browser ? { browserSkillPath: '~/agents/skills/ghostex-browser-use/SKILL.md' } : {}),
    computerUseSkillInstalled: skills.computerUse,
    ...(skills.computerUse ? { computerUseSkillPath: '~/agents/skills/ghostex-computer-use/SKILL.md' } : {}),
    cuaAppInstalled: env.cuaDriver.appInstalled,
    cuaDriverAccessibilityPermissionGranted: env.cuaDriver.accessibilityPermission,
    cuaDriverInstalled: env.cuaDriver.cliInstalled,
    cuaDriverManagedUpdatesSupported: true,
    ...(env.cuaDriver.cliInstalled ? { cuaDriverPath: '~/.local/bin/cua-driver' } : {}),
    cuaDriverScreenRecordingPermissionGranted: env.cuaDriver.screenRecordingPermission,
    detail,
    embeddedBrowserSkillInstalled: skills.embeddedBrowser,
    ...(skills.embeddedBrowser
      ? { embeddedBrowserSkillPath: '~/agents/skills/ghostex-embedded-browser-use/SKILL.md' }
      : {}),
    fable56OrchestrationSkillInstalled: skills.fable56Orchestration,
    ...(skills.fable56Orchestration
      ? {
          fable56OrchestrationSkillPath: '~/agents/skills/ghostex-fable-5.6-orchestration/SKILL.md',
        }
      : {}),
    findPrevSessionSkillInstalled: skills.findPrevSession,
    ...(skills.findPrevSession
      ? { findPrevSessionSkillPath: '~/agents/skills/ghostex-find-prev-session/SKILL.md' }
      : {}),
    generateTitleSkillInstalled: skills.generateTitle,
    ...(skills.generateTitle ? { generateTitleSkillPath: '~/agents/skills/ghostex-generate-title/SKILL.md' } : {}),
    generatedAt: new Date().toISOString(),
    ...(installed ? { ghostexPath: '/usr/local/bin/ghostex' } : {}),
    gxBlockedByExistingCommand: env.ghostexCli.gxBlockedByExistingCommand,
    ...(env.ghostexCli.gxUsable ? { gxPath: '/usr/local/bin/gx' } : {}),
    gxUsable: env.ghostexCli.gxUsable,
    installed,
    moveCodexSessionSkillInstalled: skills.moveCodexSession,
    ...(skills.moveCodexSession
      ? { moveCodexSessionSkillPath: '~/agents/skills/ghostex-move-codex-session/SKILL.md' }
      : {}),
    type: 'ghostexCliStatus',
  };
}

/**
 * `areFirstLaunchAgentHooksReady`: ANY priority agent is installed/notRequired.
 */
export function areFirstLaunchAgentHooksReady(env: SimEnvState): boolean {
  return PRIORITY_AGENT_IDS.some((agentId) => {
    const status = deriveAgentHookStatus(env, agentId);
    return status === 'installed' || status === 'notRequired';
  });
}

/** `areFirstLaunchBundledSkillsInstalled`: ALL 8 bundled skills installed. */
export function areFirstLaunchBundledSkillsInstalled(env: SimEnvState): boolean {
  return Object.values(env.bundledSkills).every(Boolean);
}

const SANDBOX_STORY_ARGS: SidebarStoryArgs = {
  createSessionOnSidebarDoubleClick: false,
  debuggingMode: false,
  fixture: 'default',
  highlightedVisibleCount: 1,
  isFocusModeActive: false,
  renameSessionOnDoubleClick: false,
  showCloseButtonOnSessionCards: true,
  showSessionCloseContextMenuAction: false,
  showSessionCommandCopyActions: false,
  showSessionDetailsCopyAction: false,
  theme: 'dark-blue',
  viewMode: 'grid',
  visibleCount: 1,
};

/**
 * The hydrate the modal host needs before Settings/first-launch modals become
 * renderable (`revision > 0`). `createSidebarStoryMessage` only honors a
 * settings override for its combined-reference fixtures, so the env-derived
 * settings are merged onto the returned HUD here instead of being passed in.
 */
export function createSandboxHydrateMessage(env: SimEnvState): SidebarHydrateMessage {
  const base = createSidebarStoryMessage(SANDBOX_STORY_ARGS);
  const settings = normalizeghostexSettings({
    ...DEFAULT_ghostex_SETTINGS,
    debuggingMode: env.settings.debuggingMode,
    sessionPersistenceProvider: env.settings.sessionPersistenceOff ? 'off' : 'zmx',
  });
  return {
    ...base,
    hud: {
      ...base.hud,
      debuggingMode: env.settings.debuggingMode,
      settings,
    },
  };
}
