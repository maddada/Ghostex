/**
 * CDXC:ModalRedesign 2026-08-24:
 * Visual review story for the Add Worktree modal so the Codex-style restyle
 * can be confirmed without launching the app. Renders the real
 * WorktreeCreateModal with mock agents; bridge calls (existing worktrees,
 * base branches) are no-ops, so the selects simply show their empty states.
 */
import type { Meta, StoryObj } from '@storybook/react-vite';
import type { SidebarAgentButton } from '@/packages/shared/sidebar-agents';
import { WorktreeCreateModal } from './worktree-create-modal';

const AGENTS: SidebarAgentButton[] = [
  { agentId: 'claude', command: 'claude', isDefault: true, name: 'Claude Code' },
  { agentId: 'codex', command: 'codex', isDefault: false, name: 'Codex' },
];

function WorktreeCreateModalStory() {
  return (
    <div className='ghostex-root h-screen w-screen bg-[#0e0e0e]' data-sidebar-theme='dark-2'>
      <WorktreeCreateModal
        agents={AGENTS}
        defaultAgentId='claude'
        isOpen
        onCancel={() => {}}
        onConfirm={() => {}}
        projectName='Ghostex'
      />
    </div>
  );
}

const meta: Meta<typeof WorktreeCreateModalStory> = {
  component: WorktreeCreateModalStory,
  title: 'Modals/Add Worktree',
};

export default meta;
type Story = StoryObj<typeof WorktreeCreateModalStory>;

export const Create: Story = {};
