import type { Meta, StoryObj } from '@storybook/react-vite';
import { userEvent, within } from 'storybook/test';
import type { SidebarAgentButton } from '@/packages/shared/sidebar-agents';
import { ExportTranscriptModal } from '../export-transcript-result-modal';
import { GitCommitModal, type GitCommitModalDraft } from '../git-commit-modal';
import { GitFileDiffModal, type GitFileDiffModalDraft } from '../git-file-diff-modal';
import { WorktreeDeleteModal, type WorktreeDeleteModalDraft } from '../worktree-delete-modal';
import { WorktreeRenameModal, type WorktreeRenameModalDraft } from '../worktree-rename-modal';
import { ModalStorySurface, modalStoryParameters } from './modal-story-surface';

const noop = () => undefined;

const AGENTS: SidebarAgentButton[] = [
  { agentId: 'codex', command: 'codex', icon: 'codex', isDefault: true, name: 'Codex' },
  { agentId: 'claude', command: 'claude', icon: 'claude', isDefault: false, name: 'Claude Code' },
];

const DIFF_DRAFT: GitFileDiffModalDraft = {
  additions: 12,
  deletions: 5,
  filePath: 'packages/core-ui/modal-gallery/overview.stories.tsx',
  patch: [
    '@@ -12,7 +12,9 @@',
    ' export const modalStories = [',
    "-  'Settings',",
    "+  'Settings',",
    "+  'Session Note',",
    "+  'Export Transcript',",
    "   'Add Project',",
  ].join('\n'),
};

const COMMIT_DRAFT: GitCommitModalDraft = {
  action: 'commit',
  agentId: 'codex',
  branch: 'feat/modal-gallery',
  changedFiles: [
    { additions: 182, deletions: 0, path: 'packages/core-ui/modal-gallery/app-host-prompts.stories.tsx' },
    { additions: 96, deletions: 2, path: 'packages/core-ui/modal-gallery/overview.stories.tsx' },
  ],
  confirmLabel: 'Commit changes',
  description: 'Review the modal gallery changes before creating the commit.',
  isWorktree: true,
  requestId: 'storybook-git-commit',
  showCommitMessage: true,
  suggestedBody: 'Collect every active React modal under one Storybook navigation root.',
  suggestedSubject: 'feat: add modal gallery',
  worktreeName: 'Ghostex-modal-gallery',
};

const DELETE_DRAFT: WorktreeDeleteModalDraft = {
  branch: 'feat/modal-gallery',
  canDeleteLocalBranch: true,
  groupId: 'story-worktree-group',
  hasChanges: true,
  localBranchName: 'feat/modal-gallery',
  projectId: 'story-worktree-project',
  remoteBranchExists: true,
  remoteBranchName: 'feat/modal-gallery',
  remoteName: 'origin',
  statusSummary: ' M packages/core-ui/styles/modals.css\n?? packages/core-ui/modal-gallery/',
  worktreeName: 'Ghostex-modal-gallery',
};

const RENAME_DRAFT: WorktreeRenameModalDraft = {
  branch: 'feat/modal-gallery',
  currentName: 'modal-gallery',
  currentPath: '/Users/story/dev/Ghostex-modal-gallery',
  parentFolderName: 'Ghostex',
  parentProjectPath: '/Users/story/dev/Ghostex',
  projectId: 'story-worktree-project',
  registeredProjectPaths: ['/Users/story/dev/Ghostex-settings-redesign'],
  renameBranchDefault: true,
  warnings: ['A remote branch already exists and will keep its current name.'],
  worktreeName: 'Ghostex-modal-gallery',
};

const meta = {
  parameters: modalStoryParameters,
  title: 'Modals/App Host/Git and Export',
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

export const GitCommit: Story = {
  render: () => (
    <ModalStorySurface>
      <GitCommitModal
        agents={AGENTS}
        draft={COMMIT_DRAFT}
        isOpen
        onCancel={noop}
        onConfirm={noop}
        onMultipleCommits={noop}
        onOpenFileDiff={noop}
        promptAgentId='codex'
        theme='dark-2'
      />
    </ModalStorySurface>
  ),
};

export const DirectMergeConfirmation: Story = {
  render: () => (
    <ModalStorySurface>
      <GitCommitModal
        agents={AGENTS}
        draft={COMMIT_DRAFT}
        isOpen
        onCancel={noop}
        onConfirm={noop}
        onDirectMerge={noop}
        onMultipleCommits={noop}
        onOpenFileDiff={noop}
        promptAgentId='codex'
        theme='dark-2'
      />
    </ModalStorySurface>
  ),
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.click(await body.findByRole('button', { name: 'Merge to main' }));
  },
};

export const GitFileDiff: Story = {
  render: () => (
    <ModalStorySurface>
      <GitFileDiffModal draft={DIFF_DRAFT} isOpen onClose={noop} theme='dark-2' />
    </ModalStorySurface>
  ),
};

export const DeleteWorktree: Story = {
  render: () => (
    <ModalStorySurface>
      <WorktreeDeleteModal draft={DELETE_DRAFT} isOpen onCancel={noop} onCommit={noop} onDelete={noop} theme='dark-2' />
    </ModalStorySurface>
  ),
};

export const RenameWorktree: Story = {
  render: () => (
    <ModalStorySurface>
      <WorktreeRenameModal draft={RENAME_DRAFT} isOpen onCancel={noop} onRename={noop} theme='dark-2' />
    </ModalStorySurface>
  ),
};

export const ExportTranscriptOptions: Story = {
  render: () => (
    <ModalStorySurface>
      <ExportTranscriptModal
        agents={AGENTS}
        defaultAgentId='codex'
        isOpen
        onClose={noop}
        onExport={noop}
        onStartNewConversation={noop}
        stage={{ stage: 'options' }}
      />
    </ModalStorySurface>
  ),
};

export const ExportTranscriptDone: Story = {
  render: () => (
    <ModalStorySurface>
      <ExportTranscriptModal
        agents={AGENTS}
        defaultAgentId='codex'
        isOpen
        onClose={noop}
        onExport={noop}
        onRevealInFinder={noop}
        onStartNewConversation={noop}
        stage={{
          agentId: 'codex',
          canReveal: true,
          path: '/Users/story/dev/Ghostex/docs/transcripts/unify-modal-styling.md',
          stage: 'done',
        }}
      />
    </ModalStorySurface>
  ),
};
