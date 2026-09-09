import { DragDropProvider, type DragDropEventHandlers } from '@dnd-kit/react';
import { isSortableOperation, useSortable } from '@dnd-kit/react/sortable';
import { IconArrowDown, IconArrowUp, IconGripVertical } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/packages/components/ui/dialog';
import type { GhostexInstalledExtension } from '@/packages/shared/ghostex-extensions';
import type { ghostexSettings } from '@/packages/shared/ghostex-settings';
import {
  titlebarViewOrderItems,
  type TitlebarViewOrderItem,
} from '@/packages/shared/ghostex-settings/titlebar-view-order';
import { formatSidebarHotkeyLabel } from '../../hotkey-label';
import { moveId } from '../drag-data';
import { setSettingsSortableRowElement } from '../fields';
import './titlebar-view-order-dialog.css';

/**
 * CDXC:Settings 2026-09-09 DECISION:
 * User: a button in Settings opens the view-order popup with a backdrop behind it.
 * The nested dialog stays inside the existing native Settings window.
 */
export function TitlebarViewOrderDialog({
  open,
  onOpenChange,
  settings,
  installed,
  onChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  settings: ghostexSettings;
  installed: readonly GhostexInstalledExtension[];
  onChange: (order: string[]) => void;
}) {
  const items = titlebarViewOrderItems(settings, installed);
  const visibleItems = items.filter((item) => item.visible);
  const move = (from: number, to: number) => {
    if (from === to || to < 0 || to >= items.length) return;
    const ids = moveId(
      items.map((item) => item.id),
      from,
      to
    );
    const knownIds = new Set(ids);
    onChange([...ids, ...settings.titlebarViewOrder.filter((id) => !knownIds.has(id))]);
  };
  const onDragEnd: DragDropEventHandlers['onDragEnd'] = (event) => {
    if (event.canceled || !isSortableOperation(event.operation)) return;
    const { source } = event.operation;
    if (source) move(source.initialIndex, source.index);
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent nested showCloseButton className='ghostex-settings-shadcn settings-titlebar-view-order-dialog'>
        <DialogHeader>
          <DialogTitle>Arrange titlebar views</DialogTitle>
          <DialogDescription>
            Drag views into order or use the arrows. Numbered view shortcuts follow the visible order. Hidden views keep
            their place for when you enable them.
          </DialogDescription>
        </DialogHeader>
        <DragDropProvider onDragEnd={onDragEnd}>
          <div
            className='settings-titlebar-view-order-list rounded-lg border border-border'
            aria-label='Titlebar view order'
          >
            {items.map((item, index) => {
              const visibleIndex = visibleItems.findIndex((view) => view.id === item.id);
              const actionId = `switchTitlebarView${visibleIndex + 1}` as keyof ghostexSettings['hotkeys'];
              const shortcut = visibleIndex >= 0 && visibleIndex < 9 ? settings.hotkeys[actionId] : '';
              return (
                <ViewOrderRow
                  key={item.id}
                  item={item}
                  index={index}
                  count={items.length}
                  shortcut={shortcut ? formatSidebarHotkeyLabel(shortcut) : ''}
                  onMove={(to) => move(index, to)}
                />
              );
            })}
          </div>
        </DragDropProvider>
        <DialogFooter>
          <Button variant='outline' onClick={() => onChange([])}>
            Reset order
          </Button>
          <Button onClick={() => onOpenChange(false)}>Done</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ViewOrderRow({
  item,
  index,
  count,
  shortcut,
  onMove,
}: {
  item: TitlebarViewOrderItem;
  index: number;
  count: number;
  shortcut: string;
  onMove: (index: number) => void;
}) {
  const sortable = useSortable({
    id: item.id,
    index,
    type: 'titlebar-view-order',
    accept: 'titlebar-view-order',
    group: 'titlebar-view-order',
  });
  return (
    <div
      ref={(element) => setSettingsSortableRowElement(sortable, element)}
      className='flex min-h-14 items-center gap-2 border-b border-border bg-popover px-2 py-2 last:border-b-0'
      data-dragging={String(sortable.isDragging)}
    >
      <Button aria-label={`Drag ${item.title}`} ref={sortable.handleRef} size='icon-sm' variant='ghost'>
        <IconGripVertical aria-hidden='true' />
      </Button>
      <div className='min-w-0 flex-1'>
        <div className='truncate text-sm'>{item.title}</div>
        <div className='text-[13px] text-muted-foreground'>
          {item.source}
          {item.visible ? '' : ' · Hidden'}
        </div>
      </div>
      {shortcut ? <span className='shrink-0 text-[13px] text-muted-foreground'>{shortcut}</span> : null}
      <Button
        aria-label={`Move ${item.title} up`}
        disabled={index === 0}
        size='icon-sm'
        variant='ghost'
        onClick={() => onMove(index - 1)}
      >
        <IconArrowUp aria-hidden='true' />
      </Button>
      <Button
        aria-label={`Move ${item.title} down`}
        disabled={index === count - 1}
        size='icon-sm'
        variant='ghost'
        onClick={() => onMove(index + 1)}
      >
        <IconArrowDown aria-hidden='true' />
      </Button>
    </div>
  );
}
