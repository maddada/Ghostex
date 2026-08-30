/**
 * CDXC:ProjectBoardRedesign 2026-08-23:
 * Live canvas for the Codex-style Kanban redesign. Renders the REAL BoardLane
 * and TicketCard components from apps/desktop/views/project-board with mock
 * tickets, plus the proposed page chrome (title row + h-8 filter row) that
 * gets ported into project-board-app.tsx once approved.
 */
import { useState } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { DragDropProvider } from '@dnd-kit/react';
import {
  IconAdjustmentsHorizontal,
  IconFilter,
  IconLayoutColumns,
  IconPlus,
  IconRefresh,
  IconSearch,
} from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '@/packages/components/ui/dropdown-menu';
import { Input } from '@/packages/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/packages/components/ui/popover';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/packages/components/ui/select';
import { BoardLane } from '@/apps/desktop/views/project-board/board-lane-card';
import { PROJECT_BOARD_STYLES } from '@/apps/desktop/views/project-board/styles';
import {
  BOARD_CARD_VIEW_FIELDS,
  loadBoardCardViewOptions,
  saveBoardCardViewOptions,
  type BoardCardViewOptions,
} from '@/apps/desktop/views/project-board/card-view-options';
import type { BoardColumn, BoardStatusKey, BoardTicket } from '@/apps/desktop/views/project-board-shared';
import type { ProjectBoardConversationLinkView } from '@/packages/shared/bead-conversation-links';
import { RedesignCanvas } from './redesign-canvas';

const COLUMNS: BoardColumn[] = [
  { key: 'backlog', label: 'Backlog', beadsStatus: 'backlog', tone: 'muted' },
  { key: 'todo', label: 'Todo', beadsStatus: 'open', tone: 'neutral' },
  { key: 'in_progress', label: 'In Progress', beadsStatus: 'in_progress', tone: 'blue' },
  { key: 'test', label: 'Test', beadsStatus: 'test', tone: 'amber' },
  { key: 'review', label: 'Review', beadsStatus: 'review', tone: 'violet' },
  { key: 'done', label: 'Done', beadsStatus: 'closed', tone: 'green' },
];

function ticket(input: Partial<BoardTicket> & Pick<BoardTicket, 'id' | 'title' | 'boardStatus'>): BoardTicket {
  return {
    displayId: input.id.toUpperCase(),
    status: input.boardStatus,
    ...input,
  };
}

const TICKETS: BoardTicket[] = [
  ticket({
    id: 'gx-101',
    boardStatus: 'backlog',
    title: 'Investigate slow startup on remote machines',
    description:
      'Cold connect to a remote gxserver takes 6-8s before the sidebar hydrates. Profile the handshake and cache the last presentation snapshot.',
    labels: ['perf', 'remote'],
    priority: 1,
    comment_count: 3,
    created_by: 'madda',
  }),
  ticket({
    id: 'gx-102',
    boardStatus: 'backlog',
    title: 'Session chat: keyboard shortcut cheatsheet',
    description: 'Add a small overlay listing chat hotkeys.',
    labels: ['chat'],
    priority: 3,
    comment_count: 0,
  }),
  ticket({
    id: 'gx-110',
    boardStatus: 'todo',
    title: 'Automate page: create dialog on stock shadcn controls',
    description:
      'Rebuild the create/edit automation dialog with default Select/Input sizes so dropdown and button heights finally match.',
    labels: ['design', 'automations'],
    priority: 1,
    comment_count: 5,
    assignee: 'claude',
    estimate: 60,
  }),
  ticket({
    id: 'gx-111',
    boardStatus: 'todo',
    title: 'Board columns dialog polish',
    priority: 2,
    comment_count: 1,
  }),
  ticket({
    id: 'gx-120',
    boardStatus: 'in_progress',
    title: 'Codex-style redesign for Kanban and Automate',
    description:
      'Flat rounded lanes, quiet regular-weight cards, one text scale, #0e0e0e background. Iterating in Storybook.',
    labels: ['design'],
    priority: 0,
    comment_count: 8,
    estimate: 240,
    assignee: 'claude',
    created_by: 'madda',
    dependent_count: 2,
  }),
  ticket({
    id: 'gx-121',
    boardStatus: 'in_progress',
    title: 'Wire triage unread counts into the sidebar badge',
    description: 'Sidebar should show pending triage results per project.',
    priority: 2,
    comment_count: 2,
    dependency_count: 1,
  }),
  ticket({
    id: 'gx-130',
    boardStatus: 'test',
    title: 'Remote worktree cleanup prompt',
    description: 'Prompt before deleting stale automation worktrees on remotes.',
    labels: ['remote', 'automations'],
    priority: 2,
    comment_count: 4,
  }),
  ticket({
    id: 'gx-140',
    boardStatus: 'review',
    title: 'Drop leaked TUI kill keys from user turns',
    description: 'Chat transcripts occasionally include stray kill-key sequences from the terminal bridge.',
    priority: 1,
    comment_count: 6,
    assignee: 'codex',
  }),
  ticket({
    id: 'gx-150',
    boardStatus: 'done',
    title: 'Refresh the macOS app icon',
    description: 'New icon shipped with the SVG source kept in-repo.',
    labels: ['desktop'],
    priority: 2,
    comment_count: 2,
  }),
  ticket({
    id: 'gx-151',
    boardStatus: 'done',
    title: 'Open Search by Prompt as an app modal',
    priority: 3,
    comment_count: 0,
  }),
];

const PRIORITY_ITEMS = [
  { label: 'All priorities', value: 'all' },
  { label: 'Urgent', value: '0' },
  { label: 'High', value: '1' },
  { label: 'Medium', value: '2' },
  { label: 'Low', value: '3' },
];

const SORT_ITEMS = [
  { label: 'Manual order', value: 'manual' },
  { label: 'Newest first', value: 'newest' },
  { label: 'Priority', value: 'priority' },
];

function KanbanPage() {
  const [tickets, setTickets] = useState(TICKETS);
  const [search, setSearch] = useState('');
  const [priority, setPriority] = useState('all');
  const [sort, setSort] = useState('manual');
  const [cardView, setCardView] = useState<BoardCardViewOptions>(loadBoardCardViewOptions);
  const toggleCardViewField = (key: keyof BoardCardViewOptions, value: boolean) => {
    setCardView((current) => {
      const next = { ...current, [key]: value };
      saveBoardCardViewOptions(next);
      return next;
    });
  };
  const linksByBeadKey = new Map<string, ProjectBoardConversationLinkView[]>();
  const noop = () => {};
  const activeFilterCount = (priority !== 'all' ? 1 : 0) + (sort !== 'manual' ? 1 : 0);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const visibleTickets = tickets.filter(
    (candidate) =>
      (search.length === 0 || candidate.title.toLowerCase().includes(search.toLowerCase())) &&
      (priority === 'all' || String(candidate.priority ?? 2) === priority)
  );
  const ticketsByColumn = Object.fromEntries(
    COLUMNS.map((column) => [column.key, visibleTickets.filter((candidate) => candidate.boardStatus === column.key)])
  ) as Record<BoardStatusKey, BoardTicket[]>;
  return (
    <RedesignCanvas>
      {/*
       * CDXC:ProjectBoardRedesign 2026-08-24:
       * The real page injects PROJECT_BOARD_STYLES, which forces the lanes'
       * classic 8px scrollbar rail. Load it here too so the story shows the
       * same card insets and scrollbar behavior as the app.
       */}
      <style>{PROJECT_BOARD_STYLES}</style>
      <header className='flex shrink-0 items-center justify-between gap-4 px-5 pb-3 pt-4'>
        <div className='min-w-0'>
          <div className='text-xs font-normal text-muted-foreground'>Project</div>
          <h1 className='truncate text-[15px] font-normal text-foreground'>Ghostex</h1>
        </div>
      </header>
      <section className='flex shrink-0 flex-wrap items-center gap-2 px-5 pb-3' aria-label='Ticket filters'>
        <div className='relative w-64'>
          <IconSearch
            aria-hidden='true'
            className='pointer-events-none absolute right-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground'
          />
          <Input
            aria-label='Search tickets'
            className='h-8 border-border pr-8'
            onChange={(event) => setSearch(event.currentTarget.value)}
            placeholder='Search tickets'
            value={search}
          />
        </div>
        {/*
         * CDXC:ProjectBoardFiltersPopover 2026-08-24:
         * Mirrors the real toolbar: one Filters button with an active-count
         * badge, selects inside its popover.
         */}
        <Popover onOpenChange={setFiltersOpen} open={filtersOpen}>
          <PopoverTrigger
            render={
              <Button aria-label='Filters' variant='outline'>
                <IconFilter data-icon='inline-start' />
                Filters
                {activeFilterCount > 0 ? (
                  <span className='inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-[color-mix(in_srgb,var(--ghostex-accent,#86d3f8)_22%,transparent)] px-1 text-[11px] leading-none text-[var(--ghostex-accent,#86d3f8)]'>
                    {activeFilterCount}
                  </span>
                ) : null}
              </Button>
            }
          />
          <PopoverContent align='start' className='w-60 gap-3 p-3'>
            <div className='flex items-center justify-between'>
              <span className='text-xs font-medium text-muted-foreground'>Filters</span>
              {activeFilterCount > 0 ? (
                <button
                  className='cursor-pointer rounded-md border-0 bg-transparent px-1.5 py-0.5 text-xs text-muted-foreground transition-colors hover:bg-white/[0.06] hover:text-foreground'
                  onClick={() => {
                    setPriority('all');
                    setSort('manual');
                  }}
                  type='button'
                >
                  Reset
                </button>
              ) : null}
            </div>
            <label className='flex flex-col gap-1.5 text-xs text-muted-foreground'>
              Priority
              <Select items={PRIORITY_ITEMS} onValueChange={setPriority} value={priority}>
                <SelectTrigger aria-label='Filter by priority' className='w-full'>
                  <SelectValue placeholder='All priorities' />
                </SelectTrigger>
                <SelectContent>
                  {PRIORITY_ITEMS.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </label>
            <label className='flex flex-col gap-1.5 text-xs text-muted-foreground'>
              Sort
              <Select items={SORT_ITEMS} onValueChange={setSort} value={sort}>
                <SelectTrigger aria-label='Sort tickets' className='w-full'>
                  <SelectValue placeholder='Manual order' />
                </SelectTrigger>
                <SelectContent>
                  {SORT_ITEMS.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </label>
          </PopoverContent>
        </Popover>
        <Button aria-label='Board columns' size='icon' title='Columns' variant='outline'>
          <IconLayoutColumns />
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button aria-label='Card details' size='icon' title='View' variant='outline'>
                <IconAdjustmentsHorizontal />
              </Button>
            }
          />
          <DropdownMenuContent align='start'>
            <DropdownMenuGroup>
              <DropdownMenuLabel>Card details</DropdownMenuLabel>
              {BOARD_CARD_VIEW_FIELDS.map((field) => (
                <DropdownMenuCheckboxItem
                  checked={cardView[field.key]}
                  closeOnClick={false}
                  key={field.key}
                  onCheckedChange={(checked: boolean) => toggleCardViewField(field.key, checked)}
                >
                  {field.label}
                </DropdownMenuCheckboxItem>
              ))}
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
        <div className='ml-auto flex items-center gap-1.5'>
          <Button aria-label='Refresh' size='icon' variant='ghost'>
            <IconRefresh />
          </Button>
          <Button variant='secondary'>
            <IconPlus data-icon='inline-start' />
            Ticket
          </Button>
        </div>
      </section>
      <div className='min-h-0 flex-1 px-5 pb-5'>
        <DragDropProvider
          onDragEnd={(event) => {
            const target = event.operation.target?.id;
            const source = event.operation.source?.data?.ticketId as string | undefined;
            if (!target || !source || event.canceled) {
              return;
            }
            setTickets((current) =>
              current.map((candidate) =>
                candidate.id === source ? { ...candidate, boardStatus: String(target) as BoardStatusKey } : candidate
              )
            );
          }}
        >
          <section
            className='horizontal-scroll-fade-mask grid h-full min-h-0 grid-flow-col auto-cols-[minmax(230px,1fr)] gap-2.5 overflow-x-auto'
            aria-label='Project issue board'
          >
            {COLUMNS.map((column) => (
              <BoardLane
                cardView={cardView}
                column={column}
                conversationAction={undefined}
                key={column.key}
                linksByBeadKey={linksByBeadKey}
                onAddTicket={noop}
                onJumpToConversation={noop}
                onOpenContextMenu={noop}
                onOpenTicket={noop}
                tickets={ticketsByColumn[column.key] ?? []}
              />
            ))}
          </section>
        </DragDropProvider>
      </div>
    </RedesignCanvas>
  );
}

const meta: Meta<typeof KanbanPage> = {
  component: KanbanPage,
  title: 'Project Board Redesign/Kanban',
};

export default meta;
type Story = StoryObj<typeof KanbanPage>;

export const Board: Story = {};
