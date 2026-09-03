/**
 * CDXC:DesignSystem 2026-08-24:
 * Reference story for the one segmented single-select control used across the
 * app (Settings, Add Worktree, Automate). It shows the stock shadcn
 * ButtonGroup shape — one bordered container, flat segments sharing a hairline,
 * only the outer corners rounded — with a highlighted selected segment, in both
 * the content-width and full-width layouts.
 */
import { useState } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { IconFolderOpen, IconGitBranch } from '@tabler/icons-react';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';

function SegmentedControlStory() {
  const [group, setGroup] = useState('button');
  const [preset, setPreset] = useState('recommended');
  const [mode, setMode] = useState('create');
  const [protocol, setProtocol] = useState('https');
  return (
    <div
      className='ghostex-root ghostex-settings-shadcn flex h-screen w-screen flex-col gap-8 bg-[#0e0e0e] p-10'
      data-sidebar-theme='dark-2'
    >
      <section className='flex flex-col gap-2'>
        <span className='text-[13px] text-muted-foreground'>Content width</span>
        <SegmentedControl aria-label='Size' onValueChange={setGroup} value={group}>
          <SegmentedControlItem value='large'>Large</SegmentedControlItem>
          <SegmentedControlItem value='button'>Button</SegmentedControlItem>
          <SegmentedControlItem value='group'>Group</SegmentedControlItem>
        </SegmentedControl>
      </section>
      <section className='flex flex-col gap-2'>
        <span className='text-[13px] text-muted-foreground'>Full width (settings fields)</span>
        <SegmentedControl aria-label='Preset' onValueChange={setPreset} stretch value={preset}>
          <SegmentedControlItem value='recommended'>Recommended</SegmentedControlItem>
          <SegmentedControlItem value='codex'>Codex</SegmentedControlItem>
          <SegmentedControlItem value='minimal'>Minimal</SegmentedControlItem>
          <SegmentedControlItem value='detailed'>Detailed</SegmentedControlItem>
        </SegmentedControl>
      </section>
      <section className='flex flex-col gap-2'>
        <span className='text-[13px] text-muted-foreground'>With icons (Add Worktree)</span>
        <SegmentedControl aria-label='Worktree mode' onValueChange={setMode} stretch value={mode}>
          <SegmentedControlItem value='create'>
            <IconGitBranch aria-hidden='true' data-icon='inline-start' />
            Create New
          </SegmentedControlItem>
          <SegmentedControlItem value='openExisting'>
            <IconFolderOpen aria-hidden='true' data-icon='inline-start' />
            Open Existing
          </SegmentedControlItem>
        </SegmentedControl>
      </section>
      <section className='flex flex-col gap-2'>
        <span className='text-[13px] text-muted-foreground'>Compact size with a disabled segment</span>
        <SegmentedControl aria-label='Protocol' onValueChange={setProtocol} size='sm' value={protocol}>
          <SegmentedControlItem value='https'>HTTPS</SegmentedControlItem>
          <SegmentedControlItem value='http'>HTTP</SegmentedControlItem>
          <SegmentedControlItem disabled value='socks'>
            SOCKS
          </SegmentedControlItem>
        </SegmentedControl>
      </section>
    </div>
  );
}

const meta: Meta<typeof SegmentedControlStory> = {
  component: SegmentedControlStory,
  title: 'Components/Segmented Control',
};

export default meta;
type Story = StoryObj<typeof SegmentedControlStory>;

export const Default: Story = {};
