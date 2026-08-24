import {
  computeNextRunAt,
  normalizeAutomationSchedule,
  type AutomationDefinition,
  type AutomationExecutionMode,
  type AutomationRun,
  type AutomationSchedule,
  type ProjectAutomationAgentOption,
  type ProjectAutomationTargetOption,
} from '@/packages/shared/automations';

export const AUTOMATION_SCHEDULE_PRESETS = [
  { label: 'Every 5 minutes', value: '5m' },
  { label: 'Every 15 minutes', value: '15m' },
  { label: 'Every 30 minutes', value: '30m' },
  { label: 'Hourly', value: 'hourly' },
  { label: 'Every 6 hours', value: '6h' },
  { label: 'Every 12 hours', value: '12h' },
  { label: 'Daily', value: 'daily' },
  { label: 'Weekdays', value: 'weekdays' },
  { label: 'Weekly', value: 'weekly' },
  { label: 'Custom cron', value: 'cron' },
] as const;

export type AutomationSchedulePreset = (typeof AUTOMATION_SCHEDULE_PRESETS)[number]['value'];
export type AutomationScheduleMode = 'repeat' | 'timer' | 'date';
export type AutomationTimerUnit = 'minutes' | 'hours' | 'days';

export const AUTOMATION_INTERVAL_MS_BY_PRESET: Partial<Record<AutomationSchedulePreset, number>> = {
  '5m': 5 * 60 * 1000,
  '15m': 15 * 60 * 1000,
  '30m': 30 * 60 * 1000,
  hourly: 60 * 60 * 1000,
  '6h': 6 * 60 * 60 * 1000,
  '12h': 12 * 60 * 60 * 1000,
};

export const AUTOMATION_WEEKDAY_OPTIONS = [
  'Sunday',
  'Monday',
  'Tuesday',
  'Wednesday',
  'Thursday',
  'Friday',
  'Saturday',
] as const;

export const AUTOMATION_TIMER_UNIT_OPTIONS: Array<{ label: string; value: AutomationTimerUnit }> = [
  { label: 'Minutes', value: 'minutes' },
  { label: 'Hours', value: 'hours' },
  { label: 'Days', value: 'days' },
];

export type AutomationDraft = {
  agentId: string;
  cronExpression: string;
  enabled: boolean;
  executionKind: AutomationExecutionMode['kind'];
  expiresAt: string;
  id?: string;
  name: string;
  prompt: string;
  projectId: string;
  runAt: string;
  scheduleMode: AutomationScheduleMode;
  schedulePreset: AutomationSchedulePreset;
  scheduleTime: string;
  setupCommand: string;
  timerAmount: string;
  timerUnit: AutomationTimerUnit;
  threadAgentSessionId: string;
  threadSessionId: string;
  weeklyDay: string;
};

export function createAutomationDraft(input: Partial<AutomationDraft> = {}): AutomationDraft {
  return {
    agentId: input.agentId ?? '',
    cronExpression: input.cronExpression ?? '*/15 * * * *',
    enabled: input.enabled ?? true,
    expiresAt: input.expiresAt ?? '',
    executionKind: input.executionKind ?? 'worktree',
    id: input.id,
    name: input.name ?? '',
    prompt: input.prompt ?? '',
    projectId: input.projectId ?? '',
    runAt: input.runAt ?? '',
    scheduleMode: input.scheduleMode ?? 'repeat',
    schedulePreset: input.schedulePreset ?? '15m',
    scheduleTime: input.scheduleTime ?? '09:00',
    setupCommand: input.setupCommand ?? '',
    timerAmount: input.timerAmount ?? '30',
    timerUnit: input.timerUnit ?? 'minutes',
    threadAgentSessionId: input.threadAgentSessionId ?? '',
    threadSessionId: input.threadSessionId ?? '',
    weeklyDay: input.weeklyDay ?? '1',
  };
}

export function resolveAutomationDraftAgentId(
  agents: readonly Pick<ProjectAutomationAgentOption, 'agentId'>[],
  defaultAgentId?: string
): string {
  /*
   * CDXC:Automations 2026-06-30-19:16:
   * New automation drafts should select the user's Default Prompt Agent by default, but only when that agent is present in the launchable options for the selected project. An unavailable saved id should not render as "Choose agent" or be saved invisibly.
   */
  const normalizedDefaultAgentId = defaultAgentId?.trim();
  return agents.find((agent) => agent.agentId === normalizedDefaultAgentId)?.agentId ?? agents[0]?.agentId ?? '';
}

export function resolveAutomationDraftProjectId(
  projects: readonly Pick<ProjectAutomationTargetOption, 'projectId'>[],
  currentProjectId: string | undefined,
  fallbackProjectId: string | undefined
): string {
  /*
   * CDXC:Automations 2026-07-01-02:33:
   * The global Create automation dialog is hosted by the Quick Automations surface, but saved automation definitions must target a real automation project. Keep an existing draft project only when it is still present in the loaded target list, so opening the dialog before bridge hydration cannot preserve `quick-automations` as the selected project.
   */
  const normalizedCurrentProjectId = currentProjectId?.trim() ?? '';
  if (normalizedCurrentProjectId && projects.some((project) => project.projectId === normalizedCurrentProjectId)) {
    return normalizedCurrentProjectId;
  }
  return projects[0]?.projectId ?? fallbackProjectId?.trim() ?? '';
}

export function createAutomationDraftFromDefinition(
  definition: AutomationDefinition,
  projectId: string
): AutomationDraft {
  const schedulePreset = resolveAutomationSchedulePreset(definition.schedule);
  const schedule = definition.schedule;
  if (schedule.kind === 'once') {
    return createAutomationDraftFromDefinitionSchedule(definition, schedulePreset, projectId, {
      runAt: toDatetimeLocalValue(schedule.runAt),
      scheduleMode: 'date',
    });
  }
  if (schedule.kind === 'weekly') {
    return createAutomationDraftFromDefinitionSchedule(definition, schedulePreset, projectId, {
      scheduleTime: schedule.time,
      weeklyDay: String(schedule.days[0] ?? 1),
    });
  }
  if (schedule.kind === 'daily') {
    return createAutomationDraftFromDefinitionSchedule(definition, schedulePreset, projectId, {
      scheduleTime: schedule.time,
    });
  }
  if (schedule.kind === 'cron') {
    return createAutomationDraftFromDefinitionSchedule(definition, schedulePreset, projectId, {
      cronExpression: schedule.expression,
    });
  }
  return createAutomationDraftFromDefinitionSchedule(definition, schedulePreset, projectId);
}

export function resolveAutomationSchedulePreset(schedule: AutomationSchedule): AutomationSchedulePreset {
  if (schedule.kind === 'once') {
    return '15m';
  }
  if (schedule.kind === 'interval') {
    const matchedPreset = Object.entries(AUTOMATION_INTERVAL_MS_BY_PRESET).find(
      ([, everyMs]) => everyMs === schedule.everyMs
    );
    return (matchedPreset?.[0] as AutomationSchedulePreset | undefined) ?? 'hourly';
  }
  if (schedule.kind === 'weekly') {
    const weekdayPreset = [1, 2, 3, 4, 5];
    if (schedule.days.length === weekdayPreset.length && weekdayPreset.every((day) => schedule.days.includes(day))) {
      return 'weekdays';
    }
    return 'weekly';
  }
  if (schedule.kind === 'daily') {
    return 'daily';
  }
  return 'cron';
}

export function createAutomationDraftFromDefinitionSchedule(
  definition: AutomationDefinition,
  schedulePreset: AutomationDraft['schedulePreset'],
  projectId: string,
  input: Partial<AutomationDraft> = {}
): AutomationDraft {
  return createAutomationDraft({
    ...input,
    agentId: definition.agentId,
    enabled: definition.enabled,
    expiresAt:
      definition.executionMode.kind === 'thread' && definition.executionMode.expiresAt
        ? toDatetimeLocalValue(definition.executionMode.expiresAt)
        : '',
    executionKind: definition.executionMode.kind,
    id: definition.id,
    name: definition.name,
    prompt: definition.prompt,
    projectId,
    schedulePreset,
    setupCommand: definition.executionMode.kind === 'worktree' ? (definition.executionMode.setupCommand ?? '') : '',
    threadAgentSessionId:
      definition.executionMode.kind === 'thread' ? (definition.executionMode.agentSessionId ?? '') : '',
    threadSessionId: definition.executionMode.kind === 'thread' ? (definition.executionMode.sessionId ?? '') : '',
  });
}

export function createAutomationDefinitionFromDraft(
  draft: AutomationDraft,
  input: { fallbackAgentId: string; projectId: string }
): AutomationDefinition | undefined {
  const name = draft.name.trim();
  const prompt = draft.prompt.trim();
  const agentId = draft.agentId.trim() || input.fallbackAgentId.trim();
  const schedule = createAutomationScheduleFromDraft(draft);
  if (!name || !prompt || !agentId || !schedule) {
    return undefined;
  }
  const now = new Date().toISOString();
  const executionMode: AutomationExecutionMode =
    draft.executionKind === 'local'
      ? { kind: 'local' }
      : draft.executionKind === 'thread'
        ? {
            agentSessionId: draft.threadAgentSessionId.trim() || undefined,
            expiresAt: datetimeLocalToIso(draft.expiresAt),
            kind: 'thread',
            sessionId: draft.threadSessionId.trim() || undefined,
          }
        : {
            kind: 'worktree',
            setupCommand: draft.setupCommand.trim() || undefined,
          };
  if (executionMode.kind === 'thread' && !executionMode.agentSessionId && !executionMode.sessionId) {
    return undefined;
  }
  return {
    agentId,
    createdAt: now,
    enabled: draft.enabled,
    executionMode,
    id: draft.id ?? `automation-${crypto.randomUUID()}`,
    name,
    nextRunAt: draft.enabled ? computeNextRunAt(schedule) : undefined,
    projectIds: [input.projectId],
    prompt,
    schedule,
    updatedAt: now,
  };
}

export function createAutomationScheduleFromDraft(draft: AutomationDraft): AutomationSchedule | undefined {
  if (draft.scheduleMode === 'timer') {
    const amount = Number(draft.timerAmount);
    const unitMs =
      draft.timerUnit === 'days' ? 24 * 60 * 60 * 1000 : draft.timerUnit === 'hours' ? 60 * 60 * 1000 : 60 * 1000;
    const delayMs = amount * unitMs;
    if (!Number.isFinite(delayMs) || delayMs < 60 * 1000 || delayMs > 365 * 24 * 60 * 60 * 1000) {
      return undefined;
    }
    return normalizeAutomationSchedule({
      kind: 'once',
      runAt: new Date(Date.now() + delayMs).toISOString(),
    });
  }
  if (draft.scheduleMode === 'date') {
    return normalizeAutomationSchedule({
      kind: 'once',
      runAt: datetimeLocalToIso(draft.runAt),
    });
  }
  const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || 'local';
  const intervalMs = AUTOMATION_INTERVAL_MS_BY_PRESET[draft.schedulePreset];
  const schedule =
    intervalMs !== undefined
      ? { everyMs: intervalMs, kind: 'interval' }
      : draft.schedulePreset === 'cron'
        ? {
            expression: draft.cronExpression,
            kind: 'cron',
            timezone,
          }
        : draft.schedulePreset === 'weekly'
          ? {
              days: [Number(draft.weeklyDay)],
              kind: 'weekly',
              time: draft.scheduleTime,
              timezone,
            }
          : draft.schedulePreset === 'weekdays'
            ? {
                days: [1, 2, 3, 4, 5],
                kind: 'weekly',
                time: draft.scheduleTime,
                timezone,
              }
            : {
                kind: 'daily',
                time: draft.scheduleTime,
                timezone,
              };
  return normalizeAutomationSchedule(schedule);
}

export function describeAutomationSchedule(schedule: AutomationSchedule): string {
  switch (schedule.kind) {
    case 'once':
      return `Once on ${new Date(schedule.runAt).toLocaleString()}`;
    case 'interval': {
      const preset = Object.entries(AUTOMATION_INTERVAL_MS_BY_PRESET).find(
        ([, everyMs]) => everyMs === schedule.everyMs
      );
      if (preset) {
        return AUTOMATION_SCHEDULE_PRESETS.find((option) => option.value === preset[0])?.label ?? preset[0];
      }
      if (schedule.everyMs % (60 * 60 * 1000) === 0) {
        const hours = schedule.everyMs / (60 * 60 * 1000);
        return hours === 1 ? 'Hourly' : `Every ${hours} hours`;
      }
      return `Every ${Math.round(schedule.everyMs / 60_000)} minutes`;
    }
    case 'daily':
      return `Daily at ${schedule.time}`;
    case 'weekly': {
      const weekdayPreset = [1, 2, 3, 4, 5];
      if (schedule.days.length === weekdayPreset.length && weekdayPreset.every((day) => schedule.days.includes(day))) {
        return `Weekdays at ${schedule.time}`;
      }
      return `Weekly ${weekdayLabel(schedule.days[0] ?? 0)} at ${schedule.time}`;
    }
    case 'cron':
      return schedule.expression;
  }
}

export function describeAutomationMode(mode: AutomationExecutionMode): string {
  switch (mode.kind) {
    case 'worktree':
      return 'Worktree';
    case 'thread':
      return 'Thread';
    case 'local':
      return 'Local checkout';
  }
}

export function automationRunStatusLabel(status: AutomationRun['status']): string {
  switch (status) {
    case 'no_findings':
      return 'No findings';
    case 'needs_attention':
      return 'Needs attention';
    default: {
      const label = status.replace(/_/gu, ' ');
      return label.charAt(0).toUpperCase() + label.slice(1);
    }
  }
}

export function isAutomationRunActive(run: Pick<AutomationRun, 'status'>): boolean {
  return run.status === 'queued' || run.status === 'running';
}

export function weekdayLabel(day: number): string {
  return ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'][day] ?? 'Weekly';
}

export function datetimeLocalToIso(value: string): string | undefined {
  const trimmed = value.trim();
  if (!trimmed) {
    return undefined;
  }
  const parsedMs = Date.parse(trimmed);
  return Number.isFinite(parsedMs) ? new Date(parsedMs).toISOString() : undefined;
}

export function toDatetimeLocalValue(value: string): string {
  const parsedMs = Date.parse(value);
  if (!Number.isFinite(parsedMs)) {
    return '';
  }
  const date = new Date(parsedMs);
  const pad = (part: number) => String(part).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}
