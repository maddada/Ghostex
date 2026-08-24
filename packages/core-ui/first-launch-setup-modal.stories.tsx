import { useState } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { FirstLaunchSetupModal, type FirstLaunchSetupPage } from './first-launch-setup-modal';
import { DEFAULT_ghostex_SETTINGS, type ghostexSettings } from '../shared/ghostex-settings';
import type { SidebarAgentHookStatusMessage, SidebarGhostexCliStatusMessage } from '../shared/session-grid-contract';
import { DEFAULT_SIDEBAR_AGENTS } from '../shared/sidebar-agents';

const initialHookStatus: SidebarAgentHookStatusMessage = {
  agents: DEFAULT_SIDEBAR_AGENTS.map((agent, index) => {
    const status = index < 4 ? 'installed' : index < 10 ? 'missing' : 'cliMissing';
    return {
      agentId: agent.agentId,
      cliCommand: agent.command.split(' ')[0] ?? agent.command,
      cliInstalled: status !== 'cliMissing',
      detail:
        status === 'installed'
          ? 'Hook config is installed.'
          : status === 'cliMissing'
            ? 'CLI was not found on PATH.'
            : 'Hook config is not installed yet.',
      hookInstalled: status === 'installed',
      paths: [`~/.ghostex/mock-hooks/${agent.agentId}.json`],
      status,
    };
  }),
  generatedAt: '2026-05-26T06:23:00.000Z',
  hookStateDirectory: '~/.ghostexterm',
  notifyHookPath: '~/.ghostexterm/notify-agent-status.js',
  type: 'agentHookStatus',
};

const installedCliStatus: SidebarGhostexCliStatusMessage = {
  cliSkillInstalled: true,
  cliSkillPath: '/Users/madda/agents/skills/ghostex-cli/SKILL.md',
  browserSkillInstalled: true,
  browserSkillPath: '/Users/madda/agents/skills/ghostex-browser-use/SKILL.md',
  computerUseSkillInstalled: true,
  computerUseSkillPath: '/Users/madda/agents/skills/ghostex-computer-use/SKILL.md',
  embeddedBrowserSkillInstalled: true,
  embeddedBrowserSkillPath: '/Users/madda/agents/skills/ghostex-embedded-browser-use/SKILL.md',
  cuaAppInstalled: true,
  cuaDriverInstalled: true,
  cuaDriverPath: '/Users/madda/.local/bin/cua-driver',
  detail:
    'Ghostex CLI is installed automatically with the app. Use ghostex for the full command or gx for the short alias. Browser MCP skill is installed for agents.',
  generateTitleSkillInstalled: true,
  generateTitleSkillPath: '/Users/madda/agents/skills/ghostex-auto-rename-session/SKILL.md',
  generatedAt: '2026-05-26T13:12:00.000Z',
  ghostexPath: '/opt/homebrew/bin/ghostex',
  gxBlockedByExistingCommand: false,
  gxUsable: false,
  installed: true,
  moveCodexSessionSkillInstalled: true,
  moveCodexSessionSkillPath: '/Users/madda/agents/skills/ghostex-move-codex-session/SKILL.md',
  type: 'ghostexCliStatus',
};

const missingCliStatus: SidebarGhostexCliStatusMessage = {
  cliSkillInstalled: false,
  browserSkillInstalled: false,
  computerUseSkillInstalled: false,
  embeddedBrowserSkillInstalled: false,
  cuaAppInstalled: false,
  cuaDriverInstallCommand: '/bin/bash -c "$(curl -fsSL https://cua.ai/driver/install.sh)"',
  cuaDriverInstalled: false,
  detail:
    'Ghostex CLI auto-install did not find a usable ghostex command on PATH. Ghostex Browser Use and Ghostex Computer Use are not installed yet.',
  generateTitleSkillInstalled: false,
  generatedAt: '2026-05-27T04:17:00.000Z',
  gxBlockedByExistingCommand: false,
  gxUsable: false,
  installed: false,
  moveCodexSessionSkillInstalled: false,
  type: 'ghostexCliStatus',
};

function installedHookStatus(): SidebarAgentHookStatusMessage {
  return {
    ...initialHookStatus,
    agents: initialHookStatus.agents.map((agent) =>
      agent.cliInstalled
        ? {
            ...agent,
            detail: 'Hook config is installed.',
            hookInstalled: true,
            status: 'installed',
          }
        : agent
    ),
  };
}

function FirstLaunchSetupModalStory({
  cliInstalled = true,
  initialPage = 'hooks',
}: {
  cliInstalled?: boolean;
  initialPage?: FirstLaunchSetupPage;
}) {
  const [settings, setSettings] = useState<ghostexSettings>(DEFAULT_ghostex_SETTINGS);
  const [agentHookStatus, setAgentHookStatus] = useState<SidebarAgentHookStatusMessage>(initialHookStatus);
  const [agentHookStatusLoading, setAgentHookStatusLoading] = useState(false);
  const [ghostexCliStatus, setGhostexCliStatus] = useState<SidebarGhostexCliStatusMessage>(
    cliInstalled ? installedCliStatus : missingCliStatus
  );
  const [ghostexCliStatusLoading, setGhostexCliStatusLoading] = useState(false);

  const installHooks = () => {
    setAgentHookStatusLoading(true);
    window.setTimeout(() => {
      setAgentHookStatus(installedHookStatus());
      setAgentHookStatusLoading(false);
    }, 500);
  };

  return (
    <div className='first-launch-setup-story-frame'>
      <FirstLaunchSetupModal
        agentHookStatus={agentHookStatus}
        agentHookStatusLoading={agentHookStatusLoading}
        ghostexCliStatus={ghostexCliStatus}
        ghostexCliStatusLoading={ghostexCliStatusLoading}
        initialPage={initialPage}
        isOpen
        onChange={setSettings}
        onClose={() => undefined}
        onInstallBrowserControl={() => {
          setGhostexCliStatusLoading(true);
          window.setTimeout(() => {
            setGhostexCliStatus({
              ...ghostexCliStatus,
              embeddedBrowserSkillInstalled: true,
              embeddedBrowserSkillPath: '/Users/madda/agents/skills/ghostex-embedded-browser-use/SKILL.md',
              detail: 'Ghostex Embedded Browser Use skill is installed for agents.',
            });
            setGhostexCliStatusLoading(false);
          }, 500);
        }}
        onInstallBrowserUseSkill={() => {
          setGhostexCliStatusLoading(true);
          window.setTimeout(() => {
            setGhostexCliStatus({
              ...ghostexCliStatus,
              browserSkillInstalled: true,
              browserSkillPath: '/Users/madda/agents/skills/ghostex-browser-use/SKILL.md',
              detail: 'Ghostex Browser Use skill is installed for agents.',
            });
            setGhostexCliStatusLoading(false);
          }, 500);
        }}
        onInstallComputerUseSkill={() => {
          setGhostexCliStatusLoading(true);
          window.setTimeout(() => {
            setGhostexCliStatus({
              ...ghostexCliStatus,
              computerUseSkillInstalled: true,
              computerUseSkillPath: '/Users/madda/agents/skills/ghostex-computer-use/SKILL.md',
              detail: 'Ghostex Computer Use skill is installed for agents.',
            });
            setGhostexCliStatusLoading(false);
          }, 500);
        }}
        onInstallCliSkill={() => {
          setGhostexCliStatusLoading(true);
          window.setTimeout(() => {
            setGhostexCliStatus({
              ...ghostexCliStatus,
              cliSkillInstalled: true,
              cliSkillPath: '/Users/madda/agents/skills/ghostex-cli/SKILL.md',
              detail: 'Ghostex Agent Orchestration skill is installed for agents.',
            });
            setGhostexCliStatusLoading(false);
          }, 500);
        }}
        onInstallGenerateTitleSkill={() => {
          setGhostexCliStatusLoading(true);
          window.setTimeout(() => {
            setGhostexCliStatus({
              ...ghostexCliStatus,
              generateTitleSkillInstalled: true,
              generateTitleSkillPath: '/Users/madda/agents/skills/ghostex-auto-rename-session/SKILL.md',
              detail: 'Ghostex Auto Rename Session skill is installed for agents.',
            });
            setGhostexCliStatusLoading(false);
          }, 500);
        }}
        onInstallCuaDriver={() => {
          setGhostexCliStatusLoading(true);
          window.setTimeout(() => {
            setGhostexCliStatus({
              ...ghostexCliStatus,
              cuaAppInstalled: true,
              computerUseSkillInstalled: true,
              computerUseSkillPath: '/Users/madda/agents/skills/ghostex-computer-use/SKILL.md',
              cuaDriverInstalled: true,
              cuaDriverPath: '/Users/madda/.local/bin/cua-driver',
              detail: 'Ghostex Computer Use is installed.',
            });
            setGhostexCliStatusLoading(false);
          }, 500);
        }}
        onInstallGhostexCli={() => {
          setGhostexCliStatusLoading(true);
          window.setTimeout(() => {
            setGhostexCliStatus({
              ...installedCliStatus,
              cliSkillInstalled: ghostexCliStatus.cliSkillInstalled,
              browserSkillInstalled: ghostexCliStatus.browserSkillInstalled,
              computerUseSkillInstalled: ghostexCliStatus.computerUseSkillInstalled,
              embeddedBrowserSkillInstalled: ghostexCliStatus.embeddedBrowserSkillInstalled,
              generateTitleSkillInstalled: ghostexCliStatus.generateTitleSkillInstalled,
            });
            setGhostexCliStatusLoading(false);
          }, 500);
        }}
        onOpenAccessibilityPreferences={() => undefined}
        onOpenScreenRecordingPreferences={() => undefined}
        onInstallAgentHooks={installHooks}
        onRequestAgentHookStatus={() => {
          setAgentHookStatusLoading(true);
          window.setTimeout(() => setAgentHookStatusLoading(false), 350);
        }}
        onRequestGhostexCliStatus={() => {
          setGhostexCliStatusLoading(true);
          window.setTimeout(() => setGhostexCliStatusLoading(false), 350);
        }}
        settings={settings}
        theme='dark-blue'
      />
    </div>
  );
}

const meta = {
  title: 'Sidebar/First Launch Setup Modal',
  parameters: {
    layout: 'fullscreen',
  },
  render: (args) => <FirstLaunchSetupModalStory cliInstalled={args.cliInstalled} initialPage={args.initialPage} />,
} satisfies Meta;

export default meta;

type Story = StoryObj<typeof meta>;

/*
 CDXC:ModalRedesign 2026-08-24:
 Only the pages in FIRST_LAUNCH_SETUP_PAGES can be opened directly; the modal
 normalizes any other id back to the first page. Video, Intro, Hooks, Skills,
 and Ready are therefore the stories that actually render distinct surfaces, and
 the guide/preferences/cli stories below still fall back to the video page.
*/
export const Video: Story = {
  args: {
    cliInstalled: true,
    initialPage: 'video',
  },
};

export const Intro: Story = {
  args: {
    cliInstalled: true,
    initialPage: 'welcome',
  },
};

export const Skills: Story = {
  args: {
    cliInstalled: false,
    initialPage: 'skills',
  },
};

export const Ready: Story = {
  args: {
    cliInstalled: true,
    initialPage: 'ready',
  },
};

export const Preferences: Story = {
  args: {
    cliInstalled: true,
    initialPage: 'preferences',
  },
};

export const Hooks: Story = {
  args: {
    cliInstalled: true,
    initialPage: 'hooks',
  },
};

export const Cli: Story = {
  args: {
    cliInstalled: true,
    initialPage: 'cli',
  },
};

export const CliNeedsInstall: Story = {
  args: {
    cliInstalled: false,
    initialPage: 'cli',
  },
};

export const GuideBrowserControl: Story = {
  args: {
    cliInstalled: true,
    initialPage: 'browserControl',
  },
};

export const GuideDesktopCua: Story = {
  args: {
    cliInstalled: true,
    initialPage: 'desktopCua',
  },
};

export const GuideAgentsSessions: Story = {
  args: {
    cliInstalled: true,
    initialPage: 'agentsSessions',
  },
};

export const GuideRemoteAccess: Story = {
  args: {
    cliInstalled: true,
    initialPage: 'remoteAccess',
  },
};
