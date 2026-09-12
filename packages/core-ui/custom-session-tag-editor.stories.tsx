import { useState } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { IconGripVertical } from '@tabler/icons-react';
import { CustomSessionTagEditorForm } from './custom-session-tag-editor';

/**
 * The New tag form as it renders inside Settings > General > Sidebar > Sidebar Tags,
 * with two of the list's own rows beneath it so the shared control can be checked
 * against the rows it sits among.
 */
function SidebarTagsSettingsStory() {
  const [isCreating, setIsCreating] = useState(true);
  const [created, setCreated] = useState<string>();
  return (
    <div
      className='ghostex-root ghostex-settings-shadcn flex min-h-screen flex-col gap-3 bg-background p-10 text-foreground'
      data-sidebar-theme='dark-2'
    >
      <div className='flex items-center justify-between'>
        <span className='text-sm font-medium'>Tag filter list</span>
        <button className='h-8 border border-border px-3 text-sm' onClick={() => setIsCreating(true)} type='button'>
          Add tag
        </button>
      </div>
      {isCreating ? (
        <div className='settings-management-row w-full border border-border bg-muted/20' style={{ padding: 10 }}>
          <CustomSessionTagEditorForm
            onCancel={() => setIsCreating(false)}
            onSubmit={(tag) => {
              setCreated(`${tag.name} · ${tag.icon} · ${tag.color}`);
              setIsCreating(false);
            }}
            suggestedColorIndex={1}
          />
        </div>
      ) : null}
      {['Testing', 'Research', 'Design'].map((label) => (
        <div
          className='settings-management-row flex w-full items-center gap-2 border border-border bg-muted/20 p-2'
          key={label}
        >
          <span className='flex size-8 items-center justify-center text-muted-foreground'>
            <IconGripVertical aria-hidden='true' size={16} />
          </span>
          <span className='settings-management-icon flex size-8 shrink-0 items-center justify-center bg-muted' />
          <span className='min-w-0 flex-1 truncate text-sm font-medium'>{label}</span>
        </div>
      ))}
      {created ? <p className='text-sm text-muted-foreground'>Created: {created}</p> : null}
    </div>
  );
}

const meta = {
  component: SidebarTagsSettingsStory,
  parameters: { layout: 'fullscreen' },
  title: 'Sessions/Custom Session Tag Editor',
} satisfies Meta<typeof SidebarTagsSettingsStory>;

export default meta;

export const SidebarTagsSettings: StoryObj<typeof meta> = {};
