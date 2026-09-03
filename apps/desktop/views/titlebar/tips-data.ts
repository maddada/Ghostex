import {
  getDefaultSidebarAgentById,
  getDefaultSidebarAgentByIcon,
  type SidebarAgentIcon,
} from '@/packages/shared/sidebar-agents';
import type {
  SidebarAgentHookStatusMessage,
  SidebarGhostexCliStatusMessage,
} from '@/packages/shared/session-grid-contract-sidebar';
import type { TitlebarNotice, TitlebarResourceGroup, TitlebarResourceSession, TitlebarTip } from './types';

/**
 * CDXC:TipsAndTricks 2026-05-30-08:31:
 * Tips are authored in code, not by end users in the dropdown. Keep this array
 * as the ordered source of truth so adding, removing, or reordering tips is a
 * normal code edit while read state survives app updates by stable tip id.
 *
 * CDXC:TipsAndTricks 2026-06-05-12:39:
 * The dropdown should teach users early that the sidebar is highly customizable.
 * Keep this as the second built-in tip so it appears immediately after the command-palette hint for users who have not marked it read.
 *
 * CDXC:TipsAndTricks 2026-06-13-10:26:
 * The first tip should introduce Cmd Shift P as the universal entry point for app actions, not only pane moves.
 *
 * CDXC:TipsAndTricks 2026-06-28-08:00:
 * Tips should actively teach the agent-facing Browser Use and Computer Use
 * skills. Ghostex-owned skills deep-link to Settings > Integrations with the
 * relevant row searched.
 */
export const TITLEBAR_TIPS: TitlebarTip[] = [
  {
    body: 'Search for project actions, pane splits and moves, session controls, settings shortcuts, and other Ghostex actions.',
    icon: 'command',
    id: 'command-palette-all-actions',
    title: 'Press Cmd Shift P anywhere to open Ghostex Quick Access',
  },
  {
    body: 'Open Settings to customize sidebar presets, visible details, agents, actions, project tools, and workspace open targets.',
    icon: 'sidebar',
    id: 'customize-sidebar-layout-and-tools',
    title: 'Customize the sidebar',
  },
  {
    body: 'The Resources menu can sleep inactive terminal sessions while keeping them restorable in the sidebar.',
    icon: 'moon',
    id: 'sleep-idle-sessions-from-resources',
    title: 'Sleep idle sessions from Resources',
  },
  {
    body: 'Click Add Worktree on a project header so a second agent can work on a branch without touching the main checkout.',
    icon: 'sidebar',
    id: 'run-same-project-in-a-worktree',
    title: 'Run the same project in a worktree',
  },
  {
    action: {
      settingsSearchQuery: 'Ghostex Computer Use',
      type: 'openSettings',
    },
    body: 'Configure Ghostex Computer Use in Settings, then ask agents to use /ghostex-computer-use for native macOS app control.',
    icon: 'resources',
    id: 'use-ghostex-computer-use-skill',
    title: 'Use /ghostex-computer-use for desktop control',
  },
  {
    action: {
      settingsSearchQuery: 'Ghostex Browser Use',
      type: 'openSettings',
    },
    body: 'Configure Ghostex Browser Use in Settings, then ask agents to use /ghostex-browser-use for page inspection, console logs, screenshots, and clicks.',
    icon: 'browser',
    id: 'use-ghostex-browser-use-skill',
    title: 'Use /ghostex-browser-use for browser panes',
  },
  {
    action: {
      settingsSearchQuery: 'Ghostex Embedded Browser Use',
      type: 'openSettings',
    },
    body: 'Configure Ghostex Embedded Browser Use in Settings, then ask agents to use /ghostex-embedded-browser-use for page inspection, console logs, screenshots, and clicks in Ghostex panes.',
    icon: 'browser',
    id: 'use-ghostex-embedded-browser-use-skill',
    title: 'Use /ghostex-embedded-browser-use for Ghostex panes',
  },
  {
    body: 'Open the Automate tab to run agents on a schedule without sitting in the session.',
    icon: 'command',
    id: 'schedule-recurring-agent-work',
    title: 'Schedule recurring agent work',
  },
  {
    body: 'Open More Options in the top right of the sidebar, click "Mobile & Remote", then follow the two steps: get the Ghostex app and connect it to this computer.',
    icon: 'resources',
    id: 'continue-session-from-mobile-app',
    title: 'Continue a session from the Mobile app',
  },
  {
    body: 'Open More Options in the top right of the sidebar, click "Search by Prompt", then type any words you remember from the prompt.',
    icon: 'search',
    id: 'find-session-by-prompt-text',
    title: 'Find any session from prompt text',
  },
  {
    body: 'In Search by Prompt, favorite a prompt so it stays at the top the next time you search.',
    icon: 'search',
    id: 'star-prompts-you-want-again',
    title: 'Star prompts you want again',
  },
  {
    body: 'Then you can easily ask agents to "work on beads with   high priority from the kanban board"',
    icon: 'command',
    id: 'add-todos-to-kanban-page',
    title: 'Add all your Todos in the Kanban page',
  },
];

/**
 * CDXC:DiagnosticsSettings 2026-06-06-07:09:
 * Debugging Mode previously wrote detailed diagnostics to disk and could affect
 * performance.
 *
 * CDXC:DiagnosticsSettings 2026-06-27-22:07:
 * Debugging Mode now exposes debug UI only. Keep the notice, but point users to
 * scenario-specific disk logging so turning on debug controls does not imply
 * every routine support log is active.
 */
export const TITLEBAR_DEBUGGING_MODE_NOTICE: TitlebarNotice = {
  body: 'Ghostex is showing debug UI controls. Routine disk logging is controlled by Diagnostic disk logging scenarios in Settings.',
  icon: 'warning',
  id: 'debugging-mode-enabled',
  settingsTarget: 'debuggingMode',
  title: 'Debug mode is on',
};

export function createTitlebarGhostexCliNotice(
  ghostexCliStatus: SidebarGhostexCliStatusMessage | undefined
): TitlebarNotice | undefined {
  /**
   * CDXC:CliInstall 2026-06-07-15:26:
   * Tips & Tricks should warn when either public CLI command is not accessible
   * on PATH. Keep the description to three lines or less while naming concrete
   * benefits: terminal commands, mobile attach, and agent integration skills.
   */
  if (!ghostexCliStatus || (ghostexCliStatus.installed === true && ghostexCliStatus.gxUsable === true)) {
    return undefined;
  }
  return {
    body: 'Install or repair the CLI to use ghostex/gx in any terminal, attach mobile clients, and install the Ghostex CLI/Browser/Computer agent skills.',
    icon: 'warning',
    id: 'ghostex-cli-not-accessible',
    settingsTarget: 'ghostexCli',
    title: 'Ghostex CLI is not accessible',
  };
}

export function createTitlebarMissingAgentHooksNotice(
  resourceGroups: TitlebarResourceGroup[],
  agentHookStatus: SidebarAgentHookStatusMessage | undefined
): TitlebarNotice | undefined {
  if (!agentHookStatus || agentHookStatus.errorMessage) {
    return undefined;
  }
  const hookStatusByAgentId = new Map(agentHookStatus.agents.map((status) => [status.agentId, status]));
  const liveSupportedAgentIds = new Set<string>();
  for (const group of resourceGroups) {
    for (const session of group.sessions) {
      if (!isTitlebarLiveTerminalAgentSession(session)) {
        continue;
      }
      const agent = getDefaultSidebarAgentByIcon(session.agentIcon as SidebarAgentIcon | undefined);
      if (!agent) {
        continue;
      }
      liveSupportedAgentIds.add(agent.agentId);
    }
  }
  const missingAgents = new Map<string, string>();
  const outdatedAgents = new Map<string, string>();
  for (const status of agentHookStatus.agents) {
    if (
      !status.cliInstalled ||
      status.status === 'installed' ||
      status.status === 'notRequired' ||
      status.status === 'cliMissing'
    ) {
      continue;
    }
    const agent = getDefaultSidebarAgentById(status.agentId);
    if (!agent) {
      continue;
    }
    if (status.status === 'updateRequired') {
      outdatedAgents.set(agent.agentId, agent.name);
    } else {
      missingAgents.set(agent.agentId, agent.name);
    }
  }
  const prioritizedAgentNames = [
    ...[...outdatedAgents].filter(([agentId]) => liveSupportedAgentIds.has(agentId)).map(([, name]) => name),
    ...[...missingAgents].filter(([agentId]) => liveSupportedAgentIds.has(agentId)).map(([, name]) => name),
    ...[...outdatedAgents].filter(([agentId]) => !liveSupportedAgentIds.has(agentId)).map(([, name]) => name),
    ...[...missingAgents].filter(([agentId]) => !liveSupportedAgentIds.has(agentId)).map(([, name]) => name),
  ];
  const agentNames = prioritizedAgentNames;
  if (agentNames.length === 0) {
    return undefined;
  }

  /**
   * CDXC:AgentHookSettings 2026-06-07-08:51:
   * Installed agent CLIs without current Ghostex hooks should surface in Tips
   * & Tricks as non-dismissable runtime notices. Hooks power reliable session
   * status, first-message naming, and sleep/resume identity; read-once tips are
   * the wrong model while the machine still has missing or stale hook setup.
   *
   * CDXC:AgentHooks 2026-06-07-11:05:
   * gxserver now distinguishes old Ghostex hooks from absent hooks. The
   * titlebar notice should ask users to update old hooks instead of saying they
   * are not installed, because the reliable fix is migration to the current
   * gxserver ingest hook rather than accepting stale native-era artifacts.
   *
   * CDXC:AgentHooks 2026-06-18-03:08:
   * The titlebar Tips dropdown must warn even before a live agent session is
   * running when installed CLIs are missing hooks. Copy should explicitly name
   * auto session naming, session status, and sleep/resume reliability.
   */
  const formattedAgents = formatTitlebarNoticeNameList(agentNames);
  const hasOutdatedHooks = outdatedAgents.size > 0;
  const hasMissingHooks = missingAgents.size > 0;
  const action = hasOutdatedHooks && hasMissingHooks ? 'setup' : hasOutdatedHooks ? 'update' : 'install';
  const actionLabel = action === 'setup' ? 'install or update' : action;
  const actionVerb = action === 'setup' ? 'set up' : action === 'update' ? 'updated' : 'installed';
  return {
    action: 'openSettings',
    body: `Open Settings > Integrations to ${actionLabel} agent hooks for ${formattedAgents}. Automatic session renaming, In Progress/Needs Attention status, and sleeping or resuming agent sessions will not work correctly until hooks are ${action === 'setup' ? 'installed or updated' : actionVerb}.`,
    icon: 'warning',
    id: `agent-hooks-${action}-${[...outdatedAgents.keys(), ...missingAgents.keys()].sort().join('-')}`,
    settingsTarget: 'agentHooks',
    title: "Warning: Agent hooks aren't installed for agent CLIs",
  };
}

export function isTitlebarLiveTerminalAgentSession(session: TitlebarResourceSession): boolean {
  return (
    session.sessionKind === 'terminal' &&
    session.isRunning === true &&
    session.isSleeping !== true &&
    Boolean(session.agentIcon)
  );
}

export function formatTitlebarNoticeNameList(names: string[]): string {
  if (names.length <= 1) {
    return names[0] ?? '';
  }
  if (names.length === 2) {
    return `${names[0]} and ${names[1]}`;
  }
  return `${names.slice(0, -1).join(', ')}, and ${names[names.length - 1]}`;
}
