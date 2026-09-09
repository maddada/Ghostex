import type { AgentAccount } from '@/packages/shared/agent-accounts';
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
  | 'primaryLimit'
  | 'secondaryLimit'
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
  | 'accountLimits'
  | 'accountPrimaryLimit'
  | 'accountWeeklyLimit'
  | 'accountModelLimits'
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

/**
 * CDXC:AgentProviders 2026-09-09 DECISION:
 * User: usage-window labels throughout the app use a colon (7d: 50%, 5h: 50%), without "used" in the chat popover or status line.
 */
function windowValue(
  used: number | undefined,
  resetsAt: number | undefined,
  now: number,
  label?: string
): string | null {
  if (used === undefined || !Number.isFinite(used)) return null;
  const reset =
    resetsAt === undefined || !Number.isFinite(resetsAt)
      ? null
      : resetsAt * 1000 > now
        ? `resets ${formatResetCountdown(resetsAt * 1000 - now)}`
        : 'reset due';
  return join([`${label ? `${label}: ` : ''}${Math.round(used)}%`, reset]);
}

/** CDXC:AgentProviders 2026-09-09 WHY:
 * Rate limits belong to the account, including before a draft has made a request or written transcript usage.
 * Linked sessions read the same main usage windows as Accounts; model-specific allowances remain separate rows.
 */
export function savedAccountRateLimits(account: AgentAccount, now: number): string | null {
  return join(
    account.usage
      .filter(
        (window) =>
          !window.model && ['fiveHour', 'sevenDay', ':primary_window', ':secondary_window'].includes(window.id)
      )
      .map((window) =>
        windowValue(
          window.usedPercent,
          window.resetsAt ? Date.parse(window.resetsAt) / 1000 : undefined,
          now,
          accountUsageLabel(window)
        )
      )
  );
}

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
  ...(['primary', 'secondary'] as const).map((key): SessionChatContextDetailRowDefinition => ({
    id: key === 'primary' ? 'primaryLimit' : 'secondaryLimit',
    group: 'usage',
    label: key === 'primary' ? 'Primary limit' : 'Secondary limit',
    description: 'Account usage window last reported by this Codex session',
    recommended: false,
    value: ({ status, now }) => {
      const window = status.codex?.[key];
      return windowValue(
        window?.usedPercentage,
        window?.resetsAt,
        now,
        window?.windowMinutes ? formatWindowDuration(window.windowMinutes) : undefined
      );
    },
  })),
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
  ...(
    [
      ['accountLimits', 'Account limits', 'All usage windows from the saved account', () => true],
      [
        'accountPrimaryLimit',
        'Account primary limit',
        'Primary usage window and reset from the saved account',
        (id: string) => id === 'fiveHour' || id === ':primary_window',
      ],
      [
        'accountWeeklyLimit',
        'Account weekly limit',
        'Weekly usage and reset from the saved account',
        (id: string) => id === 'sevenDay' || id === ':secondary_window',
      ],
      [
        'accountModelLimits',
        'Account model limits',
        'Model-specific usage windows from the saved account',
        (id: string, model?: string) =>
          Boolean(model) || !['fiveHour', 'sevenDay', 'spend', ':primary_window', ':secondary_window'].includes(id),
      ],
      [
        'accountSpending',
        'Account extra usage',
        'Account-wide extra spending allowance, distinct from session cost',
        (id: string) => id === 'spend',
      ],
    ] as const
  ).map(([id, label, description, matches]): SessionChatContextDetailRowDefinition => ({
    id,
    label,
    description,
    group: 'usage',
    recommended: false,
    value: ({ status, now }) =>
      join(
        (status.account?.usage ?? [])
          .filter((window) => matches(window.id, window.model))
          .map((window) =>
            windowValue(
              window.usedPercent,
              window.resetsAt ? Date.parse(window.resetsAt) / 1000 : undefined,
              now,
              accountUsageLabel(window)
            )
          )
      ),
  })),
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
