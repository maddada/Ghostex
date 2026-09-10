import type { SessionChatSubagentInfo } from '@/packages/shared/session-chat';

type ModelInfo = Pick<SessionChatSubagentInfo, 'model' | 'effort'>;

function shortModel(model: string): string {
  const value = model.trim().replace(/\[[^\]]*\]/g, '');
  const parts = value.toLowerCase().split(/[ -]+/);
  const family = parts.findIndex((part) => ['opus', 'sonnet', 'haiku', 'fable'].includes(part));
  if (family >= 0) {
    const version = (tokens: string[]) => {
      const found: string[] = [];
      for (const token of tokens) {
        if (!/^\d{1,2}(?:\.\d{1,2})?$/.test(token) || found.length === 2) break;
        found.push(token);
      }
      return found.join('.');
    };
    const number =
      version(parts.slice(family + 1)) || version(parts.slice(0, family).filter((part) => part !== 'claude'));
    const name = parts[family][0].toUpperCase() + parts[family].slice(1);
    return number ? `${name} ${number}` : name;
  }
  const named = /^gpt-\d+(?:\.\d+)?-(astra|sol|terra|luna)$/i.exec(value);
  if (named) return named[1][0].toUpperCase() + named[1].slice(1).toLowerCase();
  return value
    .replace(/^gpt-/i, 'GPT ')
    .replace(/-codex\b/gi, ' Codex')
    .replace(/-/g, ' ');
}

function shortEffort(effort: string): string {
  const value = effort.trim().toLowerCase();
  return value === 'xhigh' ? 'xHigh' : value ? value[0].toUpperCase() + value.slice(1) : '';
}

export function subagentModelLabel(info?: ModelInfo | null): string {
  return [info?.model ? shortModel(info.model) : 'Model not recorded', info?.effort ? shortEffort(info.effort) : '']
    .filter(Boolean)
    .join(' ');
}

/**
 * CDXC:SessionChat 2026-09-10 DECISION:
 * User: show compact model and effort labels such as "Opus 5 High" and "Opus 5 xHigh" in the subagent card and popup; move agent types such as Explore and general-purpose into the tooltip. Model labels stay out of the tooltip.
 */
export function SessionChatSubagentModel({
  info,
  loading,
  unavailable,
}: {
  info?: ModelInfo | null;
  loading?: boolean;
  unavailable?: boolean;
}) {
  return <span>{loading ? 'Loading…' : unavailable ? 'Model unavailable' : subagentModelLabel(info)}</span>;
}
