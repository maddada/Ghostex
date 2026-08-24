import { CronExpressionParser } from 'cron-parser';
import type { SidebarAgentIcon } from './sidebar-agents';

export type AutomationSchedule =
  | { kind: 'once'; runAt: string }
  | { kind: 'interval'; everyMs: number }
  | { kind: 'daily'; time: string; timezone: string }
  | { kind: 'weekly'; days: number[]; time: string; timezone: string }
  | { kind: 'cron'; expression: string; timezone: string };

export type AutomationExecutionMode =
  | { kind: 'local' }
  | { kind: 'worktree'; setupCommand?: string }
  | {
      agentSessionId?: string;
      expiresAt?: string;
      kind: 'thread';
      sessionId?: string;
    };

export type AutomationDefinition = {
  agentId: string;
  createdAt: string;
  enabled: boolean;
  executionMode: AutomationExecutionMode;
  id: string;
  name: string;
  nextRunAt?: string;
  prompt: string;
  projectIds: string[];
  schedule: AutomationSchedule;
  updatedAt: string;
};

export type AutomationRunStatus =
  'queued' | 'running' | 'findings' | 'no_findings' | 'failed' | 'needs_attention' | 'cancelled' | 'skipped';

export type AutomationRun = {
  automationId: string;
  completedAt?: string;
  createdAt: string;
  errorMessage?: string;
  findingsSummary?: string;
  id: string;
  isArchived: boolean;
  isUnread: boolean;
  projectId: string;
  sessionId?: string;
  status: AutomationRunStatus;
  worktree?: {
    branch: string;
    path: string;
    sourcePath: string;
  };
};

export type AutomationState = {
  automations: AutomationDefinition[];
  runs: AutomationRun[];
};

export type ProjectAutomationAgentOption = {
  agentId: string;
  command?: string;
  icon?: SidebarAgentIcon;
  label: string;
};

export type ProjectAutomationTargetOption = {
  canUseWorktrees: boolean;
  label: string;
  path: string;
  projectId: string;
  worktreeUnavailableReason?: string;
};

export type ProjectAutomationsBridgeState = AutomationState & {
  agents: ProjectAutomationAgentOption[];
  defaultAgentId?: string;
  projectCanUseWorktrees: boolean;
  projectId: string;
  projectName: string;
  projectPath: string;
  projects: ProjectAutomationTargetOption[];
  worktreeUnavailableReason?: string;
};

export type AutomationResultKind = Extract<AutomationRunStatus, 'findings' | 'no_findings' | 'needs_attention'>;

const DEFAULT_INTERVAL_MS = 60 * 60 * 1000;
const MIN_INTERVAL_MS = 60 * 1000;
const MAX_INTERVAL_MS = 365 * 24 * 60 * 60 * 1000;
const MAX_AUTOMATION_COUNT = 500;
const MAX_AUTOMATION_RUN_COUNT = 5_000;
const TIME_PATTERN = /^(?<hour>[01]\d|2[0-3]):(?<minute>[0-5]\d)$/u;
const AUTOMATION_RESULT_PATTERN = /AUTOMATION_RESULT:\s*(findings|no_findings|needs_attention)\b/iu;

export function createDefaultAutomationState(): AutomationState {
  return { automations: [], runs: [] };
}

export function normalizeAutomationState(candidate: unknown): AutomationState {
  if (!isRecord(candidate)) {
    return createDefaultAutomationState();
  }
  return {
    automations: normalizeAutomationDefinitions(candidate.automations),
    runs: normalizeAutomationRuns(candidate.automationRuns ?? candidate.runs),
  };
}

export function normalizeAutomationDefinitions(candidate: unknown): AutomationDefinition[] {
  if (!Array.isArray(candidate)) {
    return [];
  }
  const definitions: AutomationDefinition[] = [];
  const seenIds = new Set<string>();
  for (const entry of candidate) {
    const normalized = normalizeAutomationDefinition(entry);
    if (!normalized || seenIds.has(normalized.id)) {
      continue;
    }
    definitions.push(normalized);
    seenIds.add(normalized.id);
    if (definitions.length >= MAX_AUTOMATION_COUNT) {
      break;
    }
  }
  return definitions;
}

export function normalizeAutomationDefinition(candidate: unknown): AutomationDefinition | undefined {
  if (!isRecord(candidate)) {
    return undefined;
  }
  const id = normalizeNonEmptyString(candidate.id);
  const name = normalizeNonEmptyString(candidate.name);
  const agentId = normalizeNonEmptyString(candidate.agentId);
  const prompt = normalizeNonEmptyString(candidate.prompt);
  const schedule = normalizeAutomationSchedule(candidate.schedule);
  const executionMode = normalizeAutomationExecutionMode(candidate.executionMode);
  const projectIds = normalizeStringList(candidate.projectIds);
  if (!id || !name || !agentId || !prompt || !schedule || !executionMode || projectIds.length === 0) {
    return undefined;
  }
  const now = new Date().toISOString();
  const createdAt = normalizeDateString(candidate.createdAt) ?? now;
  const updatedAt = normalizeDateString(candidate.updatedAt) ?? createdAt;
  return {
    agentId,
    createdAt,
    enabled: candidate.enabled === true,
    executionMode,
    id,
    name,
    nextRunAt: normalizeDateString(candidate.nextRunAt),
    prompt,
    projectIds,
    schedule,
    updatedAt,
  };
}

export function normalizeAutomationSchedule(candidate: unknown): AutomationSchedule | undefined {
  if (!isRecord(candidate)) {
    return undefined;
  }
  switch (candidate.kind) {
    case 'once': {
      const runAt = normalizeDateString(candidate.runAt);
      return runAt ? { kind: 'once', runAt } : undefined;
    }
    case 'interval': {
      const everyMs = normalizeIntervalMs(candidate.everyMs);
      return everyMs ? { kind: 'interval', everyMs } : undefined;
    }
    case 'daily': {
      const time = normalizeTime(candidate.time);
      if (!time) {
        return undefined;
      }
      return { kind: 'daily', time, timezone: normalizeTimezone(candidate.timezone) };
    }
    case 'weekly': {
      const time = normalizeTime(candidate.time);
      const days = normalizeWeekdays(candidate.days);
      if (!time || days.length === 0) {
        return undefined;
      }
      return { kind: 'weekly', days, time, timezone: normalizeTimezone(candidate.timezone) };
    }
    case 'cron': {
      const expression = normalizeCronExpression(candidate.expression);
      if (!expression) {
        return undefined;
      }
      return { kind: 'cron', expression, timezone: normalizeTimezone(candidate.timezone) };
    }
    default:
      return undefined;
  }
}

export function normalizeAutomationExecutionMode(candidate: unknown): AutomationExecutionMode | undefined {
  if (!isRecord(candidate)) {
    return undefined;
  }
  switch (candidate.kind) {
    case 'local':
      return { kind: 'local' };
    case 'worktree':
      return {
        kind: 'worktree',
        setupCommand: normalizeNonEmptyString(candidate.setupCommand),
      };
    case 'thread': {
      const agentSessionId = normalizeNonEmptyString(candidate.agentSessionId);
      const sessionId = normalizeNonEmptyString(candidate.sessionId);
      if (!agentSessionId && !sessionId) {
        return undefined;
      }
      return {
        agentSessionId,
        expiresAt: normalizeDateString(candidate.expiresAt),
        kind: 'thread',
        sessionId,
      };
    }
    default:
      return undefined;
  }
}

export function normalizeAutomationRuns(candidate: unknown): AutomationRun[] {
  if (!Array.isArray(candidate)) {
    return [];
  }
  const runs: AutomationRun[] = [];
  const seenIds = new Set<string>();
  for (const entry of candidate) {
    const normalized = normalizeAutomationRun(entry);
    if (!normalized || seenIds.has(normalized.id)) {
      continue;
    }
    runs.push(normalized);
    seenIds.add(normalized.id);
    if (runs.length >= MAX_AUTOMATION_RUN_COUNT) {
      break;
    }
  }
  return runs.sort(compareAutomationRunsNewestFirst);
}

export function normalizeAutomationRun(candidate: unknown): AutomationRun | undefined {
  if (!isRecord(candidate)) {
    return undefined;
  }
  const id = normalizeNonEmptyString(candidate.id);
  const automationId = normalizeNonEmptyString(candidate.automationId);
  const projectId = normalizeNonEmptyString(candidate.projectId);
  const status = normalizeAutomationRunStatus(candidate.status);
  if (!id || !automationId || !projectId || !status) {
    return undefined;
  }
  return {
    automationId,
    completedAt: normalizeDateString(candidate.completedAt),
    createdAt: normalizeDateString(candidate.createdAt) ?? new Date().toISOString(),
    errorMessage: normalizeNonEmptyString(candidate.errorMessage),
    findingsSummary: normalizeNonEmptyString(candidate.findingsSummary),
    id,
    isArchived: candidate.isArchived === true,
    isUnread: candidate.isUnread === true,
    projectId,
    sessionId: normalizeNonEmptyString(candidate.sessionId),
    status,
    worktree: normalizeAutomationRunWorktree(candidate.worktree),
  };
}

export function normalizeAutomationRunStatus(candidate: unknown): AutomationRunStatus | undefined {
  switch (candidate) {
    case 'queued':
    case 'running':
    case 'findings':
    case 'no_findings':
    case 'failed':
    case 'needs_attention':
    case 'cancelled':
    case 'skipped':
      return candidate;
    default:
      return undefined;
  }
}

export function computeNextRunAt(
  schedule: AutomationSchedule,
  options: { from?: Date; after?: Date } = {}
): string | undefined {
  const from = options.from ?? new Date();
  const after = options.after ?? from;
  const afterMs = after.getTime();
  if (!Number.isFinite(afterMs)) {
    return undefined;
  }
  switch (schedule.kind) {
    case 'once': {
      const runAtMs = Date.parse(schedule.runAt);
      return Number.isFinite(runAtMs) && runAtMs > afterMs ? new Date(runAtMs).toISOString() : undefined;
    }
    case 'interval':
      return new Date(afterMs + schedule.everyMs).toISOString();
    case 'daily':
      return computeNextDailyRunAt(schedule.time, from, schedule.timezone);
    case 'weekly':
      return computeNextWeeklyRunAt(schedule.days, schedule.time, from, schedule.timezone);
    case 'cron':
      return computeNextCronRunAt(schedule.expression, from, schedule.timezone);
    default:
      return undefined;
  }
}

export function parseAutomationResult(text: string): {
  result?: AutomationResultKind;
  summary?: string;
} {
  const match = AUTOMATION_RESULT_PATTERN.exec(text);
  if (!match) {
    return {};
  }
  const result = match[1]?.toLowerCase() as AutomationResultKind | undefined;
  if (!result) {
    return {};
  }
  const summary = text
    .slice((match.index ?? 0) + match[0].length)
    .trim()
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(0, 6)
    .join('\n');
  return { result, summary: summary || undefined };
}

export function compareAutomationRunsNewestFirst(left: AutomationRun, right: AutomationRun): number {
  const leftTime = Date.parse(left.completedAt ?? left.createdAt);
  const rightTime = Date.parse(right.completedAt ?? right.createdAt);
  return (Number.isFinite(rightTime) ? rightTime : 0) - (Number.isFinite(leftTime) ? leftTime : 0);
}

function computeNextDailyRunAt(time: string, from: Date, timezone: string): string | undefined {
  const parsed = parseTime(time);
  if (!parsed) {
    return undefined;
  }
  return computeNextCronRunAt(`${parsed.minute} ${parsed.hour} * * *`, from, timezone);
}

function computeNextWeeklyRunAt(
  days: readonly number[],
  time: string,
  from: Date,
  timezone: string
): string | undefined {
  const parsed = parseTime(time);
  if (!parsed || days.length === 0) {
    return undefined;
  }
  const weekdayPart = [...new Set(days)].sort((left, right) => left - right).join(',');
  return computeNextCronRunAt(`${parsed.minute} ${parsed.hour} * * ${weekdayPart}`, from, timezone);
}

function computeNextCronRunAt(expression: string, from: Date, timezone: string): string | undefined {
  try {
    const interval = CronExpressionParser.parse(expression, {
      currentDate: from,
      tz: timezone === 'local' ? undefined : timezone,
    });
    return interval.next().toDate().toISOString();
  } catch {
    return undefined;
  }
}

function normalizeAutomationRunWorktree(candidate: unknown): AutomationRun['worktree'] {
  if (!isRecord(candidate)) {
    return undefined;
  }
  const branch = normalizeNonEmptyString(candidate.branch);
  const path = normalizeNonEmptyString(candidate.path);
  const sourcePath = normalizeNonEmptyString(candidate.sourcePath);
  return branch && path && sourcePath ? { branch, path, sourcePath } : undefined;
}

function normalizeIntervalMs(value: unknown): number | undefined {
  const numericValue = typeof value === 'number' ? value : Number(value);
  if (!Number.isFinite(numericValue)) {
    return DEFAULT_INTERVAL_MS;
  }
  const roundedValue = Math.round(numericValue);
  return roundedValue >= MIN_INTERVAL_MS && roundedValue <= MAX_INTERVAL_MS ? roundedValue : undefined;
}

function normalizeTime(value: unknown): string | undefined {
  return typeof value === 'string' && TIME_PATTERN.test(value) ? value : undefined;
}

function parseTime(value: string): { hour: number; minute: number } | undefined {
  const match = TIME_PATTERN.exec(value);
  if (!match?.groups) {
    return undefined;
  }
  return {
    hour: Number(match.groups.hour),
    minute: Number(match.groups.minute),
  };
}

function normalizeTimezone(value: unknown): string {
  return typeof value === 'string' && value.trim() ? value.trim() : 'local';
}

function normalizeWeekdays(value: unknown): number[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return [...new Set(value.filter((day) => Number.isInteger(day) && day >= 0 && day <= 6))].sort(
    (left, right) => left - right
  );
}

function normalizeCronExpression(value: unknown): string | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }
  const expression = value.trim().replace(/\s+/gu, ' ');
  try {
    CronExpressionParser.parse(expression);
  } catch {
    return undefined;
  }
  return expression;
}

function normalizeStringList(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return [
    ...new Set(
      value.flatMap((entry) => {
        const normalized = normalizeNonEmptyString(entry);
        return normalized ? [normalized] : [];
      })
    ),
  ];
}

function normalizeNonEmptyString(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() ? value.trim() : undefined;
}

function normalizeDateString(value: unknown): string | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }
  const parsedMs = Date.parse(value);
  return Number.isFinite(parsedMs) ? new Date(parsedMs).toISOString() : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}
