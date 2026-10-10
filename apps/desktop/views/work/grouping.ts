import { managedStore } from "@/packages/client-storage";
import type { WorkItem, WorkStatusGroup, WorkTracker } from "./types";

/**
 * CDXC:WorkMode 2026-10-10 DECISION:
 * User: "i also want you to please add dropdown that lets me group the list items by different ways that are useful". The Work list's Group by dropdown offers None (the default, the flat list), Status, Repo, Project, Assignee, Type, Pull request and Updated; filters apply first, headers show the name and a count and collapse on click, and rows inside a group keep the list's recently-updated order. The choice is remembered per viewer in the page's own storage.
 */
export type WorkGroupBy =
  | "none"
  | "status"
  | "repo"
  | "project"
  | "assignee"
  | "type"
  | "pullRequest"
  | "updated";

export const GROUP_BY_OPTIONS: { value: WorkGroupBy; label: string }[] = [
  { value: "none", label: "None" },
  { value: "status", label: "Status" },
  { value: "repo", label: "Repo" },
  { value: "project", label: "Project" },
  { value: "assignee", label: "Assignee" },
  { value: "type", label: "Type" },
  { value: "pullRequest", label: "Pull request" },
  { value: "updated", label: "Updated" },
];

export interface WorkGroup {
  key: string;
  label: string;
  items: WorkItem[];
}

/** The viewer's choice, a managed preference (packages/client-storage/catalog.ts `workGroupBy`). */
const groupByStore = managedStore("workGroupBy");

export function readStoredGroupBy(): WorkGroupBy {
  try {
    return groupByStore.get() ?? "none";
  } catch {
    // Storage blocked or an unreadable value: the list starts flat.
    return "none";
  }
}

export function storeGroupBy(groupBy: WorkGroupBy): void {
  try {
    groupByStore.set(groupBy);
  } catch {
    // Storage blocked: the choice lasts as long as the page.
  }
}

/** A group's place and name; `rank` orders groups, then `label` alphabetically. */
interface Bucket {
  key: string;
  label: string;
  rank: number;
}

/** The status filter's groups (format.ts `statusMatches`), in workflow order. */
const STATUS_BUCKETS: Record<WorkStatusGroup, Bucket> = {
  backlog: { key: "backlog", label: "Backlog", rank: 0 },
  todo: { key: "todo", label: "Todo", rank: 1 },
  progress: { key: "progress", label: "In progress", rank: 2 },
  draft: { key: "progress", label: "In progress", rank: 2 },
  review: { key: "review", label: "In review", rank: 3 },
  open: { key: "review", label: "In review", rank: 3 },
  done: { key: "done", label: "Done", rank: 4 },
  merged: { key: "done", label: "Done", rank: 4 },
  canceled: { key: "canceled", label: "Canceled", rank: 5 },
  closed: { key: "canceled", label: "Canceled", rank: 5 },
};

const DAY_MS = 24 * 60 * 60 * 1000;

function startOfDay(time: number): number {
  const date = new Date(time);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

function updatedBucket(item: WorkItem, now: number): Bucket {
  const time = item.updatedAt ? Date.parse(item.updatedAt) : Number.NaN;
  const today = startOfDay(now);
  if (Number.isNaN(time)) return { key: "older", label: "Older", rank: 3 };
  if (time >= today) return { key: "today", label: "Today", rank: 0 };
  if (time >= today - DAY_MS)
    return { key: "yesterday", label: "Yesterday", rank: 1 };
  if (time >= today - 6 * DAY_MS)
    return { key: "week", label: "This week", rank: 2 };
  return { key: "older", label: "Older", rank: 3 };
}

function pullRequestBucket(item: WorkItem): Bucket {
  const pullRequest = item.pullRequest;
  if (!pullRequest) return { key: "none", label: "No PR", rank: 0 };
  if (pullRequest.state === "draft")
    return { key: "draft", label: "Draft", rank: 1 };
  if (pullRequest.state === "merged")
    return { key: "merged", label: "Merged", rank: 5 };
  if (pullRequest.state === "closed")
    return { key: "closed", label: "Closed", rank: 6 };
  if (pullRequest.checks === "failing")
    return { key: "failing", label: "Checks failing", rank: 2 };
  if (pullRequest.checks === "pending")
    return { key: "pending", label: "Checks pending", rank: 3 };
  return { key: "ready", label: "Ready", rank: 4 };
}

function bucketOf(
  item: WorkItem,
  groupBy: WorkGroupBy,
  tracker: WorkTracker,
  now: number,
): Bucket {
  switch (groupBy) {
    case "status":
      return STATUS_BUCKETS[item.status.group];
    case "repo":
      return item.projectName
        ? {
            key: `repo:${item.projectId ?? item.projectName}`,
            label: item.projectName,
            rank: 0,
          }
        : { key: "repo:none", label: "No repo yet", rank: 1 };
    case "project": {
      const project =
        tracker === "github" ? item.githubProject : item.linearProject;
      return project
        ? { key: `project:${project.name}`, label: project.name, rank: 0 }
        : { key: "project:none", label: "No project", rank: 1 };
    }
    case "assignee":
      if (!item.assignee)
        return { key: "assignee:none", label: "Unassigned", rank: 2 };
      if (item.assignee.isMe)
        return { key: "assignee:me", label: "You", rank: 0 };
      return {
        key: `assignee:${item.assignee.name.toLowerCase()}`,
        label: item.assignee.name,
        rank: 1,
      };
    case "type":
      if (item.kind === "linearIssue")
        return { key: "linearIssue", label: "Linear tickets", rank: 0 };
      if (item.kind === "githubIssue")
        return { key: "githubIssue", label: "GitHub issues", rank: 1 };
      return { key: "pullRequest", label: "PRs without a ticket", rank: 2 };
    case "pullRequest":
      return pullRequestBucket(item);
    case "updated":
      return updatedBucket(item, now);
    case "none":
      return { key: "all", label: "", rank: 0 };
  }
}

/** Splits the filtered list into groups; each keeps the list's order (recently updated first). */
export function groupWorkItems(
  items: WorkItem[],
  groupBy: WorkGroupBy,
  tracker: WorkTracker,
  now: number,
): WorkGroup[] {
  const groups = new Map<string, Bucket & { items: WorkItem[] }>();
  for (const item of items) {
    const bucket = bucketOf(item, groupBy, tracker, now);
    const group = groups.get(bucket.key);
    if (group) group.items.push(item);
    else groups.set(bucket.key, { ...bucket, items: [item] });
  }
  return [...groups.values()]
    .sort((a, b) => a.rank - b.rank || a.label.localeCompare(b.label))
    .map(({ key, label, items: groupItems }) => ({
      key,
      label,
      items: groupItems,
    }));
}
