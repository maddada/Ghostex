import type { AccountUsageWindow, AgentAccount } from '@/packages/shared/agent-accounts';
import { accountUsageLabel } from '@/packages/shared/account-usage-label';
import { formatResetCountdown } from '@/packages/shared/reset-countdown';
import type {
  SessionChatClaudeStatus,
  SessionChatCodexStatus,
  SessionChatCodexTokens,
  SessionChatDetectedOptions,
} from '@/packages/shared/session-chat';
import {
  formatSessionChatContextPercentage,
  formatSessionChatContextTokens,
  resolveSessionChatContextMeterUsage,
} from './session-chat-context-meter';
import type { SessionChatContextDetailRowDefinition } from './session-chat-context-details';
import { formatSessionChatDuration } from './session-chat-duration';

export type ContextDetailsAgent = 'claude' | 'codex';
export type AdditionalContextDetailRowId =
  | 'contextUsed'
  | 'totalInputTokens'
  | 'totalTokens'
  | 'cachedTokens'
  | 'cacheWriteTokens'
  | 'reasoningTokens'
  | 'turnTokens'
  | 'cacheRatio'
  | 'fiveHourLimit'
  | 'sevenDayLimit'
  | 'modelLimit'
  | 'fiveHourReset'
  | 'sevenDayReset'
  | 'credits'
  | 'plan'
  | 'lastTurnDuration'
  | 'firstTokenTime'
  | 'model'
  | 'provider'
  | 'permissions'
  | 'parentThread'
  | 'startedAt'
  | 'accountName'
  | 'accountEmail'
  | 'accountSpending'
  | 'accountResets'
  | 'accountUsageUpdated'
  | 'accountUsageStatus'
  | 'accountSessions';

export interface ContextDetailStatus extends SessionChatClaudeStatus {
  codex?: SessionChatCodexStatus;
  account?: AgentAccount;
  contextUsed?: string;
  modelName?: string;
  effortName?: string;
}

export function resolveContextDetailStatus(
  agent: ContextDetailsAgent,
  options: SessionChatDetectedOptions | null | undefined,
  account?: AgentAccount
): ContextDetailStatus {
  const codex = agent === 'codex' ? options?.codexStatus : undefined;
  const usage = resolveSessionChatContextMeterUsage(options?.contextUsage, agent === 'codex');
  const request = codex?.lastRequest;
  const common =
    agent === 'claude'
      ? options?.claudeStatus
      : {
          version: codex?.version,
          currentDir: codex?.currentDir,
          totalOutputTokens: codex?.totalTokens?.outputTokens,
          remainingPercentage: usage?.usedPercentage == null ? undefined : 100 - usage.usedPercentage,
          lastRequest: request
            ? {
                inputTokens:
                  request.inputTokens === undefined
                    ? undefined
                    : Math.max(0, request.inputTokens - (request.cachedInputTokens ?? 0)),
                outputTokens: request.outputTokens,
                cacheReadTokens: request.cachedInputTokens,
                cacheWriteTokens: request.cacheWriteInputTokens,
              }
            : undefined,
        };
  return {
    ...common,
    codex,
    account: account?.provider === agent ? account : undefined,
    modelName: options?.model?.label ?? codex?.model,
    effortName: options?.effort?.label ?? codex?.effort,
    contextUsed: usage
      ? [
          formatSessionChatContextPercentage(usage.usedPercentage),
          usage.usedTokens === null
            ? null
            : `${formatSessionChatContextTokens(usage.usedTokens)}${usage.windowSize === null ? '' : `/${formatSessionChatContextTokens(usage.windowSize)}`}`,
        ]
          .filter(Boolean)
          .join(' · ')
      : undefined,
  };
}

const count = (value?: number) =>
  typeof value === 'number' && Number.isFinite(value) ? formatSessionChatContextTokens(value) : null;
const duration = (value?: number) =>
  typeof value === 'number' && Number.isFinite(value) ? formatSessionChatDuration(value) : null;
const join = (values: (string | null | undefined)[]) => values.filter(Boolean).join(' · ') || null;
const words = (value?: string) => value?.replace(/[_-]/g, ' ') ?? null;

function formatWindowDuration(minutes: number): string {
  if (minutes % 1440 === 0) return `${minutes / 1440}d`;
  if (minutes % 60 === 0) return `${minutes / 60}h`;
  return `${minutes}m`;
}

type UsageWindowKind = 'fiveHour' | 'sevenDay' | 'model';

interface UsageWindowSample {
  /** The app-wide compact window label (5h, 7d, Fable 7d). */
  label: string;
  usedPercent: number;
  /** Epoch seconds; absent when the source reported no reset time. */
  resetsAt?: number;
}

function accountWindowKind(window: AccountUsageWindow): UsageWindowKind | 'spend' {
  if (window.model) return 'model';
  if (window.id === 'fiveHour' || window.id === ':primary_window') return 'fiveHour';
  if (window.id === 'sevenDay' || window.id === ':secondary_window') return 'sevenDay';
  return window.id === 'spend' ? 'spend' : 'model';
}

function sessionUsageWindow(status: ContextDetailStatus, kind: UsageWindowKind): UsageWindowSample | null {
  if (kind === 'model') return null;
  const fallbackLabel = kind === 'fiveHour' ? '5h' : '7d';
  const claude = kind === 'fiveHour' ? status.rateLimits?.fiveHour : status.rateLimits?.sevenDay;
  if (claude && typeof claude.usedPercentage === 'number' && Number.isFinite(claude.usedPercentage)) {
    return { label: fallbackLabel, usedPercent: claude.usedPercentage, resetsAt: claude.resetsAt };
  }
  const codex = kind === 'fiveHour' ? status.codex?.primary : status.codex?.secondary;
  if (codex && typeof codex.usedPercentage === 'number' && Number.isFinite(codex.usedPercentage)) {
    return {
      label: codex.windowMinutes ? formatWindowDuration(codex.windowMinutes) : fallbackLabel,
      usedPercent: codex.usedPercentage,
      resetsAt: codex.resetsAt,
    };
  }
  return null;
}

/** CDXC:AgentProviders 2026-09-09 WHY:
 * Rate limits belong to the account, including before a draft has made a request or written transcript usage.
 * A linked session reads the same usage windows as Accounts; only an unlinked session reads the windows its agent last reported.
 */
function usageWindows(status: ContextDetailStatus, kind: UsageWindowKind): UsageWindowSample[] {
  if (status.account) {
    return accountUsageSamples(status.account, kind);
  }
  const window = sessionUsageWindow(status, kind);
  return window ? [window] : [];
}

function accountUsageSamples(account: AgentAccount, kind: UsageWindowKind | 'spend'): UsageWindowSample[] {
  return account.usage
    .filter((window) => accountWindowKind(window) === kind && Number.isFinite(window.usedPercent))
    .map((window) => {
      const resetsAt = window.resetsAt ? Date.parse(window.resetsAt) / 1000 : Number.NaN;
      return {
        label: accountUsageLabel(window),
        usedPercent: window.usedPercent,
        ...(Number.isFinite(resetsAt) ? { resetsAt } : {}),
      };
    });
}

/**
 * CDXC:AgentProviders 2026-09-09 DECISION:
 * User: usage-window labels throughout the app use a colon (7d: 50%, 5h: 50%), without "used" in the chat popover or status line.
 */
const usagePercentText = (window: UsageWindowSample): string => `${window.label}: ${Math.round(window.usedPercent)}%`;

const usageResetCountdown = (window: UsageWindowSample, now: number): string | null =>
  window.resetsAt === undefined || !Number.isFinite(window.resetsAt)
    ? null
    : window.resetsAt * 1000 > now
      ? `resets ${formatResetCountdown(window.resetsAt * 1000 - now)}`
      : 'reset due';

/** The reset rows stand alone in the status line, so each carries its window label (5h resets 2h 47m). */
const usageResetText = (window: UsageWindowSample, now: number): string | null => {
  const reset = usageResetCountdown(window, now);
  return reset ? `${window.label} ${reset}` : null;
};

function usageWindowRow(
  id: AdditionalContextDetailRowId,
  label: string,
  description: string,
  kind: UsageWindowKind,
  recommended: boolean,
  render: (window: UsageWindowSample, now: number) => string | null
): SessionChatContextDetailRowDefinition {
  return {
    id,
    label,
    description,
    group: 'usage',
    recommended,
    value: ({ status, now }) => join(usageWindows(status, kind).map((window) => render(window, now))),
  };
}

/** CDXC:AgentProviders 2026-09-11 DECISION:
 * User: the usage rows are one value each, separately for Claude and Codex: 7d limit, 5h limit, the model limit (Fable), 7d reset and 5h reset.
 * This replaced the combined "Rate limits" row and the duplicated "Account limits", "Account primary limit", "Account weekly limit", "Account model limits", "Primary limit" and "Secondary limit" rows, whose values repeated each other.
 * The catalog is static, so every model-scoped window shares the one "Model limit" row; an account with several model windows joins them on that row.
 * The five-hour and weekly rows are recommended because the retired "Rate limits" row showed exactly those values by default.
 */
export const USAGE_WINDOW_ROWS: readonly SessionChatContextDetailRowDefinition[] = [
  usageWindowRow(
    'fiveHourLimit',
    '5h limit',
    'Five-hour usage from the saved account, or the session when unlinked',
    'fiveHour',
    true,
    usagePercentText
  ),
  usageWindowRow(
    'sevenDayLimit',
    '7d limit',
    'Weekly usage from the saved account, or the session when unlinked',
    'sevenDay',
    true,
    usagePercentText
  ),
  usageWindowRow(
    'modelLimit',
    'Model limit',
    'Model-specific usage from the saved account, such as Fable',
    'model',
    false,
    usagePercentText
  ),
  usageWindowRow('fiveHourReset', '5h reset', 'When the five-hour window resets', 'fiveHour', true, usageResetText),
  usageWindowRow('sevenDayReset', '7d reset', 'When the weekly window resets', 'sevenDay', false, usageResetText),
];

function tokensRow(
  id: AdditionalContextDetailRowId,
  label: string,
  field: keyof SessionChatCodexTokens,
  description: string
): SessionChatContextDetailRowDefinition {
  return {
    id,
    label,
    description,
    group: 'context',
    recommended: false,
    value: ({ status }) => count(status.codex?.totalTokens?.[field]),
  };
}

export const CODEX_CONTEXT_DETAIL_ROWS: readonly SessionChatContextDetailRowDefinition[] = [
  tokensRow('totalInputTokens', 'Total input tokens', 'inputTokens', 'Cumulative input, including cached input'),
  tokensRow('totalTokens', 'Total tokens', 'totalTokens', 'Cumulative session usage, distinct from current context'),
  tokensRow(
    'cachedTokens',
    'Cached input tokens',
    'cachedInputTokens',
    'Cumulative cached input, already included in total input'
  ),
  tokensRow(
    'cacheWriteTokens',
    'Cache-write tokens',
    'cacheWriteInputTokens',
    'Cumulative cache writes, when reported'
  ),
  tokensRow(
    'reasoningTokens',
    'Reasoning tokens',
    'reasoningOutputTokens',
    'Cumulative reasoning output, already included in output tokens'
  ),
  {
    id: 'turnTokens',
    group: 'context',
    label: 'Current turn tokens',
    description: 'Usage attributed to the latest recorded turn',
    recommended: false,
    value: ({ status }) => count(status.codex?.turnTokens?.totalTokens),
  },
  {
    id: 'cacheRatio',
    group: 'context',
    label: 'Cached input share',
    description: 'Calculated cached input divided by cumulative input',
    recommended: false,
    value: ({ status }) => {
      const usage = status.codex?.totalTokens;
      return usage?.inputTokens && usage.cachedInputTokens !== undefined
        ? `${((usage.cachedInputTokens / usage.inputTokens) * 100).toFixed(1)}%`
        : null;
    },
  },
  /** CDXC:AgentProviders 2026-09-08 DECISION:
   * User: show how many usage resets remain from the saved Codex account in context details and the configurable status line.
   */
  {
    id: 'accountResets',
    group: 'usage',
    label: 'Account resets',
    description: 'Available usage resets from the saved account assigned to this session',
    recommended: false,
    value: ({ status }) => {
      const resets = status.account?.resetCredits;
      return resets == null ? null : `${resets} ${resets === 1 ? 'reset' : 'resets'}`;
    },
  },
  {
    id: 'credits',
    group: 'usage',
    label: 'Account credits',
    description: 'Credit balance reported by Codex, distinct from session cost',
    recommended: false,
    value: ({ status }) => {
      const credits = status.codex?.credits;
      return credits?.unlimited
        ? 'Unlimited'
        : (credits?.balance ?? (credits?.hasCredits === undefined ? null : credits.hasCredits ? 'Available' : 'None'));
    },
  },
  {
    id: 'plan',
    group: 'usage',
    label: 'Account plan',
    description: 'Plan last reported by this Codex session',
    recommended: false,
    value: ({ status }) => words(status.codex?.plan),
  },
  {
    id: 'lastTurnDuration',
    group: 'usage',
    label: 'Last turn duration',
    description: 'Codex-reported duration of the latest completed turn',
    recommended: true,
    value: ({ status }) => duration(status.codex?.lastTurnDurationMs),
  },
  {
    id: 'firstTokenTime',
    group: 'usage',
    label: 'Time to first token',
    description: 'Codex-reported time to first token on the latest completed turn',
    recommended: false,
    value: ({ status }) => duration(status.codex?.timeToFirstTokenMs),
  },
  {
    id: 'provider',
    group: 'session',
    label: 'Model provider',
    description: 'The provider recorded in the Codex session',
    recommended: false,
    value: ({ status }) => status.codex?.provider ?? null,
  },
  {
    id: 'permissions',
    group: 'session',
    label: 'Permissions',
    description: 'Sandbox and approval policy recorded at turn start',
    recommended: false,
    value: ({ status }) => join([words(status.codex?.sandbox), words(status.codex?.approvalPolicy)]),
  },
  {
    id: 'parentThread',
    group: 'session',
    label: 'Parent thread',
    description: 'Parent or fork source recorded by Codex',
    recommended: false,
    value: ({ status }) => status.codex?.parentThreadId ?? status.codex?.forkedFromId ?? null,
  },
  {
    id: 'startedAt',
    group: 'session',
    label: 'Started',
    description: 'Session creation time recorded by Codex',
    recommended: false,
    value: ({ status }) => {
      const timestamp = Date.parse(status.codex?.startedAt ?? '');
      return Number.isFinite(timestamp) ? new Date(timestamp).toLocaleString() : null;
    },
  },
];

/** CDXC:AgentProviders 2026-09-08 DECISION:
 * User: saved cswap/xswap account stats are selectable in both agents' popovers and status lines.
 * The chat reuses its existing account snapshot and follows the session's assigned account.
 */
export const SHARED_CONTEXT_DETAIL_ROWS: readonly SessionChatContextDetailRowDefinition[] = [
  {
    id: 'contextUsed',
    group: 'context',
    label: 'Context used',
    description: 'Current context percentage and tokens',
    recommended: false,
    value: ({ status }) => status.contextUsed ?? null,
  },
  {
    id: 'model',
    group: 'session',
    label: 'Model',
    description: 'The session’s reported model',
    recommended: false,
    value: ({ status }) => status.modelName ?? null,
  },
  {
    id: 'accountName',
    group: 'session',
    label: 'Account',
    description: 'Saved account assigned to this session',
    recommended: false,
    value: ({ status }) => status.account?.name ?? null,
  },
  {
    id: 'accountEmail',
    group: 'session',
    label: 'Account email',
    description: 'Email of the saved account assigned to this session',
    recommended: false,
    value: ({ status }) => status.account?.email || null,
  },
  {
    id: 'accountSpending',
    group: 'usage',
    label: 'Account extra usage',
    description: 'Account-wide extra spending allowance, distinct from session cost',
    recommended: false,
    value: ({ status, now }) =>
      status.account
        ? join(
            accountUsageSamples(status.account, 'spend').map((window) =>
              join([usagePercentText(window), usageResetCountdown(window, now)])
            )
          )
        : null,
  },
  {
    id: 'accountUsageUpdated',
    group: 'usage',
    label: 'Account usage updated',
    description: 'Age of the saved account usage snapshot',
    recommended: false,
    value: ({ status, now }) => {
      const updated = Date.parse(status.account?.usageUpdatedAt ?? '');
      return Number.isFinite(updated) ? `${formatSessionChatDuration(now - updated)} ago` : null;
    },
  },
  {
    id: 'accountUsageStatus',
    group: 'usage',
    label: 'Account usage status',
    description: 'Saved account availability or usage refresh error',
    recommended: false,
    value: ({ status }) => status.account?.usageError ?? words(status.account?.status),
  },
  {
    id: 'accountSessions',
    group: 'session',
    label: 'Account sessions',
    description: 'Number of Ghostex sessions assigned to this saved account',
    recommended: false,
    value: ({ status }) => (status.account ? String(status.account.sessionCount) : null),
  },
];
