import {
  IconAlertTriangle,
  IconArrowsSort,
  IconBox,
  IconChevronDown,
  IconCircleDot,
  IconCopy,
  IconFolder,
  IconGitPullRequest,
  IconHash,
  IconLayoutList,
  IconLayoutSidebar,
  IconPlus,
  IconRefresh,
  IconSearch,
  IconX,
} from "@tabler/icons-react";
import { useMemo, type CSSProperties, type ReactNode } from "react";
import {
  Avatar,
  Button,
  ChecksIcon,
  Dropdown,
  LiveDot,
  MenuItem,
  PullRequestChip,
  Spinner,
  StatusGlyph,
  Toggle,
  cx,
} from "./components";
import { GROUP_BY_OPTIONS, groupWorkItems, type WorkGroupBy } from "./grouping";
import {
  relativeTime,
  STATUS_FILTERS,
  statusMatches,
  type StatusFilter,
} from "./format";
import type {
  WorkItem,
  WorkItemLink,
  WorkList as WorkListData,
  WorkTracker,
} from "./types";

export interface WorkFilters {
  search: string;
  assignedToMe: boolean;
  linearIssues: boolean;
  githubIssues: boolean;
  pullRequests: boolean;
  status: StatusFilter;
  projectId: string;
  /** A Linear project's or a GitHub Project's name, whichever the workspace's tracker uses. */
  trackerProject: string;
  inSidebar: boolean;
}

/**
 * CDXC:WorkMode 2026-10-09 DECISION:
 * User: the Work list opens with "Assigned to me" turned on; the other filters (the kinds, status
 * open, repo, project, "In my sidebar") start wide open.
 */
export const DEFAULT_WORK_FILTERS: WorkFilters = {
  search: "",
  assignedToMe: true,
  linearIssues: true,
  githubIssues: true,
  pullRequests: true,
  status: "open",
  projectId: "",
  trackerProject: "",
  inSidebar: false,
};

/** The project an item shows: a Linear project, or a GitHub Project in a GitHub workspace. */
export function trackerProjectOf(
  item: WorkItem,
  tracker: WorkTracker,
): WorkItemLink | undefined {
  return tracker === "github" ? item.githubProject : item.linearProject;
}

export function filterWorkItems(
  items: WorkItem[],
  filters: WorkFilters,
  tracker: WorkTracker = "linear",
): WorkItem[] {
  const query = filters.search.trim().toLowerCase();
  return items.filter((item) => {
    if (filters.assignedToMe && !item.assignedToMe) return false;
    if (item.kind === "linearIssue" && !filters.linearIssues) return false;
    if (item.kind === "githubIssue" && !filters.githubIssues) return false;
    if (item.kind === "pullRequest" && !filters.pullRequests) return false;
    if (!statusMatches(filters.status, item.status.group)) return false;
    if (filters.projectId && item.projectId !== filters.projectId) return false;
    if (
      filters.trackerProject &&
      trackerProjectOf(item, tracker)?.name !== filters.trackerProject
    )
      return false;
    if (filters.inSidebar && item.sessions.length === 0) return false;
    if (query) {
      const haystack = [
        item.id,
        item.title,
        item.projectName,
        trackerProjectOf(item, tracker)?.name,
        item.assignee?.name,
        item.branchName,
        item.pullRequest ? `#${item.pullRequest.number}` : "",
        ...item.labels,
      ]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();
      if (!haystack.includes(query)) return false;
    }
    return true;
  });
}

function activeFilterCount(filters: WorkFilters): number {
  let count = 0;
  if (filters.assignedToMe) count += 1;
  if (!filters.linearIssues || !filters.githubIssues || !filters.pullRequests)
    count += 1;
  if (filters.status !== "open") count += 1;
  if (filters.projectId) count += 1;
  if (filters.trackerProject) count += 1;
  if (filters.inSidebar) count += 1;
  return count;
}

export function WorkListView({
  data,
  loading,
  error,
  filters,
  onFiltersChange,
  onRefresh,
  onOpen,
  onNewTicket,
  newTicketError,
  onDismissNotice,
  groupBy,
  onGroupByChange,
  collapsedGroups,
  onToggleGroup,
  onOpenUrl,
  now,
}: {
  data: WorkListData | null;
  loading: boolean;
  error: string | null;
  filters: WorkFilters;
  onFiltersChange: (filters: WorkFilters) => void;
  onRefresh: () => void;
  onOpen: (item: WorkItem) => void;
  /** Opens the app's native Create Linear Ticket dialog (apps/desktop/src/app/work_view/bridge.rs). */
  onNewTicket: () => void;
  newTicketError: string | null;
  /** Closes a notice for good (gxserver remembers it). */
  onDismissNotice: (notice: string) => void;
  groupBy: WorkGroupBy;
  onGroupByChange: (groupBy: WorkGroupBy) => void;
  /** `<groupBy>:<group key>` of every collapsed group header. */
  collapsedGroups: ReadonlySet<string>;
  onToggleGroup: (groupId: string) => void;
  /** Opens a ticket's or PR's page in the app's browser (`work.openUrl`). */
  onOpenUrl: (url: string) => void;
  now: number;
}) {
  const items = data?.items ?? [];
  // CDXC:WorkMode 2026-10-09 DECISION:
  // User: the Work page lists the workspace's primary tracker's tickets (plus PRs); the other tracker's type filter is hidden, and the project filter is "All projects" for the primary's kind of project.
  const tracker: WorkTracker = data?.tracker ?? "linear";
  const visible = useMemo(
    () => filterWorkItems(items, filters, tracker),
    [items, filters, tracker],
  );
  const groups = useMemo(
    () =>
      groupBy === "none" ? [] : groupWorkItems(visible, groupBy, tracker, now),
    [visible, groupBy, tracker, now],
  );
  // Every row lines up: the ID column is as wide as the longest visible ID (11.5px mono, about
  // 7px a character), and the Slack slot exists only when some row has Slack threads.
  const rowsStyle = useMemo(
    () =>
      ({
        "--w-id-w": `${Math.max(5, ...visible.map((item) => item.id.length)) * 7 + 2}px`,
      }) as CSSProperties,
    [visible],
  );
  const rowsClass = visible.some((item) => item.slackThreadCount)
    ? "has-slack"
    : undefined;
  const trackerProjects = useMemo(
    () =>
      [
        ...new Set(
          items
            .map((item) => trackerProjectOf(item, tracker)?.name)
            .filter((name): name is string => Boolean(name)),
        ),
      ].sort((a, b) => a.localeCompare(b)),
    [items, tracker],
  );
  const projects = data?.projects ?? [];
  const set = (patch: Partial<WorkFilters>) =>
    onFiltersChange({ ...filters, ...patch });
  const projectLabel =
    projects.find((project) => project.projectId === filters.projectId)?.name ??
    "All repos";
  const workspaceLabel =
    projects.length === 1
      ? (projects[0]?.name ?? "")
      : projects.length > 1
        ? `${projects.length} projects`
        : "";
  const groupByLabel =
    GROUP_BY_OPTIONS.find((option) => option.value === groupBy)?.label ??
    "None";
  const statusLabel =
    STATUS_FILTERS.find((option) => option.value === filters.status)?.label ??
    "Open";

  return (
    <div className="w-page work-list-page">
      <div className="w-headrow">
        <div>
          <div className="w-eyebrow">
            Work{workspaceLabel ? ` · ${workspaceLabel}` : ""}
          </div>
          <h1 className="w-title">Ongoing work</h1>
        </div>
        <div className="w-headrow-spacer" />
        {projects.length > 0 ? (
          <Button
            className="work-new-ticket"
            title={
              tracker === "github"
                ? "Create a GitHub issue"
                : "Create a Linear ticket"
            }
            onClick={onNewTicket}
          >
            <IconPlus size={14} />
            New ticket
          </Button>
        ) : null}
      </div>
      {newTicketError ? (
        <div className="w-notice is-error work-new-ticket-error">
          <IconAlertTriangle size={14} />
          <span>{newTicketError}</span>
        </div>
      ) : null}

      <div className="w-toolbar">
        <label className="w-search work-search">
          <IconSearch size={14} />
          <input
            value={filters.search}
            placeholder="Search work"
            onChange={(event) => set({ search: event.target.value })}
            spellCheck={false}
          />
          {filters.search ? (
            <button
              type="button"
              className="w-search-clear"
              aria-label="Clear search"
              onClick={() => set({ search: "" })}
            >
              <IconX size={12} />
            </button>
          ) : null}
        </label>
        <Button
          size="icon"
          title="Refresh"
          onClick={onRefresh}
          disabled={loading}
        >
          {loading ? <Spinner /> : <IconRefresh size={15} />}
        </Button>
      </div>

      <div className="w-toggles work-filters">
        <Toggle
          className="filter-assigned-to-me"
          pressed={filters.assignedToMe}
          onPressedChange={(assignedToMe) => set({ assignedToMe })}
        >
          <Avatar name="Me" size={15} />
          Assigned to me
        </Toggle>
        {tracker === "linear" ? (
          <Toggle
            className="filter-linear"
            pressed={filters.linearIssues}
            onPressedChange={(linearIssues) => set({ linearIssues })}
          >
            <IconCircleDot size={13} className="c-linear" />
            Linear issues
          </Toggle>
        ) : (
          <Toggle
            className="filter-gh-issues"
            pressed={filters.githubIssues}
            onPressedChange={(githubIssues) => set({ githubIssues })}
          >
            <IconCircleDot size={13} className="c-open" />
            GitHub issues
          </Toggle>
        )}
        <Toggle
          className="filter-prs"
          pressed={filters.pullRequests}
          onPressedChange={(pullRequests) => set({ pullRequests })}
        >
          <IconGitPullRequest size={13} className="c-open" />
          PRs
        </Toggle>
        <Dropdown
          className="filter-status"
          trigger={(open, toggle) => (
            <button
              type="button"
              className={cx("w-toggle", "is-on", open && "is-focus")}
              onClick={toggle}
            >
              {statusLabel}
              <IconChevronDown size={12} />
            </button>
          )}
        >
          {(close) =>
            STATUS_FILTERS.map((option) => (
              <MenuItem
                key={option.value}
                checked={filters.status === option.value}
                onSelect={() => {
                  set({ status: option.value });
                  close();
                }}
              >
                {option.label}
              </MenuItem>
            ))
          }
        </Dropdown>
        <Dropdown
          className="filter-project"
          trigger={(open, toggle) => (
            <button
              type="button"
              className={cx(
                "w-toggle",
                filters.projectId && "is-on",
                open && "is-focus",
              )}
              onClick={toggle}
            >
              <IconFolder size={13} />
              {projectLabel}
              <IconChevronDown size={12} />
            </button>
          )}
        >
          {(close) => (
            <>
              <MenuItem
                checked={!filters.projectId}
                onSelect={() => {
                  set({ projectId: "" });
                  close();
                }}
              >
                All repos
              </MenuItem>
              {projects.map((project) => (
                <MenuItem
                  key={project.projectId}
                  checked={filters.projectId === project.projectId}
                  onSelect={() => {
                    set({ projectId: project.projectId });
                    close();
                  }}
                >
                  {project.name}
                  {project.repo ? (
                    <span className="w-menu-hint">{project.repo}</span>
                  ) : null}
                </MenuItem>
              ))}
            </>
          )}
        </Dropdown>
        <Dropdown
          className="filter-tracker-project"
          trigger={(open, toggle) => (
            <button
              type="button"
              className={cx(
                "w-toggle",
                filters.trackerProject && "is-on",
                open && "is-focus",
              )}
              onClick={toggle}
            >
              <IconBox
                size={13}
                className={tracker === "linear" ? "c-linear" : undefined}
              />
              {filters.trackerProject || "All projects"}
              <IconChevronDown size={12} />
            </button>
          )}
        >
          {(close) => (
            <>
              <MenuItem
                checked={!filters.trackerProject}
                onSelect={() => {
                  set({ trackerProject: "" });
                  close();
                }}
              >
                All projects
              </MenuItem>
              {trackerProjects.map((name) => (
                <MenuItem
                  key={name}
                  checked={filters.trackerProject === name}
                  onSelect={() => {
                    set({ trackerProject: name });
                    close();
                  }}
                >
                  {name}
                </MenuItem>
              ))}
            </>
          )}
        </Dropdown>
        <Toggle
          className="filter-in-sidebar"
          pressed={filters.inSidebar}
          onPressedChange={(inSidebar) => set({ inSidebar })}
        >
          <IconLayoutSidebar size={13} />
          In my sidebar
        </Toggle>
        <Dropdown
          className="filter-group-by"
          trigger={(open, toggle) => (
            <button
              type="button"
              className={cx(
                "w-toggle",
                groupBy !== "none" && "is-on",
                open && "is-focus",
              )}
              onClick={toggle}
            >
              <IconLayoutList size={13} />
              {groupBy === "none" ? "Group by" : `Group: ${groupByLabel}`}
              <IconChevronDown size={12} />
            </button>
          )}
        >
          {(close) =>
            GROUP_BY_OPTIONS.map((option) => (
              <MenuItem
                key={option.value}
                checked={groupBy === option.value}
                onSelect={() => {
                  onGroupByChange(option.value);
                  close();
                }}
              >
                {option.label}
              </MenuItem>
            ))
          }
        </Dropdown>
      </div>

      <Notices data={data} error={error} />
      <GithubProjectsNotice data={data} onDismiss={onDismissNotice} />

      <div className="w-list-meta">
        <span>
          {listSummary(
            data,
            visible.length,
            activeFilterCount(filters),
            loading,
            now,
          )}
        </span>
        <span className="w-spacer" />
        <span className="w-sort">
          <IconArrowsSort size={12} />
          Recently updated
        </span>
      </div>

      {!data && loading ? (
        <SkeletonRows />
      ) : visible.length === 0 ? (
        <EmptyList
          data={data}
          filters={filters}
          onShowAll={() => set({ assignedToMe: false })}
        />
      ) : groupBy === "none" ? (
        <div
          className={cx("w-list work-list", rowsClass)}
          style={rowsStyle}
          role="list"
        >
          {visible.map((item) => (
            <WorkRow
              key={item.key}
              item={item}
              now={now}
              onOpen={onOpen}
              onOpenUrl={onOpenUrl}
            />
          ))}
        </div>
      ) : (
        <div
          className={cx("w-groups work-groups", rowsClass)}
          style={rowsStyle}
        >
          {groups.map((group) => {
            const groupId = `${groupBy}:${group.key}`;
            const collapsed = collapsedGroups.has(groupId);
            return (
              <section
                key={groupId}
                className={cx(
                  "w-group",
                  `work-group-${group.key.replace(/[^a-z0-9]+/giu, "-").toLowerCase()}`,
                  collapsed && "is-collapsed",
                )}
              >
                <button
                  type="button"
                  className="w-group-head"
                  aria-expanded={!collapsed}
                  onClick={() => onToggleGroup(groupId)}
                >
                  <IconChevronDown size={13} className="w-group-chevron" />
                  <span className="w-group-name">{group.label}</span>
                  <span className="w-group-count">{group.items.length}</span>
                </button>
                {collapsed ? null : (
                  <div className="w-list work-list" role="list">
                    {group.items.map((item) => (
                      <WorkRow
                        key={item.key}
                        item={item}
                        now={now}
                        onOpen={onOpen}
                        onOpenUrl={onOpenUrl}
                      />
                    ))}
                  </div>
                )}
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}

function listSummary(
  data: WorkListData | null,
  count: number,
  filtersOn: number,
  loading: boolean,
  now: number,
): string {
  if (!data) return loading ? "Loading your work…" : "";
  const updated = data.refreshing
    ? "updating…"
    : `updated ${relativeTime(data.generatedAt, now) || "now"}`;
  const filtered =
    filtersOn > 0 ? ` · ${filtersOn} filter${filtersOn === 1 ? "" : "s"}` : "";
  return `${count} open${filtered} · ${updated}`;
}

function Notices({
  data,
  error,
}: {
  data: WorkListData | null;
  error: string | null;
}) {
  const notices: string[] = [];
  if (error) notices.push(error);
  if (data && !data.linearConfigured && data.tracker !== "github") {
    notices.push(
      'Add a Linear API key to see Linear tickets: run "ghostex work-mode linear-key" or set it in Settings.',
    );
  }
  if (data && !data.ghAvailable)
    notices.push(
      "Install and sign in to the GitHub CLI (gh) to see pull requests and issues.",
    );
  for (const message of data?.errors ?? []) notices.push(message);
  if (notices.length === 0) return null;
  return (
    <div className="w-notices">
      {notices.map((notice) => (
        <div key={notice} className="w-notice">
          <IconAlertTriangle size={14} />
          <span>{notice}</span>
        </div>
      ))}
    </div>
  );
}

/**
 * CDXC:WorkMode 2026-10-09 DECISION:
 * User: when `gh` cannot read GitHub Projects, "Ask user to run that command if needed with a closable notice on the page that appears once": it shows the command with a Copy button, and closing it is remembered (gxserver, `/api/dismissWorkNotice`).
 */
function GithubProjectsNotice({
  data,
  onDismiss,
}: {
  data: WorkListData | null;
  onDismiss: (notice: string) => void;
}) {
  const status = data?.githubProjects;
  if (
    data?.tracker !== "github" ||
    status?.access !== "missingScope" ||
    status.noticeDismissed
  )
    return null;
  return (
    <div className="w-notice work-github-projects-notice">
      <IconAlertTriangle size={14} />
      <span className="w-notice-text">
        To show GitHub Projects, run <code>{status.command}</code>
      </span>
      <button
        type="button"
        className="w-notice-action work-github-projects-copy"
        title="Copy the command"
        onClick={() =>
          void navigator.clipboard
            ?.writeText(status.command)
            .catch(() => undefined)
        }
      >
        <IconCopy size={13} />
        Copy
      </button>
      <button
        type="button"
        className="w-notice-close work-github-projects-close"
        aria-label="Close"
        title="Close"
        onClick={() => onDismiss("githubProjectsScope")}
      >
        <IconX size={13} />
      </button>
    </div>
  );
}

function EmptyList({
  data,
  filters,
  onShowAll,
}: {
  data: WorkListData | null;
  filters: WorkFilters;
  onShowAll: () => void;
}) {
  if (
    data &&
    data.items.length > 0 &&
    filters.assignedToMe &&
    !data.items.some((item) => item.assignedToMe)
  ) {
    return (
      <div className="w-empty">
        <p>Nothing is assigned to you right now.</p>
        <Button size="sm" onClick={onShowAll}>
          Show everyone's work
        </Button>
      </div>
    );
  }
  return (
    <div className="w-empty">
      <p>
        {data && data.items.length > 0
          ? "No work matches these filters."
          : "No open work in these projects."}
      </p>
    </div>
  );
}

function SkeletonRows() {
  return (
    <div className="w-list" aria-hidden>
      {Array.from({ length: 6 }, (_, index) => (
        <div key={index} className="w-row is-skeleton">
          <span className="w-skel w-skel--glyph" />
          <span
            className="w-skel"
            style={{ width: `${55 + ((index * 17) % 35)}%` }}
          />
          <span className="w-skel w-skel--time" />
          <span className="w-skel w-skel--line2" />
        </div>
      ))}
    </div>
  );
}

/**
 * CDXC:WorkMode 2026-10-10 DECISION:
 * User: "on each card i want to be able to click on the pr id or the linear/github issue id to open that in a new tab (from the list view)". The row's ticket ID and its PR number are links that open the page in the app's browser (`work.openUrl`: a new tab, or the tab already showing that exact page); the rest of the row still opens the details.
 */
function RowLink({
  url,
  className,
  children,
  onOpenUrl,
}: {
  url: string | undefined;
  className: string;
  children: ReactNode;
  onOpenUrl: (url: string) => void;
}) {
  if (!url) return <span className={className}>{children}</span>;
  return (
    <button
      type="button"
      className={cx(className, "w-link")}
      title={`Open in browser
${url}`}
      onClick={(event) => {
        event.stopPropagation();
        onOpenUrl(url);
      }}
      onKeyDown={(event) => {
        // The row opens the details on Enter and Space; the link keeps its own keys.
        if (event.key === "Enter" || event.key === " ") event.stopPropagation();
      }}
    >
      {children}
    </button>
  );
}

/**
 * CDXC:WorkMode 2026-10-10 DECISION:
 * User: "Please let's do B, two lines with fixed slots but put the project one to be the most right between those ones so it has space and isn't forced to truncate needlessly". Line 1 is status, ID (fixed width), title, time; line 2 starts under the ID with fixed slots (repo, PR with checks, owner, Slack threads, In sidebar) and the Linear/GitHub project last, taking the rest. An empty slot keeps its width with a faint placeholder; a narrow list drops the Slack slot first, then the repo slot (work.css container queries).
 */
function WorkRow({
  item,
  now,
  onOpen,
  onOpenUrl,
}: {
  item: WorkItem;
  now: number;
  onOpen: (item: WorkItem) => void;
  onOpenUrl: (url: string) => void;
}) {
  const working = item.sessions.find((session) => session.working);
  const project = item.linearProject ?? item.githubProject;
  return (
    <div
      role="listitem"
      tabIndex={0}
      className={cx(
        "w-row",
        `work-row-${item.key.replace(/[^a-z0-9]+/giu, "-").toLowerCase()}`,
      )}
      onClick={() => onOpen(item)}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onOpen(item);
        }
      }}
    >
      <span className="w-kind">
        <StatusGlyph item={item} />
      </span>
      <RowLink
        url={item.url}
        className="w-id work-row-id"
        onOpenUrl={onOpenUrl}
      >
        {item.id}
      </RowLink>
      <span className="w-row-title">{item.title}</span>
      <span className="w-time">{relativeTime(item.updatedAt, now)}</span>
      <div className="w-l2">
        <span className="w-slot w-slot-repo work-slot-repo">
          {item.projectName ? (
            <>
              <IconFolder size={12} />
              <span className="w-slot-text" title={item.projectName}>
                {item.projectName}
              </span>
            </>
          ) : (
            <span className="w-placeholder">
              {item.kind === "linearIssue" ? "No repo yet" : "No repo"}
            </span>
          )}
        </span>
        <span className="w-slot w-slot-pr work-slot-pr">
          {item.kind === "pullRequest" ? (
            <>
              {item.pullRequest?.checks ? (
                <span
                  className="w-row-checks"
                  title={
                    item.pullRequest.checks === "passing"
                      ? "Checks passing"
                      : item.pullRequest.checks === "failing"
                        ? "Checks failing"
                        : "Checks running"
                  }
                >
                  <ChecksIcon checks={item.pullRequest.checks} />
                </span>
              ) : null}
              {item.noTicket ? (
                <span className="w-needs">
                  <IconAlertTriangle size={12} />
                  No ticket
                </span>
              ) : item.ticket ? (
                <span className="w-chip">{item.ticket}</span>
              ) : null}
            </>
          ) : item.pullRequest ? (
            <RowLink
              url={item.pullRequest.url}
              className="w-chip-link work-row-pr"
              onOpenUrl={onOpenUrl}
            >
              <PullRequestChip
                pullRequest={item.pullRequest}
                title={item.pullRequest.url ? null : undefined}
              />
            </RowLink>
          ) : (
            <span className="w-placeholder">No PR</span>
          )}
        </span>
        <span
          className="w-slot w-slot-owner work-slot-owner"
          title={
            item.assignee
              ? item.assignee.isMe
                ? "Assigned to you"
                : `Assigned to ${item.assignee.name}`
              : "Unassigned"
          }
        >
          {item.assignee ? (
            <Avatar
              name={item.assignee.name}
              url={item.assignee.avatarUrl}
              size={18}
            />
          ) : (
            <span className="w-avatar-empty" />
          )}
        </span>
        <span className="w-slot w-slot-slack work-slot-slack">
          {item.slackThreadCount ? (
            <span
              className="w-meta"
              title={`${item.slackThreadCount} Slack thread${item.slackThreadCount === 1 ? "" : "s"}`}
            >
              <IconHash size={12} className="c-slack" />
              {item.slackThreadCount}
            </span>
          ) : (
            <span className="w-placeholder">–</span>
          )}
        </span>
        <span className="w-slot w-slot-mine work-slot-mine">
          {working ? (
            <LiveDot title={`${working.title} is working right now`} />
          ) : item.sessions.length > 0 ? (
            <span className="w-mine" title="In your sidebar">
              <IconLayoutSidebar size={12} />
            </span>
          ) : null}
        </span>
        <span className="w-slot w-slot-project work-slot-project">
          {project ? (
            <>
              <IconBox
                size={12}
                className={item.linearProject ? "c-linear" : undefined}
              />
              <span className="w-slot-text" title={project.name}>
                {project.name}
              </span>
            </>
          ) : (
            <span className="w-placeholder">No project</span>
          )}
        </span>
      </div>
    </div>
  );
}
