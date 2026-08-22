/*
CDXC:AgentHistorySearch 2026-08-20:
Row and header labels, ported from the terminal picker so both surfaces read the
same ("6m ago", "Today", "3 days ago", "last active Aug 18 23:47 UTC").
*/

const SECONDS_PER_DAY = 86_400;
const MONTHS = [
  "Jan", "Feb", "Mar", "Apr", "May", "Jun",
  "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
] as const;

export function findPromptDayKey(ts: number): number {
  if (!Number.isFinite(ts) || ts <= 0) {
    return Number.NEGATIVE_INFINITY;
  }
  return Math.floor(ts / SECONDS_PER_DAY);
}

function utcParts(ts: number) {
  const date = new Date(ts * 1000);
  return {
    day: date.getUTCDate(),
    hour: date.getUTCHours(),
    minute: date.getUTCMinutes(),
    month: MONTHS[date.getUTCMonth()] ?? "???",
    year: date.getUTCFullYear(),
  };
}

/** Compact last-active label shown under each result. */
export function formatLastActiveCompact(ts: number, now: number): string {
  if (!Number.isFinite(ts) || ts <= 0) {
    return "unknown";
  }
  const delta = Math.max(0, now - ts);
  if (delta < 60) return "now";
  if (delta < 3_600) return `${Math.trunc(delta / 60)}m ago`;
  if (delta < SECONDS_PER_DAY) return `${Math.trunc(delta / 3_600)}h ago`;
  if (delta < 7 * SECONDS_PER_DAY) return `${Math.trunc(delta / SECONDS_PER_DAY)}d ago`;
  const { day, month } = utcParts(ts);
  return `${month} ${day}`;
}

/** Day-group header text when `^d` grouping is on. */
export function formatDayHeader(dayKey: number, now: number): string {
  if (!Number.isFinite(dayKey)) {
    return "Unknown day";
  }
  const today = findPromptDayKey(now);
  if (dayKey === today) return "Today";
  if (dayKey === today - 1) return "Yesterday";
  if (dayKey > today - 7 && dayKey < today) return `${today - dayKey} days ago`;
  const date = utcParts(dayKey * SECONDS_PER_DAY);
  const nowDate = utcParts(today * SECONDS_PER_DAY);
  if (date.year === nowDate.year) {
    return `${date.month} ${date.day}`;
  }
  return `${date.month} ${date.day}, ${date.year}`;
}

/** The footer's "last active …" line. */
export function formatLastActiveFull(ts: number): string {
  if (!Number.isFinite(ts) || ts <= 0) {
    return "last active unknown";
  }
  const { day, hour, minute, month } = utcParts(ts);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `last active ${month} ${day} ${pad(hour)}:${pad(minute)} UTC`;
}

/** The compact usage/model tail of the footer metadata line. */
export function formatPromptMetaLine(meta: {
  model: string;
  plan: string;
  provider: string;
  thinking: string;
  usage: {
    cacheRead: number;
    cacheWrite: number;
    contextWindow: number;
    cost: number;
    input: number;
    output: number;
    ratePercent: number;
  };
}): string {
  const parts: string[] = [];
  const { usage } = meta;
  if (usage.input > 0) parts.push(`↑${usage.input}`);
  if (usage.output > 0) parts.push(`↓${usage.output}`);
  if (usage.cacheRead > 0) parts.push(`R${usage.cacheRead}`);
  if (usage.cacheWrite > 0) parts.push(`W${usage.cacheWrite}`);
  if (usage.cost > 0) parts.push(`$${usage.cost.toFixed(3)}`);
  if (meta.plan) parts.push(`(${meta.plan})`);
  if (usage.ratePercent > 0) {
    parts.push(
      usage.contextWindow > 0
        ? `${usage.ratePercent.toFixed(1)}%/${usage.contextWindow}`
        : `${usage.ratePercent.toFixed(1)}%`,
    );
  } else if (usage.contextWindow > 0) {
    parts.push(`/${usage.contextWindow}`);
  }
  const model = [meta.provider ? `(${meta.provider})` : "", meta.model].filter(Boolean).join(" ");
  if (model) parts.push(model);
  if (meta.thinking) parts.push(`• ${meta.thinking}`);
  return parts.join(" ");
}
