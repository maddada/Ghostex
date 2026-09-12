import {
  IconAlarm,
  IconArchive,
  IconChevronLeft,
  IconClockX,
  IconMoon,
  IconNote,
  IconPencil,
  IconPin,
  IconTag,
  IconX,
} from '@tabler/icons-react';
import { KeyboardSensor, PointerSensor } from '@dnd-kit/dom';
import { DragDropProvider, type DragDropEventHandlers } from '@dnd-kit/react';
import { isSortableOperation, useSortable } from '@dnd-kit/react/sortable';
import { useEffect, useId, useRef, type ReactNode } from 'react';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/packages/components/ui/tooltip';
import { cn } from '@/packages/components/utils';
import {
  moveSessionCardHoverButton,
  normalizeSessionCardHoverButtons,
  SESSION_CARD_HOVER_BUTTON_LABELS,
  SESSION_CARD_HOVER_CHEVRON_ID,
  setSessionCardHoverButtonEnabled,
  type SessionCardHoverButtonId,
  type SessionCardHoverButtonItem,
} from '../../shared/session-card-hover-actions';
import { getSidebarReorderActivationConstraints } from '../sidebar-reorder-activation';
import { SettingRow, setSettingsSortableRowElement } from './fields';
import { type SettingModificationProps } from './types';

const HOVER_BUTTON_ICONS: Record<SessionCardHoverButtonId, ReactNode> = {
  chevron: <IconChevronLeft aria-hidden='true' size={16} stroke={2} />,
  close: <IconX aria-hidden='true' size={16} stroke={1.8} />,
  closeAfterDone: <IconClockX aria-hidden='true' size={16} stroke={1.8} />,
  note: <IconNote aria-hidden='true' size={16} stroke={1.8} />,
  park: <IconArchive aria-hidden='true' size={16} stroke={1.8} />,
  pin: <IconPin aria-hidden='true' size={16} stroke={1.8} />,
  rename: <IconPencil aria-hidden='true' size={16} stroke={1.8} />,
  sleep: <IconMoon aria-hidden='true' size={16} stroke={1.8} />,
  snooze: <IconAlarm aria-hidden='true' size={16} stroke={1.8} />,
  tag: <IconTag aria-hidden='true' size={16} stroke={1.8} />,
};

const HOVER_BUTTON_DRAG_TYPE = 'settings-session-card-hover-button';

/*
 * Mouse users hold briefly or move decisively to drag; a plain click toggles. Same gesture the
 * sidebar uses for its own reorders, so the two never disagree.
 */
const sensors = [
  PointerSensor.configure({ activationConstraints: getSidebarReorderActivationConstraints }),
  KeyboardSensor,
];

/**
 * CDXC:Sessions 2026-09-12 DECISION:
 * User: the row shows each hover button as an icon (the tooltip names it); clicking toggles it, dragging reorders it, and the chevron is dragged like any other button to decide which buttons hide behind it.
 */
export function SessionCardHoverActionsField({
  advanced,
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
  value,
}: {
  description?: string;
  label: string;
  onChange: (items: readonly SessionCardHoverButtonItem[]) => void;
  value: readonly SessionCardHoverButtonItem[];
} & SettingModificationProps) {
  const id = useId();
  const items = normalizeSessionCardHoverButtons(value);
  const handleDragEnd = ((event) => {
    if (event.canceled || !isSortableOperation(event.operation)) {
      return;
    }
    const { source, target } = event.operation;
    if (!source) {
      return;
    }
    const targetIndex = 'index' in source && typeof source.index === 'number' ? source.index : target?.index;
    if (targetIndex == null || source.initialIndex === targetIndex) {
      return;
    }
    onChange(moveSessionCardHoverButton(items, source.initialIndex, targetIndex));
  }) satisfies DragDropEventHandlers['onDragEnd'];

  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
      wide
    >
      <DragDropProvider onDragEnd={handleDragEnd} sensors={sensors}>
        <div aria-label={label} className='session-card-hover-actions-field' id={id} role='group'>
          {items.map((item, index) => (
            <SessionCardHoverButtonToggle
              index={index}
              item={item}
              key={item.id}
              onEnabledChange={(enabled) => onChange(setSessionCardHoverButtonEnabled(items, item.id, enabled))}
            />
          ))}
        </div>
      </DragDropProvider>
    </SettingRow>
  );
}

function SessionCardHoverButtonToggle({
  index,
  item,
  onEnabledChange,
}: {
  index: number;
  item: SessionCardHoverButtonItem;
  onEnabledChange: (enabled: boolean) => void;
}) {
  const sortable = useSortable({
    accept: HOVER_BUTTON_DRAG_TYPE,
    data: { id: item.id, kind: HOVER_BUTTON_DRAG_TYPE },
    group: 'settings-session-card-hover-buttons',
    id: item.id,
    index,
    type: HOVER_BUTTON_DRAG_TYPE,
  });
  const { isDragging } = sortable;
  /*
   * A finished drag also fires `click` on the source button, which would flip the toggle the user
   * was only moving. `isDragging` is already false by then, so the drag is remembered here while it
   * runs and consumed by the click that follows it.
   */
  const didDragRef = useRef(false);
  useEffect(() => {
    if (isDragging) {
      didDragRef.current = true;
    }
  }, [isDragging]);
  const label = SESSION_CARD_HOVER_BUTTON_LABELS[item.id];
  const isChevron = item.id === SESSION_CARD_HOVER_CHEVRON_ID;

  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            aria-label={label}
            aria-pressed={item.enabled}
            className={cn(
              'session-card-hover-actions-field-item',
              isChevron && 'session-card-hover-actions-field-chevron'
            )}
            data-dragging={String(Boolean(isDragging))}
            data-enabled={String(item.enabled)}
            onClick={() => {
              if (didDragRef.current) {
                didDragRef.current = false;
                return;
              }
              onEnabledChange(!item.enabled);
            }}
            ref={(element) => setSettingsSortableRowElement(sortable, element)}
            type='button'
          >
            {HOVER_BUTTON_ICONS[item.id]}
          </button>
        }
      />
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
