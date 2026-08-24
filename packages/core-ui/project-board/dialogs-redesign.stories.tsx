/**
 * CDXC:ProjectBoardDialogRedesign 2026-08-24:
 * Live canvas for the Codex-style Kanban dialogs. Renders the REAL
 * NewTicketDialog, EditTicketDialog, AutomationDialog, and BoardColumnsDialog
 * from apps/desktop/views/project-board with mock props, and injects the same
 * PROJECT_BOARD_STYLES sheet the real page loads so the portalled popup gets
 * its surface tokens (the dialog renders into document.body, outside the
 * canvas element that carries the story theme).
 */
import { useState } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { AutomationDialog } from '@/apps/desktop/views/project-board/automation-dialog';
import { BoardColumnsDialog } from '@/apps/desktop/views/project-board/board-columns-dialog';
import { EditTicketDialog, NewTicketDialog } from '@/apps/desktop/views/project-board/ticket-dialogs';
import { PROJECT_BOARD_STYLES } from '@/apps/desktop/views/project-board/styles';
import { createAutomationDraft } from '@/apps/desktop/views/project-board/automations-drafts';
import type { DetailDraft, TicketFormDraft } from '@/apps/desktop/views/project-board/types';
import type { BoardColumn, BoardTicket } from '@/apps/desktop/views/project-board-shared';
import type {
  ProjectBoardConversationLinkView,
  ProjectBoardConversationState,
} from '@/packages/shared/bead-conversation-links';
import type { ProjectAutomationsBridgeState } from '@/packages/shared/automations';
import { RedesignCanvas } from './redesign-canvas';

const COLUMNS: BoardColumn[] = [
  { key: 'backlog', label: 'Backlog', beadsStatus: 'backlog', tone: 'muted' },
  { key: 'todo', label: 'Todo', beadsStatus: 'open', tone: 'neutral' },
  { key: 'in_progress', label: 'In Progress', beadsStatus: 'in_progress', tone: 'blue' },
  { key: 'test', label: 'Test', beadsStatus: 'test', tone: 'amber' },
  { key: 'review', label: 'Review', beadsStatus: 'review', tone: 'violet' },
  { key: 'done', label: 'Done', beadsStatus: 'closed', tone: 'green' },
];

const TICKET_OPTIONS = [
  { id: 'gx-101', label: 'GX-101 · Investigate slow startup on remote machines' },
  { id: 'gx-102', label: 'GX-102 · Session chat keyboard cheatsheet' },
  { id: 'gx-103', label: 'GX-103 · Board columns dialog polish' },
];

const KNOWN_LABELS = ['perf', 'remote', 'ui', 'chat', 'kanban'];

const EDIT_TICKET: BoardTicket = {
  id: 'gx-104',
  displayId: 'GX-104',
  boardStatus: 'in_progress',
  status: 'in_progress',
  title: 'Codex-style redesign for the Kanban dialogs',
  description:
    'Bring New Ticket, Edit ticket, the automation form, and the columns dialog onto the shipped board control language: 32px controls, one text scale, no bold chrome.',
  labels: ['ui', 'kanban'],
  priority: 1,
  assignee: 'madda',
  created_by: 'madda',
  comments: [
    {
      author: 'madda',
      created_at: '2026-08-23T18:04:00Z',
      text: 'Dropdowns render at three different font sizes and the buttons are shorter than the selects next to them.',
    },
    {
      author: 'codex',
      created_at: '2026-08-24T09:12:00Z',
      text: '[agent:Codex][session:01J8Z4Q7VN3K] Pinned every dialog control to the 32px board height and one 14px scale.',
    },
  ],
};

const CONVERSATION_LINK: ProjectBoardConversationLinkView = {
  agentId: 'codex',
  agentName: 'Codex',
  agentSessionId: '01J8Z4Q7VN3K5T2RJ8XW9M6D4A',
  beadId: 'gx-104',
  createdAt: '2026-08-23T18:00:00Z',
  ghostexSessionId: 'ghostex-104',
  id: 'link-104',
  isLive: true,
  projectId: 'ghostex',
  sessionTitle: 'Kanban dialog redesign',
  status: 'active',
  updatedAt: '2026-08-24T09:12:00Z',
};

const CONVERSATION_STATE: ProjectBoardConversationState = {
  agents: [
    { agentId: 'codex', label: 'Codex' },
    { agentId: 'claude', label: 'Claude Code' },
    { agentId: 'fable', label: 'Fable' },
  ],
  defaultAgentId: 'codex',
  focusedTerminalSessionId: 'ghostex-104',
  links: [CONVERSATION_LINK],
  sessions: [
    { label: 'Kanban dialog redesign', sessionId: 'ghostex-104' },
    { label: 'Board toolbar pass', sessionId: 'ghostex-105' },
  ],
};

const AGENT_SELECT_ITEMS = CONVERSATION_STATE.agents.map((agent) => ({
  label: agent.label,
  value: agent.agentId,
}));

const AUTOMATION_STATE: ProjectAutomationsBridgeState = {
  agents: [
    { agentId: 'codex', label: 'Codex' },
    { agentId: 'claude', label: 'Claude Code' },
  ],
  automations: [],
  projectCanUseWorktrees: true,
  projectId: 'ghostex',
  projectName: 'Ghostex',
  projectPath: '/Users/madda/dev/_active/Ghostex',
  projects: [
    {
      canUseWorktrees: true,
      label: 'Ghostex',
      path: '/Users/madda/dev/_active/Ghostex',
      projectId: 'ghostex',
    },
    { canUseWorktrees: false, label: 'Notes', path: '/Users/madda/notes', projectId: 'notes' },
  ],
  runs: [],
};

const EMPTY_DETAIL: DetailDraft = {
  blockedByIds: [],
  blockingIds: [],
  comment: '',
  description: '',
  isDeleting: false,
  isSaving: false,
  labels: [],
  priority: '2',
  status: 'todo',
  title: '',
};

const noop = () => undefined;

function DialogCanvas({ children }: { children: React.ReactNode }) {
  return (
    <RedesignCanvas>
      <style>{PROJECT_BOARD_STYLES}</style>
      <div className='flex h-full items-center justify-center text-xs text-muted-foreground'>
        Project board page behind the dialog
      </div>
      {children}
    </RedesignCanvas>
  );
}

function NewTicketStory() {
  const [newTicket, setNewTicket] = useState<TicketFormDraft>({
    blockedByIds: [],
    blockingIds: [],
    description: '',
    labels: [],
    priority: '2',
    status: 'todo',
    title: '',
  });
  const [selectedAgentId, setSelectedAgentId] = useState('codex');
  const [startLocation, setStartLocation] = useState<'currentProject' | 'newWorktree'>('currentProject');
  return (
    <DialogCanvas>
      <NewTicketDialog
        agentSelectItems={AGENT_SELECT_ITEMS}
        boardColumns={COLUMNS}
        conversationAction={undefined}
        conversationState={CONVERSATION_STATE}
        imagePreviewDataUrls={{}}
        knownLabels={KNOWN_LABELS}
        newTicket={newTicket}
        newTicketStartLocation={startLocation}
        onCreateTicket={noop}
        onOpenChange={noop}
        onSelectedAgentChange={setSelectedAgentId}
        open
        selectedAgentId={selectedAgentId}
        setErrorMessage={noop}
        setNewTicket={setNewTicket}
        setNewTicketStartLocation={setStartLocation}
        ticketOptions={TICKET_OPTIONS}
      />
    </DialogCanvas>
  );
}

function EditTicketStory() {
  const [detail, setDetail] = useState<DetailDraft>({
    ...EMPTY_DETAIL,
    blockedByIds: ['gx-101'],
    blockingIds: ['gx-103'],
    description: EDIT_TICKET.description ?? '',
    labels: ['ui', 'kanban'],
    priority: '1',
    status: 'in_progress',
    ticket: EDIT_TICKET,
    title: EDIT_TICKET.title,
    tshirt: 'M',
  });
  const [selectedAgentId, setSelectedAgentId] = useState('codex');
  const [deleteConfirmingTicketId, setDeleteConfirmingTicketId] = useState('');
  return (
    <DialogCanvas>
      <EditTicketDialog
        boardColumns={COLUMNS}
        conversationAction={undefined}
        conversationState={CONVERSATION_STATE}
        deleteConfirmingTicketId={deleteConfirmingTicketId}
        detail={detail}
        detailCommentMetadataLink={CONVERSATION_LINK}
        detailConversationLinks={[CONVERSATION_LINK]}
        detailPrimaryActionDisabled={false}
        detailPrimaryActionKind='jump'
        detailPrimaryActionLabel='Jump to conversation'
        detailPrimaryConversationLink={CONVERSATION_LINK}
        imagePreviewDataUrls={{}}
        knownLabels={KNOWN_LABELS}
        onAssociateFocusedSession={noop}
        onClose={noop}
        onDeleteTicket={noop}
        onJumpToConversation={noop}
        onSaveTicketDetail={noop}
        onSelectedAgentChange={setSelectedAgentId}
        onStartTicketWork={noop}
        onUnlinkConversation={noop}
        selectedAgentId={selectedAgentId}
        setDeleteConfirmingTicketId={setDeleteConfirmingTicketId}
        setDetail={setDetail}
        setErrorMessage={noop}
        ticketOptions={TICKET_OPTIONS}
        tickets={[EDIT_TICKET]}
      />
    </DialogCanvas>
  );
}

function AutomationStory() {
  const [draft, setDraft] = useState(() =>
    createAutomationDraft({
      agentId: 'codex',
      name: 'Nightly board triage',
      projectId: 'ghostex',
      prompt: 'Review every bead in Test, run the suite, and comment the result on each ticket.',
      schedulePreset: 'daily',
    })
  );
  return (
    <DialogCanvas>
      <AutomationDialog
        automationActionId=''
        automationAgentSelectItems={[
          { label: 'Codex', value: 'codex' },
          { label: 'Claude Code', value: 'claude' },
        ]}
        automationConversationState={CONVERSATION_STATE}
        automationDraft={draft}
        automationDraftCanUseWorktrees
        automationProjectSelectItems={[
          { label: 'Ghostex', value: 'ghostex' },
          { label: 'Notes', value: 'notes' },
        ]}
        automationScheduleSelectItems={[
          { label: 'Daily', value: 'daily' },
          { label: 'Weekly', value: 'weekly' },
          { label: 'Custom cron', value: 'cron' },
        ]}
        automationSessionSelectItems={[{ label: 'Kanban dialog redesign', value: 'ghostex-104' }]}
        automationState={AUTOMATION_STATE}
        automationTimerUnitSelectItems={[
          { label: 'Minutes', value: 'minutes' },
          { label: 'Hours', value: 'hours' },
          { label: 'Days', value: 'days' },
        ]}
        automationWeekdaySelectItems={[
          { label: 'Monday', value: '1' },
          { label: 'Tuesday', value: '2' },
        ]}
        isAutomationGlobalScope={false}
        onOpenChange={noop}
        onProjectChange={noop}
        onSave={noop}
        open
        projectName='Ghostex'
        setAutomationDraft={setDraft}
      />
    </DialogCanvas>
  );
}

function BoardColumnsStory() {
  return (
    <DialogCanvas>
      <BoardColumnsDialog
        columns={COLUMNS}
        config='design,blocked'
        onClose={noop}
        onCreate={async () => undefined}
        onDelete={async () => undefined}
        onRename={async () => undefined}
        onReorder={async () => undefined}
        open
        tickets={[EDIT_TICKET]}
      />
    </DialogCanvas>
  );
}

const meta: Meta = {
  title: 'Project Board Redesign/Dialogs',
};

export default meta;

export const NewTicket: StoryObj = { render: () => <NewTicketStory /> };
export const EditTicket: StoryObj = { render: () => <EditTicketStory /> };
export const Automation: StoryObj = { render: () => <AutomationStory /> };
export const BoardColumns: StoryObj = { render: () => <BoardColumnsStory /> };
