/**
 * CDXC:Automations 2026-08-23:
 * Live canvas for the Codex-style Automate surface redesign. Renders the REAL
 * automation components from apps/desktop/views/project-board/automations.tsx
 * with mock data, plus the proposed page chrome (title, quiet section tabs,
 * right-aligned actions) that gets ported into project-board-app.tsx once the
 * look is approved. Background is #0e0e0e per design direction.
 */
import { useState } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { IconPlus, IconRefresh } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { RedesignCanvas } from './redesign-canvas';
import {
  AutomationDefinitionDetail,
  AutomationDefinitionList,
  AutomationRunDetail,
  AutomationRunList,
  selectAutomationRunsForTriage,
} from '@/apps/desktop/views/project-board/automations';
import type { AutomationDefinition, AutomationRun, ProjectAutomationAgentOption } from '@/packages/shared/automations';

const AGENTS: ProjectAutomationAgentOption[] = [
  { agentId: 'claude', label: 'Claude' },
  { agentId: 'codex', label: 'Codex' },
];

const AUTOMATIONS: AutomationDefinition[] = [
  {
    id: 'auto-1',
    agentId: 'claude',
    createdAt: '2026-08-01T09:00:00.000Z',
    updatedAt: '2026-08-20T09:00:00.000Z',
    enabled: true,
    executionMode: { kind: 'local' },
    name: 'Daily brief',
    nextRunAt: '2026-08-25T08:00:00.000Z',
    prompt:
      "Give me a morning brief with what's on my calendar, important unread emails, and anything that needs my attention today.",
    projectIds: ['proj-ghostex'],
    schedule: { kind: 'weekly', days: [1, 2, 3, 4, 5], time: '08:00', timezone: 'Asia/Beirut' },
  },
  {
    id: 'auto-2',
    agentId: 'codex',
    createdAt: '2026-08-05T12:00:00.000Z',
    updatedAt: '2026-08-21T12:00:00.000Z',
    enabled: true,
    executionMode: { kind: 'worktree', setupCommand: 'bun install' },
    name: 'Flaky test sweep',
    nextRunAt: '2026-08-24T02:00:00.000Z',
    prompt:
      'Run the shared test suite three times, list any test that does not fail deterministically, and propose the smallest stabilizing fix for each.',
    projectIds: ['proj-ghostex'],
    schedule: { kind: 'daily', time: '02:00', timezone: 'Asia/Beirut' },
  },
  {
    id: 'auto-3',
    agentId: 'claude',
    createdAt: '2026-07-28T10:00:00.000Z',
    updatedAt: '2026-08-10T10:00:00.000Z',
    enabled: false,
    executionMode: { kind: 'local' },
    name: 'Hello check',
    prompt:
      "This is an automation delivery test. Reply with one line saying hello and today's date. Do not modify any files.",
    projectIds: ['proj-ghostex'],
    schedule: { kind: 'interval', everyMs: 5 * 60 * 1000 },
  },
];

const RUNS: AutomationRun[] = [
  {
    id: 'run-1',
    automationId: 'auto-2',
    createdAt: '2026-08-23T02:00:00.000Z',
    completedAt: '2026-08-23T02:14:00.000Z',
    findingsSummary:
      '2 flaky tests found: session-grid reorder race and chat scroll restore. Proposed fixes are staged in the worktree.',
    isArchived: false,
    isUnread: true,
    projectId: 'proj-ghostex',
    sessionId: 'sess-8f2c1a',
    status: 'findings',
    worktree: {
      branch: 'automation/flaky-sweep-0823',
      path: '/Users/madda/dev/_active/Ghostex-worktrees/flaky-sweep-0823',
      sourcePath: '/Users/madda/dev/_active/Ghostex',
    },
  },
  {
    id: 'run-2',
    automationId: 'auto-1',
    createdAt: '2026-08-22T08:00:00.000Z',
    completedAt: '2026-08-22T08:03:00.000Z',
    findingsSummary: 'Quiet day: two calendar events, no unread email needs a reply.',
    isArchived: false,
    isUnread: false,
    projectId: 'proj-ghostex',
    sessionId: 'sess-1b9d44',
    status: 'no_findings',
  },
  {
    id: 'run-3',
    automationId: 'auto-2',
    createdAt: '2026-08-22T02:00:00.000Z',
    completedAt: '2026-08-22T02:09:00.000Z',
    errorMessage: 'Worktree setup command failed: bun install exited with code 1.',
    isArchived: false,
    isUnread: true,
    projectId: 'proj-ghostex',
    status: 'failed',
  },
  {
    id: 'run-4',
    automationId: 'auto-1',
    createdAt: '2026-08-23T08:00:00.000Z',
    isArchived: false,
    isUnread: false,
    projectId: 'proj-ghostex',
    sessionId: 'sess-77ac02',
    status: 'running',
  },
];

type SurfaceTab = 'automations' | 'runs' | 'triage';

function AutomatePage({ initialTab = 'automations' }: { initialTab?: SurfaceTab }) {
  const [activeTab, setActiveTab] = useState<SurfaceTab>(initialTab);
  const [selectedAutomationId, setSelectedAutomationId] = useState('auto-1');
  const [selectedRunId, setSelectedRunId] = useState('run-1');
  const [automations, setAutomations] = useState(AUTOMATIONS);
  const [runs, setRuns] = useState(RUNS);
  const noop = () => {};
  const setEnabled = (automation: AutomationDefinition, enabled: boolean) => {
    setAutomations((current) =>
      current.map((candidate) => (candidate.id === automation.id ? { ...candidate, enabled } : candidate))
    );
  };
  const markRead = (run: AutomationRun) => {
    setRuns((current) =>
      current.map((candidate) => (candidate.id === run.id ? { ...candidate, isUnread: false } : candidate))
    );
  };
  const visibleRuns = activeTab === 'triage' ? selectAutomationRunsForTriage(runs) : runs;
  return (
    <RedesignCanvas>
      <header className='grid shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-4 px-5 pb-3 pt-4'>
        <div className='min-w-0 justify-self-start'>
          <div className='text-xs font-normal text-muted-foreground'>Automations</div>
          <h1 className='truncate text-[15px] font-normal text-foreground'>Ghostex</h1>
        </div>
        <nav className='flex items-center gap-1 justify-self-center' aria-label='Automation sections'>
          {(['automations', 'runs', 'triage'] as const).map((tab) => (
            <button
              aria-current={activeTab === tab ? 'page' : undefined}
              className={`h-8 cursor-pointer rounded-lg border-0 bg-transparent px-3 text-sm font-normal transition-colors ${
                activeTab === tab
                  ? '!bg-white/[0.06] text-foreground'
                  : 'text-muted-foreground hover:text-foreground/80'
              }`}
              key={tab}
              onClick={() => setActiveTab(tab)}
              type='button'
            >
              {tab === 'automations' ? 'Automations' : tab === 'runs' ? 'Runs' : 'Triage'}
            </button>
          ))}
        </nav>
        <div className='flex items-center gap-1.5 justify-self-end'>
          <Button aria-label='Refresh' size='icon' variant='ghost'>
            <IconRefresh />
          </Button>
          <Button variant='secondary'>
            <IconPlus data-icon='inline-start' />
            Automation
          </Button>
        </div>
      </header>
      <div className='grid min-h-0 min-w-0 flex-1 grid-cols-[minmax(280px,0.9fr)_minmax(320px,1.1fr)] border-t border-border/60 [&>*]:min-w-0'>
        {activeTab === 'automations' ? (
          <>
            <div className='flex min-h-0 flex-col border-r border-border/60'>
              <AutomationDefinitionList
                actionId=''
                agents={AGENTS}
                automations={automations}
                onCreate={noop}
                onDelete={noop}
                onEdit={noop}
                onRunNow={noop}
                onSelect={setSelectedAutomationId}
                onSetEnabled={setEnabled}
                runs={runs}
                selectedAutomationId={selectedAutomationId}
              />
            </div>
            <AutomationDefinitionDetail
              actionId=''
              agents={AGENTS}
              automation={automations.find((candidate) => candidate.id === selectedAutomationId)}
              onDelete={noop}
              onEdit={noop}
              onRunNow={noop}
              onSetEnabled={setEnabled}
              runs={runs}
            />
          </>
        ) : (
          <>
            <div className='flex min-h-0 flex-col border-r border-border/60'>
              <AutomationRunList
                actionId=''
                agents={AGENTS}
                automations={automations}
                emptyTitle={activeTab === 'triage' ? 'Triage is clear' : 'No runs yet'}
                onArchive={noop}
                onMarkRead={markRead}
                onOpenSession={noop}
                onOpenWorktree={noop}
                onSelect={setSelectedRunId}
                projectName='Ghostex'
                runs={visibleRuns}
                selectedRunId={selectedRunId}
              />
            </div>
            <AutomationRunDetail
              actionId=''
              agents={AGENTS}
              automation={automations.find(
                (candidate) => candidate.id === visibleRuns.find((run) => run.id === selectedRunId)?.automationId
              )}
              onArchive={noop}
              onMarkRead={markRead}
              onOpenSession={noop}
              onOpenWorktree={noop}
              projectName='Ghostex'
              run={visibleRuns.find((run) => run.id === selectedRunId)}
            />
          </>
        )}
      </div>
    </RedesignCanvas>
  );
}

const meta: Meta<typeof AutomatePage> = {
  component: AutomatePage,
  title: 'Project Board Redesign/Automate',
};

export default meta;
type Story = StoryObj<typeof AutomatePage>;

export const Automations: Story = {};

export const Runs: Story = { args: { initialTab: 'runs' } };

export const Triage: Story = { args: { initialTab: 'triage' } };

export const Empty: Story = {
  render: () => {
    /* Same chrome, no data — exercises the empty states. */
    const [activeTab, setActiveTab] = useState<SurfaceTab>('automations');
    return (
      <RedesignCanvas>
        <header className='grid shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-4 px-5 pb-3 pt-4'>
          <div className='min-w-0 justify-self-start'>
            <div className='text-xs font-normal text-muted-foreground'>Automations</div>
            <h1 className='truncate text-[15px] font-normal text-foreground'>Ghostex</h1>
          </div>
          <nav className='flex items-center gap-1 justify-self-center'>
            {(['automations', 'runs', 'triage'] as const).map((tab) => (
              <button
                className={`h-8 cursor-pointer rounded-lg border-0 bg-transparent px-3 text-sm font-normal transition-colors ${
                  activeTab === tab
                    ? '!bg-white/[0.06] text-foreground'
                    : 'text-muted-foreground hover:text-foreground/80'
                }`}
                key={tab}
                onClick={() => setActiveTab(tab)}
                type='button'
              >
                {tab === 'automations' ? 'Automations' : tab === 'runs' ? 'Runs' : 'Triage'}
              </button>
            ))}
          </nav>
          <div className='flex items-center gap-1.5 justify-self-end'>
            <Button aria-label='Refresh' size='icon' variant='ghost'>
              <IconRefresh />
            </Button>
            <Button variant='secondary'>
              <IconPlus data-icon='inline-start' />
              Automation
            </Button>
          </div>
        </header>
        <div className='grid min-h-0 min-w-0 flex-1 grid-cols-[minmax(280px,0.9fr)_minmax(320px,1.1fr)] border-t border-border/60 [&>*]:min-w-0'>
          <div className='flex min-h-0 flex-col border-r border-border/60'>
            {activeTab === 'automations' ? (
              <AutomationDefinitionList
                actionId=''
                agents={AGENTS}
                automations={[]}
                onCreate={() => {}}
                onDelete={() => {}}
                onEdit={() => {}}
                onRunNow={() => {}}
                onSelect={() => {}}
                onSetEnabled={() => {}}
                runs={[]}
                selectedAutomationId=''
              />
            ) : (
              <AutomationRunList
                actionId=''
                agents={AGENTS}
                automations={[]}
                emptyTitle={activeTab === 'triage' ? 'Triage is clear' : 'No runs yet'}
                onArchive={() => {}}
                onMarkRead={() => {}}
                onOpenSession={() => {}}
                onOpenWorktree={() => {}}
                onSelect={() => {}}
                projectName='Ghostex'
                runs={[]}
                selectedRunId=''
              />
            )}
          </div>
          {activeTab === 'automations' ? (
            <AutomationDefinitionDetail
              actionId=''
              agents={AGENTS}
              automation={undefined}
              onDelete={() => {}}
              onEdit={() => {}}
              onRunNow={() => {}}
              onSetEnabled={() => {}}
              runs={[]}
            />
          ) : (
            <AutomationRunDetail
              actionId=''
              agents={AGENTS}
              automation={undefined}
              onArchive={() => {}}
              onMarkRead={() => {}}
              onOpenSession={() => {}}
              onOpenWorktree={() => {}}
              projectName='Ghostex'
              run={undefined}
            />
          )}
        </div>
      </RedesignCanvas>
    );
  },
};
