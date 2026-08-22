import {
  IconArchive,
  IconBell,
  IconCalendarTime,
  IconCopy,
  IconExternalLink,
  IconFolderOpen,
  IconPlayerPlay,
  IconTrash,
} from "@tabler/icons-react";
import { Button } from "@/packages/components/ui/button";
import {
  Card,
  CardContent,
} from "@/packages/components/ui/card";
import { Separator } from "@/packages/components/ui/separator";
import { Switch } from "@/packages/components/ui/switch";
import { Select } from "@/packages/components/ui/select";
import { formatShortDate } from "../project-board-shared";
import {
  compareAutomationRunsNewestFirst,
  type AutomationDefinition,
  type AutomationRun,
  type ProjectAutomationAgentOption,
} from "@/packages/shared/automations";
import { PROJECT_AUTOMATION_TRIAGE_RECENT_COMPLETED_LIMIT } from "./constants";
import {
  describeAutomationSchedule,
  describeAutomationMode,
  automationRunStatusLabel,
  isAutomationRunActive,
} from "./automations-drafts";
import {
  automationAgentLabel,
  resolveAutomationAgentIcon,
  AutomationAgentIcon,
} from "./agent-labels";

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
  return (
    run.isUnread ||
    run.status === "findings" ||
    run.status === "needs_attention" ||
    run.status === "failed"
  );
}

export function isAutomationRunRecentlyCompletedForTriage(run: AutomationRun): boolean {
  return Boolean(run.completedAt) && run.status !== "running" && run.status !== "queued";
}

export function automationTriageStatusWeight(status: AutomationRun["status"]): number {
  switch (status) {
    case "needs_attention":
    case "failed":
      return 3;
    case "findings":
      return 2;
    default:
      return 1;
  }
}

export function AutomationComingSoonOverlay({ surfaceName }: { surfaceName: string }) {
  return (
    <section
      aria-label={`${surfaceName} coming soon`}
      className="project-automation-coming-soon"
    >
      <div className="project-automation-coming-soon-panel" role="status">
        <div className="project-automation-coming-soon-icon">
          <IconCalendarTime aria-hidden="true" />
        </div>
        <span>Experimental</span>
        <h2>{surfaceName} is coming very soon</h2>
        <p>
          Enable Experimental Features in Settings to preview Automations
          Overview and project Automate pages before launch.
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
  variant = "panel",
}: {
  action?: { label: string; onClick: () => void };
  description: string;
  icon: typeof IconCalendarTime;
  title: string;
  variant?: "detail" | "panel";
}) {
  return (
    <section
      className="project-automation-empty-state"
      data-variant={variant}
      {...(variant === "detail" ? { "aria-label": title } : {})}
    >
      <div className="project-automation-empty-state-icon">
        <Icon aria-hidden="true" />
      </div>
      <strong>{title}</strong>
      <p>{description}</p>
      {action ? (
        <Button
          className="project-automation-empty-action"
          onClick={action.onClick}
          size="sm"
          type="button"
          variant="secondary"
        >
          {action.label}
        </Button>
      ) : null}
    </section>
  );
}

export function automationRunEmptyDescription(emptyTitle: string): string {
  if (emptyTitle.toLowerCase().includes("triage")) {
    return "When an automation reports findings or needs attention, the result appears here for review.";
  }
  return "Runs appear here after automations execute on their schedule or when you run them manually.";
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
        action={{ label: "Create automation", onClick: onCreate }}
        description="Schedule agents with a timer, a specific date, or a repeating cadence."
        icon={IconCalendarTime}
        title="No automations yet"
      />
    );
  }
  return (
    <section className="project-automation-list vertical-scroll-fade-mask" aria-label="Automations">
      {automations.map((automation) => {
        const lastRun = runs.find((run) => run.automationId === automation.id);
        const unreadCount = runs.filter(
          (run) => run.automationId === automation.id && run.isUnread && !run.isArchived,
        ).length;
        const agent = agents.find((candidate) => candidate.agentId === automation.agentId);
        const agentLabel = agent?.label ?? automation.agentId;
        const automationProjectName = projectNameById?.get(automation.projectIds[0] ?? "");
        const isBusy = actionId === automation.id;
        return (
          <Card
            className="project-automation-card"
            data-selected={automation.id === selectedAutomationId}
            key={automation.id}
            onClick={() => onSelect(automation.id)}
            role="button"
            size="sm"
            tabIndex={0}
          >
            <CardContent>
              <div className="project-automation-card-main">
                <div>
                  <div className="project-automation-card-title">
                    <span data-enabled={automation.enabled}>{automation.enabled ? "Enabled" : "Paused"}</span>
                    <strong>{automation.name}</strong>
                  </div>
                  <div className="project-automation-card-tags">
                    {showProjectLabels && automationProjectName ? <span>{automationProjectName}</span> : null}
                    <span>{describeAutomationSchedule(automation.schedule)}</span>
                    <span>{describeAutomationMode(automation.executionMode)}</span>
                  </div>
                  <div className="project-automation-card-agent">
                    {agent && resolveAutomationAgentIcon(agent) ? (
                      <AutomationAgentIcon icon={resolveAutomationAgentIcon(agent)!} />
                    ) : null}
                    <span>{agentLabel}</span>
                  </div>
                </div>
                <div className="project-automation-card-meta">
                  <span>{automation.nextRunAt ? formatShortDate(automation.nextRunAt) : "No next run"}</span>
                  <span>{lastRun ? automationRunStatusLabel(lastRun.status) : "Never run"}</span>
                  {unreadCount > 0 ? <span data-unread="true">{unreadCount} unread</span> : null}
                </div>
              </div>
              <div className="project-automation-card-actions">
                <Button
                  aria-label={`Run ${automation.name}`}
                  disabled={isBusy}
                  onClick={(event) => {
                    event.stopPropagation();
                    onRunNow(automation);
                  }}
                  size="icon-sm"
                  type="button"
                  variant="ghost"
                >
                  <IconPlayerPlay />
                </Button>
                <div className="project-automation-card-toggle">
                  <Switch
                    checked={automation.enabled}
                    disabled={isBusy}
                    onCheckedChange={(enabled: boolean) => {
                      onSetEnabled(automation, enabled);
                    }}
                    onClick={(event) => event.stopPropagation()}
                    size="sm"
                  />
                  <span data-enabled={automation.enabled}>{automation.enabled ? "On" : "Off"}</span>
                </div>
                <Button
                  onClick={(event) => {
                    event.stopPropagation();
                    onEdit(automation);
                  }}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  Edit
                </Button>
                <Button
                  aria-label={`Delete ${automation.name}`}
                  disabled={isBusy}
                  onClick={(event) => {
                    event.stopPropagation();
                    onDelete(automation);
                  }}
                  size="icon-sm"
                  type="button"
                  variant="ghost"
                >
                  <IconTrash />
                </Button>
              </div>
            </CardContent>
          </Card>
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
    <section className="project-automation-run-list vertical-scroll-fade-mask" aria-label="Automation runs">
      {runs.map((run) => {
        const automation = automations.find((candidate) => candidate.id === run.automationId);
        const agentLabel = automation ? automationAgentLabel(agents, automation.agentId) : "Unknown agent";
        const isActiveRun = isAutomationRunActive(run);
        return (
          <Card
            className="project-automation-run-card"
            data-selected={run.id === selectedRunId}
            data-unread={run.isUnread}
            key={run.id}
            onClick={() => onSelect(run.id)}
            role="button"
            size="sm"
            tabIndex={0}
          >
            <CardContent>
              <div className="project-automation-run-main">
                <div className="project-automation-run-heading">
                  <span data-status={run.status}>{automationRunStatusLabel(run.status)}</span>
                  <strong>{automation?.name ?? run.automationId}</strong>
                </div>
                <p>{run.findingsSummary || run.errorMessage || "Run is waiting for agent output."}</p>
                <div className="project-automation-run-meta">
                  <span>{projectName}</span>
                  <span>{agentLabel}</span>
                  <span>{formatShortDate(run.completedAt ?? run.createdAt)}</span>
                  {run.sessionId ? <span>Session {run.sessionId}</span> : null}
                  {run.worktree ? <span>{run.worktree.branch}</span> : null}
                </div>
              </div>
              <div className="project-automation-run-actions">
                {run.sessionId ? (
                  <Button
                    aria-label="Open automation session"
                    disabled={actionId === run.id}
                    onClick={(event) => {
                      event.stopPropagation();
                      onOpenSession(run);
                    }}
                    size="icon-sm"
                    type="button"
                    variant="ghost"
                  >
                    <IconExternalLink />
                  </Button>
                ) : null}
                {run.worktree ? (
                  <Button
                    aria-label="Open automation worktree"
                    disabled={actionId === run.id}
                    onClick={(event) => {
                      event.stopPropagation();
                      onOpenWorktree(run);
                    }}
                    size="icon-sm"
                    type="button"
                    variant="ghost"
                  >
                    <IconFolderOpen />
                  </Button>
                ) : null}
                {run.isUnread ? (
                  <Button
                    aria-label="Mark run read"
                    disabled={actionId === run.id}
                    onClick={(event) => {
                      event.stopPropagation();
                      onMarkRead(run);
                    }}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    Read
                  </Button>
                ) : null}
                <Button
                  aria-label="Archive run"
                  disabled={actionId === run.id || isActiveRun}
                  onClick={(event) => {
                    event.stopPropagation();
                    onArchive(run);
                  }}
                  size="icon-sm"
                  type="button"
                  variant="ghost"
                >
                  <IconArchive />
                </Button>
              </div>
            </CardContent>
          </Card>
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
      <section className="project-automation-detail project-automation-detail--empty" aria-label="Automation details">
        <AutomationEmptyState
          description="Select an automation from the list to see its schedule, prompt, and recent runs."
          icon={IconCalendarTime}
          title="No automation selected"
          variant="detail"
        />
      </section>
    );
  }
  const automationRuns = runs
    .filter((run) => run.automationId === automation.id)
    .slice(0, 5);
  const agent = agents.find((candidate) => candidate.agentId === automation.agentId);
  const agentLabel = agent?.label ?? automation.agentId;
  const agentIcon = agent ? resolveAutomationAgentIcon(agent) : undefined;
  const automationProjectName = projectNameById?.get(automation.projectIds[0] ?? "");
  const isBusy = actionId === automation.id;
  return (
    <section className="project-automation-detail vertical-scroll-fade-mask" aria-label="Automation details">
      <div className="project-automation-detail-header">
        <div>
          <span data-enabled={automation.enabled}>{automation.enabled ? "Enabled" : "Paused"}</span>
          <h2>{automation.name}</h2>
        </div>
        <div className="project-automation-detail-actions">
          <Button
            aria-label={`Run ${automation.name}`}
            disabled={isBusy}
            onClick={() => onRunNow(automation)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <IconPlayerPlay />
          </Button>
          <div className="project-automation-detail-toggle">
            <Switch
              checked={automation.enabled}
              disabled={isBusy}
              onCheckedChange={(enabled: boolean) => onSetEnabled(automation, enabled)}
              size="sm"
            />
            <span data-enabled={automation.enabled}>{automation.enabled ? "Enabled" : "Paused"}</span>
          </div>
          <Button onClick={() => onEdit(automation)} size="sm" type="button" variant="outline">
            Edit
          </Button>
          <Button
            aria-label={`Delete ${automation.name}`}
            disabled={isBusy}
            onClick={() => onDelete(automation)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <IconTrash />
          </Button>
        </div>
      </div>
      <dl className="project-automation-detail-grid">
        {showProjectLabels && automationProjectName ? (
          <div>
            <dt>Project</dt>
            <dd>{automationProjectName}</dd>
          </div>
        ) : null}
        <div>
          <dt>Schedule</dt>
          <dd>{describeAutomationSchedule(automation.schedule)}</dd>
        </div>
        <div>
          <dt>Next run</dt>
          <dd>{automation.nextRunAt ? formatShortDate(automation.nextRunAt) : "Not scheduled"}</dd>
        </div>
        <div>
          <dt>Agent</dt>
          <dd>
            {agentIcon ? <AutomationAgentIcon icon={agentIcon} /> : null}
            <span>{agentLabel}</span>
          </dd>
        </div>
        <div>
          <dt>Mode</dt>
          <dd>{describeAutomationMode(automation.executionMode)}</dd>
        </div>
        {automation.executionMode.kind === "worktree" && automation.executionMode.setupCommand ? (
          <div>
            <dt>Setup</dt>
            <dd>{automation.executionMode.setupCommand}</dd>
          </div>
        ) : null}
        {automation.executionMode.kind === "thread" ? (
          <div>
            <dt>Thread</dt>
            <dd>{automation.executionMode.sessionId}</dd>
          </div>
        ) : null}
        {automation.executionMode.kind === "thread" && automation.executionMode.expiresAt ? (
          <div>
            <dt>Expires</dt>
            <dd>{formatShortDate(automation.executionMode.expiresAt)}</dd>
          </div>
        ) : null}
      </dl>
      <Separator />
      <div className="project-automation-detail-section">
        <h3>Prompt</h3>
        <pre>{automation.prompt}</pre>
      </div>
      <div className="project-automation-detail-section">
        <h3>Recent runs</h3>
        {automationRuns.length > 0 ? (
          <div className="project-automation-detail-run-stack">
            {automationRuns.map((run) => (
              <div key={run.id}>
                <span data-status={run.status}>{automationRunStatusLabel(run.status)}</span>
                <p>{formatShortDate(run.completedAt ?? run.createdAt)}</p>
              </div>
            ))}
          </div>
        ) : (
          <p>No runs yet.</p>
        )}
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
      <section className="project-automation-detail project-automation-detail--empty" aria-label="Automation run details">
        <AutomationEmptyState
          description="Select a run from the list to review its status, summary, and linked session."
          icon={IconBell}
          title="No run selected"
          variant="detail"
        />
      </section>
    );
  }
  const agent = automation ? agents.find((candidate) => candidate.agentId === automation.agentId) : undefined;
  const agentLabel = agent?.label ?? (automation ? automation.agentId : "Unknown agent");
  const agentIcon = agent ? resolveAutomationAgentIcon(agent) : undefined;
  const isBusy = actionId === run.id;
  const isActiveRun = isAutomationRunActive(run);
  return (
    <section className="project-automation-detail vertical-scroll-fade-mask" aria-label="Automation run details">
      <div className="project-automation-detail-header">
        <div>
          <span data-status={run.status}>{automationRunStatusLabel(run.status)}</span>
          <h2>{automation?.name ?? run.automationId}</h2>
        </div>
        <div className="project-automation-detail-actions">
          {run.sessionId ? (
            <Button
              aria-label="Open automation session"
              disabled={isBusy}
              onClick={() => onOpenSession(run)}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              <IconExternalLink />
            </Button>
          ) : null}
          {run.worktree ? (
            <Button
              aria-label="Open automation worktree"
              disabled={isBusy}
              onClick={() => onOpenWorktree(run)}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              <IconFolderOpen />
            </Button>
          ) : null}
          {run.isUnread ? (
            <Button disabled={isBusy} onClick={() => onMarkRead(run)} size="sm" type="button" variant="outline">
              Read
            </Button>
          ) : null}
          <Button
            aria-label="Archive run"
            disabled={isBusy || isActiveRun}
            onClick={() => onArchive(run)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <IconArchive />
          </Button>
        </div>
      </div>
      <dl className="project-automation-detail-grid">
        <div>
          <dt>Project</dt>
          <dd>{projectName}</dd>
        </div>
        <div>
          <dt>Agent</dt>
          <dd>
            {agentIcon ? <AutomationAgentIcon icon={agentIcon} /> : null}
            <span>{agentLabel}</span>
          </dd>
        </div>
        <div>
          <dt>Created</dt>
          <dd>{formatShortDate(run.createdAt)}</dd>
        </div>
        <div>
          <dt>Completed</dt>
          <dd>{run.completedAt ? formatShortDate(run.completedAt) : "Still running"}</dd>
        </div>
        {run.sessionId ? (
          <div>
            <dt>Session</dt>
            <dd>
              <span>{run.sessionId}</span>
              <Button
                aria-label="Copy automation session id"
                onClick={() => void navigator.clipboard.writeText(run.sessionId ?? "")}
                size="icon-sm"
                type="button"
                variant="ghost"
              >
                <IconCopy />
              </Button>
            </dd>
          </div>
        ) : null}
        {run.worktree ? (
          <>
            <div>
              <dt>Branch</dt>
              <dd>
                <span>{run.worktree.branch}</span>
                <Button
                  aria-label="Copy automation worktree branch"
                  onClick={() => void navigator.clipboard.writeText(run.worktree?.branch ?? "")}
                  size="icon-sm"
                  type="button"
                  variant="ghost"
                >
                  <IconCopy />
                </Button>
              </dd>
            </div>
            <div>
              <dt>Worktree</dt>
              <dd>
                <span>{run.worktree.path}</span>
                <Button
                  aria-label="Copy automation worktree path"
                  onClick={() => void navigator.clipboard.writeText(run.worktree?.path ?? "")}
                  size="icon-sm"
                  type="button"
                  variant="ghost"
                >
                  <IconCopy />
                </Button>
              </dd>
            </div>
          </>
        ) : null}
      </dl>
      <Separator />
      <div className="project-automation-detail-section">
        <h3>Result</h3>
        <p>{run.findingsSummary || run.errorMessage || "Run is waiting for agent output."}</p>
      </div>
    </section>
  );
}