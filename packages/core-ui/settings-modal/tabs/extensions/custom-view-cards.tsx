import { useSortable } from '@dnd-kit/react/sortable';
import { IconGripVertical, IconPencil, IconPlus, IconTrash, IconWorld } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { Switch } from '@/packages/components/ui/switch';
import type { GhostexCustomView } from '@/packages/shared/ghostex-settings';
import { projectViewDescription } from '@/packages/shared/ghostex-settings/project-views';
import { ExtensionGridCard, type ExtensionFilterSubject } from '../../../extensions-modal';
import { createSettingsCustomViewDragData } from '../../drag-data';
import { setSettingsSortableRowElement } from '../../fields';

export function customViewFilterSubject(view: GhostexCustomView): ExtensionFilterSubject {
  return {
    categories: [],
    searchText: [projectViewDescription(view), view.url],
    source: 'custom',
    title: view.name,
    types: ['view'],
  };
}

function customViewKindLabel(view: GhostexCustomView): string {
  switch (view.source?.kind) {
    case 'report':
      return 'HTML report';
    case 'dev-server':
      return 'Dev server';
    default:
      return 'Website';
  }
}

export function CustomViewCard({
  editing,
  index,
  onEdit,
  onEnabledChange,
  onRemove,
  sortable: sortableEnabled,
  view,
}: {
  editing?: boolean;
  index: number;
  onEdit: () => void;
  onEnabledChange: (enabled: boolean) => void;
  onRemove: () => void;
  /** Reordering is off while a filter hides some views, since the drop index would not match the full list. */
  sortable: boolean;
  view: GhostexCustomView;
}) {
  const sortable = useSortable({
    accept: 'settings-custom-view',
    data: createSettingsCustomViewDragData(view.id),
    disabled: !sortableEnabled,
    group: 'settings-custom-views',
    id: view.id,
    index,
    type: 'settings-custom-view',
  });
  const { handleRef, isDragging } = sortable;

  return (
    <div
      className='extension-grid-card-sortable'
      data-dragging={String(Boolean(isDragging))}
      ref={(element) => setSettingsSortableRowElement(sortable, element)}
    >
      <ExtensionGridCard
        actions={
          <>
            <Button aria-label={`Edit ${view.name}`} onClick={onEdit} size='icon-xs' type='button' variant='ghost'>
              <IconPencil aria-hidden='true' />
            </Button>
            <Button aria-label={`Remove ${view.name}`} onClick={onRemove} size='icon-xs' type='button' variant='ghost'>
              <IconTrash aria-hidden='true' />
            </Button>
          </>
        }
        control={
          <Switch
            aria-label={`${view.enabled ? 'Disable' : 'Enable'} ${view.name}`}
            checked={view.enabled}
            onCheckedChange={onEnabledChange}
            size='sm'
          />
        }
        dataAttributes={{ 'data-view-id': view.id }}
        description={projectViewDescription(view)}
        editing={editing}
        enabled={view.enabled}
        icon={
          <span
            aria-hidden='true'
            className='extensions-icon flex size-9 shrink-0 items-center justify-center p-1.5 text-[#b9b9b9]'
          >
            <IconWorld className='size-4' />
          </span>
        }
        leading={
          sortableEnabled ? (
            <Button
              aria-label={`Reorder ${view.name}`}
              className='extension-grid-card-grip'
              ref={handleRef}
              size='icon-xs'
              type='button'
              variant='ghost'
            >
              <IconGripVertical aria-hidden='true' />
            </Button>
          ) : null
        }
        meta={customViewKindLabel(view)}
        title={view.name}
      />
    </div>
  );
}

export function AddCustomViewCard({ onClick }: { onClick: () => void }) {
  return (
    <button className='extension-grid-add-card' onClick={onClick} type='button'>
      <IconPlus aria-hidden='true' className='size-4' />
      Add view
    </button>
  );
}
