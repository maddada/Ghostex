import type { Meta, StoryObj } from '@storybook/react-vite';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/packages/components/ui/card';
import { Separator } from '@/packages/components/ui/separator';
import { ModalStorySurface, modalStoryParameters } from './modal-story-surface';

type ModalGroup = {
  description: string;
  modals: readonly string[];
  title: string;
};

const MODAL_GROUPS: readonly ModalGroup[] = [
  {
    description: 'The canonical desktop app-modal host surfaces.',
    modals: [
      'Add Project',
      'Add Worktree',
      'Agent Config',
      'Agents Hub',
      'Command Palette',
      'Delayed Actions',
      'Discover Ghostex',
      'Extensions',
      'First Launch Setup',
      'Settings',
      'Watch Walkthrough',
    ],
    title: 'Large workflows',
  },
  {
    description: 'Project, session, remote, and reusable confirmation prompts.',
    modals: [
      'Confirmation',
      'First User Message',
      'Missing Project Folder',
      'Portless Setup',
      'Previous Sessions',
      'Projects',
      'Remote gxserver Install',
      'Remote Project Picker',
      'Session Note',
      'Session Rename',
      'Stashed Prompts',
      'Update Available',
    ],
    title: 'Prompts and utilities',
  },
  {
    description: 'Source control review and project lifecycle dialogs.',
    modals: ['Delete Worktree', 'Handoff / Export', 'Git Commit', 'Git File Diff', 'Rename Worktree'],
    title: 'Git and export',
  },
  {
    description: 'Dialogs rendered within embedded app pages instead of the desktop app-modal host.',
    modals: [
      'Automation',
      'Beads Migration Confirmation',
      'Board Columns',
      'Chat Image Viewer',
      'Docs Rename',
      'Edit Ticket',
      'Extension Install Consent',
      'New Ticket',
      'Save Chat Message to Markdown',
      'Web Machines',
      'Web Prompt Search',
      'Worktree Cleanup',
    ],
    title: 'Embedded and nested',
  },
];

function ModalGalleryOverview() {
  const modalCount = MODAL_GROUPS.reduce((total, group) => total + group.modals.length, 0);

  return (
    <ModalStorySurface>
      <main className='mx-auto flex w-full max-w-6xl flex-col gap-6 p-8 text-foreground'>
        <header className='flex flex-col gap-2'>
          <p className='text-sm text-muted-foreground'>Design review inventory</p>
          <h1 className='text-3xl font-medium tracking-tight'>Ghostex React modals</h1>
          <p className='max-w-3xl text-sm leading-6 text-muted-foreground'>
            {modalCount} shipped modal surfaces are collected under the Modals folder. Each visual story mounts the real
            production component in isolation, because the dialogs portal to the document body and cannot be safely
            stacked in one canvas.
          </p>
        </header>
        <Separator />
        <section className='grid gap-4 md:grid-cols-2'>
          {MODAL_GROUPS.map((group) => (
            <Card key={group.title} size='sm'>
              <CardHeader>
                <CardTitle>{group.title}</CardTitle>
                <CardDescription>{group.description}</CardDescription>
              </CardHeader>
              <CardContent>
                <ul className='grid gap-2 text-sm sm:grid-cols-2'>
                  {group.modals.map((modal) => (
                    <li className='rounded-md border border-border bg-muted/30 px-3 py-2' key={modal}>
                      {modal}
                    </li>
                  ))}
                </ul>
              </CardContent>
            </Card>
          ))}
        </section>
        <p className='text-xs leading-5 text-muted-foreground'>
          Unmounted legacy components are intentionally excluded: Add Repository, the standalone Hotkeys modal, and
          Configure Agents. Their active entry points now use Add Project or Settings.
        </p>
      </main>
    </ModalStorySurface>
  );
}

const meta = {
  parameters: modalStoryParameters,
  title: 'Modals/Overview',
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

export const Inventory: Story = { render: () => <ModalGalleryOverview /> };
