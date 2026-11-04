import type { WorkItem, WorkItemRef, WorkStatusGroup } from './types';

/** "now", "4m", "3h", "2d", "5w" — the same short form the sidebar's cards use. */
export function relativeTime(iso: string | undefined | null, now = Date.now()): string {
  if (!iso) return '';
  const time = Date.parse(iso);
  if (Number.isNaN(time)) return '';
  const seconds = Math.max(0, Math.round((now - time) / 1000));
  if (seconds < 60) return 'now';
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.round(hours / 24);
  if (days < 14) return `${days}d`;
  return `${Math.round(days / 7)}w`;
}

export function formatDuration(seconds: number | null | undefined): string {
  if (seconds == null || seconds < 0) return '—';
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return minutes > 0 ? `${minutes}m ${String(rest).padStart(2, '0')}s` : `${rest}s`;
}

/** The status filter's choices; "open" keeps every row the list holds (all of it is open work). */
export type StatusFilter = 'open' | 'todo' | 'progress' | 'review';

export const STATUS_FILTERS: { value: StatusFilter; label: string }[] = [
  { value: 'open', label: 'Open' },
  { value: 'todo', label: 'Todo' },
  { value: 'progress', label: 'In progress' },
  { value: 'review', label: 'In review' },
];

export function statusMatches(filter: StatusFilter, group: WorkStatusGroup): boolean {
  switch (filter) {
    case 'open':
      return group !== 'done' && group !== 'canceled' && group !== 'merged' && group !== 'closed';
    case 'todo':
      return group === 'todo' || group === 'backlog';
    case 'progress':
      return group === 'progress' || group === 'draft';
    case 'review':
      return group === 'review' || group === 'open';
  }
}

/** The request fields that name an item for `work.read` and `work.startChat`. */
export function itemRef(item: WorkItem): WorkItemRef {
  if (item.kind === 'ticket') {
    return { projectId: item.projectId, source: item.source, ticketId: item.ticketId, sourceKey: item.sourceKey };
  }
  return {
    projectId: item.projectId,
    pullRequest: item.pullRequestRef ?? item.pullRequest?.url,
  };
}

export function refKey(ref: WorkItemRef): string {
  // A GitHub issue number is only unique within its project's repo.
  if (ref.source === 'github') return `issue:${ref.projectId ?? ''}#${ref.ticketId}`;
  if (ref.ticketId != null) return `${ref.source}:${ref.ticketId}`;
  return `pr:${ref.pullRequest ?? ''}`;
}

/** The label of a row's button that opens it in the browser: a ticket in its own source, a PR on GitHub. */
export function openItemLabel(item: Pick<WorkItem, 'kind' | 'source'>): string {
  return item.kind === 'ticket' ? sourceText(item.source).open : 'Open on GitHub';
}

/** How the page names a ticket's source; one it does not know is named by its id. */
export function sourceText(source: string | undefined, label = '') {
  switch (source) {
    case 'linear':
      return {
        app: 'Linear',
        open: 'Open in Linear',
        kind: 'Linear issue',
        card: 'Linear issue',
        group: 'Linear tickets',
        tone: 'c-linear',
        cardClass: 'linear-issue-card',
      };
    case 'github':
      return {
        app: 'GitHub',
        open: 'Open on GitHub',
        kind: 'GitHub issue',
        card: `Issue ${label}`,
        group: 'GitHub issues',
        tone: 'c-open',
        cardClass: 'github-issue-card',
      };
    default:
      return {
        app: source ?? 'Tracker',
        open: `Open in ${source ?? 'tracker'}`,
        kind: 'Ticket',
        card: `Ticket ${label}`,
        group: `${source ?? 'Other'} tickets`,
        tone: '',
        cardClass: 'ticket-card',
      };
  }
}

export function initials(name: string | undefined | null): string {
  const words = (name ?? '')
    .trim()
    .split(/[\s._-]+/u)
    .filter(Boolean);
  if (words.length === 0) return '?';
  const first = words[0] ?? '';
  const second = words[1] ?? '';
  return (first.charAt(0) + (second ? second.charAt(0) : first.charAt(1))).toUpperCase();
}

/** A stable hue per person, so the same teammate always gets the same avatar color. */
export function personHue(name: string | undefined | null): number {
  let hash = 0;
  for (const char of name ?? '') hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return hash % 360;
}
