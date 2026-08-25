import { useMemo, useState } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import type {
  GhostexExtensionCatalogEntry,
  GhostexExtensionManifest,
  GhostexInstalledExtension,
} from '@/packages/shared/ghostex-extensions';
import { InstalledExtensionCard } from './extension-card';
import { StoreExtensionDetail } from './extension-detail';
import { InstallConsentDialog } from './install-consent';
import { InstalledTab } from './installed-tab';

const ICON =
  "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 48 48' fill='none' stroke='%23e5e5e5' stroke-width='2'%3E%3Crect x='8' y='8' width='32' height='32' rx='8'/%3E%3Cpath d='M16 24h16M24 16v16'/%3E%3C/svg%3E";

const WEB_MANIFEST: GhostexExtensionManifest = {
  author: 'Ghostex',
  categories: ['Developer Tools', 'Productivity'],
  defaultPlacement: 'view',
  description: 'Runs the project Storybook and opens it as a focused workspace view.',
  icon: 'icon.svg',
  name: 'storybook',
  permissions: ['exec', 'network'],
  placements: ['view', 'popup'],
  preferences: [
    {
      description: 'The command used to launch Storybook.',
      name: 'command',
      required: true,
      title: 'Launch command',
      type: 'textfield',
    },
  ],
  server: { command: 'bun storybook --port {port}', readiness: { httpGet: '/' } },
  title: 'Storybook',
  version: '1.2.0',
};

const TERMINAL_MANIFEST: GhostexExtensionManifest = {
  author: 'Ghostex',
  categories: ['Developer Tools'],
  description: 'A fast terminal interface for inspecting the current repository.',
  icon: 'icon.svg',
  kind: 'terminal-pane',
  name: 'lazygit',
  permissions: ['exec'],
  terminal: { command: 'lazygit', cwd: '{projectPath}', requires: ['lazygit'] },
  title: 'Lazygit',
  version: '1.0.0',
};

const INSTALLED: GhostexInstalledExtension[] = [
  {
    id: 'storybook',
    manifest: WEB_MANIFEST,
    runtime: { state: 'stopped' },
    state: {
      enabled: true,
      grantedPermissions: ['exec', 'network'],
      pinned: true,
      placement: 'view',
      preferences: { command: 'bun storybook' },
      storage: {},
      terminalPlacement: 'splitRight',
      version: '1.1.0',
    },
  },
  {
    id: 'lazygit',
    manifest: TERMINAL_MANIFEST,
    runtime: { state: 'stopped' },
    state: {
      enabled: false,
      grantedPermissions: ['exec'],
      pinned: false,
      preferences: {},
      storage: {},
      terminalPlacement: 'splitRight',
      version: '1.0.0',
    },
  },
];

const CATALOG_ENTRY: GhostexExtensionCatalogEntry = {
  ...WEB_MANIFEST,
  changelog: 'extensions/storybook/CHANGELOG.md',
  readme: 'extensions/storybook/README.md',
  screenshots: ['extensions/storybook/metadata/workspace.png'],
  sha256: '0'.repeat(64),
  zip: 'storybook-1.2.0.zip',
};

const meta = {
  parameters: { layout: 'fullscreen' },
  title: 'Extensions/Modal',
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

function StoryFrame({ children }: { children: React.ReactNode }) {
  return <div className='dark min-h-screen bg-[#0e0e0e] p-6 text-foreground'>{children}</div>;
}

export const InstalledCard: Story = {
  render: () => {
    const [enabled, setEnabled] = useState(true);
    const extension = useMemo(() => ({ ...INSTALLED[0], state: { ...INSTALLED[0].state, enabled } }), [enabled]);
    return (
      <StoryFrame>
        <div className='max-w-xl'>
          <InstalledExtensionCard
            extension={extension}
            iconUrl={ICON}
            onDetails={() => undefined}
            onRemove={() => undefined}
            onSetEnabled={setEnabled}
          />
        </div>
      </StoryFrame>
    );
  },
};

export const InstalledGrid: Story = {
  render: () => (
    <StoryFrame>
      <InstalledTab
        extensions={INSTALLED}
        iconUrlFor={() => ICON}
        onDetails={() => undefined}
        onRemove={() => undefined}
        onSetEnabled={() => undefined}
      />
    </StoryFrame>
  ),
};

export const StoreDetail: Story = {
  render: () => (
    <StoryFrame>
      <div className='mx-auto flex min-h-[720px] max-w-6xl border border-border/60'>
        <StoreExtensionDetail
          changelogMarkdown={'## 1.2.0\n\n- Adds popup placement and faster readiness checks.'}
          entry={CATALOG_ENTRY}
          iconUrl={ICON}
          onBack={() => undefined}
          onInstall={() => undefined}
          readmeMarkdown={
            '# Storybook\n\nRun your project Storybook inside Ghostex, with the active project and worktree context already selected.\n\n## Features\n\n- Starts on demand\n- Works as a full view or popup\n- Stops with gxserver'
          }
          screenshotUrls={[]}
        />
      </div>
    </StoryFrame>
  ),
};

export const ConsentDialog: Story = {
  render: () => (
    <StoryFrame>
      <InstallConsentDialog entry={CATALOG_ENTRY} onCancel={() => undefined} onConfirm={() => undefined} open />
    </StoryFrame>
  ),
};
