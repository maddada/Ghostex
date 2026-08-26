import { useEffect } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import type { SidebarAgentButton } from '@/packages/shared/sidebar-agents';
import { AgentConfigModal } from '../agent-config-modal';
import { ConfirmationModal } from '../confirmation-modal';
import { FirstUserMessageModal } from '../first-user-message-modal';
import { MissingProjectFolderModal } from '../missing-project-folder-modal';
import { PortlessSetupModal } from '../portless-setup-modal';
import { RemoteGxserverInstallModal } from '../remote-gxserver-install-modal';
import { RemoteProjectPickerModal } from '../remote-project-picker/remote-project-picker-modal';
import { ScratchPadModal } from '../scratch-pad-modal';
import { SessionNoteModal } from '../session-note-modal';
import { SessionRenameModal } from '../session-rename-modal';
import { useSidebarStore } from '../sidebar-store';
import { UpdateAvailableModal } from '../update-available-modal';
import { WatchGhostexVideoModal } from '../watch-ghostex-video-modal';
import { ModalStorySurface, modalStoryParameters } from './modal-story-surface';

const noop = () => undefined;

const AGENTS: SidebarAgentButton[] = [
  { agentId: 'codex', command: 'codex', icon: 'codex', isDefault: true, name: 'Codex' },
  { agentId: 'claude', command: 'claude', icon: 'claude', isDefault: false, name: 'Claude Code' },
];

const meta = {
  parameters: modalStoryParameters,
  title: 'Modals/App Host/Prompts and Setup',
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

export const Confirmation: Story = {
  render: () => (
    <ModalStorySurface>
      <ConfirmationModal
        confirmLabel='Delete session'
        description='This removes the session from Ghostex. The project files are not changed.'
        isOpen
        onCancel={noop}
        onConfirm={noop}
        title='Delete this session?'
      />
    </ModalStorySurface>
  ),
};

export const FirstUserMessage: Story = {
  render: () => (
    <ModalStorySurface>
      <FirstUserMessageModal
        isOpen
        message='Please review the modal system, identify the visual inconsistencies, and propose one shared design language.'
        onClose={noop}
        title='Modal visual audit'
      />
    </ModalStorySurface>
  ),
};

export const MissingProjectFolder: Story = {
  render: () => (
    <ModalStorySurface>
      <MissingProjectFolderModal
        isOpen
        onCancel={noop}
        onLocate={noop}
        onRemove={noop}
        projectName='Ghostex'
        projectPath='/Users/story/dev/Ghostex'
      />
    </ModalStorySurface>
  ),
};

export const RemoteGxserverInstall: Story = {
  render: () => (
    <ModalStorySurface>
      <RemoteGxserverInstallModal isOpen machineName='Build Server' onApprove={noop} onCancel={noop} />
    </ModalStorySurface>
  ),
};

export const RemoteProjectPicker: Story = {
  render: () => (
    <ModalStorySurface>
      <RemoteProjectPickerModal
        isOpen
        machineName='Build Server'
        onAddProject={async () => undefined}
        onBrowse={async () => ({
          entries: [
            { fullPath: '/home/story/Ghostex', name: 'Ghostex' },
            { fullPath: '/home/story/sites', name: 'sites' },
          ],
          parentPath: '/home',
        })}
        onClose={noop}
      />
    </ModalStorySurface>
  ),
};

export const PortlessSetup: Story = {
  render: () => (
    <ModalStorySurface>
      <PortlessSetupModal
        isOpen
        mode='firstSetup'
        onAdminAction={noop}
        onCancel={noop}
        onDisable={noop}
        onPostpone={noop}
        protocol='https'
      />
    </ModalStorySurface>
  ),
};

export const UpdateAvailable: Story = {
  render: () => (
    <ModalStorySurface>
      <UpdateAvailableModal
        isOpen
        onCancel={noop}
        onDownload={noop}
        onRestart={noop}
        update={{
          notesMarkdown: '- Unified modal review gallery\n- Faster project switching\n- Improved remote sessions',
          portable: false,
          state: 'available',
          version: '8.1.0',
        }}
      />
    </ModalStorySurface>
  ),
};

export const AgentConfig: Story = {
  render: () => (
    <ModalStorySurface>
      <AgentConfigModal
        draft={{ acceptAllMode: 'inherit', agentId: 'codex', command: 'codex', icon: 'codex', name: 'Codex' }}
        isOpen
        onCancel={noop}
        onSave={noop}
        theme='dark-2'
      />
    </ModalStorySurface>
  ),
};

export const SessionRename: Story = {
  render: () => (
    <ModalStorySurface>
      <SessionRenameModal
        agents={AGENTS}
        canGenerateNameFromSessionHistory
        initialTitle='Unify modal styling'
        isOpen
        onCancel={noop}
        onConfirm={noop}
        promptAgentId='codex'
      />
    </ModalStorySurface>
  ),
};

export const SessionNote: Story = {
  render: () => (
    <ModalStorySurface>
      <SessionNoteModal
        initialNote='Compare spacing, typography, action order, and surface colors across every modal.'
        isOpen
        onCancel={noop}
        onConfirm={noop}
        sessionTitle='Unify modal styling'
      />
    </ModalStorySurface>
  ),
};

function ScratchPadStory() {
  useEffect(() => {
    useSidebarStore.setState({
      scratchPadContent: 'Modal review notes:\n\n- Header hierarchy\n- Footer alignment\n- Button order',
    });
    return () => useSidebarStore.getState().reset();
  }, []);

  return (
    <ModalStorySurface>
      <ScratchPadModal isOpen onClose={noop} onDebug={noop} onSave={noop} />
    </ModalStorySurface>
  );
}

export const ScratchPad: Story = { render: () => <ScratchPadStory /> };

export const VideoWalkthrough: Story = {
  render: () => (
    <ModalStorySurface>
      <WatchGhostexVideoModal isOpen onClose={noop} theme='dark-2' />
    </ModalStorySurface>
  ),
};
