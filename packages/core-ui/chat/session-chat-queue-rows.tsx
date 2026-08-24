// The Ghostex prompt-queue strip (plan 016 §4). It renders directly above the
// composer input, INSIDE the composer container, so the queue reads as part of
// what is about to be said rather than as part of the conversation.
//
// NAMING COLLISION, READ THIS TWICE: this is not `SessionChatMessage.queued`
// and not `.ghostex-chat-queued-label`. Those are the agent CLI's own internal
// queue and live in the transcript. These rows hold prompts the agent has
// never seen; gxserver releases one each time the agent stops.
//
// One row is ONE line: the first non-empty line of the prompt, ellipsized.
// Controls are a drag handle, Edit, Send now and Delete; a failed row swaps in
// its reason and a Retry. Every control is independently hidden when its
// endpoint is unreachable, so nothing here is ever a button that 404s.

import {
  IconAlertTriangle,
  IconArrowUp,
  IconGripVertical,
  IconLoader2,
  IconPencil,
  IconRefresh,
  IconTrash,
} from '@tabler/icons-react';
import { PointerSensor } from '@dnd-kit/dom';
import { DragDropProvider, type DragDropEventHandlers } from '@dnd-kit/react';
import { isSortableOperation, useSortable } from '@dnd-kit/react/sortable';
import type { ReactNode } from 'react';
import type { SessionChatQueuedPrompt } from '../../shared/session-chat';
import { Button } from '../../components/ui/button';
import { cn } from '@/packages/components/utils';
import { AppTooltip } from '../app-tooltip';
import {
  getSessionChatQueueDragActivationConstraints,
  isSessionChatQueueRowBusy,
  moveSessionChatQueueRow,
  sessionChatQueuePromptIds,
  sessionChatQueueRowPreview,
} from './session-chat-queue';

const queueRowSensors = [
  PointerSensor.configure({
    activationConstraints: getSessionChatQueueDragActivationConstraints,
  }),
];

export interface SessionChatQueueRowActions {
  /** Pull the row into the composer. Absent hides Edit. */
  onEdit?: (prompt: SessionChatQueuedPrompt) => void;
  /** Deliver now, exactly like pressing Enter. Absent hides Send now. */
  onSendNow?: (prompt: SessionChatQueuedPrompt) => void;
  /** Drop the row. Absent hides Delete. */
  onDelete?: (prompt: SessionChatQueuedPrompt) => void;
  /** Move a failed row back to queued. Absent hides Retry. */
  onRetry?: (prompt: SessionChatQueuedPrompt) => void;
  /** Commit a drag with the full id list, head first. Absent hides handles. */
  onReorder?: (promptIds: string[]) => void;
}

export interface SessionChatQueueRowsProps extends SessionChatQueueRowActions {
  /** Authoritative queue, head first. Empty renders nothing. */
  prompts: readonly SessionChatQueuedPrompt[];
  /** Input is held elsewhere: rows stay visible but every control is inert. */
  disabled?: boolean;
}

export function SessionChatQueueRows({
  disabled = false,
  onDelete,
  onEdit,
  onReorder,
  onRetry,
  onSendNow,
  prompts,
}: SessionChatQueueRowsProps) {
  if (prompts.length === 0) {
    return null;
  }

  const canDrag = onReorder !== undefined && !disabled && prompts.length > 1;

  const handleDragEnd = ((event) => {
    if (event.canceled || !isSortableOperation(event.operation) || !onReorder) {
      return;
    }
    const { source, target } = event.operation;
    if (!source) {
      return;
    }
    const toIndex = 'index' in source && typeof source.index === 'number' ? source.index : target?.index;
    if (toIndex == null || source.initialIndex === toIndex) {
      return;
    }
    onReorder(sessionChatQueuePromptIds(moveSessionChatQueueRow(prompts, source.initialIndex, toIndex)));
  }) satisfies DragDropEventHandlers['onDragEnd'];

  // The provider is unconditional: useSortable needs a manager in context, and
  // rows without a reachable reorder endpoint simply mount their sortable
  // disabled rather than mounting a different tree.
  const rows = prompts.map((prompt, index) => (
    <SessionChatQueueRow
      canDrag={canDrag}
      disabled={disabled}
      index={index}
      key={prompt.id}
      prompt={prompt}
      {...(onDelete ? { onDelete } : {})}
      {...(onEdit ? { onEdit } : {})}
      {...(onRetry ? { onRetry } : {})}
      {...(onSendNow ? { onSendNow } : {})}
    />
  ));

  return (
    <div aria-label='Queued prompts' className='ghostex-chat-queue-rows' role='list'>
      <DragDropProvider onDragEnd={handleDragEnd}>{rows}</DragDropProvider>
    </div>
  );
}

function SessionChatQueueRow({
  canDrag,
  disabled,
  index,
  onDelete,
  onEdit,
  onRetry,
  onSendNow,
  prompt,
}: SessionChatQueueRowActions & {
  canDrag: boolean;
  disabled: boolean;
  index: number;
  prompt: SessionChatQueuedPrompt;
}) {
  const busy = isSessionChatQueueRowBusy(prompt);
  const failed = prompt.state === 'failed';
  // A row the scheduler already claimed must not be edited, reordered or
  // deleted: those would race the send that is already in flight.
  const locked = disabled || busy;
  const sortable = useSortable({
    accept: 'session-chat-queued-prompt',
    disabled: !canDrag || busy,
    group: 'session-chat-queue',
    id: prompt.id,
    index,
    sensors: queueRowSensors,
    type: 'session-chat-queued-prompt',
  });
  const { handleRef, isDragging } = sortable;
  const setRowRef = (element: HTMLDivElement | null): void => {
    sortable.ref(element);
    sortable.sourceRef(element);
  };

  return (
    <div
      className={cn('ghostex-chat-queue-row', failed && 'ghostex-chat-queue-row-failed')}
      data-dragging={isDragging ? 'true' : undefined}
      data-state={prompt.state}
      ref={setRowRef}
      role='listitem'
    >
      {canDrag && !busy ? (
        <button
          aria-label='Reorder queued prompt'
          className='ghostex-chat-queue-row-handle'
          ref={handleRef}
          type='button'
        >
          <IconGripVertical aria-hidden='true' size={14} stroke={1.8} />
        </button>
      ) : (
        /*
        The gutter always holds SOMETHING. An empty one reads as an unexplained
        indent, so a row that cannot be dragged — one row, no reorder endpoint,
        a row already being delivered — shows the grip inert (or the delivery
        spinner) instead of a blank column the text is pushed away from.
        */
        <span aria-hidden='true' className='ghostex-chat-queue-row-handle-slot'>
          {busy ? (
            <IconLoader2 className='animate-spin' size={13} stroke={2} />
          ) : (
            <IconGripVertical className='ghostex-chat-queue-row-handle-inert' size={14} stroke={1.8} />
          )}
        </span>
      )}
      <span className='ghostex-chat-queue-row-text' title={prompt.text}>
        {sessionChatQueueRowPreview(prompt.text)}
      </span>
      {failed ? (
        <span className='ghostex-chat-queue-row-error' title={prompt.errorMessage ?? 'Delivery failed.'}>
          <IconAlertTriangle aria-hidden='true' size={13} stroke={2} />
          {prompt.errorMessage ?? 'Delivery failed.'}
        </span>
      ) : null}
      <span className='ghostex-chat-queue-row-actions'>
        {failed && onRetry ? (
          <QueueRowButton
            disabled={disabled}
            icon={<IconRefresh aria-hidden='true' size={14} stroke={2} />}
            label='Retry'
            onClick={() => onRetry(prompt)}
          />
        ) : null}
        {onEdit ? (
          <QueueRowButton
            disabled={locked}
            icon={<IconPencil aria-hidden='true' size={14} stroke={2} />}
            label='Edit'
            onClick={() => onEdit(prompt)}
          />
        ) : null}
        {onSendNow ? (
          <QueueRowButton
            disabled={locked}
            icon={<IconArrowUp aria-hidden='true' size={14} stroke={2.2} />}
            label='Send now'
            onClick={() => onSendNow(prompt)}
          />
        ) : null}
        {onDelete ? (
          <QueueRowButton
            disabled={locked}
            icon={<IconTrash aria-hidden='true' size={14} stroke={2} />}
            label='Delete'
            onClick={() => onDelete(prompt)}
          />
        ) : null}
      </span>
    </div>
  );
}

function QueueRowButton({
  disabled,
  icon,
  label,
  onClick,
}: {
  disabled: boolean;
  icon: ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <AppTooltip content={label}>
      <span className='inline-flex'>
        <Button
          aria-label={label}
          className='ghostex-chat-queue-row-button'
          disabled={disabled}
          onClick={onClick}
          size='icon-sm'
          type='button'
          variant='ghost'
        >
          {icon}
        </Button>
      </span>
    </AppTooltip>
  );
}
