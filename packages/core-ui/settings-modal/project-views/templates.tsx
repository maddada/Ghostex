import { useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { IconPlus, IconTrash, IconWorld } from '@tabler/icons-react';
import {
  BUILTIN_PROJECT_VIEW_TEMPLATES,
  DEFAULT_PROJECT_VIEW_SOURCE,
  projectViewDescription,
  type ProjectViewTemplate,
} from '@/packages/shared/ghostex-settings/project-views';
import { SettingsInput, SettingsListItem } from '../fields';

export function ProjectViewTemplates({
  templates,
  onSelect,
  onRemove,
  onCancel,
}: {
  templates: readonly ProjectViewTemplate[];
  onSelect: (template: ProjectViewTemplate) => void;
  onRemove: (id: string) => void;
  onCancel: () => void;
}) {
  const [query, setQuery] = useState('');
  const all = [...BUILTIN_PROJECT_VIEW_TEMPLATES, ...templates];
  return (
    <div className='flex flex-col gap-3 py-3'>
      <SettingsInput
        aria-label='Search view templates'
        value={query}
        onChange={(e) => setQuery(e.currentTarget.value)}
        placeholder='Search templates'
        autoFocus
      />
      <div className='settings-list-rows'>
        {all
          .filter((t) => `${t.name} ${projectViewDescription(t)}`.toLowerCase().includes(query.toLowerCase()))
          .map((template) => (
            <SettingsListItem
              key={template.id}
              title={template.name}
              detail={projectViewDescription(template)}
              icon={<IconWorld className='size-4' />}
            >
              <div className='flex items-center gap-2'>
                {templates.some((t) => t.id === template.id) ? (
                  <Button
                    type='button'
                    variant='ghost'
                    size='icon-sm'
                    aria-label={`Remove template ${template.name}`}
                    onClick={() => onRemove(template.id)}
                  >
                    <IconTrash />
                  </Button>
                ) : null}
                <Button type='button' variant='outline' onClick={() => onSelect(template)}>
                  Use template
                </Button>
              </div>
            </SettingsListItem>
          ))}
      </div>
      <div className='settings-management-actions'>
        <Button
          type='button'
          variant='outline'
          onClick={() =>
            onSelect({ id: '', name: '', url: '', availability: 'all', source: { ...DEFAULT_PROJECT_VIEW_SOURCE } })
          }
        >
          <IconPlus />
          Start from scratch
        </Button>
        <Button type='button' variant='ghost' onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
}
