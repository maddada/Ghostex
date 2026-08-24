import { useEffect, useState, type ComponentType } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';

type TitlebarHostModule = typeof import('../../apps/desktop/views/titlebar-host');

const RESOURCE_PROCESS_TABLE = [
  '100 1 7.4 314572 /Applications/Ghostex.app/Contents/MacOS/Ghostex',
  '110 100 2.3 184320 /Applications/Ghostex.app/Contents/Frameworks/Ghostex Helper --type=gpu-process',
  '200 1 1.8 65536 /Applications/Ghostex.app/Contents/Resources/zmx attach ghostex-story-codex',
  '201 200 4.6 196608 codex --session ghostex-story-codex',
  '202 201 0.7 53248 node /workspace/ghostex/scripts/dev-server.mjs',
  '210 1 0.4 48128 /Applications/Ghostex.app/Contents/Resources/zmx attach ghostex-story-claude',
  '211 210 2.1 143360 claude --session ghostex-story-claude',
  '300 100 3.8 121856 /Applications/Ghostex.app/Contents/Frameworks/Ghostex Helper --type=renderer --client-id=42',
  '301 100 1.2 81920 /Applications/Ghostex.app/Contents/Frameworks/Ghostex Helper --type=utility --client-id=42',
  '377 100 0.9 232448 /Applications/Ghostex.app/Contents/Resources/Web/code-server --port 3775',
  '450 1 0.6 75776 /Applications/Ghostex.app/Contents/Resources/zmx attach detached-review',
  '451 450 1.4 110592 opencode --session detached-review',
].join('\n');

const RESOURCE_LISTENERS = ['p202', 'cnode', 'n127.0.0.1:5173', 'p377', 'ccode-server', 'n127.0.0.1:3775'].join('\n');

const RESOURCE_CWDS = ['p202', 'n/workspace/ghostex', 'p377', 'n/workspace/ghostex'].join('\n');

function installTitlebarStoryHost(panel: 'resources' | 'tips') {
  const storyWindow = window as unknown as Record<string, unknown>;
  storyWindow.__ghostex_TITLEBAR_PANEL_KIND__ = panel;
  storyWindow.__ghostex_NATIVE_HOST__ = {
    activeMode: 'agents',
    browserTabs: [
      {
        browserId: 42,
        id: 'browser-docs',
        isActive: true,
        kind: 'browser',
        projectId: 'ghostex-project',
        title: 'Ghostex documentation',
        url: 'https://ghostex.dev/docs',
      },
    ],
    codeEditorProjectIds: ['ghostex-project'],
    cwd: '/workspace/ghostex',
    debuggingMode: true,
    ghostexCliStatus: { gxUsable: false, installed: false },
    gxserverDaemon: {
      alwaysStart: true,
      message: 'gxserver is running and uses the expected protocol.',
      ok: true,
      pid: 987,
      state: 'running',
      version: '5.7.0',
    },
    projectId: 'ghostex-project',
    projectName: 'Ghostex',
    resourceGroups: [
      {
        groupId: 'ghostex-main',
        isActive: true,
        projectId: 'ghostex-project',
        projectName: 'Ghostex',
        projectPath: '/workspace/ghostex',
        sessions: [
          {
            activity: 'working',
            agentIcon: 'codex',
            isLive: true,
            isRunning: true,
            nativePaneState: 'mounted',
            projectId: 'ghostex-project',
            providerSessionState: 'exists',
            sessionId: 'codex-session',
            sessionKind: 'terminal',
            sessionPersistenceName: 'ghostex-story-codex',
            sessionPersistenceProvider: 'zmx',
            terminalTitle: 'Implement native titlebar panels',
            title: 'Implement native titlebar panels',
          },
          {
            activity: 'idle',
            agentIcon: 'claude',
            isLive: true,
            isRunning: true,
            nativePaneState: 'unmounted',
            projectId: 'ghostex-project',
            providerSessionState: 'exists',
            sessionId: 'claude-session',
            sessionKind: 'terminal',
            sessionPersistenceName: 'ghostex-story-claude',
            sessionPersistenceProvider: 'zmx',
            title: 'Review Resources parity',
          },
        ],
        title: 'Ghostex',
      },
    ],
    sessionPersistenceProvider: 'zmx',
    sidebarTheme: 'dark',
    webLinkOpenTarget: 'internal-browser',
    workspaceName: 'Ghostex',
  };

  storyWindow.webkit = {
    messageHandlers: {
      ghostexNativeHost: {
        postMessage(command: Record<string, unknown>) {
          if (command.type !== 'runProcess' || typeof command.requestId !== 'string') {
            return;
          }
          const executable = String(command.executable ?? '');
          const args = Array.isArray(command.args) ? command.args.map(String) : [];
          let stdout = '';
          if (executable === '/bin/ps') {
            stdout = RESOURCE_PROCESS_TABLE;
          } else if (executable === '/usr/sbin/lsof' && args.includes('-iTCP')) {
            stdout = RESOURCE_LISTENERS;
          } else if (executable === '/usr/sbin/lsof' && args.includes('cwd')) {
            stdout = RESOURCE_CWDS;
          }
          window.setTimeout(() => {
            window.dispatchEvent(
              new CustomEvent('ghostex-native-host-event', {
                detail: {
                  exitCode: 0,
                  requestId: command.requestId,
                  stderr: '',
                  stdout,
                  type: 'processResult',
                },
              })
            );
          }, 0);
        },
      },
    },
  };
}

function ProductionTitlebarPanel({ panel }: { panel: 'resources' | 'tips' }) {
  const [Host, setHost] = useState<ComponentType>();

  useEffect(() => {
    installTitlebarStoryHost(panel);
    void import('../../apps/desktop/views/titlebar-host').then((module: TitlebarHostModule) => {
      setHost(() => module.GhostexTitlebarHost);
    });
  }, [panel]);

  return (
    <div style={{ background: '#050505', height: 650, overflow: 'hidden', width: panel === 'tips' ? 556 : 656 }}>
      {Host ? <Host /> : null}
    </div>
  );
}

const meta = {
  title: 'Titlebar/Reading Panels',
  parameters: { layout: 'fullscreen' },
} satisfies Meta;

export default meta;

type Story = StoryObj<typeof meta>;

export const Tips: Story = {
  render: () => <ProductionTitlebarPanel panel='tips' />,
};

export const Resources: Story = {
  render: () => <ProductionTitlebarPanel panel='resources' />,
};
