import { useState } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { DEFAULT_ghostex_SETTINGS, type ghostexSettings } from '@/packages/shared/ghostex-settings';
import { DEFAULT_SIDEBAR_AGENTS } from '@/packages/shared/sidebar-agents';
import type { OnboardingComputerUseState, OnboardingDetectedAgent } from './contract';
import { OnboardingModal } from './onboarding-modal';

const INSTALLED_IDS = ['claude', 'codex', 'cursor'];
const ACCOUNT_LABELS: Record<string, string> = {
  claude: 'uses your Claude account',
  codex: 'uses your ChatGPT account',
  cursor: 'uses your Cursor account',
};

function mockAgents(installed: readonly string[], hooksInstalled: boolean): OnboardingDetectedAgent[] {
  return DEFAULT_SIDEBAR_AGENTS.map((agent) => ({
    agentId: agent.agentId,
    name: agent.agentId === 'claude' ? 'Claude Code' : agent.agentId === 'cursor' ? 'Cursor Agent' : agent.name,
    accountLabel: ACCOUNT_LABELS[agent.agentId],
    installed: installed.includes(agent.agentId),
    hooksInstalled: installed.includes(agent.agentId) && hooksInstalled,
    detail: installed.includes(agent.agentId)
      ? `/opt/homebrew/bin/${agent.command.split(' ')[0]}`
      : 'CLI was not found on PATH.',
  }));
}

type StoryArgs = {
  initialPanel: number | 'finished';
  installedAgents: readonly string[];
  hooksInstalled: boolean;
  agentsLoading: boolean;
  computerUseState: OnboardingComputerUseState;
  browserSkillInstalled: boolean;
  hasProjects: boolean;
  pickedProjectFolder?: string;
};

/** A stand-in for the modal-host adapter: settings and detection results live in local state. */
function OnboardingModalStory(args: StoryArgs) {
  const [settings, setSettings] = useState<ghostexSettings>({
    ...DEFAULT_ghostex_SETTINGS,
    defaultPromptAgentId: 'claude',
    codeViewTabHidden: true,
    kanbanViewTabHidden: true,
    automateViewTabHidden: true,
  });
  const [agents, setAgents] = useState(() => mockAgents(args.installedAgents, args.hooksInstalled));
  const [agentsLoading, setAgentsLoading] = useState(args.agentsLoading);
  const [computerUseState, setComputerUseState] = useState(args.computerUseState);
  const [browserSkillInstalled, setBrowserSkillInstalled] = useState(args.browserSkillInstalled);
  const [pickedProjectFolder, setPickedProjectFolder] = useState(args.pickedProjectFolder);
  const [open, setOpen] = useState(true);
  const rescan = () => {
    setAgentsLoading(true);
    window.setTimeout(() => setAgentsLoading(false), 1400);
  };
  return (
    <div style={{ position: 'relative', width: '100vw', height: '100vh', background: '#040507' }}>
      {!open && (
        <button
          type='button'
          style={{ position: 'absolute', inset: 'auto 16px 16px auto', color: '#fff' }}
          onClick={() => setOpen(true)}
        >
          Reopen onboarding
        </button>
      )}
      <OnboardingModal
        isOpen={open}
        initialPanel={args.initialPanel}
        theme='dark-blue'
        hasProjects={args.hasProjects}
        settings={settings}
        onChange={setSettings}
        onClose={() => setOpen(false)}
        agents={agents}
        agentsLoading={agentsLoading}
        onRescanAgents={rescan}
        onInstallAgentHooks={(agentIds) => {
          window.setTimeout(
            () =>
              setAgents((current) =>
                current.map((agent) => (agentIds.includes(agent.agentId) ? { ...agent, hooksInstalled: true } : agent))
              ),
            900
          );
        }}
        onOpenInstallGuide={(url) => window.open(url, '_blank', 'noopener')}
        computerUseState={computerUseState}
        onInstallComputerUse={() => {
          setComputerUseState('installing');
          window.setTimeout(() => setComputerUseState('permissions'), 1800);
        }}
        onOpenAccessibilityPreferences={() => setComputerUseState('on')}
        onOpenScreenRecordingPreferences={() => setComputerUseState('on')}
        browserSkillInstalled={browserSkillInstalled}
        onInstallBrowserSkill={() => setBrowserSkillInstalled(true)}
        onUninstallBrowserSkill={() => setBrowserSkillInstalled(false)}
        onOpenRemoteSettings={() => window.alert('Settings -> Remote would open here')}
        onOpenExternalUrl={(url) => window.open(url, '_blank', 'noopener')}
        onPickProjectFolder={() => setPickedProjectFolder('~/Projects/my-app')}
        pickedProjectFolder={pickedProjectFolder}
        onFinishFirstLaunch={(options) => {
          console.info('onFinishFirstLaunch', options);
          return new Promise((resolve) => window.setTimeout(resolve, 900));
        }}
        onOpenSettings={() => window.alert('Settings would open here')}
      />
    </div>
  );
}

const meta = {
  title: 'Modals/Onboarding/Onboarding',
  parameters: { layout: 'fullscreen' },
  args: {
    initialPanel: 1,
    installedAgents: INSTALLED_IDS,
    hooksInstalled: false,
    agentsLoading: false,
    computerUseState: 'off',
    browserSkillInstalled: true,
    hasProjects: false,
    pickedProjectFolder: '~/Projects/my-app',
  } satisfies StoryArgs,
  render: (args) => <OnboardingModalStory key={JSON.stringify(args)} {...args} />,
} satisfies Meta<StoryArgs>;

export default meta;

type Story = StoryObj<typeof meta>;

export const Welcome: Story = {};
export const Agents: Story = { args: { initialPanel: 2 } };
export const AgentsScanning: Story = { args: { initialPanel: 2, agentsLoading: true } };
export const AgentsAlreadyConnected: Story = { args: { initialPanel: 2, hooksInstalled: true } };
export const AgentsNoneInstalled: Story = { args: { initialPanel: 2, installedAgents: [] } };
export const ComputerUseInstalling: Story = { args: { initialPanel: 2, computerUseState: 'installing' } };
export const ComputerUsePermissions: Story = { args: { initialPanel: 2, computerUseState: 'permissions' } };
export const ComputerUseOn: Story = { args: { initialPanel: 2, computerUseState: 'on' } };
export const Workspace: Story = { args: { initialPanel: 3 } };
export const WorkspaceNoBrowserSkill: Story = { args: { initialPanel: 3, browserSkillInstalled: false } };
export const Mobile: Story = { args: { initialPanel: 4 } };
export const GetStarted: Story = { args: { initialPanel: 5 } };
export const GetStartedNoFolder: Story = { args: { initialPanel: 5, pickedProjectFolder: undefined } };
export const GetStartedNoFolderWithProjects: Story = {
  args: { initialPanel: 5, pickedProjectFolder: undefined, hasProjects: true },
};
export const Finished: Story = { args: { initialPanel: 'finished', hooksInstalled: true } };
