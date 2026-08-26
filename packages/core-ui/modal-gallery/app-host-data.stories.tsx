import type { Meta, StoryObj } from '@storybook/react-vite';
import type {
  GhostexExtensionCatalogEntry,
  GhostexExtensionStatePatch,
  GhostexInstalledExtension,
} from '@/packages/shared/ghostex-extensions';
import { ExtensionsModal } from '../extensions-modal';
import type { ExtensionsModalTransport } from '../extensions-modal/transport';
import { ModalStorySurface, modalStoryParameters } from './modal-story-surface';

const noop = () => undefined;

const EXTENSION: GhostexInstalledExtension = {
  id: 'storybook',
  manifest: {
    author: 'Ghostex',
    categories: ['Developer Tools'],
    defaultPlacement: 'view',
    description: 'Run a project Storybook as a focused Ghostex workspace view.',
    icon: 'icon.svg',
    name: 'storybook',
    permissions: ['exec', 'network'],
    placements: ['view', 'popup'],
    server: { command: 'bun run storybook', readiness: { httpGet: '/' } },
    title: 'Storybook',
    version: '1.2.0',
  },
  runtime: { state: 'stopped' },
  state: {
    chatBarAutoOpen: false,
    enabled: true,
    grantedPermissions: ['exec', 'network'],
    pinned: true,
    placement: 'view',
    preferences: {},
    storage: {},
    terminalPlacement: 'splitRight',
    version: '1.2.0',
  },
};

const CATALOG_ENTRY: GhostexExtensionCatalogEntry = {
  ...EXTENSION.manifest,
  changelog: 'extensions/storybook/CHANGELOG.md',
  readme: 'extensions/storybook/README.md',
  screenshots: [],
  sha256: '0'.repeat(64),
  zip: 'storybook-1.2.0.zip',
};

function patchExtension(extension: GhostexInstalledExtension, patch: GhostexExtensionStatePatch) {
  return { ...extension, state: { ...extension.state, ...patch } };
}

const extensionsTransport: ExtensionsModalTransport = {
  catalog: async () => ({
    catalog: { extensions: [CATALOG_ENTRY], publishedAt: '2026-08-26T00:00:00.000Z', schemaVersion: 1 },
    source: 'cache',
    url: 'https://extensions.example.test/catalog.json',
  }),
  install: async () => ({ extension: EXTENSION }),
  list: async () => ({ extensions: [EXTENSION] }),
  setState: async (_id, patch) => ({ extension: patchExtension(EXTENSION, patch) }),
  uninstall: async (id) => ({ id, uninstalled: true }),
};

const meta = {
  parameters: modalStoryParameters,
  title: 'Modals/App Host/Data and Libraries',
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

export const Extensions: Story = {
  render: () => (
    <ModalStorySurface>
      <ExtensionsModal isOpen onClose={noop} theme='dark-2' transport={extensionsTransport} />
    </ModalStorySurface>
  ),
};
