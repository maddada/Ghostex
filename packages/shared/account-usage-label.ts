import type { AccountUsageWindow } from './agent-accounts';

/**
 * CDXC:AgentProviders 2026-09-09 DECISION:
 * User: usage labels use the compact 7d: 50% and 5h: 50% format throughout the app, without "used" after percentages.
 */
export function accountUsageLabel(window: AccountUsageWindow): string {
  const seconds = window.limitWindowSeconds;
  let duration: string | undefined;
  if (seconds && seconds > 0) {
    duration = seconds % 86400 === 0
      ? `${seconds / 86400}d`
      : seconds % 3600 === 0
        ? `${seconds / 3600}h`
        : `${Math.floor(seconds / 60)}m`;
  } else if (window.id === 'fiveHour') {
    duration = '5h';
  } else if (window.id === 'sevenDay' || window.model) {
    duration = '7d';
  }
  return duration ? `${window.model ? `${window.model} ` : ''}${duration}` : window.label;
}
