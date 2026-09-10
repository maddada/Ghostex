// The configuration dialog behind the pen icon in the context meter popover
// (see session-chat-context-details.ts for the decision it implements). It is
// a plain in-page shadcn Dialog like the chat's Rewind and Save-to-Markdown
// dialogs: the chat runs inside CEF on desktop and in a browser tab on web, so
// no native child window is involved. Edits are a draft until Save.

import { IconFileImport, IconFileExport, IconGripVertical, IconStar, IconStarFilled, IconX } from '@tabler/icons-react';
import { PointerActivationConstraints, PointerSensor } from '@dnd-kit/dom';
import { DragDropProvider, type DragDropEventHandlers } from '@dnd-kit/react';
import { isSortableOperation, useSortable } from '@dnd-kit/react/sortable';
import { useEffect, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/packages/components/ui/dialog';
import { Switch } from '@/packages/components/ui/switch';
import { cn } from '@/packages/components/utils';
import type { SessionChatTheme } from '@/packages/shared/session-chat';
import { AppTooltip } from '../app-tooltip';
import { AccountText, useAccountText } from '../accounts/account-text';
import { postAppModalHostMessage } from '../app-modal-host-bridge';
import { createAppToastRequest } from '@/packages/shared/app-toast-contract';
import type { ContextDetailStatus, ContextDetailsAgent } from './session-chat-context-details-agents';
import {
  copySessionChatContextDetailsPreferences,
  mapSessionChatContextDetailsPreferences,
  DEFAULT_SESSION_CHAT_CONTEXT_DETAILS_PREFERENCES,
  SESSION_CHAT_CONTEXT_DETAIL_GROUPS,
  isSessionChatContextDetailShown,
  isSessionChatContextDetailStarred,
  orderedSessionChatContextDetailRows,
  orderedSessionChatStarredRows,
  readSessionChatContextDetailsPreferences,
  useSessionChatContextDetailsClock,
  writeSessionChatContextDetailsPreferences,
  type SessionChatContextDetailGroupId,
  type SessionChatContextDetailRowDefinition,
  type SessionChatContextDetailSession,
  type SessionChatContextDetailsPreferences,
} from './session-chat-context-details';

const rowSensors = [
  PointerSensor.configure({
    activationConstraints: () => [new PointerActivationConstraints.Distance({ value: 4 })],
  }),
];

function moveRow<T>(rows: readonly T[], from: number, to: number): T[] {
  const next = [...rows];
  const [moved] = next.splice(from, 1);
  if (moved !== undefined) {
    next.splice(to, 0, moved);
  }
  return next;
}

export function SessionChatContextDetailsDialog({
  agent = 'claude',
  onOpenChange,
  open,
  session,
  status,
  theme,
}: {
  agent?: ContextDetailsAgent;
  onOpenChange: (open: boolean) => void;
  open: boolean;
  /** Ghostex's own title, id and draft state for the session row. */
  session: SessionChatContextDetailSession | null;
  /** Live sample values next to each row; absent rows show a dash. */
  status: ContextDetailStatus | undefined;
  theme: SessionChatTheme;
}) {
  const [draft, setDraft] = useState<SessionChatContextDetailsPreferences>(() =>
    readSessionChatContextDetailsPreferences(agent)
  );
  const now = useSessionChatContextDetailsClock();

  useEffect(() => {
    if (open) {
      setDraft(readSessionChatContextDetailsPreferences(agent));
    }
  }, [open, agent]);

  const toggleShown = (row: SessionChatContextDetailRowDefinition, shown: boolean) => {
    setDraft((current) => ({ ...current, shown: { ...current.shown, [row.id]: shown } }));
  };
  const toggleStarred = (row: SessionChatContextDetailRowDefinition) => {
    setDraft((current) => {
      const starred = !isSessionChatContextDetailStarred(current, row);
      const withoutRow = orderedSessionChatStarredRows(current, agent)
        .map((starredRow) => starredRow.id)
        .filter((id) => id !== row.id);
      return {
        ...current,
        starred: { ...current.starred, [row.id]: starred },
        // A newly starred row joins the end of the status line.
        starredOrder: starred ? [...withoutRow, row.id] : withoutRow,
      };
    });
  };
  const reorderStarred = (from: number, to: number) => {
    setDraft((current) => ({
      ...current,
      starredOrder: moveRow(orderedSessionChatStarredRows(current, agent), from, to).map((row) => row.id),
    }));
  };
  const starredRows = orderedSessionChatStarredRows(draft, agent);
  const handleStarredDragEnd = ((event) => {
    if (event.canceled || !isSortableOperation(event.operation)) {
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
    reorderStarred(source.initialIndex, toIndex);
  }) satisfies DragDropEventHandlers['onDragEnd'];
  const reorder = (group: SessionChatContextDetailGroupId, from: number, to: number) => {
    setDraft((current) => ({
      ...current,
      order: {
        ...current.order,
        [group]: moveRow(orderedSessionChatContextDetailRows(current, group, agent), from, to).map((row) => row.id),
      },
    }));
  };

  const otherAgent = agent === 'claude' ? 'codex' : 'claude';
  const otherAgentName = otherAgent === 'claude' ? 'Claude Code' : 'Codex';
  const transferSettings = (direction: 'to' | 'from') => {
    let message;
    try {
      let result: { matched: number; skipped: number };
      if (direction === 'to') {
        result = copySessionChatContextDetailsPreferences(draft, agent);
      } else {
        const imported = mapSessionChatContextDetailsPreferences(
          readSessionChatContextDetailsPreferences(otherAgent),
          otherAgent,
          draft
        );
        setDraft(imported.preferences);
        result = imported;
      }
      message = createAppToastRequest(
        'success',
        `Settings copied ${direction} ${otherAgentName}`,
        `${result.matched} selections mapped. ${result.skipped} fields have no counterpart.${direction === 'from' ? ' Save to apply these changes.' : ''}`
      );
    } catch {
      message = createAppToastRequest('error', 'Could not copy settings', 'The browser could not save the settings.');
    }
    postAppModalHostMessage(message, 'SessionChatContextDetails:copy');
  };

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent
        className={cn(
          'ghostex-session-chat-popup ghostex-chat-context-details-dialog w-full rounded-xl sm:max-w-xl font-sans [--radius:0.625rem]',
          theme === 'dark' && 'dark'
        )}
      >
        <DialogHeader>
          <div className='flex w-full items-center justify-between gap-2'>
            <DialogTitle>Context details</DialogTitle>
            <div className='ml-auto flex shrink-0 items-center gap-1'>
              <AppTooltip content={`Copy settings from ${otherAgentName}`}>
                <Button
                  aria-label={`Copy settings from ${otherAgentName}`}
                  size='icon-xs'
                  variant='ghost'
                  type='button'
                  onClick={() => transferSettings('from')}
                >
                  <IconFileImport className='size-3.5' />
                </Button>
              </AppTooltip>
              <AppTooltip content={`Copy settings to ${otherAgentName}`}>
                <Button
                  aria-label={`Copy settings to ${otherAgentName}`}
                  size='icon-xs'
                  variant='ghost'
                  type='button'
                  onClick={() => transferSettings('to')}
                >
                  <IconFileExport className='size-3.5' />
                </Button>
              </AppTooltip>
            </div>
          </div>
          <DialogDescription>
            Pick the rows shown under the context meter in {agent === 'claude' ? 'Claude Code' : 'Codex'} sessions. Drag
            to reorder within a group. Star a row to show its value under the chat box.
          </DialogDescription>
        </DialogHeader>
        <div className='ghostex-chat-context-details-dialog-body -mx-1 flex max-h-[60vh] flex-col gap-1 overflow-x-hidden overflow-y-auto px-1'>
          {SESSION_CHAT_CONTEXT_DETAIL_GROUPS.map((group) => {
            const rows = orderedSessionChatContextDetailRows(draft, group.id, agent);
            const handleDragEnd = ((event) => {
              if (event.canceled || !isSortableOperation(event.operation)) {
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
              reorder(group.id, source.initialIndex, toIndex);
            }) satisfies DragDropEventHandlers['onDragEnd'];
            return (
              <section aria-label={group.label} className='flex flex-col' key={group.id}>
                <h3 className='ghostex-chat-context-details-group-label mt-2 mb-0.5 px-1 text-[10px] font-medium tracking-[0.06em] text-muted-foreground/70 uppercase'>
                  {group.label}
                </h3>
                {/*
                One provider per group: a row's sortable only knows its own
                group's manager, so a drag can never land in another group.
                */}
                <DragDropProvider onDragEnd={handleDragEnd}>
                  {rows.map((row, index) => (
                    <ContextDetailOptionRow
                      group={group.id}
                      index={index}
                      key={row.id}
                      onToggleShown={(shown) => toggleShown(row, shown)}
                      onToggleStarred={() => toggleStarred(row)}
                      row={row}
                      sample={status ? row.value({ status, now, session }) : null}
                      shown={isSessionChatContextDetailShown(draft, row)}
                      starred={isSessionChatContextDetailStarred(draft, row)}
                    />
                  ))}
                </DragDropProvider>
              </section>
            );
          })}
        </div>
        {/*
        The status line's own section, pinned under the scrolling groups: the
        starred rows as chips in the order they render under the chat box, drag
        to rearrange freely (this order is separate from the groups above).
        */}
        <section
          aria-label='Status line'
          className='ghostex-chat-context-details-status-line -mx-1 border-t border-border/60 px-1 pt-3'
        >
          <div className='mb-1.5 flex items-baseline justify-between gap-2'>
            <h3 className='text-[10px] font-medium tracking-[0.06em] text-muted-foreground/70 uppercase'>
              Status line
            </h3>
            <span className='text-[11px] text-muted-foreground'>
              {starredRows.length === 0 ? 'Star rows above to show them under the chat box.' : 'Drag to arrange.'}
            </span>
          </div>
          {starredRows.length > 0 ? (
            <DragDropProvider onDragEnd={handleStarredDragEnd}>
              <div className='flex flex-wrap gap-1.5'>
                {starredRows.map((row, index) => (
                  <StarredRowChip index={index} key={row.id} onRemove={() => toggleStarred(row)} row={row} />
                ))}
              </div>
            </DragDropProvider>
          ) : null}
        </section>
        <DialogFooter className='sm:justify-between'>
          <Button
            className='text-muted-foreground'
            onClick={() => setDraft(DEFAULT_SESSION_CHAT_CONTEXT_DETAILS_PREFERENCES)}
            size='sm'
            type='button'
            variant='ghost'
          >
            Reset to recommended
          </Button>
          <div className='flex gap-2'>
            <Button onClick={() => onOpenChange(false)} size='sm' type='button' variant='outline'>
              Cancel
            </Button>
            <Button
              onClick={() => {
                writeSessionChatContextDetailsPreferences(draft, agent);
                onOpenChange(false);
              }}
              size='sm'
              type='button'
            >
              Save
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ContextDetailOptionRow({
  group,
  index,
  onToggleShown,
  onToggleStarred,
  row,
  sample,
  shown,
  starred,
}: {
  group: SessionChatContextDetailGroupId;
  index: number;
  onToggleShown: (shown: boolean) => void;
  onToggleStarred: () => void;
  row: SessionChatContextDetailRowDefinition;
  sample: string | null;
  shown: boolean;
  starred: boolean;
}) {
  const formatAccountText = useAccountText();
  const kind = `session-chat-context-detail:${group}`;
  const sortable = useSortable({
    accept: kind,
    group,
    id: row.id,
    index,
    sensors: rowSensors,
    type: kind,
  });
  const { handleRef, isDragging } = sortable;
  const setRowRef = (element: HTMLDivElement | null): void => {
    sortable.ref(element);
    sortable.sourceRef(element);
  };

  return (
    <div
      className={cn(
        'ghostex-chat-context-details-option flex items-center gap-2 rounded-md px-1 py-1.5 hover:bg-accent/40',
        isDragging && 'bg-accent/60'
      )}
      data-dragging={isDragging ? 'true' : undefined}
      ref={setRowRef}
    >
      <button
        aria-label={`Reorder ${row.label}`}
        className='ghostex-chat-queue-row-handle'
        ref={handleRef}
        type='button'
      >
        <IconGripVertical aria-hidden='true' size={14} stroke={1.8} />
      </button>
      <div className='min-w-0 flex-1'>
        <div className='text-xs text-foreground'>{row.label}</div>
        <div className='truncate text-[11px] text-muted-foreground'>{row.description}</div>
      </div>
      <div
        className='max-w-[13rem] shrink-0 truncate text-[11px] text-muted-foreground tabular-nums'
        title={sample === null ? undefined : formatAccountText(sample)}
      >
        {sample === null ? '\u2014' : <AccountText text={sample} />}
      </div>
      <AppTooltip content={starred ? 'Remove from the status line' : 'Show under the chat box'} side='top'>
        <Button
          aria-label={starred ? `Unstar ${row.label}` : `Star ${row.label}`}
          aria-pressed={starred}
          className={cn('rounded-md', starred ? 'text-amber-300 hover:text-amber-200' : 'text-muted-foreground')}
          onClick={onToggleStarred}
          size='icon-xs'
          type='button'
          variant='ghost'
        >
          {starred ? <IconStarFilled size={13} /> : <IconStar size={13} stroke={1.8} />}
        </Button>
      </AppTooltip>
      <Switch aria-label={`Show ${row.label}`} checked={shown} onCheckedChange={onToggleShown} size='sm' />
    </div>
  );
}

function StarredRowChip({
  index,
  onRemove,
  row,
}: {
  index: number;
  onRemove: () => void;
  row: SessionChatContextDetailRowDefinition;
}) {
  const sortable = useSortable({
    accept: 'session-chat-context-detail:starred',
    id: `starred:${row.id}`,
    index,
    sensors: rowSensors,
    type: 'session-chat-context-detail:starred',
  });
  const { handleRef, isDragging } = sortable;
  const setChipRef = (element: HTMLDivElement | null): void => {
    sortable.ref(element);
    sortable.sourceRef(element);
  };

  return (
    <div
      className={cn(
        'ghostex-chat-context-details-chip flex h-6 items-center gap-1 rounded-md border border-border/70 bg-accent/30 pr-1 pl-0.5 text-[11px] text-foreground',
        isDragging && 'bg-accent/70'
      )}
      data-dragging={isDragging ? 'true' : undefined}
      ref={setChipRef}
    >
      <button
        aria-label={`Move ${row.label}`}
        className='ghostex-chat-queue-row-handle !h-5 !w-4'
        ref={handleRef}
        type='button'
      >
        <IconGripVertical aria-hidden='true' size={12} stroke={1.8} />
      </button>
      <span>{row.label}</span>
      <Button
        aria-label={`Unstar ${row.label}`}
        className='size-4 rounded-sm text-muted-foreground'
        onClick={onRemove}
        size='icon-xs'
        type='button'
        variant='ghost'
      >
        <IconX size={11} stroke={2} />
      </Button>
    </div>
  );
}
