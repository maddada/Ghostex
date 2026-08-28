import { useEffect, useState } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { userEvent, within } from 'storybook/test';
import { ManageRenameDialog } from '@/apps/desktop/views/manage/file-tree-ui';
import { MANAGE_STYLES } from '@/apps/desktop/views/manage/styles';
import { PROJECT_BOARD_STYLES } from '@/apps/desktop/views/project-board/styles';
import { RemoteMigrateGateNotice } from '@/apps/desktop/views/project-board/remote-migrate-gate';
import { FindPromptsHost } from '@/apps/web/src/app/find-prompts-host';
import { MachinesControl } from '@/apps/web/src/machines/MachinesControl';
import '@/apps/web/src/styles.css';
import { SessionChatImageViewerProvider, useSessionChatImageViewer } from '../chat/session-chat-image-viewer';
import { SessionChatSaveMarkdownDialog } from '../chat/session-chat-save-markdown-dialog';
import { ModalStorySurface, modalStoryParameters } from './modal-story-surface';

const noop = () => undefined;

const meta = {
  parameters: modalStoryParameters,
  title: 'Modals/Embedded and Web',
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

function ManageRenameStory() {
  const [value, setValue] = useState('modal-comparison.md');
  return (
    <ModalStorySurface>
      <style>{MANAGE_STYLES}</style>
      <ManageRenameDialog isRenaming={false} onCancel={noop} onChange={setValue} onSubmit={noop} value={value} />
    </ModalStorySurface>
  );
}

export const DocsRename: Story = { render: () => <ManageRenameStory /> };

export const SaveChatMessageToMarkdown: Story = {
  render: () => (
    <ModalStorySurface>
      <SessionChatSaveMarkdownDialog
        listExistingPaths={async () => ['docs/2026-08-26/Modal review 1.md']}
        markdown='## Modal review\n\nUnify headers, spacing, fields, and footer actions.'
        onOpenChange={noop}
        open
        save={async ({ path }) => ({ path })}
        sessionTitle='Modal review'
        theme='dark'
      />
    </ModalStorySurface>
  ),
};

export const BeadsMigrationConfirmation: Story = {
  render: () => (
    <ModalStorySurface>
      <style>{PROJECT_BOARD_STYLES}</style>
      <div className='project-board-root min-h-screen p-6'>
        <RemoteMigrateGateNotice
          gate={{
            currentVersion: 3,
            decision: 'manual',
            latestVersion: 4,
            options: [
              {
                commands: ['bd migrate', 'bd sync'],
                id: 'migrate',
                risk: 'running this on multiple clones forks the shared schema',
                when: 'this is the designated canonical clone',
              },
              {
                commands: ['bd adopt'],
                id: 'adopt',
                risk: 'unpushed local database changes can be lost',
                when: 'another clone has already migrated and published',
              },
            ],
            pending: 1,
          }}
          projectPath='/Users/story/dev/Ghostex'
        />
      </div>
    </ModalStorySurface>
  ),
};

const IMAGE_DATA_URL =
  "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='1200' height='800' viewBox='0 0 1200 800'%3E%3Crect width='1200' height='800' fill='%23191919'/%3E%3Crect x='80' y='80' width='1040' height='640' rx='32' fill='%23262626' stroke='%235e5e5e' stroke-width='4'/%3E%3Ctext x='600' y='390' text-anchor='middle' fill='%23f1f1f1' font-family='sans-serif' font-size='54'%3EModal comparison image%3C/text%3E%3Ctext x='600' y='460' text-anchor='middle' fill='%23a3a3a3' font-family='sans-serif' font-size='28'%3EFull-size chat image viewer%3C/text%3E%3C/svg%3E";

function OpenImageViewer() {
  const viewer = useSessionChatImageViewer();
  useEffect(() => {
    viewer?.open({ alt: 'Modal comparison image', url: IMAGE_DATA_URL });
  }, [viewer]);
  return null;
}

export const ChatImageViewer: Story = {
  render: () => (
    <ModalStorySurface>
      <SessionChatImageViewerProvider>
        <OpenImageViewer />
      </SessionChatImageViewerProvider>
    </ModalStorySurface>
  ),
};

export const WebMachines: Story = {
  render: () => (
    <div className='agents-workspace min-h-screen p-6'>
      <MachinesControl />
    </div>
  ),
  play: async ({ canvasElement }) => {
    await userEvent.click(await within(canvasElement).findByRole('button', { name: 'Machines' }));
  },
};

export const WebPromptSearch: Story = {
  render: () => (
    <ModalStorySurface>
      <FindPromptsHost machineId='local' onClose={noop} />
    </ModalStorySurface>
  ),
};
