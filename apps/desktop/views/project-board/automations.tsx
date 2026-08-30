import type * as React from 'react';
import {
  IconArchive,
  IconBell,
  IconCalendarTime,
  IconCopy,
  IconExternalLink,
  IconFolderOpen,
  IconPlayerPlay,
  IconTrash,
} from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { Switch } from '@/packages/components/ui/switch';
import { formatShortDate } from '../project-board-shared';
import {
  compareAutomationRunsNewestFirst,
  type AutomationDefinition,
  type AutomationRun,
  type ProjectAutomationAgentOption,
} from '@/packages/shared/automations';
import { PROJECT_AUTOMATION_TRIAGE_RECENT_COMPLETED_LIMIT } from './constants';
import {
  describeAutomationSchedule,
  describeAutomationMode,
  automationRunStatusLabel,
  isAutomationRunActive,
} from './automations-drafts';
import { automationAgentLabel, resolveAutomationAgentIcon, AutomationAgentIcon } from './agent-labels';

export function compareAutomationRunsForTriage(left: AutomationRun, right: AutomationRun): number {
  const unreadDelta = Number(right.isUnread) - Number(left.isUnread);
  if (unreadDelta !== 0) {
    return unreadDelta;
  }
  const statusDelta = automationTriageStatusWeight(right.status) - automationTriageStatusWeight(left.status);
  if (statusDelta !== 0) {
    return statusDelta;
  }
  return compareAutomationRunsNewestFirst(left, right);
}

export function selectAutomationRunsForTriage(runs: AutomationRun[]): AutomationRun[] {
  const selectedRuns = new Map<string, AutomationRun>();
  for (const run of runs.filter(isAutomationRunActionableInTriage).sort(compareAutomationRunsForTriage)) {
    selectedRuns.set(run.id, run);
  }
  for (const run of runs
    .filter(isAutomationRunRecentlyCompletedForTriage)
    .sort(compareAutomationRunsNewestFirst)
    .slice(0, PROJECT_AUTOMATION_TRIAGE_RECENT_COMPLETED_LIMIT)) {
    selectedRuns.set(run.id, run);
  }
  return [...selectedRuns.values()].sort(compareAutomationRunsForTriage);
}

export function isAutomationRunActionableInTriage(run: AutomationRun): boolean {
  return run.isUnread || run.status === 'findings' || run.status === 'needs_attention' || run.status === 'failed';
}

export function isAutomationRunRecentlyCompletedForTriage(run: AutomationRun): boolean {
  return Boolean(run.completedAt) && run.status !== 'running' && run.status !== 'queued';
}

export function automationTriageStatusWeight(status: AutomationRun['status']): number {
  switch (status) {
    case 'needs_attention':
    case 'failed':
      return 3;
    case 'findings':
      return 2;
    default:
      return 1;
  }
}

/*
 * CDXC:AutomationsRedesign 2026-08-23:
 * The Automate surface follows the Codex scheduled-tasks look: flat list rows
 * on the left, quiet grouped label/value cards on the right, one text scale,
 * regular font weights, and default shadcn controls. All styling is Tailwind
 * against the shared shadcn tokens; no bespoke `.project-automation-*` CSS.
 */

function automationRunStatusTone(status: AutomationRun['status']): string {
  switch (status) {
    case 'findings':
      return 'text-emerald-400/90';
    case 'failed':
    case 'needs_attention':
      return 'text-red-400/90';
    case 'running':
    case 'queued':
      return 'text-[var(--ghostex-accent)]';
    default:
      return 'text-muted-foreground';
  }
}

function AutomationSectionLabel({ children }: { children: React.ReactNode }) {
  return <h3 className='text-[13px] font-normal text-muted-foreground'>{children}</h3>;
}

function AutomationGroupCard({ children }: { children: React.ReactNode }) {
  return (
    <div className='min-w-0 overflow-hidden divide-y divide-border/60 rounded-xl border border-border/80 bg-white/[0.03]'>
      {children}
    </div>
  );
}

function AutomationDetailRow({ label, children }: { label: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className='flex min-h-11 min-w-0 items-center justify-between gap-4 px-4 py-2.5'>
      <dt className='shrink-0 text-sm font-normal text-foreground/90'>{label}</dt>
      <dd className='flex min-w-0 items-center gap-1.5 text-sm font-normal text-muted-foreground'>{children}</dd>
    </div>
  );
}

export function AutomationComingSoonOverlay({ surfaceName }: { surfaceName: string }) {
  return (
    <section aria-label={`${surfaceName} coming soon`} className='flex min-h-0 flex-1 items-center justify-center p-7'>
      <div className='flex max-w-md flex-col items-center gap-3 text-center' role='status'>
        <div className='flex size-12 items-center justify-center rounded-xl border border-border/80 bg-white/[0.04] text-muted-foreground'>
          <IconCalendarTime aria-hidden='true' className='size-6' />
        </div>
        <span className='text-xs text-muted-foreground'>Experimental</span>
        <h2 className='text-base font-normal text-foreground'>{surfaceName} is coming very soon</h2>
        <p className='max-w-sm text-sm leading-relaxed text-muted-foreground'>
          Enable Experimental Features in Settings to preview Automations Overview and project Automate pages before
          launch.
        </p>
      </div>
    </section>
  );
}

export function AutomationEmptyState({
  action,
  description,
  icon: Icon,
  title,
  variant = 'panel',
}: {
  action?: { label: string; onClick: () => void };
  description: string;
  icon: typeof IconCalendarTime;
  title: string;
  variant?: 'detail' | 'panel';
}) {
  return (
    <section
      className='flex h-full min-h-0 flex-1 flex-col items-center justify-center gap-2.5 p-8 text-center'
      data-variant={variant}
      {...(variant === 'detail' ? { 'aria-label': title } : {})}
    >
      <div className='mb-1 flex size-12 items-center justify-center rounded-xl border border-border/80 bg-white/[0.04] text-muted-foreground'>
        <Icon aria-hidden='true' className='size-6' />
      </div>
      <span className='text-sm text-foreground'>{title}</span>
      <p className='max-w-xs text-[13px] leading-relaxed text-muted-foreground'>{description}</p>
      {action ? (
        <Button className='mt-2' onClick={action.onClick} size='sm' type='button' variant='secondary'>
          {action.label}
        </Button>
      ) : null}
    </section>
  );
}

export function automationRunEmptyDescription(emptyTitle: string): string {
  if (emptyTitle.toLowerCase().includes('triage')) {
    return 'When an automation reports findings or needs attention, the result appears here for review.';
  }
  return 'Runs appear here after automations execute on their schedule or when you run them manually.';
}

export function AutomationDefinitionList({
  actionId,
  agents,
  automations,
  onCreate,
  onDelete,
  onEdit,
  onRunNow,
  onSelect,
  onSetEnabled,
  projectNameById,
  runs,
  selectedAutomationId,
  showProjectLabels = false,
}: {
  actionId: string;
  agents: ProjectAutomationAgentOption[];
  automations: AutomationDefinition[];
  onCreate: () => void;
  onDelete: (automation: AutomationDefinition) => void;
  onEdit: (automation: AutomationDefinition) => void;
  onRunNow: (automation: AutomationDefinition) => void;
  onSelect: (automationId: string) => void;
  onSetEnabled: (automation: AutomationDefinition, enabled: boolean) => void;
  projectNameById?: ReadonlyMap<string, string>;
  runs: AutomationRun[];
  selectedAutomationId: string;
  showProjectLabels?: boolean;
}) {
  if (automations.length === 0) {
    return (
      <AutomationEmptyState
        action={{ label: 'Create automation', onClick: onCreate }}
        description='Schedule agents with a timer, a specific date, or a repeating cadence.'
        icon={IconCalendarTime}
        title='No automations yet'
      />
    );
  }
  return (
    <section
      className='vertical-scroll-fade-mask flex min-h-0 flex-1 flex-col gap-px overflow-auto p-2 [--edge-fade-distance:16px]'
      aria-label='Automations'
    >
      {automations.map((automation) => {
        const unreadCount = runs.filter(
          (run) => run.automationId === automation.id && run.isUnread && !run.isArchived
        ).length;
        const agent = agents.find((candidate) => candidate.agentId === automation.agentId);
        const agentIcon = agent ? resolveAutomationAgentIcon(agent) : undefined;
        const automationProjectName = projectNameById?.get(automation.projectIds[0] ?? '');
        const isBusy = actionId === automation.id;
        const isSelected = automation.id === selectedAutomationId;
        const subtitle = [
          showProjectLabels && automationProjectName ? automationProjectName : undefined,
          describeAutomationSchedule(automation.schedule),
          automation.nextRunAt ? `Next run ${formatShortDate(automation.nextRunAt)}` : 'Not scheduled',
        ].filter(Boolean);
        return (
          <div
            className='group/autorow flex w-full cursor-default items-center gap-3 rounded-lg px-3 py-2.5 text-left outline-none transition-colors hover:bg-white/[0.04] focus-visible:bg-white/[0.04] data-[selected=true]:bg-white/[0.06]'
            data-selected={isSelected}
            key={automation.id}
            onClick={() => onSelect(automation.id)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                onSelect(automation.id);
              }
            }}
            role='button'
            tabIndex={0}
          >
            <span
              aria-hidden='true'
              className={`size-1.5 shrink-0 rounded-full ${automation.enabled ? 'bg-emerald-400/80' : 'bg-white/20'}`}
            />
            <div className='min-w-0 flex-1'>
              <div className='flex min-w-0 items-center gap-2'>
                {agentIcon ? <AutomationAgentIcon icon={agentIcon} /> : null}
                <span className='truncate text-sm font-normal text-foreground'>{automation.name}</span>
                {!automation.enabled ? <span className='shrink-0 text-xs text-muted-foreground'>Paused</span> : null}
                {unreadCount > 0 ? (
                  <span className='shrink-0 text-xs text-[var(--ghostex-accent)]'>{unreadCount} unread</span>
                ) : null}
              </div>
              <p className='mt-0.5 truncate text-xs font-normal text-muted-foreground'>{subtitle.join(' · ')}</p>
            </div>
            <Switch
              aria-label={automation.enabled ? `Pause ${automation.name}` : `Enable ${automation.name}`}
              checked={automation.enabled}
              className='shrink-0'
              disabled={isBusy}
              onCheckedChange={(enabled: boolean) => {
                onSetEnabled(automation, enabled);
              }}
              onClick={(event) => event.stopPropagation()}
              size='sm'
            />
          </div>
        );
      })}
    </section>
  );
}

export function AutomationRunList({
  actionId,
  agents,
  automations,
  emptyTitle,
  onArchive,
  onMarkRead,
  onOpenSession,
  onOpenWorktree,
  onSelect,
  projectName,
  runs,
  selectedRunId,
}: {
  actionId: string;
  agents: ProjectAutomationAgentOption[];
  automations: AutomationDefinition[];
  emptyTitle: string;
  onArchive: (run: AutomationRun) => void;
  onMarkRead: (run: AutomationRun) => void;
  onOpenSession: (run: AutomationRun) => void;
  onOpenWorktree: (run: AutomationRun) => void;
  onSelect: (runId: string) => void;
  projectName: string;
  runs: AutomationRun[];
  selectedRunId: string;
}) {
  if (runs.length === 0) {
    return (
      <AutomationEmptyState
        description={automationRunEmptyDescription(emptyTitle)}
        icon={IconBell}
        title={emptyTitle}
      />
    );
  }
  return (
    <section
      className='vertical-scroll-fade-mask flex min-h-0 flex-1 flex-col gap-px overflow-auto p-2 [--edge-fade-distance:16px]'
      aria-label='Automation runs'
    >
      {runs.map((run) => {
        const automation = automations.find((candidate) => candidate.id === run.automationId);
        const isActiveRun = isAutomationRunActive(run);
        const isSelected = run.id === selectedRunId;
        return (
          <div
            className='group/autorow flex w-full cursor-default items-center gap-3 rounded-lg px-3 py-2.5 text-left outline-none transition-colors hover:bg-white/[0.04] focus-visible:bg-white/[0.04] data-[selected=true]:bg-white/[0.06]'
            data-selected={isSelected}
            data-unread={run.isUnread}
            key={run.id}
            onClick={() => onSelect(run.id)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                onSelect(run.id);
              }
            }}
            role='button'
            tabIndex={0}
          >
            <span
              aria-hidden='true'
              className={`size-1.5 shrink-0 rounded-full ${
                run.isUnread ? 'bg-[var(--ghostex-accent)]' : 'bg-transparent'
              }`}
            />
            <div className='min-w-0 flex-1'>
              <div className='flex min-w-0 items-baseline gap-2'>
                <span className='truncate text-sm font-normal text-foreground'>
                  {automation?.name ?? run.automationId}
                </span>
                <span className={`shrink-0 text-xs font-normal ${automationRunStatusTone(run.status)}`}>
                  {automationRunStatusLabel(run.status)}
                </span>
              </div>
              <p className='mt-0.5 truncate text-xs font-normal text-muted-foreground'>
                {run.findingsSummary || run.errorMessage || 'Run is waiting for agent output.'}
              </p>
            </div>
            <span className='shrink-0 text-xs font-normal text-muted-foreground'>
              {formatShortDate(run.completedAt ?? run.createdAt)}
            </span>
            <div className='flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover/autorow:opacity-100 group-focus-within/autorow:opacity-100'>
              {run.sessionId ? (
                <Button
                  aria-label='Open automation session'
                  disabled={actionId === run.id}
                  onClick={(event) => {
                    event.stopPropagation();
                    onOpenSession(run);
                  }}
                  size='icon-sm'
                  type='button'
                  variant='ghost'
                >
                  <IconExternalLink />
                </Button>
              ) : null}
              {run.worktree ? (
                <Button
                  aria-label='Open automation worktree'
                  disabled={actionId === run.id}
                  onClick={(event) => {
                    event.stopPropagation();
                    onOpenWorktree(run);
                  }}
                  size='icon-sm'
                  type='button'
                  variant='ghost'
                >
                  <IconFolderOpen />
                </Button>
              ) : null}
              {run.isUnread ? (
                <Button
                  aria-label='Mark run read'
                  disabled={actionId === run.id}
                  onClick={(event) => {
                    event.stopPropagation();
                    onMarkRead(run);
                  }}
                  size='icon-sm'
                  type='button'
                  variant='ghost'
                >
                  <IconBell />
                </Button>
              ) : null}
              <Button
                aria-label='Archive run'
                disabled={actionId === run.id || isActiveRun}
                onClick={(event) => {
                  event.stopPropagation();
                  onArchive(run);
                }}
                size='icon-sm'
                type='button'
                variant='ghost'
              >
                <IconArchive />
              </Button>
            </div>
          </div>
        );
      })}
    </section>
  );
}

export function AutomationDefinitionDetail({
  actionId,
  agents,
  automation,
  onDelete,
  onEdit,
  onRunNow,
  onSetEnabled,
  projectNameById,
  runs,
  showProjectLabels = false,
}: {
  actionId: string;
  agents: ProjectAutomationAgentOption[];
  automation: AutomationDefinition | undefined;
  onDelete: (automation: AutomationDefinition) => void;
  onEdit: (automation: AutomationDefinition) => void;
  onRunNow: (automation: AutomationDefinition) => void;
  onSetEnabled: (automation: AutomationDefinition, enabled: boolean) => void;
  projectNameById?: ReadonlyMap<string, string>;
  runs: AutomationRun[];
  showProjectLabels?: boolean;
}) {
  if (!automation) {
    return (
      <section className='flex h-full min-h-0 flex-1 items-center justify-center' aria-label='Automation details'>
        <AutomationEmptyState
          description='Select an automation from the list to see its schedule, prompt, and recent runs.'
          icon={IconCalendarTime}
          title='No automation selected'
          variant='detail'
        />
      </section>
    );
  }
  const automationRuns = runs.filter((run) => run.automationId === automation.id).slice(0, 5);
  const agent = agents.find((candidate) => candidate.agentId === automation.agentId);
  const agentLabel = agent?.label ?? automation.agentId;
  const agentIcon = agent ? resolveAutomationAgentIcon(agent) : undefined;
  const automationProjectName = projectNameById?.get(automation.projectIds[0] ?? '');
  const isBusy = actionId === automation.id;
  return (
    <section
      className='vertical-scroll-fade-mask min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto [--edge-fade-distance:16px]'
      aria-label='Automation details'
    >
      <div className='mx-auto flex w-full min-w-0 max-w-2xl flex-col gap-6 p-6'>
        <div className='flex items-start justify-between gap-4'>
          <div className='min-w-0'>
            <span
              className={`text-[13px] font-normal ${
                automation.enabled ? 'text-[var(--ghostex-accent)]' : 'text-muted-foreground'
              }`}
            >
              {automation.enabled ? 'Active' : 'Paused'}
            </span>
            <h2 className='mt-1 truncate text-lg font-normal text-foreground'>{automation.name}</h2>
          </div>
          <div className='flex shrink-0 items-center gap-1.5'>
            <Button
              aria-label={`Run ${automation.name}`}
              disabled={isBusy}
              onClick={() => onRunNow(automation)}
              size='icon-sm'
              type='button'
              variant='ghost'
            >
              <IconPlayerPlay />
            </Button>
            {/*
             * CDXC:ProjectBoardRedesign 2026-08-24:
             * Enable/pause lives only on the list rows; the detail pane keeps
             * just run/edit/delete so the same control is not shown twice.
             */}
            <Button onClick={() => onEdit(automation)} size='sm' type='button' variant='ghost'>
              Edit
            </Button>
            <Button
              aria-label={`Delete ${automation.name}`}
              disabled={isBusy}
              onClick={() => onDelete(automation)}
              size='icon-sm'
              type='button'
              variant='ghost'
            >
              <IconTrash />
            </Button>
          </div>
        </div>
        <div className='min-w-0 overflow-hidden rounded-xl border border-border/80 bg-white/[0.03] p-4'>
          <p className='whitespace-pre-wrap break-words text-sm font-normal leading-relaxed text-foreground/90'>
            {automation.prompt}
          </p>
        </div>
        <div className='flex flex-col gap-2.5'>
          <AutomationSectionLabel>Details</AutomationSectionLabel>
          <AutomationGroupCard>
            <dl className='min-w-0 divide-y divide-border/60'>
              {showProjectLabels && automationProjectName ? (
                <AutomationDetailRow label='Project'>{automationProjectName}</AutomationDetailRow>
              ) : null}
              <AutomationDetailRow label='Schedule'>
                {describeAutomationSchedule(automation.schedule)}
              </AutomationDetailRow>
              <AutomationDetailRow label='Next run'>
                {automation.nextRunAt ? formatShortDate(automation.nextRunAt) : 'Not scheduled'}
              </AutomationDetailRow>
              <AutomationDetailRow label='Agent'>
                {agentIcon ? <AutomationAgentIcon icon={agentIcon} /> : null}
                <span className='truncate'>{agentLabel}</span>
              </AutomationDetailRow>
              <AutomationDetailRow label='Mode'>{describeAutomationMode(automation.executionMode)}</AutomationDetailRow>
              {automation.executionMode.kind === 'worktree' && automation.executionMode.setupCommand ? (
                <AutomationDetailRow label='Setup'>
                  <span className='truncate'>{automation.executionMode.setupCommand}</span>
                </AutomationDetailRow>
              ) : null}
              {automation.executionMode.kind === 'thread' ? (
                <AutomationDetailRow label='Thread'>
                  <span className='truncate'>
                    {automation.executionMode.agentSessionId ?? automation.executionMode.sessionId}
                  </span>
                </AutomationDetailRow>
              ) : null}
              {automation.executionMode.kind === 'thread' && automation.executionMode.expiresAt ? (
                <AutomationDetailRow label='Expires'>
                  {formatShortDate(automation.executionMode.expiresAt)}
                </AutomationDetailRow>
              ) : null}
            </dl>
          </AutomationGroupCard>
        </div>
        <div className='flex flex-col gap-2.5'>
          <AutomationSectionLabel>Recent runs</AutomationSectionLabel>
          <AutomationGroupCard>
            {automationRuns.length > 0 ? (
              automationRuns.map((run) => (
                <div className='flex min-h-11 items-center justify-between gap-4 px-4 py-2.5' key={run.id}>
                  <span className={`text-sm font-normal ${automationRunStatusTone(run.status)}`}>
                    {automationRunStatusLabel(run.status)}
                  </span>
                  <span className='text-sm font-normal text-muted-foreground'>
                    {formatShortDate(run.completedAt ?? run.createdAt)}
                  </span>
                </div>
              ))
            ) : (
              <div className='flex min-h-11 items-center px-4 py-2.5'>
                <span className='text-sm font-normal text-muted-foreground'>No runs yet.</span>
              </div>
            )}
          </AutomationGroupCard>
        </div>
      </div>
    </section>
  );
}

export function AutomationRunDetail({
  actionId,
  agents,
  automation,
  onArchive,
  onMarkRead,
  onOpenSession,
  onOpenWorktree,
  projectName,
  run,
}: {
  actionId: string;
  agents: ProjectAutomationAgentOption[];
  automation: AutomationDefinition | undefined;
  onArchive: (run: AutomationRun) => void;
  onMarkRead: (run: AutomationRun) => void;
  onOpenSession: (run: AutomationRun) => void;
  onOpenWorktree: (run: AutomationRun) => void;
  projectName: string;
  run: AutomationRun | undefined;
}) {
  if (!run) {
    return (
      <section className='flex h-full min-h-0 flex-1 items-center justify-center' aria-label='Automation run details'>
        <AutomationEmptyState
          description='Select a run from the list to review its status, summary, and linked session.'
          icon={IconBell}
          title='No run selected'
          variant='detail'
        />
      </section>
    );
  }
  const agent = automation ? agents.find((candidate) => candidate.agentId === automation.agentId) : undefined;
  const agentLabel = agent?.label ?? (automation ? automation.agentId : 'Unknown agent');
  const agentIcon = agent ? resolveAutomationAgentIcon(agent) : undefined;
  const isBusy = actionId === run.id;
  const isActiveRun = isAutomationRunActive(run);
  return (
    <section
      className='vertical-scroll-fade-mask min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto [--edge-fade-distance:16px]'
      aria-label='Automation run details'
    >
      <div className='mx-auto flex w-full min-w-0 max-w-2xl flex-col gap-6 p-6'>
        <div className='flex items-start justify-between gap-4'>
          <div className='min-w-0'>
            <span className={`text-[13px] font-normal ${automationRunStatusTone(run.status)}`}>
              {automationRunStatusLabel(run.status)}
            </span>
            <h2 className='mt-1 truncate text-lg font-normal text-foreground'>
              {automation?.name ?? run.automationId}
            </h2>
          </div>
          <div className='flex shrink-0 items-center gap-1.5'>
            {run.sessionId ? (
              <Button
                aria-label='Open automation session'
                disabled={isBusy}
                onClick={() => onOpenSession(run)}
                size='icon-sm'
                type='button'
                variant='ghost'
              >
                <IconExternalLink />
              </Button>
            ) : null}
            {run.worktree ? (
              <Button
                aria-label='Open automation worktree'
                disabled={isBusy}
                onClick={() => onOpenWorktree(run)}
                size='icon-sm'
                type='button'
                variant='ghost'
              >
                <IconFolderOpen />
              </Button>
            ) : null}
            {run.isUnread ? (
              <Button disabled={isBusy} onClick={() => onMarkRead(run)} size='sm' type='button' variant='ghost'>
                Read
              </Button>
            ) : null}
            <Button
              aria-label='Archive run'
              disabled={isBusy || isActiveRun}
              onClick={() => onArchive(run)}
              size='icon-sm'
              type='button'
              variant='ghost'
            >
              <IconArchive />
            </Button>
          </div>
        </div>
        <div className='min-w-0 overflow-hidden rounded-xl border border-border/80 bg-white/[0.03] p-4'>
          <p className='whitespace-pre-wrap break-words text-sm font-normal leading-relaxed text-foreground/90'>
            {run.findingsSummary || run.errorMessage || 'Run is waiting for agent output.'}
          </p>
        </div>
        <div className='flex flex-col gap-2.5'>
          <AutomationSectionLabel>Details</AutomationSectionLabel>
          <AutomationGroupCard>
            <dl className='min-w-0 divide-y divide-border/60'>
              <AutomationDetailRow label='Project'>{projectName}</AutomationDetailRow>
              <AutomationDetailRow label='Agent'>
                {agentIcon ? <AutomationAgentIcon icon={agentIcon} /> : null}
                <span className='truncate'>{agentLabel}</span>
              </AutomationDetailRow>
              <AutomationDetailRow label='Created'>{formatShortDate(run.createdAt)}</AutomationDetailRow>
              <AutomationDetailRow label='Completed'>
                {run.completedAt ? formatShortDate(run.completedAt) : 'Still running'}
              </AutomationDetailRow>
              {run.sessionId ? (
                <AutomationDetailRow label='Session'>
                  <span className='truncate'>{run.sessionId}</span>
                  <Button
                    aria-label='Copy automation session id'
                    onClick={() => void navigator.clipboard.writeText(run.sessionId ?? '')}
                    size='icon-sm'
                    type='button'
                    variant='ghost'
                  >
                    <IconCopy />
                  </Button>
                </AutomationDetailRow>
              ) : null}
              {run.worktree ? (
                <>
                  <AutomationDetailRow label='Branch'>
                    <span className='truncate'>{run.worktree.branch}</span>
                    <Button
                      aria-label='Copy automation worktree branch'
                      onClick={() => void navigator.clipboard.writeText(run.worktree?.branch ?? '')}
                      size='icon-sm'
                      type='button'
                      variant='ghost'
                    >
                      <IconCopy />
                    </Button>
                  </AutomationDetailRow>
                  <AutomationDetailRow label='Worktree'>
                    <span className='truncate'>{run.worktree.path}</span>
                    <Button
                      aria-label='Copy automation worktree path'
                      onClick={() => void navigator.clipboard.writeText(run.worktree?.path ?? '')}
                      size='icon-sm'
                      type='button'
                      variant='ghost'
                    >
                      <IconCopy />
                    </Button>
                  </AutomationDetailRow>
                </>
              ) : null}
            </dl>
          </AutomationGroupCard>
        </div>
      </div>
    </section>
  );
}
