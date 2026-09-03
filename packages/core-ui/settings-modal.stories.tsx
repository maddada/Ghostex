import { useState } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { SettingsModal, type TailcatSettingsRpc } from './settings-modal';
import { DEFAULT_ghostex_SETTINGS, type ghostexSettings } from '../shared/ghostex-settings';
import { DEFAULT_SIDEBAR_AGENTS } from '../shared/sidebar-agents';
import { encodeEasyConnectCode, encodeTailscaleCode } from '../shared/ghostex-remote-pairing';
import type {
  GxserverPairedDevice,
  GxserverRemoteAccessStatus,
  GxserverTailcatStatus,
} from '../shared/gxserver-protocol';
import type {
  SidebarAgentHookStatusMessage,
  SidebarGhostexCliStatusMessage,
  SidebarProjectSettingsItem,
} from '../shared/session-grid-contract';

const modalSettings: ghostexSettings = {
  ...DEFAULT_ghostex_SETTINGS,
  agentManagerZoomPercent: 95,
  showCloseButtonOnSessionCards: true,
  terminalFontSize: 16,
  terminalFontWeight: 400,
  terminalLineHeight: 1.35,
};

const storyProjects: SidebarProjectSettingsItem[] = [
  {
    beadsDirectory: '',
    beadsDisplayKey: 'ZMX',
    name: 'Ghostex',
    path: '/Users/you/dev/ghostex',
    projectId: 'project-ghostex',
    worktreeCommand: 'bun install',
  },
  {
    beadsDirectory: '/Users/you/dev/infra/.beads',
    beadsDisplayKey: 'INF',
    name: 'Infra Control Plane',
    path: '/Users/you/dev/platform/infra-control-plane',
    projectId: 'project-infra',
    worktreeCommand: 'pnpm install',
  },
  {
    beadsDirectory: '',
    beadsDisplayKey: 'WEB',
    name: 'Customer Web',
    path: '/Users/you/dev/products/customer-web-application',
    projectId: 'project-web',
    worktreeCommand: '',
  },
  {
    beadsDirectory: '',
    beadsDisplayKey: 'OPS',
    name: 'Operations Dashboard',
    path: '/Users/you/dev/internal/tools/operations-dashboard',
    projectId: 'project-ops',
    worktreeCommand: 'bun run setup',
  },
];

/**
 * A scripted gxserver for the Remote tab stories: answers every endpoint the
 * page polls from in-memory state, so toggles, Turn on SSH access, and Remove
 * behave like the real daemon without one.
 */
function createRemoteStoryRpc({
  easyConnectEnabled = true,
  sshEnabled = true,
}: {
  easyConnectEnabled?: boolean;
  sshEnabled?: boolean;
}): TailcatSettingsRpc {
  const address = 'tc1q8v3k2m9x7p4r6t8w1y5z2a4c6e8g0j3l5n7q9s1u3w5y7a9c1e3g5i7k9m1o3q5s7u9w1y3';
  const easyConnect: GxserverTailcatStatus = {
    allowedClientKeys: [],
    binaryFound: true,
    binaryPath: '/Applications/Ghostex.app/Contents/Resources/bin/tailcat',
    binaryVersion: '0.4.2',
    enabled: easyConnectEnabled,
    lastError: null,
    ports: [22, 58744],
    running: easyConnectEnabled,
    token: easyConnectEnabled ? address : null,
  };
  const access: GxserverRemoteAccessStatus = {
    computerName: "Mohamad's Laptop",
    platform: 'macos',
    ssh: { checkedAt: new Date().toISOString(), detail: null, enabled: sshEnabled, port: 22 },
    tailscale: {
      account: 'madda@github',
      installed: true,
      ip: '100.77.81.4',
      magicDnsName: 'laptop.tail1a2b.ts.net',
      running: true,
      sshEnabled: false,
    },
    username: 'madda',
  };
  let devices: GxserverPairedDevice[] = [
    {
      id: 'dev-1',
      lastSeenAt: new Date().toISOString(),
      name: 'Pixel 9 Pro',
      pairedAt: new Date().toISOString(),
      platform: 'android',
      sshKeyFingerprint: 'SHA256:aaaa',
    },
    {
      id: 'dev-2',
      lastSeenAt: new Date(Date.now() - 26 * 60 * 60 * 1000).toISOString(),
      name: 'Studio',
      pairedAt: '2026-09-01T10:00:00.000Z',
      platform: 'macos',
      sshKeyFingerprint: 'SHA256:bbbb',
    },
  ];
  return async (path, params) => {
    await new Promise((resolve) => setTimeout(resolve, 120));
    switch (path) {
      case '/api/tailcatStatus':
        return { status: easyConnect };
      case '/api/updateTailcatState':
        if (params.kind === 'setEnabled') {
          easyConnect.enabled = params.enabled === true;
          easyConnect.running = easyConnect.enabled;
          easyConnect.token = easyConnect.enabled ? address : null;
        } else if (params.kind === 'setPorts' && Array.isArray(params.ports)) {
          easyConnect.ports = params.ports as number[];
        } else if (params.kind === 'setAllowedClientKeys' && Array.isArray(params.allowedClientKeys)) {
          easyConnect.allowedClientKeys = params.allowedClientKeys as string[];
        }
        return { status: easyConnect };
      case '/api/remoteAccessStatus':
        return access;
      case '/api/enableSshAccess':
        access.ssh = { ...access.ssh, checkedAt: new Date().toISOString() };
        return { message: 'The admin prompt was cancelled.', outcome: 'cancelled', ssh: access.ssh };
      case '/api/remotePairingCode': {
        const tailscaleCode = {
          host: 'laptop.tail1a2b.ts.net',
          ip: '100.77.81.4',
          name: access.computerName,
          port: 22,
          user: access.username,
          v: 1 as const,
        };
        const easyConnectCode = {
          address,
          name: access.computerName,
          port: 58744,
          secret: 'story-secret',
          sshPort: 22,
          user: access.username,
          v: 1 as const,
        };
        return {
          ...(easyConnect.enabled
            ? { easyConnect: { code: easyConnectCode, payload: encodeEasyConnectCode(easyConnectCode) } }
            : {}),
          tailscale: { code: tailscaleCode, payload: encodeTailscaleCode(tailscaleCode) },
        };
      }
      case '/api/pairedDevices':
        return { devices };
      case '/api/removePairedDevice':
        devices = devices.filter((device) => device.id !== params.deviceId);
        return { devices };
      default:
        throw new Error(`Story gxserver has no handler for ${path}.`);
    }
  };
}

function SettingsModalStory({
  cuaDriverInstalled,
  cuaPermissionsGranted,
  initialSettings = modalSettings,
  initialTab = 'settings',
  projects,
  remoteRpc,
}: {
  cuaDriverInstalled?: boolean;
  cuaPermissionsGranted?: boolean;
  initialSettings?: ghostexSettings;
  initialTab?: 'settings' | 'integrations' | 'projects' | 'agents' | 'actions' | 'openTargets' | 'hotkeys' | 'remote';
  projects?: SidebarProjectSettingsItem[];
  remoteRpc?: TailcatSettingsRpc;
}) {
  const [settings, setSettings] = useState<ghostexSettings>(initialSettings);
  const [agentHookStatus, setAgentHookStatus] = useState<SidebarAgentHookStatusMessage>({
    agents: DEFAULT_SIDEBAR_AGENTS.map((agent, index) => ({
      agentId: agent.agentId,
      cliCommand: agent.command.split(' ')[0] ?? agent.command,
      cliInstalled: index < 10,
      detail: index < 4 ? 'Hook config is installed.' : 'Hook config is not installed.',
      hookInstalled: index < 4,
      paths: [`~/.ghostex/mock-hooks/${agent.agentId}.json`],
      status: index < 4 ? 'installed' : index < 10 ? 'missing' : 'cliMissing',
    })),
    generatedAt: '2026-05-27T04:17:00.000Z',
    hookStateDirectory: '~/.ghostexterm',
    notifyHookPath: '~/.ghostexterm/notify-agent-status.js',
    type: 'agentHookStatus',
  });
  const [ghostexCliStatus, setGhostexCliStatus] = useState<SidebarGhostexCliStatusMessage>({
    cliSkillInstalled: false,
    browserSkillInstalled: false,
    computerUseSkillInstalled: false,
    embeddedBrowserSkillInstalled: false,
    cuaAppInstalled: false,
    cuaDriverAccessibilityPermissionGranted: cuaPermissionsGranted,
    cuaDriverInstallCommand: '/bin/bash -c "$(curl -fsSL https://cua.ai/driver/install.sh)"',
    cuaDriverInstalled: cuaDriverInstalled ?? cuaPermissionsGranted !== undefined,
    cuaDriverScreenRecordingPermissionGranted: cuaPermissionsGranted,
    detail:
      'Ghostex CLI is installed automatically with the app. Use ghostex for the full command. Ghostex Browser Use and Ghostex Computer Use are not installed yet.',
    generateTitleSkillInstalled: false,
    generatedAt: '2026-05-27T04:17:00.000Z',
    ghostexPath: '/opt/homebrew/bin/ghostex',
    gxBlockedByExistingCommand: false,
    gxUsable: false,
    installed: true,
    moveCodexSessionSkillInstalled: false,
    type: 'ghostexCliStatus',
  });

  return (
    <div
      style={{
        background: '#0e0e0e',
        height: '100vh',
        width: '100vw',
      }}
    >
      <SettingsModal
        agentHookStatus={agentHookStatus}
        ghostexCliStatus={ghostexCliStatus}
        initialTab={initialTab}
        isOpen
        onChange={setSettings}
        onClose={() => undefined}
        onInstallAgentHooks={() =>
          setAgentHookStatus({
            ...agentHookStatus,
            agents: agentHookStatus.agents.map((agent) =>
              agent.cliInstalled
                ? { ...agent, detail: 'Hook config is installed.', hookInstalled: true, status: 'installed' }
                : agent
            ),
          })
        }
        onInstallBrowserControl={() =>
          setGhostexCliStatus({
            ...ghostexCliStatus,
            embeddedBrowserSkillInstalled: true,
            embeddedBrowserSkillPath: '/Users/madda/agents/skills/ghostex-embedded-browser-use/SKILL.md',
          })
        }
        onInstallBrowserUseSkill={() =>
          setGhostexCliStatus({
            ...ghostexCliStatus,
            browserSkillInstalled: true,
            browserSkillPath: '/Users/madda/agents/skills/ghostex-browser-use/SKILL.md',
          })
        }
        onInstallComputerUseSkill={() =>
          setGhostexCliStatus({
            ...ghostexCliStatus,
            computerUseSkillInstalled: true,
            computerUseSkillPath: '/Users/madda/agents/skills/ghostex-computer-use/SKILL.md',
          })
        }
        onInstallCliSkill={() =>
          setGhostexCliStatus({
            ...ghostexCliStatus,
            cliSkillInstalled: true,
            cliSkillPath: '/Users/madda/agents/skills/ghostex-cli/SKILL.md',
          })
        }
        onInstallGenerateTitleSkill={() =>
          setGhostexCliStatus({
            ...ghostexCliStatus,
            generateTitleSkillInstalled: true,
            generateTitleSkillPath: '/Users/madda/agents/skills/ghostex-auto-rename-session/SKILL.md',
          })
        }
        onInstallCuaDriver={() =>
          setGhostexCliStatus({
            ...ghostexCliStatus,
            computerUseSkillInstalled: true,
            computerUseSkillPath: '/Users/madda/agents/skills/ghostex-computer-use/SKILL.md',
            cuaAppInstalled: true,
            cuaDriverAccessibilityPermissionGranted: true,
            cuaDriverInstalled: true,
            cuaDriverPath: '/Users/madda/.local/bin/cua-driver',
            cuaDriverScreenRecordingPermissionGranted: true,
          })
        }
        onInstallGhostexCli={() => setGhostexCliStatus({ ...ghostexCliStatus, installed: true })}
        onOpenAccessibilityPreferences={() => undefined}
        onOpenScreenRecordingPreferences={() => undefined}
        onRequestAgentHookStatus={() => undefined}
        onRequestGhostexCliStatus={() => undefined}
        projects={projects}
        settings={settings}
        tailcatRpc={remoteRpc}
        theme={settings.sidebarTheme === 'light-orange' ? 'light-orange' : 'dark-blue'}
      />
    </div>
  );
}

const meta = {
  title: 'Modals/App Host/Settings',
  parameters: {
    layout: 'fullscreen',
  },
  render: () => <SettingsModalStory />,
} satisfies Meta;

export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const DarkGray: Story = {
  render: () => (
    <SettingsModalStory
      initialSettings={{
        ...modalSettings,
        sidebarTheme: 'plain',
      }}
    />
  ),
};

export const AccessibilityOff: Story = {
  render: () => <SettingsModalStory cuaPermissionsGranted={false} />,
};

export const Integrations: Story = {
  render: () => <SettingsModalStory cuaPermissionsGranted={false} initialTab='integrations' />,
};

/*
 * CDXC:TrycuaPrerequisite 2026-08-24:
 * The pre-install state is the one the layout has to teach: one Trycua step
 * with the exact command it runs, then the skills that depend on it.
 */
export const IntegrationsTrycuaMissing: Story = {
  render: () => <SettingsModalStory cuaDriverInstalled={false} initialTab='integrations' />,
};

export const Projects: Story = {
  render: () => <SettingsModalStory initialTab='projects' projects={storyProjects} />,
};

/*
 * CDXC:ModalRedesign 2026-08-24:
 * Review stories for the Codex restyle. Hotkeys is the densest list surface in
 * Settings (recorder chips, per-row reset buttons), Agents is the densest
 * management surface (cards, row actions, section header actions), and Theming
 * is the densest control surface (color pickers, selects, sliders, switches on
 * one card). Between them they cover every field primitive the redesign
 * touches.
 */
export const Hotkeys: Story = {
  render: () => <SettingsModalStory initialTab='hotkeys' />,
};

export const Agents: Story = {
  render: () => <SettingsModalStory initialTab='agents' />,
};

/*
 * CDXC:RemotePairing 2026-09-03:
 * Settings -> Remote against a scripted gxserver: Easy Connect running with a
 * pairing code and two paired devices, Tailscale detected, SSH access on.
 */
export const Remote: Story = {
  render: () => <SettingsModalStory initialTab='remote' remoteRpc={createRemoteStoryRpc({ sshEnabled: true })} />,
};

export const RemoteSshAccessOff: Story = {
  render: () => <SettingsModalStory initialTab='remote' remoteRpc={createRemoteStoryRpc({ sshEnabled: false })} />,
};

export const RemoteEasyConnectOff: Story = {
  render: () => (
    <SettingsModalStory initialTab='remote' remoteRpc={createRemoteStoryRpc({ easyConnectEnabled: false })} />
  ),
};

export const Theming: Story = {
  play: async ({ canvasElement, step }) => {
    const body = within(canvasElement.ownerDocument.body);

    await step('jump the General page to the Theming section', async () => {
      await userEvent.click(await body.findByRole('button', { name: 'Appearance' }));
      await waitFor(() => {
        expect(body.getByText('Background Contrast')).toBeTruthy();
      });
    });
  },
  render: () => (
    <SettingsModalStory
      initialSettings={{
        ...modalSettings,
        showAdvancedSettings: true,
      }}
    />
  ),
};

export const CustomColorPicker: Story = {
  play: async ({ canvasElement, step }) => {
    const body = within(canvasElement.ownerDocument.body);

    await step('open the nested color picker dialog', async () => {
      await userEvent.click(await body.findByRole('button', { name: 'Appearance' }));
      await userEvent.click(await body.findByRole('button', { name: 'Accent Color custom color picker' }));
      await body.findByRole('dialog', { name: 'Pick Color' });
    });
  },
  render: () => (
    <SettingsModalStory
      initialSettings={{
        ...modalSettings,
        showAdvancedSettings: true,
      }}
    />
  ),
};

export const LightOrange: Story = {
  render: () => (
    <SettingsModalStory
      initialSettings={{
        ...modalSettings,
        sidebarTheme: 'light-orange',
      }}
    />
  ),
};

export const NarrowModal: Story = {
  parameters: {
    viewport: {
      defaultViewport: 'narrowSettings',
      viewports: {
        narrowSettings: {
          name: 'Narrow settings modal',
          styles: {
            height: '900px',
            width: '520px',
          },
        },
      },
    },
  },
  render: () => <SettingsModalStory />,
};
